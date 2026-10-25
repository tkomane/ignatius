use super::*;

#[test]
fn dropped_operation_preserves_pending_and_reports_lost_prefix_once() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let operation = notices.begin(NoticeScope::Query);
    notices.retain(&privacy, "NOTICE", "synthetic prefix", "00000");
    assert_eq!(operation.take_statement().len(), 1);
    notices.retain(&privacy, "NOTICE", "synthetic pending", "00000");
    drop(operation);
    let report = notices
        .take_idle()
        .expect("drop releases operation and retains pending payload");
    assert_eq!(report.unattributed[0].message, "synthetic pending");
    assert_eq!(report.counts.abandoned, NoticeCount::Exact(1));
    let checkpoint = notices.checkpoint();
    assert_eq!(checkpoint.totals.abandoned, NoticeCount::Exact(1));
    assert!(notices.take_idle().is_none());
    let next = notices.begin(NoticeScope::Query).finish();
    assert!(!next.counts.needs_report());
    assert_eq!(next.checkpoint.totals, checkpoint.totals);
    assert!(next.checkpoint.revision > checkpoint.revision);
}

#[test]
fn statement_drains_and_idle_adoption_never_restore_active_allowance() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    notices.retain(&privacy, "NOTICE", "synthetic idle", "00000");
    let operation = notices.begin(NoticeScope::Query);
    for index in 0..255 {
        notices.retain(&privacy, "NOTICE", &index.to_string(), "00000");
        assert_eq!(operation.take_statement().len(), 1);
        assert!(
            notices.take_idle().is_none(),
            "public drains cannot evade active spending"
        );
    }
    notices.retain(&privacy, "WARNING", "synthetic overflow", "01000");
    assert!(operation.take_statement().is_empty());
    let report = operation.finish();
    assert_eq!(report.retained, Some(256));
    assert_eq!(report.unattributed.len(), 1);
    assert_eq!(report.unattributed[0].message, "synthetic idle");
    assert_eq!(report.counts.omitted, NoticeCount::Exact(1));
    assert_eq!(report.checkpoint.totals.omitted, NoticeCount::Exact(1));
    assert!(notices.take_idle().is_none());
    let next = notices.begin(NoticeScope::Query);
    notices.retain(&privacy, "NOTICE", "new operation", "00000");
    assert_eq!(next.take_statement().len(), 1);
    assert_eq!(next.finish().retained, Some(1));
}

#[test]
fn field_bytes_utf8_shortening_and_ordered_prefix_are_independent_bounds() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let operation = notices.begin(NoticeScope::Query);
    let message = "界".repeat(30_000);
    for _ in 0..17 {
        notices.retain(&privacy, "NOTICE", &message, "00000");
    }
    notices.retain(
        &privacy,
        "NOTICE",
        "small later value must stay omitted",
        "00000",
    );
    let report = operation.finish();
    assert_eq!(report.retained, Some(15));
    assert_eq!(report.counts.omitted, NoticeCount::Exact(3));
    assert_eq!(report.counts.shortened, NoticeCount::Exact(15));
    assert!(
        report
            .unattributed
            .iter()
            .all(|n| n.message.ends_with("[notice truncated]")
                && n.message.len() <= MAX_NOTICE_MESSAGE_BYTES)
    );
    assert!(report.unattributed.iter().map(Bucket::bytes).sum::<usize>() <= MAX_NOTICE_TEXT_BYTES);
}

#[test]
fn oversized_severity_or_code_rejects_the_prefix_before_owned_copies() {
    for severity in [true, false] {
        let notices = Notices::default();
        let privacy = AtomicBool::new(false);
        let huge = "x".repeat(MAX_NOTICE_TEXT_BYTES + 1);
        notices.retain(
            &privacy,
            if severity { &huge } else { "NOTICE" },
            "small",
            if severity { "00000" } else { &huge },
        );
        notices.retain(&privacy, "NOTICE", "later small message", "00000");
        let report = notices.take_idle().unwrap();
        assert_eq!(report.retained, Some(0));
        assert!(report.unattributed.is_empty());
        assert_eq!(report.counts.omitted, NoticeCount::Exact(2));
    }
}

#[test]
fn parameter_entry_rewrites_backlog_before_charging_and_stays_private() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    notices.retain(&privacy, "secret severity", &"s".repeat(70_000), "bad-code");
    notices.activate_privacy(&privacy);
    let operation = notices.begin(NoticeScope::Export);
    notices.retain(
        &privacy,
        "transformed secret",
        &"p".repeat(2 * 1024 * 1024),
        "00000",
    );
    let report = operation.finish();
    assert_eq!(report.counts.shortened, NoticeCount::Exact(0));
    assert_eq!(report.retained, Some(2));
    assert!(privacy.load(Ordering::SeqCst));
    for notice in &report.unattributed {
        assert_eq!(notice.severity, "NOTICE");
        assert_eq!(
            notice.message,
            super::super::session::PRIVATE_NOTICE_MESSAGE
        );
    }
    assert_eq!(report.unattributed[0].code, None);
    let next = notices.begin(NoticeScope::Query);
    notices.retain(&privacy, "new secret", "secret notice after drain", "00000");
    assert_eq!(
        next.finish().unattributed[0].message,
        super::super::session::PRIVATE_NOTICE_MESSAGE
    );
}

#[test]
fn empty_messages_count_and_exact_field_byte_boundary_is_admitted() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    for _ in 0..257 {
        notices.retain(&privacy, "", "", "");
    }
    let report = notices.take_idle().unwrap();
    assert_eq!(report.retained, Some(256));
    assert_eq!(report.counts.omitted, NoticeCount::Exact(1));
    let notices = Notices::default();
    let severity = "x".repeat(MAX_NOTICE_TEXT_BYTES - 1);
    notices.retain(&privacy, &severity, "", "x");
    let report = notices.take_idle().unwrap();
    assert_eq!(report.retained, Some(1));
    assert!(!report.counts.needs_report());
}

#[test]
fn saturation_is_a_lower_bound_and_poison_is_unknown_never_empty() {
    let notices = Notices::default();
    {
        let mut state = notices.state.lock().unwrap();
        state.checkpoint.revision = u64::MAX;
        state.checkpoint.totals.omitted = NoticeCount::Exact(u64::MAX);
    }
    let privacy = AtomicBool::new(false);
    for _ in 0..257 {
        notices.retain(&privacy, "", "", "");
    }
    let report = notices.take_idle().unwrap();
    assert_eq!(report.checkpoint.revision, u64::MAX);
    assert_eq!(
        report.checkpoint.totals.omitted,
        NoticeCount::AtLeast(u64::MAX)
    );
    let notices = std::sync::Arc::new(Notices::default());
    let child = std::sync::Arc::clone(&notices);
    assert!(
        std::thread::spawn(move || {
            let _held = child.state.lock().unwrap();
            panic!("synthetic accounting poison");
        })
        .join()
        .is_err()
    );
    assert_eq!(notices.checkpoint(), NoticeCheckpoint::UNKNOWN);
    let report = notices.take_idle().unwrap();
    assert_eq!(report.retained, None);
    assert_eq!(report.counts, NoticeCounts::UNKNOWN);
    notices.activate_privacy(&privacy);
    assert!(privacy.load(Ordering::SeqCst));
    assert_eq!(notices.begin(NoticeScope::Query).finish().retained, None);
}

#[test]
fn pending_shortening_waits_for_transfer_and_private_rewrite_suppresses_it() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let operation = notices.begin(NoticeScope::Query);
    notices.retain(&privacy, "NOTICE", &"x".repeat(70_000), "00000");
    drop(operation);
    assert_eq!(
        notices.checkpoint().totals.shortened,
        NoticeCount::Exact(0),
        "pending payload has not been transferred"
    );
    notices.activate_privacy(&privacy);
    let report = notices.begin(NoticeScope::Query).finish();
    assert_eq!(report.unattributed.len(), 1);
    assert_eq!(report.counts.shortened, NoticeCount::Exact(0));
    assert_eq!(report.checkpoint.totals.shortened, NoticeCount::Exact(0));
}

#[test]
fn dropped_ordinary_pending_shortening_is_committed_once_on_actual_transfer() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let operation = notices.begin(NoticeScope::Query);
    // A server-forged suffix is not provenance for shortening.
    notices.retain(&privacy, "NOTICE", "short ... [notice truncated]", "00000");
    notices.retain(&privacy, "NOTICE", &"x".repeat(70_000), "00000");
    drop(operation);
    assert_eq!(notices.checkpoint().totals.shortened, NoticeCount::Exact(0));
    let report = notices.take_idle().unwrap();
    assert_eq!(report.unattributed.len(), 2);
    assert_eq!(report.counts.shortened, NoticeCount::Exact(1));
    assert_eq!(report.checkpoint.totals.shortened, NoticeCount::Exact(1));
    assert!(notices.take_idle().is_none());
}

#[test]
fn probe_query_describe_and_final_probe_have_distinct_in_memory_transfers() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    notices.retain(&privacy, "NOTICE", "idle prelude", "00000");
    let operation = notices.begin(NoticeScope::Query);
    notices.retain(&privacy, "NOTICE", "semantics probe", "00000");
    operation.hold_unattributed();
    notices.retain(&privacy, "NOTICE", "actual query phase", "00000");
    let statement = operation.take_statement();
    assert_eq!(
        statement
            .iter()
            .map(|n| n.message.as_str())
            .collect::<Vec<_>>(),
        ["actual query phase"]
    );
    notices.retain(&privacy, "NOTICE", "prepare describe", "00000");
    operation.hold_unattributed();
    notices.retain(&privacy, "NOTICE", "final transaction probe", "00000");
    let report = operation.finish();
    assert_eq!(report.retained, Some(5));
    assert_eq!(
        report
            .unattributed
            .iter()
            .map(|n| n.message.as_str())
            .collect::<Vec<_>>(),
        [
            "idle prelude",
            "semantics probe",
            "prepare describe",
            "final transaction probe"
        ]
    );
}

#[test]
fn dropped_shortened_prefix_commits_once_but_pending_private_text_does_not() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let operation = notices.begin(NoticeScope::Query);
    notices.retain(&privacy, "NOTICE", &"x".repeat(70_000), "00000");
    assert_eq!(operation.take_statement().len(), 1);
    notices.retain(&privacy, "NOTICE", &"y".repeat(70_000), "00000");
    drop(operation);
    assert_eq!(notices.checkpoint().totals.shortened, NoticeCount::Exact(1));
    notices.activate_privacy(&privacy);
    let report = notices.begin(NoticeScope::Query).finish();
    assert_eq!(report.counts.shortened, NoticeCount::Exact(1));
    assert_eq!(report.counts.abandoned, NoticeCount::Exact(1));
    assert_eq!(report.checkpoint.totals.shortened, NoticeCount::Exact(1));
    assert_eq!(report.unattributed.len(), 1);
}

#[test]
fn retirement_snapshot_includes_uncommitted_active_and_idle_without_resetting() {
    let notices = Notices::default();
    let privacy = AtomicBool::new(false);
    let operation = notices.begin(NoticeScope::Query);
    for _ in 0..257 {
        notices.retain(&privacy, "NOTICE", "synthetic", "00000");
    }
    assert_eq!(notices.checkpoint().totals.omitted, NoticeCount::Exact(0));
    assert_eq!(notices.retirement_counts().omitted, NoticeCount::Exact(1));
    assert_eq!(notices.retirement_counts().omitted, NoticeCount::Exact(1));
    assert_eq!(
        notices.checkpoint().totals.omitted,
        NoticeCount::Exact(0),
        "snapshot does not commit"
    );
    notices.retain(&privacy, "NOTICE", "still exhausted", "00000");
    assert_eq!(operation.finish().counts.omitted, NoticeCount::Exact(2));
    for _ in 0..257 {
        notices.retain(&privacy, "NOTICE", "idle", "00000");
    }
    assert_eq!(notices.retirement_counts().omitted, NoticeCount::Exact(3));
    assert_eq!(notices.checkpoint().totals.omitted, NoticeCount::Exact(2));
    assert_eq!(
        notices.take_idle().unwrap().counts.omitted,
        NoticeCount::Exact(1)
    );
    assert_eq!(notices.retirement_counts().omitted, NoticeCount::Exact(3));
}

#[test]
fn notice_peer_zero_loss_windows_have_distinct_order_without_empty_idle_churn() {
    let notices = Notices::default();
    let first = notices.begin(NoticeScope::Query).finish();
    let second = notices.begin(NoticeScope::Plan).finish();
    assert!(
        second.checkpoint.revision > first.checkpoint.revision,
        "latest zero-loss windows still need monotonic ordering"
    );
    assert!(!second.counts.needs_report());
    let checkpoint = notices.checkpoint();
    assert!(notices.take_idle().is_none());
    assert_eq!(notices.checkpoint(), checkpoint);
}
