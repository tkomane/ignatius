//! Domain engine for PL/pgSQL routine execution, testing harness, and profiling (`032-plpgsql-routine-runner-and-profiler`).
//!
//! Provides pure models for PostgreSQL functions and procedures, argument parameter
//! binding, transactional savepoint harness SQL synthesis, notice parsing, and
//! diagnostic profiling reports.

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The semantic kind of PostgreSQL routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineKind {
    /// Standard function (`FUNCTION`) returning a value or set of rows.
    Function,
    /// Stored procedure (`PROCEDURE`) executed via `CALL` without return value.
    Procedure,
    /// Aggregate function (`AGGREGATE`).
    Aggregate,
    /// Window function (`WINDOW`).
    Window,
}

impl RoutineKind {
    /// Formats the routine kind as a human-readable title.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Function => "Function",
            Self::Procedure => "Procedure",
            Self::Aggregate => "Aggregate",
            Self::Window => "Window",
        }
    }

    /// Formats the routine kind as an uppercase SQL keyword.
    #[must_use]
    pub const fn sql_keyword(self) -> &'static str {
        match self {
            Self::Function => "FUNCTION",
            Self::Procedure => "PROCEDURE",
            Self::Aggregate => "AGGREGATE",
            Self::Window => "WINDOW",
        }
    }
}

/// The mode of an argument parameter in a routine signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamMode {
    /// Input parameter (`IN`).
    In,
    /// Output parameter (`OUT`).
    Out,
    /// Bidirectional input/output parameter (`INOUT`).
    InOut,
    /// Variadic array argument (`VARIADIC`).
    Variadic,
    /// Table column return parameter (`TABLE`).
    Table,
}

impl ParamMode {
    /// Parses a PostgreSQL catalog `proargmodes` character into a `ParamMode`.
    #[must_use]
    pub const fn from_catalog_char(c: char) -> Self {
        match c {
            'o' => Self::Out,
            'b' => Self::InOut,
            'v' => Self::Variadic,
            't' => Self::Table,
            _ => Self::In,
        }
    }

    /// Returns the uppercase SQL representation of the parameter mode.
    #[must_use]
    pub const fn sql_keyword(self) -> &'static str {
        match self {
            Self::In => "IN",
            Self::Out => "OUT",
            Self::InOut => "INOUT",
            Self::Variadic => "VARIADIC",
            Self::Table => "TABLE",
        }
    }
}

/// A formal parameter declared in a routine signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineParam {
    /// Name of the parameter, or auto-generated position name `arg<N>`.
    pub name: String,
    /// PostgreSQL data type name (e.g. `integer`, `text`, `timestamp with time zone`).
    pub data_type: String,
    /// Directional mode of the parameter (`IN`, `OUT`, `INOUT`, etc.).
    pub mode: ParamMode,
    /// Optional default value expression.
    pub default_value: Option<String>,
    /// Indicates whether the parameter has a default value.
    pub has_default: bool,
}

/// Complete metadata and signature of a PostgreSQL routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineSignature {
    /// Database schema containing the routine.
    pub schema: String,
    /// Routine identifier.
    pub name: String,
    /// Specific routine kind (function, procedure, aggregate, window).
    pub kind: RoutineKind,
    /// Implementation language (e.g. `plpgsql`, `sql`, `c`, `internal`).
    pub language: String,
    /// Declared return data type name (e.g. `void`, `record`, `table`, `trigger`).
    pub return_type: String,
    /// Indicates whether the routine returns multiple rows (`SETOF`).
    pub is_set_returning: bool,
    /// List of formal parameters.
    pub params: Vec<RoutineParam>,
    /// Source code definition or function body.
    pub source: String,
    /// Volatility category: `immutable`, `stable`, or `volatile`.
    pub volatility: String,
    /// Indicates whether the routine executes with definer privileges (`SECURITY DEFINER`).
    pub security_definer: bool,
}

impl RoutineSignature {
    /// Filters parameters that require an input value when invoked.
    #[must_use]
    pub fn input_params(&self) -> Vec<&RoutineParam> {
        self.params
            .iter()
            .filter(|p| {
                matches!(
                    p.mode,
                    ParamMode::In | ParamMode::InOut | ParamMode::Variadic
                )
            })
            .collect()
    }

    /// Formats a clean routine signature heading, e.g. `public.calc_tax(amount numeric, rate numeric) -> numeric`.
    #[must_use]
    pub fn display_signature(&self) -> String {
        let args = self
            .params
            .iter()
            .map(|p| {
                let mode_prefix = match p.mode {
                    ParamMode::In => String::new(),
                    _ => format!("{} ", p.mode.sql_keyword()),
                };
                let default_suffix = p
                    .default_value
                    .as_deref()
                    .map_or(String::new(), |d| format!(" DEFAULT {d}"));
                format!("{mode_prefix}{} {}{default_suffix}", p.name, p.data_type)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let ret = if self.is_set_returning {
            format!("SETOF {}", self.return_type)
        } else {
            self.return_type.clone()
        };

        format!("{}.{}({}) -> {}", self.schema, self.name, args, ret)
    }
}

/// Options configuring the execution of a routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineExecutionOptions {
    /// Schema of the routine to execute.
    pub schema: String,
    /// Name of the routine to execute.
    pub name: String,
    /// Arguments provided by parameter name or ordinal index (`$1`, `$2` or parameter name).
    pub args: BTreeMap<String, String>,
    /// Whether to run within a temporary `SAVEPOINT` and automatically roll back.
    pub rollback_savepoint: bool,
    /// Optional execution timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// A captured asynchronous `RAISE NOTICE`, `RAISE WARNING`, or `RAISE INFO` diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineNotice {
    /// Notice severity level (e.g. `NOTICE`, `WARNING`, `INFO`, `DEBUG`).
    pub severity: String,
    /// Primary notice message.
    pub message: String,
    /// Optional secondary detail message.
    pub detail: Option<String>,
    /// Optional recommendation or hint.
    pub hint: Option<String>,
    /// Call-stack or error location context.
    pub context: Option<String>,
}

/// Execution outcome of a routine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineExecutionResult {
    /// Full routine name (`schema.name`).
    pub routine_name: String,
    /// Whether execution completed without errors.
    pub success: bool,
    /// Column headers for returned records.
    pub columns: Vec<String>,
    /// Tabular rows returned by the routine.
    pub rows: Vec<Vec<String>>,
    /// Captured asynchronous `RAISE` notices and diagnostics.
    pub notices: Vec<RoutineNotice>,
    /// Wall-clock execution elapsed duration in milliseconds.
    pub elapsed_ms: f64,
    /// Indicates whether execution was isolated and rolled back via a savepoint.
    pub rolled_back: bool,
    /// Executed SQL statement.
    pub executed_sql: String,
}

/// Severity classification of a routine profiling finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileFindingSeverity {
    /// Critical logic bug or unhandled error path.
    Error,
    /// Warning such as dead code or unindexed table scan.
    Warning,
    /// Optimization or readability suggestion.
    Info,
    /// Code style or conventions recommendation.
    Style,
}

impl ProfileFindingSeverity {
    /// Renders an uppercase label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warning => "WARNING",
            Self::Info => "INFO",
            Self::Style => "STYLE",
        }
    }
}

/// A diagnostic finding reported during static profiling of a PL/pgSQL routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileFinding {
    /// Diagnostic classification code (e.g. `unused_variable`, `type_mismatch`, `dead_code`).
    pub code: String,
    /// Finding severity.
    pub severity: ProfileFindingSeverity,
    /// Diagnostic description.
    pub message: String,
    /// Source line number within the function body if available.
    pub line: Option<usize>,
    /// Suspect SQL statement or expression fragment if identified.
    pub statement: Option<String>,
}

/// Full diagnostic performance and static analysis profile of a routine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineProfileReport {
    /// Routine signature analyzed.
    pub routine: RoutineSignature,
    /// Diagnostic findings detected by plpgsql_check or static inspection.
    pub findings: Vec<ProfileFinding>,
    /// Sample invocation SQL used for profiling.
    pub invocation_sql: String,
    /// Notices captured during execution.
    pub notices: Vec<RoutineNotice>,
    /// Execution timing in milliseconds.
    pub elapsed_ms: f64,
    /// Whether execution was rolled back.
    pub rolled_back: bool,
    /// Whether the server has `plpgsql_check` extension active.
    pub has_plpgsql_check: bool,
}

/// Parses a raw PostgreSQL argument signature string (from `pg_get_function_arguments`) into structured `RoutineParam` list.
#[must_use]
pub fn parse_routine_arguments(args_str: &str) -> Vec<RoutineParam> {
    let mut params = Vec::new();
    if args_str.trim().is_empty() {
        return params;
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_quotes = false;

    for c in args_str.chars() {
        match c {
            '\'' => {
                in_quotes = !in_quotes;
                current.push(c);
            }
            '(' if !in_quotes => {
                depth = depth.saturating_add(1);
                current.push(c);
            }
            ')' if !in_quotes => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' if depth == 0 && !in_quotes => {
                chunks.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    for (idx, chunk) in chunks.iter().enumerate() {
        let (decl, default_value, has_default) =
            if let Some(pos) = chunk.to_uppercase().find(" DEFAULT ") {
                let (d, rest) = chunk.split_at(pos);
                let def_val = rest[" DEFAULT ".len()..].trim().to_string();
                (d.trim(), Some(def_val), true)
            } else {
                (chunk.as_str(), None, false)
            };

        let mut words: Vec<&str> = decl.split_whitespace().collect();
        let mode = if let Some(&first) = words.first() {
            match first.to_uppercase().as_str() {
                "IN" => {
                    words.remove(0);
                    ParamMode::In
                }
                "OUT" => {
                    words.remove(0);
                    ParamMode::Out
                }
                "INOUT" => {
                    words.remove(0);
                    ParamMode::InOut
                }
                "VARIADIC" => {
                    words.remove(0);
                    ParamMode::Variadic
                }
                "TABLE" => {
                    words.remove(0);
                    ParamMode::Table
                }
                _ => ParamMode::In,
            }
        } else {
            ParamMode::In
        };

        let (name, data_type) = if words.len() >= 2 {
            (words[0].to_string(), words[1..].join(" "))
        } else if words.len() == 1 {
            (format!("arg{}", idx + 1), words[0].to_string())
        } else {
            (format!("arg{}", idx + 1), "text".to_string())
        };

        params.push(RoutineParam {
            name,
            data_type,
            mode,
            default_value,
            has_default,
        });
    }

    params
}

/// Builds the SQL statement required to execute a routine with given argument values.
#[must_use]
pub fn build_routine_call_sql(
    routine: &RoutineSignature,
    args: &BTreeMap<String, String>,
) -> String {
    let input_params = routine.input_params();

    let mut formatted_args = Vec::new();
    for (idx, param) in input_params.iter().enumerate() {
        let val = args
            .get(&param.name)
            .or_else(|| args.get(&format!("${}", idx + 1)))
            .or_else(|| args.get(&idx.to_string()));

        match val {
            Some(v) => {
                let trimmed = v.trim();
                if trimmed.eq_ignore_ascii_case("null") {
                    formatted_args.push(format!("NULL::{}", param.data_type));
                } else if trimmed.starts_with('\'')
                    || trimmed.parse::<i64>().is_ok()
                    || trimmed.parse::<f64>().is_ok()
                    || trimmed.eq_ignore_ascii_case("true")
                    || trimmed.eq_ignore_ascii_case("false")
                {
                    formatted_args.push(format!("{trimmed}::{}", param.data_type));
                } else {
                    let escaped = trimmed.replace('\'', "''");
                    formatted_args.push(format!("'{escaped}'::{}", param.data_type));
                }
            }
            None => {
                if let Some(ref def) = param.default_value {
                    formatted_args.push(format!("{def}::{}", param.data_type));
                } else {
                    formatted_args.push(format!("NULL::{}", param.data_type));
                }
            }
        }
    }

    let joined_args = formatted_args.join(", ");
    let qualified_name = format!("\"{}\".\"{}\"", routine.schema, routine.name);

    match routine.kind {
        RoutineKind::Procedure => format!("CALL {qualified_name}({joined_args});"),
        RoutineKind::Function | RoutineKind::Aggregate | RoutineKind::Window => {
            if routine.is_set_returning || routine.return_type.eq_ignore_ascii_case("record") {
                format!("SELECT * FROM {qualified_name}({joined_args});")
            } else {
                format!("SELECT {qualified_name}({joined_args});")
            }
        }
    }
}

/// Builds a test harness execution statement wrapping routine invocation in a savepoint rollback.
#[must_use]
pub fn build_test_harness_sql(
    routine: &RoutineSignature,
    args: &BTreeMap<String, String>,
    rollback: bool,
) -> String {
    let call_sql = build_routine_call_sql(routine, args);
    if !rollback {
        return call_sql;
    }

    format!(
        "BEGIN;\n\
         SAVEPOINT ignatius_routine_test;\n\
         {call_sql}\n\
         ROLLBACK TO SAVEPOINT ignatius_routine_test;\n\
         ROLLBACK;"
    )
}

/// Performs static analysis on routine source code to detect common PL/pgSQL pitfalls
/// when `plpgsql_check` is not available on the PostgreSQL instance.
#[must_use]
pub fn static_inspect_routine_source(routine: &RoutineSignature) -> Vec<ProfileFinding> {
    let mut findings = Vec::new();

    let lines: Vec<&str> = routine.source.lines().collect();
    for (line_idx, line) in lines.iter().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        // 1. Detect dynamic SQL without USING or quote_ident/quote_literal
        if trimmed.to_uppercase().contains("EXECUTE ")
            && !trimmed.to_uppercase().contains("USING")
            && (trimmed.contains("||") || trimmed.contains("format("))
        {
            findings.push(ProfileFinding {
                code: "dynamic_sql_injection_risk".to_string(),
                severity: ProfileFindingSeverity::Warning,
                message: "Dynamic EXECUTE query uses string concatenation without parameterized USING clauses"
                    .to_string(),
                line: Some(line_num),
                statement: Some(trimmed.to_string()),
            });
        }

        // 2. Detect missing STRICT in SELECT INTO
        if trimmed.to_uppercase().starts_with("SELECT ")
            && trimmed.to_uppercase().contains(" INTO ")
            && !trimmed.to_uppercase().contains(" STRICT ")
        {
            findings.push(ProfileFinding {
                code: "select_into_missing_strict".to_string(),
                severity: ProfileFindingSeverity::Info,
                message: "SELECT INTO without STRICT may silently ignore multiple matching rows"
                    .to_string(),
                line: Some(line_num),
                statement: Some(trimmed.to_string()),
            });
        }

        // 3. Detect unindexed sequential full-table DELETE or UPDATE without WHERE
        let upper = trimmed.to_uppercase();
        if (upper.starts_with("DELETE FROM") || upper.starts_with("UPDATE "))
            && !upper.contains("WHERE ")
            && !upper.contains("RETURNING ")
        {
            findings.push(ProfileFinding {
                code: "unbounded_mutation".to_string(),
                severity: ProfileFindingSeverity::Warning,
                message:
                    "Mutation statement does not declare a WHERE clause and affects all table rows"
                        .to_string(),
                line: Some(line_num),
                statement: Some(trimmed.to_string()),
            });
        }
    }

    // 4. Check for declared parameters not referenced in the source
    for param in &routine.params {
        if !param.name.is_empty()
            && !routine.source.contains(&param.name)
            && matches!(param.mode, ParamMode::In | ParamMode::InOut)
        {
            findings.push(ProfileFinding {
                code: "unused_parameter".to_string(),
                severity: ProfileFindingSeverity::Style,
                message: format!(
                    "Declared parameter '{}' does not appear in routine body",
                    param.name
                ),
                line: None,
                statement: Some(format!("{} {}", param.name, param.data_type)),
            });
        }
    }

    findings
}

/// Renders a tabular ASCII representation of routine execution results.
#[must_use]
pub fn render_routine_result_table(result: &RoutineExecutionResult) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "=== Routine Execution: {} ===\n",
        result.routine_name
    ));
    out.push_str(&format!(
        "Status: {} | Duration: {:.2}ms | Rollback: {}\n\n",
        if result.success { "SUCCESS" } else { "FAILED" },
        result.elapsed_ms,
        if result.rolled_back {
            "YES (safe savepoint)"
        } else {
            "NO"
        }
    ));

    if !result.notices.is_empty() {
        out.push_str("--- Captured Notices ---\n");
        for notice in &result.notices {
            out.push_str(&format!("[{}] {}\n", notice.severity, notice.message));
            if let Some(ref detail) = notice.detail {
                out.push_str(&format!("  Detail: {detail}\n"));
            }
            if let Some(ref hint) = notice.hint {
                out.push_str(&format!("  Hint: {hint}\n"));
            }
        }
        out.push('\n');
    }

    if !result.columns.is_empty() {
        out.push_str("--- Return Records ---\n");
        let header = result.columns.join(" | ");
        out.push_str(&header);
        out.push('\n');
        out.push_str(&"-".repeat(header.len().max(20)));
        out.push('\n');

        for row in &result.rows {
            out.push_str(&row.join(" | "));
            out.push('\n');
        }
        out.push_str(&format!("({} rows)\n", result.rows.len()));
    } else {
        out.push_str("No tabular data returned.\n");
    }

    out
}

/// Renders a structured JSON representation of routine execution results.
#[must_use]
pub fn render_routine_result_json(result: &RoutineExecutionResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
}

/// Renders a tabular ASCII representation of a routine profile report.
#[must_use]
pub fn render_routine_profile_table(report: &RoutineProfileReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "=== PL/pgSQL Routine Profile: {} ===\n",
        report.routine.display_signature()
    ));
    out.push_str(&format!(
        "Language: {} | Volatility: {} | Security Definer: {}\n",
        report.routine.language, report.routine.volatility, report.routine.security_definer
    ));
    out.push_str(&format!(
        "Profiler: {} | Execution Elapsed: {:.2}ms | Rollback: {}\n\n",
        if report.has_plpgsql_check {
            "plpgsql_check"
        } else {
            "builtin static analyzer"
        },
        report.elapsed_ms,
        if report.rolled_back { "YES" } else { "NO" }
    ));

    out.push_str(&format!("Invocation SQL:\n  {}\n\n", report.invocation_sql));

    if report.findings.is_empty() {
        out.push_str("Diagnostics: No performance or logic warnings detected.\n");
    } else {
        out.push_str(&format!(
            "--- Findings ({} issues) ---\n",
            report.findings.len()
        ));
        for finding in &report.findings {
            let line_info = finding
                .line
                .map_or(String::new(), |l| format!(" [line {l}]"));
            out.push_str(&format!(
                "[{}] {}{}: {}\n",
                finding.severity.label(),
                finding.code,
                line_info,
                finding.message
            ));
            if let Some(ref stmt) = finding.statement {
                out.push_str(&format!("  Statement: {stmt}\n"));
            }
        }
    }

    if !report.notices.is_empty() {
        out.push_str("\n--- Captured Notices ---\n");
        for notice in &report.notices {
            out.push_str(&format!("[{}] {}\n", notice.severity, notice.message));
        }
    }

    out
}

/// Renders a structured JSON representation of a routine profile report.
#[must_use]
pub fn render_routine_profile_json(report: &RoutineProfileReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}
