use super::*;

struct LimitedRowWriter {
    remaining: usize,
    bytes: Vec<u8>,
    failed: bool,
    flushes: usize,
}

impl io::Write for LimitedRowWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        assert!(!self.failed, "serialization retried after a writer error");
        if self.remaining == 0 {
            self.failed = true;
            return Err(io::Error::other("synthetic writer boundary"));
        }
        let accepted = self.remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..accepted]);
        self.remaining -= accepted;
        Ok(accepted)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

fn row_fault_options(format: Format) -> OutputOptions {
    OutputOptions {
        format,
        header: format == Format::Insert,
        insert_table: (format == Format::Insert).then(|| "snapshot".into()),
        ..OutputOptions::default()
    }
}

fn check_complete_row_count(format: Format) {
    let row = vec![Cell::Text("synthetic,\n\"row'\t界".into())];
    let mut wrong_counts = Vec::new();
    for prefix in [0, 1] {
        let options = row_fault_options(format);
        let mut complete = StreamWriter::new(options.clone()).unwrap();
        let mut encoded = Vec::new();
        complete
            .columns(&mut encoded, vec!["value".into()])
            .unwrap();
        if prefix == 1 {
            complete.row(&mut encoded, &row).unwrap();
        }
        let prefix_bytes = encoded.len();
        complete.row(&mut encoded, &row).unwrap();
        let row_bytes = encoded.len() - prefix_bytes;
        assert!(row_bytes > 2);
        assert_eq!(complete.rows(), prefix + 1);
        for before_failure in [0, row_bytes / 2, row_bytes - 1] {
            let mut output = LimitedRowWriter {
                remaining: prefix_bytes + before_failure,
                bytes: Vec::new(),
                failed: false,
                flushes: 0,
            };
            let mut stream = StreamWriter::new(options.clone()).unwrap();
            stream.columns(&mut output, vec!["value".into()]).unwrap();
            if prefix == 1 {
                stream.row(&mut output, &row).unwrap();
            }
            assert_eq!(stream.rows(), prefix);
            let error = stream.row(&mut output, &row).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert_eq!(output.bytes, encoded[..prefix_bytes + before_failure]);
            assert_eq!(output.flushes, 0, "row writes must remain buffered");
            if stream.rows() != prefix {
                wrong_counts.push((prefix, before_failure, stream.rows()));
            }
        }
    }
    assert!(
        wrong_counts.is_empty(),
        "{format:?}: rejected rows counted at (successful-prefix, accepted-row-bytes, observed-count): {wrong_counts:?}"
    );
}

#[test]
fn w10_csv_counts_only_complete_rows() {
    check_complete_row_count(Format::Csv);
}
#[test]
fn w10_tsv_counts_only_complete_rows() {
    check_complete_row_count(Format::Tsv);
}
#[test]
fn w10_ndjson_counts_only_complete_rows() {
    check_complete_row_count(Format::Ndjson);
}
#[test]
fn w10_insert_counts_only_complete_rows() {
    check_complete_row_count(Format::Insert);
}

#[test]
fn w10_header_or_column_failure_never_counts_a_row() {
    for format in [Format::Csv, Format::Tsv] {
        let mut stream = StreamWriter::new(OutputOptions {
            format,
            ..OutputOptions::default()
        })
        .unwrap();
        let mut output = LimitedRowWriter {
            remaining: 0,
            bytes: Vec::new(),
            failed: false,
            flushes: 0,
        };
        assert!(stream.columns(&mut output, vec!["value".into()]).is_err());
        assert_eq!(stream.rows(), 0);
        assert!(output.bytes.is_empty());
    }
    let mut stream = StreamWriter::new(row_fault_options(Format::Insert)).unwrap();
    let mut output = Vec::new();
    assert!(
        stream
            .columns(&mut output, vec!["same".into(), "same".into()])
            .is_err()
    );
    assert_eq!(stream.rows(), 0);
    assert!(output.is_empty());
}
use crate::query::result::{ExecutionStatus, JobId, StatementResult};
use std::time::Duration;

fn execution(sets: Vec<ResultSet>) -> Execution {
    Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: sets
            .into_iter()
            .map(|set| StatementResult {
                rows_affected: Some(set.rows_seen),
                result_set: Some(set),
                elapsed: Duration::from_millis(1),
                notices: Vec::new(),
            })
            .collect(),
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(2),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    }
}

fn sample() -> ResultSet {
    let mut set = ResultSet::new(vec!["id".into(), "name".into(), "note".into()], 100);
    set.push(vec![
        Cell::Text("1".into()),
        Cell::Text("Ada".into()),
        Cell::Null,
    ]);
    set.push(vec![
        Cell::Text("2".into()),
        Cell::Text("with, comma".into()),
        Cell::Text("say \"hi\"".into()),
    ]);
    set
}

fn render(format: Format, execution: &Execution) -> String {
    let mut out = Vec::new();
    write_execution(
        &mut out,
        execution,
        &OutputOptions {
            format,
            ..OutputOptions::default()
        },
    )
    .expect("write");
    String::from_utf8(out).expect("utf8")
}

fn render_insert(execution: &Execution) -> String {
    let mut out = Vec::new();
    write_execution(
        &mut out,
        execution,
        &OutputOptions {
            format: Format::Insert,
            insert_table: Some("orders".into()),
            ..OutputOptions::default()
        },
    )
    .expect("write");
    String::from_utf8(out).expect("utf8")
}

#[test]
fn csv_quotes_only_what_needs_quoting_and_encodes_null_as_empty() {
    let text = render(Format::Csv, &execution(vec![sample()]));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "id,name,note");
    assert_eq!(
        lines[1], "1,Ada,",
        "SQL NULL is an empty CSV field by default"
    );
    assert_eq!(lines[2], "2,\"with, comma\",\"say \"\"hi\"\"\"");
}

#[test]
fn csv_null_encoding_is_configurable() {
    let mut out = Vec::new();
    write_execution(
        &mut out,
        &execution(vec![sample()]),
        &OutputOptions {
            format: Format::Csv,
            null_encoding: "\\N".into(),
            ..OutputOptions::default()
        },
    )
    .expect("write");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("1,Ada,\\N"), "{text}");
}

#[test]
fn tsv_escapes_control_characters_rather_than_quoting() {
    let mut set = ResultSet::new(vec!["v".into()], 10);
    set.push(vec![Cell::Text("a\tb\nc".into())]);
    let text = render(Format::Tsv, &execution(vec![set]));
    assert!(text.contains("a\\tb\\nc"), "{text:?}");
    assert_eq!(
        text.lines().count(),
        2,
        "the escaped value stays on one line"
    );
}

#[test]
fn json_distinguishes_null_from_empty_string() {
    let mut set = ResultSet::new(vec!["a".into(), "b".into()], 10);
    set.push(vec![Cell::Null, Cell::Text(String::new())]);
    let text = render(Format::Json, &execution(vec![set]));
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert!(parsed[0]["rows"][0]["a"].is_null());
    assert_eq!(parsed[0]["rows"][0]["b"], "");
}

#[test]
fn json_reports_truncation_and_the_true_row_count() {
    let mut set = ResultSet::new(vec!["n".into()], 1);
    set.push(vec![Cell::Text("1".into())]);
    set.push(vec![Cell::Text("2".into())]);
    let text = render(Format::Json, &execution(vec![set]));
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed[0]["rows"].as_array().expect("rows").len(), 1);
    assert_eq!(parsed[0]["rows_seen"], 2);
    assert_eq!(parsed[0]["truncated"], true);
}

#[test]
fn ndjson_writes_one_object_per_line_and_refuses_multiple_result_sets() {
    let text = render(Format::Ndjson, &execution(vec![sample()]));
    assert_eq!(text.lines().count(), 2);
    for line in text.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("each line is valid json");
    }

    let mut out = Vec::new();
    let err = write_execution(
        &mut out,
        &execution(vec![sample(), sample()]),
        &OutputOptions {
            format: Format::Ndjson,
            ..OutputOptions::default()
        },
    )
    .expect_err("must refuse rather than guess");
    assert!(err.to_string().contains("--format json"), "{err}");
}

#[test]
fn machine_formats_carry_no_decoration() {
    for format in [
        Format::Csv,
        Format::Tsv,
        Format::Json,
        Format::Ndjson,
        Format::Insert,
    ] {
        assert!(format.is_machine_readable());
        let text = if format == Format::Insert {
            render_insert(&execution(vec![sample()]))
        } else {
            render(format, &execution(vec![sample()]))
        };
        assert!(
            !text.contains('\x1b'),
            "{format:?} emitted an escape sequence"
        );
        assert!(
            !text.contains("rows)"),
            "{format:?} emitted a row-count banner"
        );
        assert!(!text.contains('│'), "{format:?} emitted table drawing");
    }
}

#[test]
fn the_table_format_aligns_columns_and_states_the_row_count() {
    let text = render(Format::Table, &execution(vec![sample()]));
    assert!(text.contains("id"), "{text}");
    assert!(
        text.contains("[null]"),
        "NULL stays distinguishable on screen"
    );
    assert!(text.contains("(2 rows)"), "{text}");
}

#[test]
fn the_table_format_escapes_hostile_values() {
    let mut set = ResultSet::new(vec!["v".into()], 10);
    set.push(vec![Cell::Text("\x1b[2Jboom".into())]);
    let text = render(Format::Table, &execution(vec![set]));
    assert!(!text.contains('\x1b'), "{text:?}");
}

#[test]
fn markdown_escapes_pipes_so_the_table_stays_a_table() {
    let mut set = ResultSet::new(vec!["a|b".into()], 10);
    set.push(vec![Cell::Text("x|y".into())]);
    set.push(vec![Cell::Null]);
    let text = render(Format::Markdown, &execution(vec![set]));
    assert!(text.contains(r"| a\|b |"), "{text}");
    assert!(text.contains(r"| x\|y |"), "{text}");
    assert!(text.contains("_null_"), "NULL is marked, not blank: {text}");
}

#[test]
fn insert_quotes_identifiers_and_preserves_text_null_empty_and_controls() {
    let mut set = ResultSet::new(vec!["id".into(), "note".into(), "empty".into()], 10);
    set.push(vec![
        Cell::Text("1".into()),
        Cell::Text("O'Reilly \\path\n\tcafé".into()),
        Cell::Text(String::new()),
    ]);
    set.push(vec![Cell::Null, Cell::Null, Cell::Text("done".into())]);
    let text = render_insert(&execution(vec![set]));
    assert!(
        text.contains("INSERT INTO \"orders\" (\"id\", \"note\", \"empty\") VALUES"),
        "{text}"
    );
    assert!(text.contains("E'1'"), "{text}");
    assert!(text.contains("E'O\\'Reilly \\\\path\\n\\tcafé'"), "{text}");
    assert!(
        text.contains("E''"),
        "empty text must not become SQL NULL: {text}"
    );
    assert!(text.contains("(NULL, NULL, E'done');"), "{text}");
}

#[test]
fn insert_quotes_hostile_table_and_column_names_without_interpolating_them() {
    let mut set = ResultSet::new(vec!["note\"; DROP".into()], 10);
    set.push(vec![Cell::Text("x'); DROP TABLE orders; --".into())]);
    let mut out = Vec::new();
    write_execution(
        &mut out,
        &execution(vec![set]),
        &OutputOptions {
            format: Format::Insert,
            insert_table: Some("target\"; DROP TABLE orders; --".into()),
            ..OutputOptions::default()
        },
    )
    .expect("write");
    let text = String::from_utf8(out).expect("utf8");
    assert!(
        text.starts_with(
            "INSERT INTO \"target\"\"; DROP TABLE orders; --\" (\"note\"\"; DROP\") VALUES"
        ),
        "{text}"
    );
    assert!(text.contains("E'x\\'); DROP TABLE orders; --'"), "{text}");
}

#[test]
fn insert_refuses_ambiguous_columns_and_nul_before_buffered_output() {
    let mut duplicate = ResultSet::new(vec!["id".into(), "id".into()], 10);
    duplicate.push(vec![Cell::Text("1".into()), Cell::Text("2".into())]);
    let mut out = Vec::new();
    let error = write_execution(
        &mut out,
        &execution(vec![duplicate]),
        &OutputOptions {
            format: Format::Insert,
            insert_table: Some("orders".into()),
            ..OutputOptions::default()
        },
    )
    .expect_err("duplicate labels must refuse");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(out.is_empty(), "refusal must not write partial SQL");

    let mut nul = ResultSet::new(vec!["note".into()], 10);
    nul.push(vec![Cell::Text("bad\0value".into())]);
    let mut out = Vec::new();
    let error = write_execution(
        &mut out,
        &execution(vec![nul]),
        &OutputOptions {
            format: Format::Insert,
            insert_table: Some("orders".into()),
            ..OutputOptions::default()
        },
    )
    .expect_err("NUL must refuse");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(out.is_empty(), "refusal must not write partial SQL");
}

#[test]
fn insert_validates_all_result_sets_before_writing_any_statement() {
    let mut duplicate = ResultSet::new(vec!["id".into(), "id".into()], 10);
    duplicate.push(vec![Cell::Text("1".into()), Cell::Text("2".into())]);
    let mut out = Vec::new();
    let error = write_execution(
        &mut out,
        &execution(vec![sample(), duplicate]),
        &OutputOptions {
            format: Format::Insert,
            insert_table: Some("orders".into()),
            ..OutputOptions::default()
        },
    )
    .expect_err("a later ambiguous result set must refuse");
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(out.is_empty(), "no earlier result set may leak to stdout");
}

#[test]
fn insert_options_refuse_implicit_or_headerless_destinations() {
    for options in [
        OutputOptions {
            format: Format::Insert,
            ..OutputOptions::default()
        },
        OutputOptions {
            format: Format::Insert,
            header: false,
            insert_table: Some("orders".into()),
            ..OutputOptions::default()
        },
    ] {
        assert_eq!(
            validate_options(&options)
                .expect_err("invalid INSERT options")
                .exit_code(),
            crate::ExitCode::Usage
        );
    }
}

#[test]
fn multiple_result_sets_are_separated_in_every_format_that_allows_them() {
    for format in [Format::Csv, Format::Tsv, Format::Markdown, Format::Table] {
        let text = render(format, &execution(vec![sample(), sample()]));
        assert!(
            text.contains("\n\n"),
            "{format:?} ran two result sets together"
        );
    }
    let text = render(Format::Json, &execution(vec![sample(), sample()]));
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed.as_array().expect("array").len(), 2);
}

#[test]
fn headers_can_be_suppressed_for_piping() {
    let mut out = Vec::new();
    write_execution(
        &mut out,
        &execution(vec![sample()]),
        &OutputOptions {
            format: Format::Csv,
            header: false,
            ..OutputOptions::default()
        },
    )
    .expect("write");
    let text = String::from_utf8(out).expect("utf8");
    assert!(!text.starts_with("id,name"), "{text}");
    assert_eq!(text.lines().count(), 2);
}

#[test]
fn a_streamed_export_matches_what_the_buffered_writer_produces() {
    // The two paths must not drift: a script cannot care whether the rows
    // went through a file or through a pipe.
    for format in [Format::Csv, Format::Tsv, Format::Ndjson, Format::Insert] {
        let mut options = OutputOptions {
            format,
            ..OutputOptions::default()
        };
        if format == Format::Insert {
            options.insert_table = Some("orders".into());
        }
        let buffered = {
            let mut out = Vec::new();
            write_execution(&mut out, &execution(vec![sample()]), &options).expect("write");
            String::from_utf8(out).expect("utf8")
        };

        let streamed = {
            let mut out = Vec::new();
            let mut writer = StreamWriter::new(options.clone()).expect("streamable");
            let set = sample();
            writer
                .columns(&mut out, set.columns.clone())
                .expect("columns");
            for row in &set.rows {
                writer.row(&mut out, row).expect("row");
            }
            assert_eq!(writer.rows(), 2);
            String::from_utf8(out).expect("utf8")
        };

        assert_eq!(
            buffered, streamed,
            "{format:?} differs between the two paths"
        );
    }
}

#[test]
fn result_view_metadata_cannot_change_any_script_output() {
    let plain = sample();
    let mut described = sample();
    assert!(described.set_column_types(vec![Some("int4".into()), Some("text".into()), None,]));

    for format in [
        Format::Table,
        Format::Csv,
        Format::Tsv,
        Format::Json,
        Format::Ndjson,
        Format::Markdown,
    ] {
        assert_eq!(
            render(format, &execution(vec![plain.clone()])),
            render(format, &execution(vec![described.clone()])),
            "the local result view must not alter {format:?} output"
        );
    }
}

#[test]
fn formats_that_cannot_stream_are_refused_rather_than_buffered() {
    for format in [Format::Json, Format::Markdown, Format::Table] {
        let error = StreamWriter::new(OutputOptions {
            format,
            ..OutputOptions::default()
        })
        .expect_err("must refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Usage);
        assert!(
            error.next_action.expect("action").contains("ndjson"),
            "it should name a format that works"
        );
    }
}

#[test]
fn a_streamed_export_can_omit_its_header() {
    let mut out = Vec::new();
    let mut writer = StreamWriter::new(OutputOptions {
        format: Format::Csv,
        header: false,
        ..OutputOptions::default()
    })
    .expect("streamable");
    writer
        .columns(&mut out, vec!["id".into()])
        .expect("columns");
    writer
        .row(&mut out, &[Cell::Text("1".into())])
        .expect("row");
    assert_eq!(String::from_utf8(out).expect("utf8"), "1\n");
}

#[test]
fn a_failed_execution_writes_nothing_to_the_data_stream() {
    // The diagnostic goes to stderr. Anything written here would land in the
    // middle of whatever a script is parsing.
    let execution = Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: Vec::new(),
        status: ExecutionStatus::Failed,
        elapsed: Duration::from_millis(1),
        error: Some(crate::diagnostics::Diagnostic::new(
            crate::diagnostics::DiagnosticKind::Query,
            "relation does not exist",
            "running statement 1",
        )),
        transaction: crate::query::result::TransactionState::Autocommit,
    };
    for format in [Format::Table, Format::Csv, Format::Json, Format::Ndjson] {
        let text = render(format, &execution);
        assert!(
            text.trim().is_empty() || format == Format::Json,
            "{format:?} wrote {text:?} to stdout for a failed query"
        );
    }
}

#[test]
fn a_statement_with_no_rows_reports_what_it_did() {
    let execution = Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![StatementResult {
            result_set: None,
            rows_affected: Some(3),
            elapsed: Duration::from_millis(4),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(5),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
    };
    let text = render(Format::Table, &execution);
    assert!(text.contains("3 rows affected"), "{text}");
}

/// An execution that failed without producing anything, as a query with a
/// bad column name does.
fn failed_execution() -> Execution {
    Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: Vec::new(),
        status: ExecutionStatus::Failed,
        elapsed: Duration::from_millis(1),
        error: Some(crate::diagnostics::Diagnostic::new(
            crate::diagnostics::DiagnosticKind::Query,
            "column \"nope\" does not exist",
            "running statement 1",
        )),
        transaction: crate::query::result::TransactionState::Autocommit,
    }
}

#[test]
fn a_failed_run_writes_no_json_document_at_all() {
    // `[]` on stdout and the reason on stderr reads, to anything parsing
    // stdout alone, as a query that succeeded and matched nothing. The
    // exit code is the contract, but a format that quietly answers the
    // wrong question is the kind of untruthful convenience principle I is
    // about.
    assert_eq!(render(Format::Json, &failed_execution()), "");
    assert_eq!(render(Format::Ndjson, &failed_execution()), "");
    assert_eq!(render(Format::Csv, &failed_execution()), "");
}

#[test]
fn a_run_that_succeeded_and_returned_no_result_sets_still_says_so() {
    // The other half of the distinction: here `[]` is true. Suppressing it
    // would be the same fault in the opposite direction.
    let mut execution = failed_execution();
    execution.error = None;
    execution.status = ExecutionStatus::Succeeded;
    assert_eq!(render(Format::Json, &execution).trim(), "[]");
}
