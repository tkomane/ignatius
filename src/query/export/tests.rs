use super::*;

#[test]
fn w10_injected_abandon_flush_failure_does_not_claim_rows_are_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("failed.csv");
    let mut export = Export::create(&destination, false).unwrap();
    export
        .writer()
        .write_all(b"value\n\"synthetic\nrow\"\n")
        .unwrap();
    export.count_row();
    let calls = std::cell::Cell::new(0);
    let abandoned = export.abandon_with(|_| {
        calls.set(calls.get() + 1);
        Err(std::io::Error::other("synthetic flush failure"))
    });
    assert_eq!(calls.get(), 1);
    assert_eq!(abandoned.rows, 1);
    assert_eq!(abandoned.flush, PartialFlush::Unconfirmed);
    assert!(abandoned.partial.exists() && !destination.exists());
    assert_eq!(
        std::fs::read(&abandoned.partial).unwrap(),
        b"value\n\"synthetic\nrow\"\n",
        "existing best-effort close recovery is preserved, without upgrading confirmation"
    );
    assert_eq!(
        abandoned.message(),
        format!(
            "Export incomplete - 1 complete row(s) accepted by the export writer; partial-file row count is unconfirmed because flushing failed: {}",
            abandoned.partial.display()
        )
    );
    assert!(!abandoned.message().contains("synthetic"));
}

#[test]
fn w10_successful_abandon_flush_preserves_existing_count_message_and_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("flushed.csv");
    let mut export = Export::create(&destination, false).unwrap();
    let bytes = b"value\n\"synthetic\nrow\"\n";
    export.writer().write_all(bytes).unwrap();
    export.count_row();
    let abandoned = export.abandon_with(|writer| writer.flush());
    assert_eq!(abandoned.rows, 1);
    assert_eq!(abandoned.flush, PartialFlush::Flushed);
    assert_eq!(std::fs::read(&abandoned.partial).unwrap(), bytes);
    assert_eq!(
        abandoned.message(),
        format!(
            "Export incomplete - 1 row(s) written to {}",
            abandoned.partial.display()
        )
    );
    assert!(!destination.exists());
}

#[test]
fn w10_injected_finish_flush_failure_keeps_neutral_partial_recovery_guidance() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("finish.csv");
    let mut export = Export::create(&destination, false).unwrap();
    export.writer().write_all(b"value\nrow\n").unwrap();
    export.count_row();
    let partial = export.partial_path().to_path_buf();
    let error = export
        .finish_with(|_| Err(std::io::Error::other("synthetic flush failure")))
        .unwrap_err();
    assert_eq!(error.exit_code(), crate::ExitCode::ExportInterrupted);
    assert!(partial.exists() && !destination.exists());
    assert_eq!(
        std::fs::read(&partial).unwrap(),
        b"value\nrow\n",
        "existing best-effort close preserves buffered recovery data"
    );
    let action = error.next_action.unwrap();
    assert!(action.contains(&partial.display().to_string()));
    assert!(
        !action.contains("rows written"),
        "flush failed, so the disk row count is unconfirmed: {action}"
    );
}

#[test]
fn a_partial_file_is_named_after_its_destination() {
    assert_eq!(
        partial_path(Path::new("/tmp/report.csv")),
        PathBuf::from("/tmp/report.csv.partial")
    );
    // The extension is added, not replaced, so the intended name is visible.
    assert_eq!(
        partial_path(Path::new("report")),
        PathBuf::from("report.partial")
    );
}

#[test]
fn rows_land_in_the_partial_file_and_only_move_on_completion() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("report.csv");

    let mut export = Export::create(&destination, false).expect("create");
    write!(export.writer(), "id\n1\n").expect("write");
    export.count_row();

    assert!(
        !destination.exists(),
        "nothing appears at the destination until it is complete"
    );
    assert!(
        export.partial_path().exists(),
        "the work is in the partial file"
    );

    let finished = export.finish().expect("finish");
    assert_eq!(finished.rows, 1);
    assert_eq!(finished.path, destination);
    assert!(destination.exists());
    assert!(
        !partial_path(&destination).exists(),
        "the partial file is gone once it is complete"
    );
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        "id\n1\n"
    );
}

#[test]
fn abandoning_keeps_the_work_and_says_where_it_is() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("report.csv");

    let mut export = Export::create(&destination, false).expect("create");
    for row in 0..3 {
        writeln!(export.writer(), "{row}").expect("write");
        export.count_row();
    }
    let abandoned = export.abandon();

    assert_eq!(abandoned.rows, 3);
    assert!(abandoned.partial.exists(), "the rows written are kept");
    assert!(
        !destination.exists(),
        "an incomplete export never occupies the destination"
    );
    let message = abandoned.message();
    assert!(message.contains("Export incomplete"), "{message}");
    assert!(
        message.contains("3 row(s)"),
        "the count is stated: {message}"
    );
    assert!(
        message.contains("report.csv.partial"),
        "and the path: {message}"
    );
}

#[test]
fn the_way_out_of_a_refusal_is_the_callers_to_word() {
    // A flag is the answer in a script and no answer at all to someone in
    // the full-screen client, who has no flags to pass.
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("report.csv");
    std::fs::write(&destination, "precious data").expect("write");

    let error = Export::create_with_advice(&destination, false, "choose another name")
        .expect_err("must refuse");
    let action = error.next_action.expect("an action");
    assert_eq!(action, "choose another name");
    assert!(!action.contains("--force"));
}

#[test]
fn an_existing_file_is_never_replaced_by_accident() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("report.csv");
    std::fs::write(&destination, "precious data").expect("write");

    let error = Export::create(&destination, false).expect_err("must refuse");
    assert_eq!(error.exit_code(), crate::ExitCode::Usage);
    assert!(error.next_action.expect("action").contains("--force"));
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        "precious data",
        "the existing file is untouched"
    );
}

#[test]
fn a_stale_partial_is_refused_in_both_force_modes_without_changing_bytes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("stale.csv");
    let partial = partial_path(&destination);
    let bytes = b"recovery data\n";
    std::fs::write(&partial, bytes).expect("write stale partial");

    for force in [false, true] {
        let error = Export::create(&destination, force).expect_err("stale partial must refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Usage);
        assert!(
            error.headline.contains(&partial.display().to_string()),
            "{}",
            error.headline
        );
        let action = error.next_action.expect("recovery hint");
        let lower = action.to_ascii_lowercase();
        assert!(lower.contains("inspect"), "{action}");
        assert!(
            lower.contains("move") || lower.contains("choose"),
            "{action}"
        );
        assert_eq!(std::fs::read(&partial).expect("read stale partial"), bytes);
        assert!(!destination.exists());
    }
}

#[test]
fn an_active_second_export_is_refused_in_both_force_modes_and_the_first_finishes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("active.csv");
    let bytes = b"id\n1\n";
    let mut first = Export::create(&destination, false).expect("create first export");
    first.writer().write_all(bytes).expect("write first export");
    first.writer().flush().expect("flush first export");
    assert_eq!(
        std::fs::read(first.partial_path()).expect("read active partial"),
        bytes
    );

    for force in [false, true] {
        let error = Export::create(&destination, force).expect_err("active export must refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Usage);
        assert!(
            error
                .headline
                .contains(&first.partial_path().display().to_string()),
            "{}",
            error.headline
        );
        let action = error.next_action.expect("recovery hint");
        let lower = action.to_ascii_lowercase();
        assert!(lower.contains("inspect"), "{action}");
        assert!(
            lower.contains("move") || lower.contains("choose"),
            "{action}"
        );
        assert_eq!(
            std::fs::read(first.partial_path()).expect("read preserved active partial"),
            bytes
        );
    }

    first.finish().expect("first export finishes");
    assert_eq!(
        std::fs::read(&destination).expect("read destination"),
        bytes
    );
}

#[test]
fn an_independent_final_keeps_its_bytes_when_finish_is_interrupted() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("racing.csv");
    let partial_bytes = b"id\n1\n";
    let final_bytes = b"independent final\n";
    let mut export = Export::create(&destination, false).expect("create export");
    export
        .writer()
        .write_all(partial_bytes)
        .expect("write export");
    export.writer().flush().expect("flush partial");
    let partial = export.partial_path().to_path_buf();
    std::fs::write(&destination, final_bytes).expect("create independent final");

    let error = export
        .finish()
        .expect_err("existing final must interrupt finish");
    assert_eq!(error.exit_code(), crate::ExitCode::ExportInterrupted);
    assert_eq!(
        std::fs::read(&partial).expect("read partial"),
        partial_bytes
    );
    assert_eq!(
        std::fs::read(&destination).expect("read final"),
        final_bytes
    );
}

#[test]
fn forced_abandon_flush_failure_keeps_prior_final_and_preserves_partial() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("abandoned.csv");
    let final_bytes = b"prior final\n";
    let partial_bytes = b"new partial\n";
    std::fs::write(&destination, final_bytes).expect("write prior final");
    let mut export = Export::create(&destination, true).expect("create forced export");
    export
        .writer()
        .write_all(partial_bytes)
        .expect("write partial");
    let partial = export.partial_path().to_path_buf();

    let write_error = export.write_failure(&io::Error::other("synthetic write failure"));
    assert_eq!(write_error.exit_code(), crate::ExitCode::ExportInterrupted);
    let action = write_error.next_action.expect("recovery guidance");
    assert!(action.contains(&partial.display().to_string()), "{action}");
    assert!(!action.contains("rows written"), "{action}");

    let abandoned = export.abandon_with(|_| Err(std::io::Error::other("synthetic flush failure")));
    assert_eq!(abandoned.flush, PartialFlush::Unconfirmed);
    assert_eq!(
        std::fs::read(&partial).expect("read partial"),
        partial_bytes
    );
    assert_eq!(
        std::fs::read(&destination).expect("read final"),
        final_bytes
    );
}

#[cfg(windows)]
#[test]
fn a_long_windows_path_publishes_exact_bytes_without_a_partial() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut parent = root.path().to_path_buf();
    for index in 0..12 {
        parent = parent.join(format!("nested-directory-{index:02}-ordinary-name"));
        std::fs::create_dir_all(&parent).expect("create nested directory");
    }
    let destination = parent.join("long-path-export.csv");
    assert!(destination.to_string_lossy().len() > 260);
    let bytes = b"id\n1\n";

    let mut export = Export::create(&destination, false).expect("create long-path export");
    export
        .writer()
        .write_all(bytes)
        .expect("write long-path export");
    export.finish().expect("publish long-path export");

    assert_eq!(
        std::fs::read(&destination).expect("read long-path export"),
        bytes
    );
    assert!(!partial_path(&destination).exists());
}

#[test]
fn force_replaces_the_file_but_only_once_the_new_one_is_complete() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("report.csv");
    std::fs::write(&destination, "old data").expect("write");

    let mut export = Export::create(&destination, true).expect("create with force");
    write!(export.writer(), "new data").expect("write");
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        "old data",
        "the old file survives until the new one is finished"
    );

    export.finish().expect("finish");
    assert_eq!(
        std::fs::read_to_string(&destination).expect("read"),
        "new data"
    );
}

#[test]
fn a_missing_parent_directory_is_created() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("nested").join("deeper").join("report.csv");
    let export = Export::create(&destination, false).expect("create");
    export.finish().expect("finish");
    assert!(destination.exists());
}

#[test]
fn an_unwritable_destination_fails_with_an_action() {
    // A path whose parent is a file, not a directory. Every platform
    // refuses that, which is what makes it a portable test.
    let dir = tempfile::tempdir().expect("temp dir");
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, "in the way").expect("write");

    let error = Export::create(&blocker.join("report.csv"), false).expect_err("must fail");
    assert!(error.next_action.is_some(), "a failure must say what to do");
    assert_eq!(error.exit_code(), crate::ExitCode::Usage);
}
