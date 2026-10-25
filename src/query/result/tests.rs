use super::*;

fn row(value: &str) -> Vec<Cell> {
    vec![Cell::Text(value.to_owned())]
}

#[test]
fn a_result_set_stops_at_its_cap_but_keeps_counting() {
    let mut set = ResultSet::new(vec!["n".into()], 3);
    for i in 0..10 {
        let retained = set.push(row(&i.to_string()));
        assert_eq!(retained, i < 3, "row {i}");
    }
    assert_eq!(set.retained(), 3, "memory stays bounded");
    assert_eq!(set.rows_seen, 10, "the true count is still known");
    assert!(set.is_truncated());
    assert_eq!(set.window_label(), "showing 3 of 10 rows (limit 3 reached)");
}

#[test]
fn an_untruncated_set_says_so_plainly() {
    let mut set = ResultSet::new(vec!["n".into()], 100);
    set.push(row("1"));
    assert!(!set.is_truncated());
    assert_eq!(set.window_label(), "1 row");
    set.push(row("2"));
    assert_eq!(set.window_label(), "2 rows");
}

#[test]
fn an_empty_set_is_not_truncated() {
    let set = ResultSet::new(vec!["n".into()], 10);
    assert!(!set.is_truncated());
    assert_eq!(set.window_label(), "0 rows");
}

#[test]
fn oversized_notice_is_bounded_and_marked() {
    let message = "é".repeat(MAX_NOTICE_MESSAGE_BYTES);
    let bounded = bounded_notice_message(&message);
    assert!(bounded.len() <= MAX_NOTICE_MESSAGE_BYTES);
    assert!(bounded.ends_with(" ... [notice truncated]"));
    assert!(bounded.is_char_boundary(bounded.len()));
}

#[test]
fn summaries_distinguish_rows_returned_from_rows_affected() {
    let mut set = ResultSet::new(vec!["n".into()], 10);
    set.push(row("1"));
    let select = StatementResult {
        result_set: Some(set),
        rows_affected: Some(1),
        elapsed: Duration::from_millis(87),
        notices: Vec::new(),
    };
    assert_eq!(select.summary(), "1 row, 87 ms");

    let update = StatementResult {
        result_set: None,
        rows_affected: Some(4),
        elapsed: Duration::from_millis(12),
        notices: Vec::new(),
    };
    assert_eq!(update.summary(), "4 rows affected, 12 ms");

    let ddl = StatementResult {
        result_set: None,
        rows_affected: None,
        elapsed: Duration::from_millis(3),
        notices: Vec::new(),
    };
    assert_eq!(ddl.summary(), "completed, 3 ms");
}

#[test]
fn status_wording_never_claims_more_than_is_known() {
    assert_eq!(
        ExecutionStatus::ConnectionLost.label(),
        "Connection lost - query outcome unknown"
    );
    assert_eq!(
        ExecutionStatus::Cancelled.label(),
        "Query cancelled by server"
    );
    assert_ne!(
        ExecutionStatus::Failed.label(),
        ExecutionStatus::Cancelled.label()
    );
}

#[test]
fn an_execution_aggregates_across_statements() {
    let mut first = ResultSet::new(vec!["a".into()], 2);
    first.push(row("1"));
    first.push(row("2"));
    first.push(row("3"));
    let mut second = ResultSet::new(vec!["b".into()], 10);
    second.push(row("x"));

    let execution = Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![
            StatementResult {
                result_set: Some(first),
                rows_affected: Some(3),
                elapsed: Duration::from_millis(5),
                notices: Vec::new(),
            },
            StatementResult {
                result_set: Some(second),
                rows_affected: Some(1),
                elapsed: Duration::from_millis(2),
                notices: Vec::new(),
            },
        ],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(7),
        error: None,
        transaction: TransactionState::Autocommit,
    };
    assert_eq!(execution.retained_rows(), 3);
    assert!(execution.any_truncated());
    assert_eq!(execution.job.to_string(), "job-1");
}

#[test]
fn transaction_states_come_from_the_server_and_each_says_what_to_do() {
    assert_eq!(
        TransactionState::from_probe(Some(true)),
        TransactionState::Open
    );
    assert_eq!(
        TransactionState::from_probe(Some(false)),
        TransactionState::Autocommit
    );
    assert_eq!(
        TransactionState::from_probe(None),
        TransactionState::Unknown,
        "an unreadable state is unknown, not assumed to be fine"
    );

    // The failed state is the one people need a way out of, and it names it.
    assert!(
        TransactionState::Failed
            .recovery()
            .expect("a way out")
            .contains("ROLLBACK")
    );
    assert!(TransactionState::Autocommit.recovery().is_none());
    for state in [
        TransactionState::Autocommit,
        TransactionState::Open,
        TransactionState::Failed,
        TransactionState::Unknown,
    ] {
        assert!(!state.label().is_empty());
    }
}

#[test]
fn a_zero_cap_keeps_nothing_but_still_reports_the_count() {
    let mut set = ResultSet::new(vec!["n".into()], 0);
    assert!(!set.push(row("1")));
    assert_eq!(set.retained(), 0);
    assert_eq!(set.rows_seen, 1);
    assert!(set.is_truncated());
}

#[test]
fn type_metadata_is_optional_and_positionally_safe() {
    let mut set = ResultSet::new(vec!["n".into(), "label".into()], 10);
    assert_eq!(set.column_types, vec![None, None]);
    assert!(set.set_column_types(vec![Some("int4".into()), Some("text".into())]));
    assert_eq!(set.column_type(0), Some("int4"));
    assert_eq!(set.column_type(1), Some("text"));

    assert!(!set.set_column_types(vec![Some("wrong".into())]));
    assert_eq!(set.column_types, vec![None, None]);
    assert!(set.column_type(8).is_none());
}

#[test]
fn partial_type_metadata_stays_partial_instead_of_shifting_labels() {
    let set = ResultSet::with_column_types(
        vec!["id".into(), "payload".into(), "created".into()],
        vec![Some("int8".into()), None, Some("timestamptz".into())],
        10,
    );
    assert_eq!(set.column_type(0), Some("int8"));
    assert!(set.column_type(1).is_none());
    assert_eq!(set.column_type(2), Some("timestamptz"));
}

#[test]
fn notice_count_precision_and_schema_do_not_fabricate_exact_deltas() {
    assert_eq!(
        NoticeCount::Exact(u64::MAX).plus(NoticeCount::Exact(1)),
        NoticeCount::AtLeast(u64::MAX)
    );
    assert_eq!(
        NoticeCount::AtLeast(9).since(NoticeCount::Exact(4)),
        NoticeCount::AtLeast(5)
    );
    assert_eq!(
        NoticeCount::AtLeast(9).since(NoticeCount::AtLeast(4)),
        NoticeCount::Unknown
    );
    assert_eq!(
        NoticeCount::Exact(1).since(NoticeCount::Exact(2)),
        NoticeCount::Unknown
    );
    let report = NoticeReport::unavailable(NoticeScope::Query).to_json();
    assert!(report["retained"].is_null());
    assert!(report["omitted"]["count"].is_null());
    assert_eq!(report["omitted"]["precision"], "unknown");
    let counts = NoticeCounts {
        omitted: NoticeCount::AtLeast(3),
        shortened: NoticeCount::Unknown,
        abandoned: NoticeCount::Exact(2),
    };
    assert_eq!(
        counts.lines(),
        [
            "Client omitted at least 3 observed server notices after reaching the notice-retention limit.",
            "Client shortened an unknown number of retained server notice messages.",
            "Client did not return 2 retained server notices because an operation was abandoned."
        ]
    );
}

#[test]
fn notice_admission_length_matches_actual_bytes_at_utf8_cutoffs() {
    for character in ["x", "é", "界", "🦀"] {
        for length in [
            0,
            1,
            MAX_NOTICE_MESSAGE_BYTES - 24,
            MAX_NOTICE_MESSAGE_BYTES - 1,
            MAX_NOTICE_MESSAGE_BYTES,
            MAX_NOTICE_MESSAGE_BYTES + 1,
        ] {
            let message = character.repeat(length);
            let retained = bounded_notice_message(&message);
            assert_eq!(bounded_notice_message_len(&message), retained.len());
            assert!(retained.len() <= MAX_NOTICE_MESSAGE_BYTES);
            if message.len() <= MAX_NOTICE_MESSAGE_BYTES {
                assert_eq!(retained, message);
            } else {
                assert!(retained.ends_with(" ... [notice truncated]"));
            }
        }
    }
}
