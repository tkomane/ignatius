//! Pure `psql`-style backslash meta-command lexer and bounded watch dispatcher.
//!
//! This module translates interactive backslash commands (`\d`, `\dt`, `\du`,
//! `\l+`, `\conninfo`, `\x`, `\timing`, `\watch`, `\?`) into typed intents and
//! bounded, read-only `pg_catalog` SQL queries. It performs no network I/O, no
//! process execution, and no terminal interaction.
//!
//! Shell escapes (`\!`) and unsupported file-side `psql` commands (`\i`,
//! `\copy`, `\set`) are refused explicitly with actionable `Usage` diagnostics.

use crate::branding::BINARY_NAME;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::query::result::ResultSet;
use crate::query::statements::{
    skip_block_comment, skip_dollar_quoted, skip_double_quoted, skip_line_comment,
    skip_single_quoted, strip_comments,
};
use crate::query::value::Cell;

/// Maximum permitted `\watch` interval in seconds (1 hour).
pub const MAX_WATCH_INTERVAL_SECS: u64 = 3600;

/// Default `\watch` interval in seconds when omitted (`\watch`).
pub const DEFAULT_WATCH_INTERVAL_SECS: u64 = 2;

/// Maximum number of iterations a single `\watch` session may execute.
pub const MAX_WATCH_ITERATIONS: u64 = 1000;

/// Maximum number of catalog rows requested by any synthesized backslash query.
pub const CATALOG_ROW_LIMIT: usize = 500;

/// Parsed interactive `psql`-style meta-command or bounded watch directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaCommand {
    /// Read-only PostgreSQL catalog query synthesized from a backslash command.
    CatalogQuery {
        /// Normalized meta-command name (for example, `"\dt"` or `"\d+"`).
        command: String,
        /// Human-readable title for the result view.
        title: String,
        /// Bounded, `pg_catalog`-qualified read-only SQL statement.
        sql: String,
    },
    /// Toggles or explicitly sets expanded vertical record display (`\x`).
    ToggleExpanded(Option<bool>),
    /// Toggles or explicitly sets per-statement timing display (`\timing`).
    ToggleTiming(Option<bool>),
    /// Starts bounded periodic re-execution (`\watch [secs]`).
    StartWatch {
        /// Polling interval in seconds (`1..=MAX_WATCH_INTERVAL_SECS`).
        interval_secs: u64,
        /// Preceding SQL statement(s) when invoked with companion syntax.
        companion_sql: Option<String>,
    },
    /// Stops an active watch loop (`\watch stop`, `\watch off`).
    StopWatch,
    /// Displays the built-in meta-command help table (`\?`, `\h`).
    Help,
}

/// Returns whether the input starts with a backslash meta-command or ends with a
/// trailing `\watch` directive outside SQL strings, quoted identifiers, and comments.
#[must_use]
pub fn contains_metacommand(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('\\') {
        return true;
    }
    find_trailing_watch(input).is_some()
}

/// Parses an interactive editor submission for a standalone backslash
/// meta-command or trailing companion `\watch` directive.
///
/// Returns `None` when the buffer is plain SQL (including backslashes inside
/// PostgreSQL literals, quoted identifiers, dollar-quoted bodies, or comments).
#[must_use]
pub fn parse_input(input: &str) -> Option<Result<MetaCommand, Diagnostic>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('\\') {
        return Some(parse_standalone(trimmed));
    }
    if let Some((pos, watch_line)) = find_trailing_watch(input) {
        let prefix = input[..pos].trim();
        let companion_sql = (!strip_comments(prefix).trim().is_empty()).then(|| prefix.to_owned());
        return Some(parse_watch_line(watch_line, companion_sql));
    }
    None
}

/// Builds the static, in-memory help table returned for `\?` and `\h`.
#[must_use]
pub fn help_result() -> ResultSet {
    let entries: [(&str, &str, &str); 20] = [
        (
            "\\d",
            "\\d [pattern]",
            "List tables, views, sequences, or inspect table columns",
        ),
        (
            "\\d+",
            "\\d+ [pattern]",
            "List relations with sizes/comments, or inspect column storage and indexes",
        ),
        (
            "\\dt",
            "\\dt [pattern]",
            "List user tables (including partitioned tables)",
        ),
        ("\\dv", "\\dv [pattern]", "List views"),
        ("\\dm", "\\dm [pattern]", "List materialized views"),
        (
            "\\di",
            "\\di [pattern]",
            "List indexes and their target tables",
        ),
        ("\\ds", "\\ds [pattern]", "List sequences"),
        ("\\dn", "\\dn [pattern]", "List schemas (namespaces)"),
        (
            "\\du",
            "\\du [pattern]",
            "List database roles and memberships",
        ),
        (
            "\\dg",
            "\\dg [pattern]",
            "List database roles and memberships (alias of \\du)",
        ),
        ("\\df", "\\df [pattern]", "List functions and procedures"),
        ("\\dx", "\\dx [pattern]", "List installed extensions"),
        ("\\l", "\\l [pattern]", "List databases"),
        (
            "\\l+",
            "\\l+ [pattern]",
            "List databases with size, tablespace, and description",
        ),
        (
            "\\conninfo",
            "\\conninfo",
            "Show current database, user, server address, port, and version",
        ),
        (
            "\\x",
            "\\x [on|off]",
            "Toggle or set expanded vertical display",
        ),
        (
            "\\timing",
            "\\timing [on|off]",
            "Toggle or set per-statement timing display",
        ),
        (
            "\\watch",
            "\\watch [1..3600]",
            "Re-run SQL every N seconds (default 2s, max 1000 iterations)",
        ),
        (
            "\\watch stop",
            "\\watch stop | off",
            "Stop an active \\watch loop",
        ),
        (
            "\\? / \\h",
            "\\? | \\h",
            "Show this meta-command help table",
        ),
    ];

    let mut result = ResultSet::with_column_types(
        vec![
            "Command".to_owned(),
            "Syntax".to_owned(),
            "Description".to_owned(),
        ],
        vec![
            Some("text".to_owned()),
            Some("text".to_owned()),
            Some("text".to_owned()),
        ],
        entries.len().max(64),
    );
    for (command, syntax, description) in entries {
        result.push(vec![
            Cell::Text(command.to_owned()),
            Cell::Text(syntax.to_owned()),
            Cell::Text(description.to_owned()),
        ]);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PatternArg {
    segments: Vec<String>,
}

impl PatternArg {
    fn into_object_pattern(self, command: &str) -> Result<(Option<String>, String), Diagnostic> {
        match self.segments.as_slice() {
            [name] => {
                if name.is_empty() {
                    return Err(usage_diagnostic(
                        format!("Empty object pattern in '{command}'"),
                        "parsing meta-command pattern",
                        "Supply a non-empty object pattern or omit the pattern to list all objects.",
                    ));
                }
                Ok((None, name.clone()))
            }
            [schema, name] => {
                if schema.is_empty() || name.is_empty() {
                    return Err(usage_diagnostic(
                        format!("Incomplete qualified pattern in '{command}'"),
                        "parsing meta-command pattern",
                        "Use `[schema.]name` with non-empty schema and object patterns.",
                    ));
                }
                Ok((Some(schema.clone()), name.clone()))
            }
            _ => Err(usage_diagnostic(
                format!("Invalid qualified pattern in '{command}'"),
                "parsing meta-command pattern",
                "Use at most one unquoted dot (`[schema.]name`), or double-quote names containing dots.",
            )),
        }
    }

    fn into_unqualified_pattern(self, command: &str) -> Result<String, Diagnostic> {
        match self.segments.as_slice() {
            [name] => {
                if name.is_empty() {
                    return Err(usage_diagnostic(
                        format!("Empty pattern in '{command}'"),
                        "parsing meta-command pattern",
                        "Supply a non-empty pattern or omit it to list all entries.",
                    ));
                }
                Ok(name.clone())
            }
            _ => Err(usage_diagnostic(
                format!("Schema-qualified pattern is not supported for '{command}'"),
                "parsing meta-command pattern",
                "Pass a single unqualified name pattern (without a dot separator).",
            )),
        }
    }
}

fn parse_standalone(trimmed: &str) -> Result<MetaCommand, Diagnostic> {
    if trimmed.contains('\n') {
        return Err(usage_diagnostic(
            "Backslash meta-commands must fit on a single line",
            "parsing backslash meta-command",
            "Run one meta-command at a time, or place \\watch [secs] on the final line after SQL.",
        ));
    }

    let (cmd, rest) = split_command_head(trimmed)?;
    if cmd == "\\!" {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "Shell escape '\\!' is not supported in Ignatius",
            "parsing backslash meta-command",
        )
        .likely_cause("Ignatius does not spawn arbitrary shell commands from the SQL buffer.")
        .next_action(
            "Run external commands directly in your terminal, or type \\? to list supported meta-commands.",
        ));
    }

    if cmd == "\\watch" {
        return parse_watch_args(rest, None);
    }

    let args = parse_args(rest)?;
    match cmd.as_str() {
        "\\?" | "\\h" => {
            expect_no_args(&cmd, &args)?;
            Ok(MetaCommand::Help)
        }
        "\\x" => Ok(MetaCommand::ToggleExpanded(parse_toggle_arg(&cmd, &args)?)),
        "\\timing" => Ok(MetaCommand::ToggleTiming(parse_toggle_arg(&cmd, &args)?)),
        "\\conninfo" => {
            expect_no_args(&cmd, &args)?;
            Ok(MetaCommand::CatalogQuery {
                command: cmd,
                title: "Connection Info".to_owned(),
                sql: "SELECT pg_catalog.current_database() AS database, \
                      current_user AS \"user\", \
                      pg_catalog.inet_server_addr()::text AS server_addr, \
                      pg_catalog.inet_server_port() AS server_port, \
                      pg_catalog.version() AS server_version;"
                    .to_owned(),
            })
        }
        "\\d" => build_describe_query(false, expect_optional_arg(&cmd, args)?),
        "\\d+" => build_describe_query(true, expect_optional_arg(&cmd, args)?),
        "\\dt" => build_relation_query(
            &cmd,
            false,
            "Tables",
            "'r', 'p'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\dt+" => build_relation_query(
            &cmd,
            true,
            "Tables",
            "'r', 'p'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\dv" => build_relation_query(
            &cmd,
            false,
            "Views",
            "'v'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\dv+" => {
            build_relation_query(&cmd, true, "Views", "'v'", expect_optional_arg(&cmd, args)?)
        }
        "\\dm" => build_relation_query(
            &cmd,
            false,
            "Materialized Views",
            "'m'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\dm+" => build_relation_query(
            &cmd,
            true,
            "Materialized Views",
            "'m'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\ds" => build_relation_query(
            &cmd,
            false,
            "Sequences",
            "'S'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\ds+" => build_relation_query(
            &cmd,
            true,
            "Sequences",
            "'S'",
            expect_optional_arg(&cmd, args)?,
        ),
        "\\di" => build_index_query(&cmd, false, expect_optional_arg(&cmd, args)?),
        "\\di+" => build_index_query(&cmd, true, expect_optional_arg(&cmd, args)?),
        "\\dn" => build_schema_query(&cmd, false, expect_optional_arg(&cmd, args)?),
        "\\dn+" => build_schema_query(&cmd, true, expect_optional_arg(&cmd, args)?),
        "\\du" | "\\dg" => build_role_query(&cmd, false, expect_optional_arg(&cmd, args)?),
        "\\du+" | "\\dg+" => build_role_query(&cmd, true, expect_optional_arg(&cmd, args)?),
        "\\df" => build_function_query(&cmd, false, expect_optional_arg(&cmd, args)?),
        "\\df+" => build_function_query(&cmd, true, expect_optional_arg(&cmd, args)?),
        "\\dx" => build_extension_query(&cmd, expect_optional_arg(&cmd, args)?),
        "\\l" => build_database_query(&cmd, false, expect_optional_arg(&cmd, args)?),
        "\\l+" => build_database_query(&cmd, true, expect_optional_arg(&cmd, args)?),
        other => Err(unsupported_command_diagnostic(other)),
    }
}

fn unsupported_command_diagnostic(cmd: &str) -> Diagnostic {
    let lower = cmd.to_ascii_lowercase();
    match lower.as_str() {
        "\\i" | "\\ir" => Diagnostic::new(
            DiagnosticKind::Usage,
            format!("Unsupported psql meta-command '{cmd}'"),
            "parsing backslash meta-command",
        )
        .likely_cause("Ignatius does not include local script files via backslash commands.")
        .next_action(format!(
            "Use `{BINARY_NAME} query -f <FILE>` to execute a SQL script, or type \\? for supported meta-commands."
        )),
        "\\copy" => Diagnostic::new(
            DiagnosticKind::Usage,
            format!("Unsupported psql meta-command '{cmd}'"),
            "parsing backslash meta-command",
        )
        .likely_cause("Client-side \\copy is handled by dedicated streaming commands in Ignatius.")
        .next_action(format!(
            "Use `{BINARY_NAME} export` or `{BINARY_NAME} import`, or type \\? for supported meta-commands."
        )),
        "\\q" | "\\quit" => Diagnostic::new(
            DiagnosticKind::Usage,
            format!("Unsupported psql meta-command '{cmd}'"),
            "parsing backslash meta-command",
        )
        .likely_cause("Interactive exit is bound to the application keymap rather than a SQL buffer command.")
        .next_action("Press Ctrl+Q to exit Ignatius, or type \\? to list supported meta-commands."),
        _ => Diagnostic::new(
            DiagnosticKind::Usage,
            format!("Unsupported psql meta-command '{cmd}'"),
            "parsing backslash meta-command",
        )
        .likely_cause("This psql meta-command is not part of Ignatius's supported command set.")
        .next_action(format!(
            "Type \\? to list supported meta-commands, or use `{BINARY_NAME} query -f` and `{BINARY_NAME} import` from the CLI."
        )),
    }
}

fn split_command_head(trimmed: &str) -> Result<(String, &str), Diagnostic> {
    let after_slash = trimmed.strip_prefix('\\').unwrap_or(trimmed);
    if after_slash.is_empty() {
        return Err(usage_diagnostic(
            "Empty backslash meta-command",
            "parsing backslash meta-command",
            "Type \\? to list supported meta-commands.",
        ));
    }

    if let Some(rest) = after_slash.strip_prefix('!') {
        return Ok(("\\!".to_owned(), rest));
    }

    if let Some(rest) = after_slash.strip_prefix('?') {
        if rest.is_empty() || rest.starts_with([' ', '\t', ';']) {
            return Ok(("\\?".to_owned(), rest));
        }
        return Err(unsupported_command_diagnostic(trimmed));
    }

    let mut end = 0usize;
    for (idx, ch) in after_slash.char_indices() {
        if ch.is_ascii_alphabetic() {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return Err(usage_diagnostic(
            format!("Invalid meta-command '{trimmed}'"),
            "parsing backslash meta-command",
            "Type \\? to list supported meta-commands.",
        ));
    }
    if after_slash[end..].starts_with('+') {
        end += 1;
    }

    let rest = &after_slash[end..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t', ';']) {
        return Err(usage_diagnostic(
            format!(
                "Missing whitespace after meta-command '\\{}'",
                &after_slash[..end]
            ),
            "parsing backslash meta-command",
            "Separate the meta-command from its argument with whitespace, or type \\? for syntax.",
        ));
    }

    Ok((format!("\\{}", &after_slash[..end]), rest))
}

fn parse_args(mut rest: &str) -> Result<Vec<PatternArg>, Diagnostic> {
    let mut args = Vec::new();
    loop {
        rest = rest.trim_start_matches([' ', '\t']);
        if rest.is_empty() {
            break;
        }
        if let Some(after_semi) = rest.strip_prefix(';') {
            if after_semi.trim().is_empty() {
                break;
            }
            return Err(usage_diagnostic(
                "Unexpected trailing text after ';' in meta-command",
                "parsing meta-command arguments",
                "Remove trailing tokens after ';' or run a single meta-command at a time.",
            ));
        }

        let (arg, next_rest) = parse_one_arg(rest)?;
        args.push(arg);
        rest = next_rest;
    }
    Ok(args)
}

fn parse_one_arg(input: &str) -> Result<(PatternArg, &str), Diagnostic> {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut segments = Vec::new();
    let mut current = String::new();

    while i < bytes.len() {
        let ch = input[i..].chars().next().ok_or_else(|| {
            usage_diagnostic(
                "Unexpected end of meta-command argument",
                "parsing meta-command argument",
                "Check the meta-command syntax with \\?.",
            )
        })?;
        match ch {
            ' ' | '\t' => break,
            ';' if input[i + 1..].trim().is_empty() => break,
            '\0' => {
                return Err(usage_diagnostic(
                    "NUL byte is not permitted in a meta-command pattern",
                    "parsing meta-command pattern",
                    "Remove the NUL byte and retry.",
                ));
            }
            '"' => {
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    let qch = input[i..].chars().next().unwrap_or('\0');
                    if qch == '\0' {
                        return Err(usage_diagnostic(
                            "NUL byte is not permitted in a meta-command pattern",
                            "parsing meta-command pattern",
                            "Remove the NUL byte and retry.",
                        ));
                    }
                    if qch == '"' {
                        if input[i + 1..].starts_with('"') {
                            current.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        current.push(qch);
                        i += qch.len_utf8();
                    }
                }
                if !closed {
                    return Err(usage_diagnostic(
                        "Unclosed double quote in meta-command argument",
                        "parsing meta-command argument",
                        "Close the double-quoted identifier with `\"`.",
                    ));
                }
            }
            '.' => {
                segments.push(std::mem::take(&mut current));
                i += 1;
            }
            other => {
                current.push(other);
                i += other.len_utf8();
            }
        }
    }
    segments.push(current);

    Ok((PatternArg { segments }, &input[i..]))
}

fn expect_no_args(cmd: &str, args: &[PatternArg]) -> Result<(), Diagnostic> {
    if args.is_empty() {
        return Ok(());
    }
    Err(usage_diagnostic(
        format!("Unexpected argument for '{cmd}'"),
        "parsing meta-command arguments",
        format!("Run '{cmd}' without extra arguments, or type \\? for syntax."),
    ))
}

fn expect_optional_arg(
    cmd: &str,
    mut args: Vec<PatternArg>,
) -> Result<Option<PatternArg>, Diagnostic> {
    if args.len() > 1 {
        return Err(usage_diagnostic(
            format!("Too many arguments for '{cmd}'"),
            "parsing meta-command arguments",
            format!(
                "Pass at most one pattern argument to '{cmd}', or quote spaces with `\"...\"`."
            ),
        ));
    }
    Ok(args.pop())
}

fn parse_toggle_arg(cmd: &str, args: &[PatternArg]) -> Result<Option<bool>, Diagnostic> {
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() > 1 {
        return Err(usage_diagnostic(
            format!("Too many arguments for '{cmd}'"),
            "parsing toggle meta-command",
            format!("Use `{cmd}`, `{cmd} on`, or `{cmd} off`."),
        ));
    }
    if args[0].segments.len() != 1 {
        return Err(usage_diagnostic(
            format!("Invalid argument for '{cmd}'"),
            "parsing toggle meta-command",
            format!("Use `{cmd}`, `{cmd} on`, or `{cmd} off`."),
        ));
    }
    match args[0].segments[0].to_ascii_lowercase().as_str() {
        "on" | "true" | "1" => Ok(Some(true)),
        "off" | "false" | "0" => Ok(Some(false)),
        other => Err(usage_diagnostic(
            format!("Invalid state '{other}' for '{cmd}'"),
            "parsing toggle meta-command",
            format!("Use `{cmd}` to toggle, or pass `on` or `off`."),
        )),
    }
}

fn find_trailing_watch(input: &str) -> Option<(usize, &str)> {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut last_watch_candidate: Option<usize> = None;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = skip_single_quoted(input, i),
            b'"' => i = skip_double_quoted(input, i),
            b'$' => {
                let after = skip_dollar_quoted(input, i);
                i = if after == i { i + 1 } else { after };
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => i = skip_line_comment(input, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = skip_block_comment(input, i),
            b'\\' => {
                let at_boundary =
                    i == 0 || matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r' | b';');
                if at_boundary && is_watch_prefix(&input[i..]) {
                    last_watch_candidate = Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }

    let pos = last_watch_candidate?;
    let tail = &input[pos..];
    let (first_line, after_line) = match tail.split_once('\n') {
        Some((line, rest)) => (line.trim_end_matches('\r').trim_end(), rest.trim()),
        None => (tail.trim_end(), ""),
    };
    if !after_line.is_empty() {
        return None;
    }
    Some((pos, first_line))
}

fn is_watch_prefix(slice: &str) -> bool {
    let Some(rest) = slice.strip_prefix("\\watch") else {
        return false;
    };
    rest.is_empty() || rest.starts_with([' ', '\t', '\r', '\n', ';'])
}

fn parse_watch_line(
    watch_line: &str,
    companion_sql: Option<String>,
) -> Result<MetaCommand, Diagnostic> {
    let (_, rest) = split_command_head(watch_line)?;
    parse_watch_args(rest, companion_sql)
}

fn parse_watch_args(rest: &str, companion_sql: Option<String>) -> Result<MetaCommand, Diagnostic> {
    let args = parse_args(rest)?;
    if args.is_empty() {
        return Ok(MetaCommand::StartWatch {
            interval_secs: DEFAULT_WATCH_INTERVAL_SECS,
            companion_sql,
        });
    }
    if args.len() > 1 || args[0].segments.len() != 1 {
        return Err(usage_diagnostic(
            "Invalid arguments for '\\watch'",
            "parsing \\watch directive",
            format!("Use `\\watch [1..{MAX_WATCH_INTERVAL_SECS}]` or `\\watch stop`."),
        ));
    }

    let token = args[0].segments[0].as_str();
    let lower = token.to_ascii_lowercase();
    if matches!(lower.as_str(), "stop" | "off") {
        if companion_sql.is_some() {
            return Err(usage_diagnostic(
                "Cannot combine SQL with '\\watch stop'",
                "parsing \\watch directive",
                "Run `\\watch stop` on its own to stop an active watch loop.",
            ));
        }
        return Ok(MetaCommand::StopWatch);
    }

    if token.starts_with('-') {
        return Err(usage_diagnostic(
            format!("Watch interval '{token}' cannot be negative"),
            "parsing \\watch interval",
            format!("Choose an integer between 1 and {MAX_WATCH_INTERVAL_SECS} seconds."),
        ));
    }

    let Ok(secs) = token.parse::<u64>() else {
        return Err(usage_diagnostic(
            format!("Invalid watch interval '{token}'"),
            "parsing \\watch interval",
            format!(
                "Choose a whole number between 1 and {MAX_WATCH_INTERVAL_SECS} seconds, or `stop`."
            ),
        ));
    };

    if secs == 0 {
        return Err(usage_diagnostic(
            "Watch interval must be at least 1 second",
            "parsing \\watch interval",
            format!("Choose an integer between 1 and {MAX_WATCH_INTERVAL_SECS} seconds."),
        ));
    }
    if secs > MAX_WATCH_INTERVAL_SECS {
        return Err(usage_diagnostic(
            format!(
                "Watch interval {secs}s exceeds the maximum of {MAX_WATCH_INTERVAL_SECS} seconds"
            ),
            "parsing \\watch interval",
            format!("Choose an integer between 1 and {MAX_WATCH_INTERVAL_SECS} seconds."),
        ));
    }

    Ok(MetaCommand::StartWatch {
        interval_secs: secs,
        companion_sql,
    })
}

fn like_literal(glob: &str) -> String {
    let mut out = String::with_capacity(glob.len() + 16);
    out.push('\'');
    for ch in glob.chars() {
        match ch {
            '!' => out.push_str("!!"),
            '%' => out.push_str("!%"),
            '_' => out.push_str("!_"),
            '*' => out.push('%'),
            '?' => out.push('_'),
            '\'' => out.push_str("''"),
            other => out.push(other),
        }
    }
    out.push_str("' ESCAPE '!'");
    out
}

fn schema_and_name_filter(
    schema_col: &str,
    name_col: &str,
    schema_pat: Option<&str>,
    name_pat: Option<&str>,
) -> String {
    match (schema_pat, name_pat) {
        (Some(schema), Some(name)) => format!(
            "{schema_col} LIKE {} AND {name_col} LIKE {}",
            like_literal(schema),
            like_literal(name)
        ),
        (None, Some(name)) => format!(
            "{schema_col} NOT IN ('pg_catalog', 'information_schema') \
             AND {schema_col} !~ '^pg_toast' \
             AND {name_col} LIKE {}",
            like_literal(name)
        ),
        _ => format!(
            "{schema_col} NOT IN ('pg_catalog', 'information_schema') \
             AND {schema_col} !~ '^pg_toast'"
        ),
    }
}

fn build_describe_query(
    detailed: bool,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let command = if detailed { "\\d+" } else { "\\d" };
    let Some(arg) = pattern else {
        return build_relation_query(
            command,
            detailed,
            "Relations",
            "'r', 'p', 'v', 'm', 'S', 'f'",
            None,
        );
    };

    let (schema_pat, name_pat) = arg.into_object_pattern(command)?;
    let filter = schema_and_name_filter(
        "n.nspname",
        "c.relname",
        schema_pat.as_deref(),
        Some(&name_pat),
    );
    let extra_cols = if detailed {
        ", CASE a.attstorage \
             WHEN 'p' THEN 'plain' \
             WHEN 'e' THEN 'external' \
             WHEN 'x' THEN 'extended' \
             WHEN 'm' THEN 'main' \
             ELSE a.attstorage::text \
         END AS storage, \
         pg_catalog.pg_size_pretty(pg_catalog.pg_total_relation_size(c.oid)) AS table_size, \
         (SELECT pg_catalog.string_agg(ic.relname, ', ' ORDER BY ic.relname) \
          FROM pg_catalog.pg_index ix \
          JOIN pg_catalog.pg_class ic ON ic.oid = ix.indexrelid \
          WHERE ix.indrelid = c.oid) AS indexes"
    } else {
        ""
    };

    let sql = format!(
        "SELECT n.nspname AS schema, \
         c.relname AS relation, \
         a.attnum AS pos, \
         a.attname AS \"column\", \
         pg_catalog.format_type(a.atttypid, a.atttypmod) AS type, \
         NOT a.attnotnull AS nullable, \
         pg_catalog.pg_get_expr(d.adbin, d.adrelid) AS default_expr{extra_cols}, \
         pg_catalog.col_description(c.oid, a.attnum) AS description \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid \
         LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum \
         WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') \
         AND a.attnum > 0 \
         AND NOT a.attisdropped \
         AND {filter} \
         ORDER BY n.nspname, c.relname, a.attnum \
         LIMIT {CATALOG_ROW_LIMIT};"
    );

    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: format!("Describe {name_pat}"),
        sql,
    })
}

fn build_relation_query(
    command: &str,
    detailed: bool,
    title: &str,
    relkinds: &str,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let (schema_pat, name_pat) = match pattern {
        Some(arg) => {
            let (s, n) = arg.into_object_pattern(command)?;
            (s, Some(n))
        }
        None => (None, None),
    };
    let filter = schema_and_name_filter(
        "n.nspname",
        "c.relname",
        schema_pat.as_deref(),
        name_pat.as_deref(),
    );
    let extra = if detailed {
        ", pg_catalog.pg_size_pretty(pg_catalog.pg_total_relation_size(c.oid)) AS size, \
         pg_catalog.obj_description(c.oid, 'pg_class') AS description"
    } else {
        ""
    };
    let sql = format!(
        "SELECT n.nspname AS schema, \
         c.relname AS name, \
         CASE c.relkind \
             WHEN 'r' THEN 'table' \
             WHEN 'p' THEN 'partitioned table' \
             WHEN 'v' THEN 'view' \
             WHEN 'm' THEN 'materialized view' \
             WHEN 'S' THEN 'sequence' \
             WHEN 'f' THEN 'foreign table' \
             ELSE c.relkind::text \
         END AS type, \
         pg_catalog.pg_get_userbyid(c.relowner) AS owner{extra} \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relkind IN ({relkinds}) \
         AND {filter} \
         ORDER BY n.nspname, c.relname \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: title.to_owned(),
        sql,
    })
}

fn build_index_query(
    command: &str,
    detailed: bool,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let (schema_pat, name_pat) = match pattern {
        Some(arg) => {
            let (s, n) = arg.into_object_pattern(command)?;
            (s, Some(n))
        }
        None => (None, None),
    };
    let filter = schema_and_name_filter(
        "n.nspname",
        "c.relname",
        schema_pat.as_deref(),
        name_pat.as_deref(),
    );
    let extra = if detailed {
        ", pg_catalog.pg_size_pretty(pg_catalog.pg_relation_size(c.oid)) AS size, \
         pg_catalog.obj_description(c.oid, 'pg_class') AS description"
    } else {
        ""
    };
    let sql = format!(
        "SELECT n.nspname AS schema, \
         c.relname AS name, \
         CASE c.relkind \
             WHEN 'i' THEN 'index' \
             WHEN 'I' THEN 'partitioned index' \
             ELSE c.relkind::text \
         END AS type, \
         pg_catalog.pg_get_userbyid(c.relowner) AS owner, \
         tc.relname AS table_name{extra} \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         LEFT JOIN pg_catalog.pg_index ix ON ix.indexrelid = c.oid \
         LEFT JOIN pg_catalog.pg_class tc ON tc.oid = ix.indrelid \
         WHERE c.relkind IN ('i', 'I') \
         AND {filter} \
         ORDER BY n.nspname, c.relname \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: "Indexes".to_owned(),
        sql,
    })
}

fn build_schema_query(
    command: &str,
    detailed: bool,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let filter = match pattern {
        Some(arg) => {
            let pat = arg.into_unqualified_pattern(command)?;
            format!("n.nspname LIKE {}", like_literal(&pat))
        }
        None => "n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'".to_owned(),
    };
    let extra = if detailed {
        ", n.nspacl::text AS access_privileges, \
         pg_catalog.obj_description(n.oid, 'pg_namespace') AS description"
    } else {
        ""
    };
    let sql = format!(
        "SELECT n.nspname AS name, \
         pg_catalog.pg_get_userbyid(n.nspowner) AS owner{extra} \
         FROM pg_catalog.pg_namespace n \
         WHERE {filter} \
         ORDER BY n.nspname \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: "Schemas".to_owned(),
        sql,
    })
}

fn build_role_query(
    command: &str,
    detailed: bool,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let where_clause = match pattern {
        Some(arg) => {
            let pat = arg.into_unqualified_pattern(command)?;
            format!("WHERE r.rolname LIKE {} ", like_literal(&pat))
        }
        None => String::new(),
    };
    let extra = if detailed {
        ", pg_catalog.shobj_description(r.oid, 'pg_authid') AS description"
    } else {
        ""
    };
    let sql = format!(
        "SELECT r.rolname AS role_name, \
         pg_catalog.concat_ws(', ', \
             CASE WHEN r.rolsuper THEN 'Superuser' END, \
             CASE WHEN r.rolcreaterole THEN 'Create role' END, \
             CASE WHEN r.rolcreatedb THEN 'Create DB' END, \
             CASE WHEN NOT r.rolcanlogin THEN 'Cannot login' END, \
             CASE WHEN r.rolreplication THEN 'Replication' END, \
             CASE WHEN r.rolbypassrls THEN 'Bypass RLS' END \
         ) AS attributes, \
         (SELECT pg_catalog.string_agg(g.rolname, ', ' ORDER BY g.rolname) \
          FROM pg_catalog.pg_auth_members m \
          JOIN pg_catalog.pg_roles g ON g.oid = m.roleid \
          WHERE m.member = r.oid) AS member_of{extra} \
         FROM pg_catalog.pg_roles r \
         {where_clause}\
         ORDER BY r.rolname \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: "Roles".to_owned(),
        sql,
    })
}

fn build_function_query(
    command: &str,
    detailed: bool,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let (schema_pat, name_pat) = match pattern {
        Some(arg) => {
            let (s, n) = arg.into_object_pattern(command)?;
            (s, Some(n))
        }
        None => (None, None),
    };
    let filter = schema_and_name_filter(
        "n.nspname",
        "p.proname",
        schema_pat.as_deref(),
        name_pat.as_deref(),
    );
    let extra_cols = if detailed {
        ", l.lanname AS language, \
         pg_catalog.obj_description(p.oid, 'pg_proc') AS description"
    } else {
        ""
    };
    let extra_join = if detailed {
        "JOIN pg_catalog.pg_language l ON l.oid = p.prolang "
    } else {
        ""
    };
    let sql = format!(
        "SELECT n.nspname AS schema, \
         p.proname AS name, \
         CASE WHEN p.prokind = 'p' THEN '' ELSE pg_catalog.pg_get_function_result(p.oid) END AS result_data_type, \
         pg_catalog.pg_get_function_arguments(p.oid) AS argument_data_types, \
         CASE p.prokind \
             WHEN 'f' THEN 'func' \
             WHEN 'p' THEN 'proc' \
             WHEN 'a' THEN 'agg' \
             WHEN 'w' THEN 'window' \
             ELSE p.prokind::text \
         END AS type{extra_cols} \
         FROM pg_catalog.pg_proc p \
         JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
         {extra_join}\
         WHERE {filter} \
         ORDER BY n.nspname, p.proname, argument_data_types \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: "Functions".to_owned(),
        sql,
    })
}

fn build_extension_query(
    command: &str,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let where_clause = match pattern {
        Some(arg) => {
            let pat = arg.into_unqualified_pattern(command)?;
            format!("WHERE e.extname LIKE {} ", like_literal(&pat))
        }
        None => String::new(),
    };
    let sql = format!(
        "SELECT e.extname AS name, \
         e.extversion AS version, \
         n.nspname AS schema, \
         pg_catalog.obj_description(e.oid, 'pg_extension') AS description \
         FROM pg_catalog.pg_extension e \
         JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace \
         {where_clause}\
         ORDER BY e.extname \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: "Extensions".to_owned(),
        sql,
    })
}

fn build_database_query(
    command: &str,
    detailed: bool,
    pattern: Option<PatternArg>,
) -> Result<MetaCommand, Diagnostic> {
    let where_clause = match pattern {
        Some(arg) => {
            let pat = arg.into_unqualified_pattern(command)?;
            format!("WHERE d.datname LIKE {} ", like_literal(&pat))
        }
        None => String::new(),
    };
    let (extra_cols, extra_join) = if detailed {
        (
            ", pg_catalog.pg_size_pretty(pg_catalog.pg_database_size(d.datname)) AS size, \
             ts.spcname AS tablespace, \
             pg_catalog.shobj_description(d.oid, 'pg_database') AS description",
            "JOIN pg_catalog.pg_tablespace ts ON ts.oid = d.dattablespace ",
        )
    } else {
        ("", "")
    };
    let sql = format!(
        "SELECT d.datname AS name, \
         pg_catalog.pg_get_userbyid(d.datdba) AS owner, \
         pg_catalog.pg_encoding_to_char(d.encoding) AS encoding, \
         d.datcollate AS collate, \
         d.datctype AS ctype, \
         d.datacl::text AS access_privileges{extra_cols} \
         FROM pg_catalog.pg_database d \
         {extra_join}\
         {where_clause}\
         ORDER BY d.datname \
         LIMIT {CATALOG_ROW_LIMIT};"
    );
    Ok(MetaCommand::CatalogQuery {
        command: command.to_owned(),
        title: "Databases".to_owned(),
        sql,
    })
}

fn usage_diagnostic(
    headline: impl Into<String>,
    attempted: impl Into<String>,
    next_action: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(DiagnosticKind::Usage, headline, attempted).next_action(next_action)
}

#[cfg(test)]
mod tests;
