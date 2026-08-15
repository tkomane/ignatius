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
    /// Something that is not a command.
    Unknown(String),
}

impl Command {
    /// Parses a backslash command.
    #[must_use]
    pub fn parse(line: &str) -> Self {
        match line.trim().trim_start_matches('\\').trim() {
            "q" | "quit" | "exit" => Self::Quit,
            "?" | "h" | "help" => Self::Help,
            "conninfo" | "c" => Self::Connection,
            other => Self::Unknown(other.to_owned()),
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

    /// Adds one line of input.
    pub fn feed(&mut self, line: &str) -> Input {
        // A meta-command is only a meta-command at the start of a statement.
        // Otherwise a backslash inside SQL would be intercepted.
        if !self.is_continuing() && line.trim_start().starts_with('\\') {
            return Input::Command(Command::parse(line));
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
    let session = runtime.block_on(crate::postgres::connect(&target, timeout))?;
    let info = session.info().clone();

    writeln!(
        err,
        "{} {} - plain mode. Type \\? for help, \\q to leave.",
        crate::branding::PRODUCT_NAME,
        crate::branding::VERSION
    )
    .ok();
    writeln!(err, "{}", connection_summary(&info)).ok();

    let options = OutputOptions {
        format: Format::Table,
        // Plain mode never draws with characters that only make sense to an eye.
        unicode: false,
        ..OutputOptions::default()
    };

    let mut reader = Reader::default();
    let mut transaction = TransactionState::Autocommit;
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
            Input::Blank | Input::Incomplete => continue,
            Input::Command(Command::Quit) => return Ok(crate::ExitCode::Success),
            Input::Command(Command::Help) => {
                writeln!(err, "{}", help_text()).ok();
                continue;
            }
            Input::Command(Command::Connection) => {
                writeln!(err, "{}", connection_summary(&info)).ok();
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
    }
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
