//! The plain, line-oriented client.
//!
//! A full-screen interface is difficult for a screen reader, useless in
//! `TERM=dumb`, and gone from the scrollback the moment it exits. This is the
//! same product without any of that: one line in, one answer out, no alternate
//! screen, no raw mode, no cursor addressing, nothing that only makes sense to
//! an eye.
//!
//! It is not a degraded mode. Everything the full-screen client can say, this
//! says in words, because that was the rule the interface was built to anyway.
//!
//! Meta-commands use the backslash prefix people already know from `psql`. Only
//! a few exist, and they are listed by `\?` rather than implied by familiarity.

use crate::app::model::PendingRun;
use crate::query::result::{Execution, TransactionState};
use crate::query::statements;
use secrecy::SecretString;
use std::fmt::Write as _;

/// A command that is not SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Leave.
    Quit,
    /// List what is available.
    Help,
    /// Show the connection.
    Connection,
    /// Format the pending SQL buffer locally.
    Format,
    /// Print local schema-aware candidates for the current buffer.
    Complete {
        /// Optional prefix supplied by the person rather than the cursor.
        prefix: Option<String>,
    },
    /// Accept one candidate from the most recent completion list.
    Use(String),
    /// Something that is not a command.
    Unknown(String),
}

impl Command {
    /// Parses a backslash command.
    #[must_use]
    pub fn parse(line: &str) -> Self {
        let input = line.trim().trim_start_matches('\\').trim();
        let (name, argument) = input
            .find(char::is_whitespace)
            .map_or((input, ""), |index| {
                (&input[..index], input[index..].trim())
            });
        match name {
            "q" | "quit" | "exit" => Self::Quit,
            "?" | "h" | "help" => Self::Help,
            "conninfo" | "c" => Self::Connection,
            "format" | "fmt" => Self::Format,
            "complete" | "completion" => Self::Complete {
                prefix: (!argument.is_empty()).then(|| argument.to_owned()),
            },
            "use" => Self::Use(argument.to_owned()),
            _ => Self::Unknown(input.to_owned()),
        }
    }
}

/// What a line of input turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Nothing yet: the statement is not finished.
    Incomplete,
    /// A complete statement, ready to run.
    Statement(String),
    /// A meta-command.
    Command(Command),
    /// Nothing at all.
    Blank,
}

/// Accumulates typed lines into statements.
///
/// A statement is complete when it ends with a semicolon that the lexer agrees
/// is a boundary, so a semicolon inside a string or a dollar-quoted body keeps
/// the input going rather than sending half a function.
#[derive(Debug, Default)]
pub struct Reader {
    buffer: String,
}

impl Reader {
    /// Whether a statement is part-typed, which the prompt reflects.
    #[must_use]
    pub fn is_continuing(&self) -> bool {
        !self.buffer.trim().is_empty()
    }

    /// Discards what has been typed so far.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// The SQL that is currently being accumulated.
    #[must_use]
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// The cursor at the end of the last line, before the newline that
    /// `feed` stores as the line separator.
    #[must_use]
    pub fn completion_cursor(&self) -> usize {
        self.buffer
            .strip_suffix('\n')
            .map_or(self.buffer.len(), str::len)
    }

    /// Applies one explicitly chosen completion to the accumulated SQL.
    pub fn apply_completion(&mut self, range: std::ops::Range<usize>, replacement: &str) -> bool {
        crate::query::completion::replace_range(&mut self.buffer, range, replacement)
    }

    /// Replaces the pending SQL after a successful local transformation.
    pub fn replace_buffer(&mut self, text: String) {
        self.buffer = text;
    }

    /// Adds one line of input.
    pub fn feed(&mut self, line: &str) -> Input {
        // A meta-command is only a meta-command at the start of a statement.
        // Otherwise a backslash inside SQL would be intercepted.
        if line.trim_start().starts_with('\\') {
            let command = Command::parse(line);
            // Completion is the one pair of meta-commands allowed while SQL is
            // being accumulated. They are whole-line, explicit requests and
            // leave the SQL buffer in place; existing commands such as \q keep
            // their safer start-of-statement-only behavior.
            let context = crate::query::completion::context(&self.buffer, self.completion_cursor());
            let format_comment_only = matches!(command, Command::Format)
                && crate::query::format::is_comment_only(&self.buffer);
            let out_of_band = matches!(
                command,
                Command::Complete { .. } | Command::Use(_) | Command::Format
            ) && (!context.in_literal_or_comment || format_comment_only);
            if !self.is_continuing() || out_of_band {
                return Input::Command(command);
            }
        }
        if !self.is_continuing() && line.trim().is_empty() {
            return Input::Blank;
        }

        self.buffer.push_str(line);
        self.buffer.push('\n');

        // Complete when the lexer agrees the last statement was terminated.
        let statements = statements::split(&self.buffer);
        let complete = statements.last().is_some_and(|last| last.terminated);
        if complete && !statements.is_empty() {
            let sql = std::mem::take(&mut self.buffer);
            return Input::Statement(sql);
        }
        Input::Incomplete
    }
}

/// The prompt shown before each line.
///
/// It carries the facts a person needs before pressing enter: the database, the
/// classification the user gave it, and whether a transaction is open or has
/// failed. All words, because this mode exists for people who are not looking.
#[must_use]
pub fn prompt(
    database: &str,
    environment: &crate::connection::Environment,
    transaction: TransactionState,
    continuing: bool,
) -> String {
    if continuing {
        return "... > ".to_owned();
    }
    let mut prompt = database.to_owned();
    if environment.is_production() {
        // Never abbreviated, never a colour, and never left off.
        let _ = write!(prompt, " [{}]", environment.label());
    }
    match transaction {
        TransactionState::Open => prompt.push_str(" [in transaction]"),
        TransactionState::Failed => prompt.push_str(" [transaction failed]"),
        TransactionState::Unknown => prompt.push_str(" [transaction state unknown]"),
        TransactionState::Autocommit => {}
    }
    prompt.push_str(" => ");
    prompt
}

/// The line printed after a statement runs.
#[must_use]
pub fn outcome_line(execution: &Execution) -> String {
    let mut parts = vec![execution.status.label().to_owned()];
    for statement in &execution.statements {
        parts.push(statement.summary());
    }
    if let Some(recovery) = execution.transaction.recovery() {
        parts.push(recovery.to_owned());
    }
    parts.join(". ")
}

/// The confirmation question asked before a write to a production target.
#[must_use]
pub fn confirmation_question(pending: &PendingRun) -> String {
    if pending.impact.needs_typed_confirmation() {
        format!(
            "This statement {}, and this database is classified as production.\n\
             Type the database name ({}) to run it, or press enter to cancel: ",
            pending.impact.label(),
            pending.required
        )
    } else {
        format!(
            "This statement {}, and this database is classified as production.\n\
             Type yes to run it, or press enter to cancel: ",
            pending.impact.label()
        )
    }
}

/// Whether an answer satisfies the confirmation.
#[must_use]
pub fn confirmation_accepted(pending: &PendingRun, answer: &str) -> bool {
    let answer = answer.trim();
    if pending.impact.needs_typed_confirmation() {
        answer == pending.required
    } else {
        answer.eq_ignore_ascii_case("yes")
    }
}

/// The text `\?` prints.
#[must_use]
pub fn help_text() -> String {
    format!(
        "Type SQL, ending with a semicolon. It runs when the statement is complete.\n\
         \n\
         \\q      leave\n\
         \\?      this help\n\
         \\c      show the connection and what it is protected by\n\
         \\format  format the pending SQL buffer locally\n\
         \\complete [prefix]  show schema-aware candidates for the SQL buffer\n\
         \\use <number|name>  insert one candidate from that list\n\
         Named values such as :customer_id are prompted without echo. For\n\
         scripts, pass --param-env NAME=VARIABLE; values stay out of arguments\n\
         and output remains data-only.\n\
         \n\
         Ctrl+C cancels a running statement. Ctrl+D leaves.\n\
         This is {}'s plain mode: no full-screen interface, nothing that only\n\
         makes sense to an eye. Run without --plain for the full-screen client.",
        crate::branding::PRODUCT_NAME
    )
}

/// Runs the plain client until the user leaves.
///
/// Everything goes to the given streams: results to the data stream, prompts
/// and diagnostics to the message stream. That split is what lets the same code
/// be driven by a person, a pipe, or a screen reader.
pub fn run(
    target: crate::connection::ConnectionTarget,
    config: &crate::config::Config,
    history: &crate::history::History,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> Result<crate::ExitCode, crate::diagnostics::Diagnostic> {
    use crate::cli::output::{Format, OutputOptions, write_execution};
    use crate::query::result::JobId;
    use std::io::BufRead;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Internal,
                "could not start the async runtime",
                "opening the plain client",
            )
            .likely_cause(error.to_string())
        })?;

    let timeout = std::time::Duration::from_millis(config.query.statement_timeout_ms);
    let provider = target.auth.clone();
    let session = connect_or_ask(&runtime, target, timeout, err)?;
    let info = session.info().clone();

    writeln!(
        err,
        "{} {} - plain mode. Type \\? for help, \\q to leave.",
        crate::branding::PRODUCT_NAME,
        crate::branding::VERSION
    )
    .ok();
    writeln!(err, "{}", connection_summary(&info)).ok();
    if let Some(provider) = &provider {
        // Which credential route opened this session. With a token there was no
        // password to type, so nothing about connecting recorded the answer to
        // "how am I authenticated here" unless it is said.
        writeln!(err, "Authenticated with the {provider} identity provider.").ok();
    }
    if !history.is_recording() {
        // A session that keeps no record says so once, at the top, where it can
        // be read rather than inferred from an empty file later.
        writeln!(err, "Statements are not being recorded in this session.").ok();
    }

    let options = OutputOptions {
        format: Format::Table,
        // Plain mode never draws with characters that only make sense to an eye.
        unicode: false,
        ..OutputOptions::default()
    };

    let mut reader = Reader::default();
    let mut transaction = TransactionState::Autocommit;
    let mut completion_catalog: Option<Option<crate::query::completion::CompletionCatalog>> = None;
    let mut pending_completion: Option<crate::query::completion::CompletionResult> = None;
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        write_plain_idle(err, &session)?;
        write!(
            err,
            "{}",
            prompt(
                &info.database,
                &info.environment,
                transaction,
                reader.is_continuing()
            )
        )
        .ok();
        err.flush().ok();

        let Some(line) = lines.next() else {
            // End of input. Leaving quietly is the right answer to Ctrl+D.
            writeln!(err).ok();
            write_plain_idle(err, &session)?;
            return Ok(crate::ExitCode::Success);
        };
        let line = line.map_err(|error| {
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Internal,
                "could not read input",
                "reading a line",
            )
            .likely_cause(error.to_string())
        })?;

        let sql = match reader.feed(&line) {
            Input::Blank => continue,
            Input::Incomplete => {
                // A numbered list belongs to the exact buffer it was computed
                // from. Any new SQL input invalidates it before another `\use`
                // can apply a stale range.
                pending_completion = None;
                continue;
            }
            Input::Command(Command::Quit) => {
                write_plain_idle(err, &session)?;
                return Ok(crate::ExitCode::Success);
            }
            Input::Command(Command::Help) => {
                writeln!(err, "{}", help_text()).ok();
                continue;
            }
            Input::Command(Command::Connection) => {
                writeln!(err, "{}", connection_summary(&info)).ok();
                continue;
            }
            Input::Command(Command::Format) => {
                let source = reader.buffer().to_owned();
                if source.trim().is_empty() || crate::query::format::is_comment_only(&source) {
                    writeln!(err, "There is no SQL to format.").ok();
                    pending_completion = None;
                    continue;
                }
                match crate::query::format_sql(&source, reader.completion_cursor()) {
                    Ok(formatted) if formatted.text != source => {
                        reader.replace_buffer(formatted.text.clone());
                        let preview = crate::query::value::sanitize_for_display(&formatted.text);
                        writeln!(err, "Formatted SQL buffer:").ok();
                        write!(err, "{preview}").ok();
                        if !formatted.text.ends_with('\n') {
                            writeln!(err).ok();
                        }
                    }
                    Ok(_) => {
                        writeln!(err, "SQL buffer is already formatted.").ok();
                    }
                    Err(error) => {
                        writeln!(err, "{}", error.message(&source)).ok();
                    }
                }
                pending_completion = None;
                continue;
            }
            Input::Command(Command::Complete { prefix }) => {
                if completion_catalog.is_none() {
                    writeln!(err, "Loading schema snapshot for completion...").ok();
                    let loaded = runtime.block_on(session.completion_catalog());
                    match loaded {
                        Ok(catalog) => completion_catalog = Some(Some(catalog)),
                        Err(error) => {
                            writeln!(err, "Schema completion unavailable: {}", error.headline).ok();
                            completion_catalog = Some(None);
                        }
                    }
                }
                let catalog = completion_catalog.as_ref().and_then(Option::as_ref);
                let result = crate::query::completion::complete_with_prefix(
                    reader.buffer(),
                    reader.completion_cursor(),
                    catalog,
                    prefix.as_deref(),
                );
                write_plain_completion(err, &result, catalog.is_some());
                pending_completion = Some(result);
                continue;
            }
            Input::Command(Command::Use(choice)) => {
                let Some(result) = pending_completion.as_ref() else {
                    writeln!(err, "No completion list is open. Type \\complete first.").ok();
                    continue;
                };
                let chosen = choose_plain_completion(result, &choice);
                let Some(candidate) = chosen else {
                    writeln!(
                        err,
                        "There is no completion candidate {choice:?}; use its number or exact name."
                    )
                    .ok();
                    continue;
                };
                let accepted =
                    reader.apply_completion(result.replacement.clone(), &candidate.insert_text);
                if accepted {
                    writeln!(
                        err,
                        "Completion inserted: {}",
                        plain_ascii(&candidate.label)
                    )
                    .ok();
                    pending_completion = None;
                } else {
                    writeln!(
                        err,
                        "Completion could not replace the current word; nothing changed."
                    )
                    .ok();
                }
                continue;
            }
            Input::Command(Command::Unknown(name)) => {
                writeln!(
                    err,
                    "There is no \\{name} command. \\? lists the ones there are."
                )
                .ok();
                continue;
            }
            Input::Statement(sql) => sql,
        };
        pending_completion = None;

        // The same guardrail the full-screen client applies, asked in words.
        let impact = crate::query::classify_all(&statements::split(&sql));
        if info.environment.is_production() && impact.needs_confirmation() {
            let pending = PendingRun {
                sql: sql.clone(),
                impact,
                typed: String::new(),
                required: info.database.clone(),
                source: None,
            };
            write!(err, "{}", confirmation_question(&pending)).ok();
            err.flush().ok();
            let answer = lines
                .next()
                .transpose()
                .unwrap_or_default()
                .unwrap_or_default();
            if !confirmation_accepted(&pending, &answer) {
                writeln!(err, "Cancelled. Nothing was sent.").ok();
                continue;
            }
        }

        let template = match crate::query::discover_parameters(&sql) {
            Ok(template) => template,
            Err(error) => {
                writeln!(
                    err,
                    "{}",
                    error
                        .diagnostic("discovering named parameters")
                        .render_plain(true)
                )
                .ok();
                continue;
            }
        };
        let parameters = if template.is_empty() {
            None
        } else {
            match prompt_parameters(&template, err) {
                Ok(Some(parameters)) => Some(parameters),
                Ok(None) => continue,
                Err(error) => {
                    writeln!(err, "{}", error.render_plain(true)).ok();
                    continue;
                }
            }
        };

        let execution = runtime.block_on(
            crate::cli::interactive::execute_cancellable_with_parameters(
                &session,
                &sql,
                parameters.as_ref(),
                config.query.max_buffered_rows,
                JobId(1),
            ),
        );
        transaction = execution.transaction;

        // Stdout carries rows and nothing else, so a transcript can be piped
        // into a file and still be data. A statement that returned no rows says
        // what it did on stderr, in the outcome line below, rather than putting
        // a second copy of the same sentence in with the data.
        if execution
            .statements
            .iter()
            .any(|statement| statement.result_set.is_some())
        {
            write_execution(out, &execution, &options).ok();
            out.flush().ok();
        }

        for statement in &execution.statements {
            for notice in &statement.notices {
                // Server text is hostile input: escape control characters before
                // they can reach the terminal, where they could repaint it.
                writeln!(
                    err,
                    "{}: {}",
                    crate::query::sanitize_for_display(&notice.severity),
                    crate::query::sanitize_for_display(&notice.message)
                )
                .ok();
            }
        }
        write_plain_unattributed(err, &execution.notice_report)?;
        crate::cli::write_notice_counts(err, execution.notice_report.counts)
            .map_err(crate::cli::io_diagnostic)?;
        if let Some(error) = &execution.error {
            write!(err, "{}", error.render_plain(true)).ok();
            write_error_location(err, &sql, error, parameters.is_some());
        }
        writeln!(err, "{}", outcome_line(&execution)).ok();

        // Recorded after the fact, so what is kept is what really ran. A history
        // that cannot be written is said once and never stops the session.
        let entry = crate::history::Entry::now(
            &info.target,
            &info.database,
            &info.environment.label(),
            &sql,
            crate::history::Outcome::from_status(&execution.status),
            execution.elapsed,
        );
        match history.record(&entry) {
            Ok(recorded) => {
                if let Some(note) = recorded.note() {
                    writeln!(err, "{note}").ok();
                }
            }
            Err(diagnostic) => {
                writeln!(err, "{}", diagnostic.headline).ok();
            }
        }
    }
}

/// Collects one hidden answer per distinct named parameter in first-use order.
fn prompt_parameters(
    template: &crate::query::ParameterTemplate,
    err: &mut impl std::io::Write,
) -> Result<Option<crate::query::ParameterBindings>, crate::diagnostics::Diagnostic> {
    if !crate::cli::prompt::can_ask() {
        writeln!(
            err,
            "This statement has named parameters, but plain mode has no interactive terminal."
        )
        .ok();
        writeln!(
            err,
            "Use `{} query --param-env NAME=VARIABLE` for automation, or run without --plain.",
            crate::branding::BINARY_NAME
        )
        .ok();
        return Ok(None);
    }

    let mut values = Vec::with_capacity(template.names().len());
    for name in template.names() {
        let question = format!("Value for :{name} (hidden literal text; empty is valid): ");
        let Some(value) = crate::cli::prompt::read_hidden(&question, err)? else {
            writeln!(err, "Cancelled. Nothing was sent.").ok();
            return Ok(None);
        };
        values.push(SecretString::from(value));
    }

    let bindings = crate::query::ParameterBindings::from_secrets(template.names().to_vec(), values)
        .map_err(|error| error.diagnostic("collecting named parameter values"))?;
    bindings
        .validate()
        .map_err(|error| error.diagnostic("collecting named parameter values"))?;
    Ok(Some(bindings))
}

/// Adds a line-oriented source excerpt to a plain diagnostic when PostgreSQL's
/// position can be mapped honestly into the submitted SQL.
fn write_error_location(
    err: &mut impl std::io::Write,
    sql: &str,
    error: &crate::diagnostics::Diagnostic,
    parameterized: bool,
) {
    if error.kind != crate::diagnostics::DiagnosticKind::Query {
        return;
    }
    if parameterized && error.position.is_some() {
        writeln!(
            err,
            "  Location: unavailable; the server position belongs to the expanded request after parameter binding."
        )
        .ok();
        return;
    }
    let Some(position) = error.position else {
        writeln!(
            err,
            "  Location: unavailable; PostgreSQL did not report an editor position."
        )
        .ok();
        return;
    };
    let Some(statement_number) = error.statement_number.filter(|number| *number > 0) else {
        writeln!(
            err,
            "  Location: unavailable; the failed statement number was not reported."
        )
        .ok();
        return;
    };
    let Some(location) =
        crate::query::error_location::locate(sql, None, statement_number, position)
    else {
        writeln!(
            err,
            "  Location: unavailable; server character {} is outside the submitted statement.",
            position.character
        )
        .ok();
        return;
    };
    writeln!(
        err,
        "  Location: statement {}, line {}, column {}",
        location.statement_number, location.line, location.column
    )
    .ok();
    if let Some(marker) = crate::query::error_location::render_marker(sql, &location) {
        for line in marker.lines() {
            writeln!(err, "  {line}").ok();
        }
    }
}

/// Opens the connection, asking for a password if the server wants one.
///
/// The ask happens only when there is a terminal at both ends. A script must
/// fail rather than hang waiting for something nobody can type, which is why
/// the question is asked once, of a person, and never of a pipe.
fn connect_or_ask(
    runtime: &tokio::runtime::Runtime,
    target: crate::connection::ConnectionTarget,
    timeout: std::time::Duration,
    err: &mut impl std::io::Write,
) -> Result<crate::postgres::Session, crate::diagnostics::Diagnostic> {
    let first = runtime.block_on(crate::postgres::connect_reported(&target, timeout));
    write_plain_unattributed(err, &first.notice_report)?;
    crate::cli::write_notice_counts(err, first.notice_report.counts)
        .map_err(crate::cli::io_diagnostic)?;
    let diagnostic = match first.value {
        Ok(session) => return Ok(session),
        Err(diagnostic) => diagnostic,
    };

    // A target authenticating through a cloud provider is never asked about.
    // The credential came from a program, it was accepted or refused on its own
    // terms, and nothing a person can type will change that. Prompting would
    // describe the wrong problem and waste a password on the way.
    if diagnostic.kind != crate::diagnostics::DiagnosticKind::Authentication
        || target.auth.is_some()
        || !crate::cli::prompt::can_ask()
    {
        return Err(diagnostic);
    }

    writeln!(err, "{}", diagnostic.headline).ok();
    let question = format!("Password for {}: ", target.safe_display());
    let Some(password) = crate::cli::prompt::read_password(&question, err)? else {
        // Choosing not to answer leaves the original refusal, which is the
        // truthful outcome rather than a new error about the prompt.
        return Err(diagnostic);
    };

    // The password goes into one attempt and is dropped with it.
    let retry = target.with_password(secrecy::SecretString::from(password));
    let retry = runtime.block_on(crate::postgres::connect_reported(&retry, timeout));
    write_plain_unattributed(err, &retry.notice_report)?;
    crate::cli::write_notice_counts(err, retry.notice_report.counts)
        .map_err(crate::cli::io_diagnostic)?;
    retry.value
}

/// Prints a completion result without control sequences or colour.
fn write_plain_completion(
    err: &mut impl std::io::Write,
    result: &crate::query::completion::CompletionResult,
    catalog_ready: bool,
) {
    writeln!(
        err,
        "Completion for {}. Showing {} of {} matching candidates ({} available).",
        plain_ascii(&result.scope.label()),
        result.candidates.len(),
        result.matching_count,
        result.total_count
    )
    .ok();
    if catalog_ready {
        writeln!(err, "Candidates come from one local schema snapshot.").ok();
    } else {
        writeln!(
            err,
            "Only SQL keywords are available because the schema snapshot is unavailable."
        )
        .ok();
    }
    for (index, candidate) in result.candidates.iter().enumerate() {
        writeln!(
            err,
            "{}  {}  {}",
            index + 1,
            plain_ascii(&candidate.label),
            plain_ascii(&candidate.plain_detail())
        )
        .ok();
    }
    if result.candidates.is_empty() {
        writeln!(err, "No candidates for this position.").ok();
    } else {
        writeln!(
            err,
            "Use \\use <number|exact-name> to insert one; Esc is not needed in plain mode."
        )
        .ok();
    }
}

/// Finds a numbered or exact-name candidate from a printed result.
fn choose_plain_completion<'a>(
    result: &'a crate::query::completion::CompletionResult,
    choice: &str,
) -> Option<&'a crate::query::completion::Candidate> {
    if let Ok(number) = choice.trim().parse::<usize>() {
        return number
            .checked_sub(1)
            .and_then(|index| result.candidates.get(index));
    }
    let choice = choice.trim();
    let choice = choice
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            choice
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(choice);
    result
        .candidates
        .iter()
        .find(|candidate| candidate.exact_name(choice))
}

/// Escapes non-ASCII completion text for a plain transcript.
fn plain_ascii(value: &str) -> String {
    let safe = crate::query::value::sanitize_for_display(value);
    let mut result = String::with_capacity(safe.len());
    for character in safe.chars() {
        if character.is_ascii() {
            result.push(character);
        } else {
            result.push_str(&format!("\\u{{{:04X}}}", character as u32));
        }
    }
    result
}

/// One line describing what this session is and what protects it.
#[must_use]
fn connection_summary(info: &crate::postgres::SessionInfo) -> String {
    format!(
        "Connected to {} as {} on PostgreSQL {}. [{}] [{}]. {}",
        info.database,
        info.user,
        info.server_version,
        info.environment.label(),
        info.posture(),
        info.tls.description()
    )
}

fn write_plain_unattributed(
    err: &mut impl std::io::Write,
    report: &crate::query::result::NoticeReport,
) -> Result<(), crate::diagnostics::Diagnostic> {
    if !report.unattributed.is_empty() {
        writeln!(
            err,
            "Server notices received on connection (causation unconfirmed):"
        )
        .map_err(crate::cli::io_diagnostic)?;
        for notice in &report.unattributed {
            writeln!(
                err,
                "{}: {}",
                crate::query::sanitize_for_display(&notice.severity),
                crate::query::sanitize_for_display(&notice.message)
            )
            .map_err(crate::cli::io_diagnostic)?;
        }
    }
    Ok(())
}

fn write_plain_idle(
    err: &mut impl std::io::Write,
    session: &crate::postgres::Session,
) -> Result<(), crate::diagnostics::Diagnostic> {
    if let Some(report) = session.take_connection_notice_report() {
        write_plain_unattributed(err, &report)?;
        crate::cli::write_notice_counts(err, report.counts).map_err(crate::cli::io_diagnostic)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
