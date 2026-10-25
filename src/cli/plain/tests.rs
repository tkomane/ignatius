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
    assert_eq!(reader.feed("\\format"), Input::Command(Command::Format));
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
fn plain_format_command_keeps_pending_sql_and_replaces_only_on_success() {
    let mut reader = Reader::default();
    reader.feed("select o.id,o.total from orders o where o.total>0");
    let before = reader.buffer().to_owned();

    assert_eq!(reader.feed("\\format"), Input::Command(Command::Format));
    assert_eq!(reader.buffer(), before);

    let source = reader.buffer().to_owned();
    let formatted = crate::query::format_sql(&source, reader.completion_cursor())
        .expect("format should succeed");
    reader.replace_buffer(formatted.text.clone());
    assert_eq!(reader.buffer(), formatted.text);
    assert!(reader.buffer().contains("\nfrom orders"));

    assert!(matches!(reader.feed(";"), Input::Statement(sql) if sql.contains("\nfrom orders")));
    assert!(!reader.is_continuing());
}

#[test]
fn plain_format_command_treats_comment_only_input_as_empty() {
    let mut reader = Reader::default();
    reader.feed("-- only a note");
    assert_eq!(reader.feed("\\format"), Input::Command(Command::Format));
    assert_eq!(reader.buffer(), "-- only a note\n");
    assert!(crate::query::statements::split(reader.buffer()).is_empty());
}

#[test]
fn a_completion_command_inside_a_dollar_quoted_body_is_not_intercepted() {
    let mut reader = Reader::default();
    reader.feed("DO $$");
    assert_eq!(reader.feed("\\complete"), Input::Incomplete);
    assert!(reader.is_continuing());
}

#[test]
fn a_format_command_inside_a_dollar_quoted_body_is_not_intercepted() {
    let mut reader = Reader::default();
    reader.feed("DO $$");
    assert_eq!(reader.feed("\\format"), Input::Incomplete);
    assert!(reader.buffer().contains("\\format"));
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
        notice_report: crate::query::result::NoticeReport::default(),
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
        source: None,
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
fn plain_errors_keep_statement_location_and_a_safe_source_caret() {
    let sql = "SELECT 1;\nSELECT café FROM orders WHERE id = 0;\n";
    let diagnostic = crate::diagnostics::Diagnostic::new(
        crate::diagnostics::DiagnosticKind::Query,
        "syntax error",
        "running statement 2",
    )
    .in_statement(2)
    .at_position(19);
    let mut out = Vec::new();
    write_error_location(&mut out, sql, &diagnostic, false);
    let text = String::from_utf8(out).expect("plain UTF-8");
    assert!(
        text.contains("Location: statement 2, line 2, column 19"),
        "{text}"
    );
    assert!(text.contains("2 | SELECT café FROM orders"), "{text}");
    assert!(text.contains('^'), "{text}");
    assert!(!text.contains('\x1b'), "terminal controls leaked: {text:?}");
}

#[test]
fn plain_errors_explain_when_a_server_position_is_unavailable() {
    let diagnostic = crate::diagnostics::Diagnostic::new(
        crate::diagnostics::DiagnosticKind::Query,
        "permission denied",
        "running statement 1",
    )
    .in_statement(1);
    let mut out = Vec::new();
    write_error_location(&mut out, "SELECT 1;", &diagnostic, false);
    let text = String::from_utf8(out).expect("plain UTF-8");
    assert!(text.contains("Location: unavailable"), "{text}");
    assert!(!text.contains('^'), "a missing position must not guess");
}

#[test]
fn plain_parameter_prompt_refuses_without_a_terminal_and_names_the_safe_route() {
    let template = crate::query::discover_parameters("SELECT :secret").expect("template");
    let mut out = Vec::new();
    let result = prompt_parameters(&template, &mut out).expect("refusal is not an error");
    assert!(result.is_none());
    let text = String::from_utf8(out).expect("plain UTF-8");
    assert!(text.contains("no interactive terminal"), "{text}");
    assert!(text.contains("--param-env NAME=VARIABLE"), "{text}");
    assert!(!text.contains("secret-value"));
}

#[test]
fn plain_parameterized_errors_do_not_draw_a_template_caret() {
    let diagnostic = crate::diagnostics::Diagnostic::new(
        crate::diagnostics::DiagnosticKind::Query,
        "syntax error",
        "running statement 1",
    )
    .in_statement(1)
    .at_position(17);
    let mut out = Vec::new();
    write_error_location(&mut out, "SELECT :value;", &diagnostic, true);
    let text = String::from_utf8(out).expect("plain UTF-8");
    assert!(text.contains("expanded request"), "{text}");
    assert!(
        !text.contains('^'),
        "the expanded position cannot mark a template"
    );
}

#[test]
fn help_lists_every_command_that_exists() {
    let help = help_text();
    for command in ["\\q", "\\?", "\\c", "\\format"] {
        assert!(help.contains(command), "{command} missing from help");
    }
    assert!(help.contains("Ctrl+C"), "cancelling must be discoverable");
    assert!(help.contains("semicolon"), "how to run a statement: {help}");
}
