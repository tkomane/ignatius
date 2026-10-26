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
