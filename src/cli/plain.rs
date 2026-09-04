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
            let out_of_band = matches!(command, Command::Complete { .. } | Command::Use(_))
                && !crate::query::completion::context(&self.buffer, self.completion_cursor())
                    .in_literal_or_comment;
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
         \\complete [prefix]  show schema-aware candidates for the SQL buffer\n\
         \\use <number|name>  insert one candidate from that list\n\
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
            Input::Command(Command::Quit) => return Ok(crate::ExitCode::Success),
            Input::Command(Command::Help) => {
                writeln!(err, "{}", help_text()).ok();
                continue;
            }
            Input::Command(Command::Connection) => {
                writeln!(err, "{}", connection_summary(&info)).ok();
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

        let execution = runtime.block_on(crate::cli::interactive::execute_cancellable(
            &session,
            &sql,
            config.query.max_buffered_rows,
            JobId(1),
        ));
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
                writeln!(err, "{}: {}", notice.severity, notice.message).ok();
            }
        }
        if let Some(error) = &execution.error {
            write!(err, "{}", error.render_plain(true)).ok();
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
    let first = runtime.block_on(crate::postgres::connect(&target, timeout));
    let diagnostic = match first {
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
    runtime.block_on(crate::postgres::connect(&retry, timeout))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Environment;

    #[test]
    fn a_statement_runs_when_it_is_finished_and_not_before() {
        let mut reader = Reader::default();
        assert_eq!(reader.feed("SELECT customer_id"), Input::Incomplete);
        assert!(
            reader.is_continuing(),
            "the prompt should show a continuation"
        );
        assert_eq!(reader.feed("FROM orders"), Input::Incomplete);

        let Input::Statement(sql) = reader.feed("WHERE total > 0;") else {
            panic!("the statement should be complete");
        };
        assert!(sql.contains("SELECT customer_id"));
        assert!(sql.contains("WHERE total > 0;"));
        assert!(!reader.is_continuing(), "the buffer is emptied");
    }

    #[test]
    fn a_semicolon_inside_a_string_does_not_end_the_statement() {
        let mut reader = Reader::default();
        assert_eq!(reader.feed("SELECT 'a;b'"), Input::Incomplete);
        assert!(
            reader.is_continuing(),
            "sending half a statement is exactly what the lexer prevents"
        );
        assert!(matches!(reader.feed(";"), Input::Statement(_)));
    }

    #[test]
    fn a_dollar_quoted_body_keeps_the_input_going() {
        let mut reader = Reader::default();
        reader.feed("CREATE FUNCTION f() RETURNS int AS $$");
        assert_eq!(reader.feed("  SELECT 1; SELECT 2;"), Input::Incomplete);
        assert!(matches!(
            reader.feed("$$ LANGUAGE sql;"),
            Input::Statement(_)
        ));
    }

    #[test]
    fn meta_commands_are_only_meta_at_the_start_of_a_statement() {
        let mut reader = Reader::default();
        assert_eq!(reader.feed("\\q"), Input::Command(Command::Quit));
        assert_eq!(reader.feed("\\?"), Input::Command(Command::Help));
        assert_eq!(
            reader.feed("\\complete ord"),
            Input::Command(Command::Complete {
                prefix: Some("ord".into())
            })
        );
        assert_eq!(
            reader.feed("\\use 1"),
            Input::Command(Command::Use("1".into()))
        );
        assert_eq!(reader.feed("  \\help  "), Input::Command(Command::Help));
        assert_eq!(
            reader.feed("\\nonsense"),
            Input::Command(Command::Unknown("nonsense".into()))
        );

        // Mid-statement, a backslash is just text.
        reader.feed("SELECT 'a\\b'");
        assert!(reader.is_continuing());
        assert_eq!(
            reader.feed("\\q"),
            Input::Incomplete,
            "a backslash inside SQL must not be intercepted"
        );
    }

    #[test]
    fn plain_completion_commands_keep_the_sql_buffer_and_replace_only_on_use() {
        let mut reader = Reader::default();
        assert_eq!(reader.feed("SELECT * FROM ord"), Input::Incomplete);
        let before = reader.buffer().to_owned();
        assert!(matches!(
            reader.feed("\\complete"),
            Input::Command(Command::Complete { prefix: None })
        ));
        assert_eq!(reader.buffer(), before);
        let end = reader.completion_cursor();
        let start = end - "ord".len();
        assert!(reader.apply_completion(start..end, "\"orders\""));
        assert_eq!(reader.buffer(), "SELECT * FROM \"orders\"\n");
    }

    #[test]
    fn a_completion_command_inside_a_dollar_quoted_body_is_not_intercepted() {
        let mut reader = Reader::default();
        reader.feed("DO $$");
        assert_eq!(reader.feed("\\complete"), Input::Incomplete);
        assert!(reader.is_continuing());
    }

    #[test]
    fn a_blank_line_at_the_start_does_nothing() {
        let mut reader = Reader::default();
        assert_eq!(reader.feed(""), Input::Blank);
        assert_eq!(reader.feed("   "), Input::Blank);
        assert!(!reader.is_continuing());
    }

    #[test]
    fn the_prompt_carries_the_facts_a_person_needs_before_pressing_enter() {
        let plain = prompt(
            "orders",
            &Environment::Local,
            TransactionState::Autocommit,
            false,
        );
        assert_eq!(plain, "orders => ");

        let production = prompt(
            "orders",
            &Environment::Production,
            TransactionState::Autocommit,
            false,
        );
        assert!(
            production.contains("[PROD]"),
            "the classification is never left off: {production}"
        );

        let failed = prompt(
            "orders",
            &Environment::Local,
            TransactionState::Failed,
            false,
        );
        assert!(failed.contains("[transaction failed]"), "{failed}");

        let open = prompt("orders", &Environment::Local, TransactionState::Open, false);
        assert!(open.contains("[in transaction]"), "{open}");

        assert_eq!(
            prompt(
                "orders",
                &Environment::Production,
                TransactionState::Open,
                true
            ),
            "... > ",
            "a continuation line stays out of the way"
        );
    }

    #[test]
    fn the_outcome_line_says_what_happened_and_what_to_do() {
        use crate::query::result::{ExecutionStatus, JobId, ResultSet, StatementResult};
        use crate::query::value::Cell;
        use std::time::Duration;

        let mut set = ResultSet::new(vec!["n".into()], 10);
        set.push(vec![Cell::Text("1".into())]);
        let execution = Execution {
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(1),
                elapsed: Duration::from_millis(4),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(5),
            error: None,
            transaction: TransactionState::Failed,
        };

        let line = outcome_line(&execution);
        assert!(line.contains("Completed"), "{line}");
        assert!(line.contains("1 row"), "{line}");
        assert!(
            line.contains("ROLLBACK"),
            "a failed transaction names the way out even here: {line}"
        );
    }

    #[test]
    fn a_production_write_is_confirmed_in_words() {
        let pending = PendingRun {
            sql: "DROP TABLE orders".into(),
            impact: crate::query::Impact::Destructive,
            typed: String::new(),
            required: "orders".into(),
        };
        let question = confirmation_question(&pending);
        assert!(question.contains("destroys data"), "{question}");
        assert!(question.contains("production"), "{question}");
        assert!(
            question.contains("orders"),
            "it names what to type: {question}"
        );

        assert!(confirmation_accepted(&pending, "orders"));
        assert!(confirmation_accepted(&pending, " orders \n"));
        assert!(!confirmation_accepted(&pending, "yes"));
        assert!(!confirmation_accepted(&pending, ""));

        let lesser = PendingRun {
            impact: crate::query::Impact::Write,
            ..pending
        };
        assert!(confirmation_accepted(&lesser, "yes"));
        assert!(confirmation_accepted(&lesser, "YES"));
        assert!(!confirmation_accepted(&lesser, ""));
    }

    #[test]
    fn help_lists_every_command_that_exists() {
        let help = help_text();
        for command in ["\\q", "\\?", "\\c"] {
            assert!(help.contains(command), "{command} missing from help");
        }
        assert!(help.contains("Ctrl+C"), "cancelling must be discoverable");
        assert!(help.contains("semicolon"), "how to run a statement: {help}");
    }
}
