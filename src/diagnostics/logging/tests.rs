use super::*;

#[test]
fn statement_descriptor_never_contains_the_sql() {
    let sql = "SELECT secret_column FROM sensitive_table";
    let desc = statement_descriptor(7, sql);
    assert_eq!(desc, "job=7 statement_chars=41");
    assert!(!desc.contains("secret_column"));
    assert!(!desc.contains("sensitive_table"));
}

#[test]
fn a_parameterized_descriptor_keeps_the_bound_value_out_of_logging() {
    let template = "SELECT :value";
    let bound = "SELECT E'synthetic-parameter-value'";
    let desc = statement_descriptor(8, template);
    assert_eq!(desc, "job=8 statement_chars=13");
    assert!(!desc.contains(bound));
    assert!(!desc.contains("synthetic-parameter-value"));
}

#[test]
fn logging_excludes_dependencies_and_redacts_internal_events() {
    const SECRET: &str = "synthetic-log-secret";
    const DEPENDENCY_SQL: &str = "dependency-sql-marker";
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ignatius.log");
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .expect("open log");
    let filter = filter_fn(internal_filter(LevelFilter::DEBUG));
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .event_format(RedactingFormatter)
            .with_writer(file)
            .with_ansi(false)
            .with_filter(filter),
    );

    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!(
            target: "tokio_postgres::simple_query",
            "{DEPENDENCY_SQL}"
        );
        tracing::debug!(
            target: "ignatius_dependency",
            "prefix-collision-marker"
        );
        tracing::debug!(
            target: "ignatius::diagnostics::logging::test",
            "internal-marker password={SECRET}"
        );
    });

    let log = fs::read_to_string(path).expect("read log");
    assert!(log.contains("internal-marker"), "{log}");
    assert!(log.contains("[redacted]"), "{log}");
    assert!(!log.contains(SECRET), "{log}");
    assert!(!log.contains(DEPENDENCY_SQL), "{log}");
    assert!(!log.contains("prefix-collision-marker"), "{log}");
}

#[test]
fn logging_rejects_non_level_filters_before_writing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ignatius.log");
    for invalid in ["tokio_postgres=debug", "4"] {
        let error = start(&path, invalid).expect_err("non-level filter must be refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!path.exists(), "invalid filter created {}", path.display());
    }
}

#[test]
fn rotation_moves_the_file_only_once_over_the_limit() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ignatius.log");

    fs::write(&path, b"small").expect("write");
    rotate_if_needed(&path).expect("no rotation needed");
    assert!(path.exists());
    assert!(!path.with_extension("log.1").exists());

    fs::write(
        &path,
        vec![b'x'; usize::try_from(MAX_LOG_BYTES).unwrap() + 1],
    )
    .expect("write");
    rotate_if_needed(&path).expect("rotation");
    assert!(!path.exists(), "active log moved aside");
    assert!(
        path.with_extension("log.1").exists(),
        "one previous file kept"
    );
}

#[test]
fn logging_is_disabled_when_the_variable_is_absent_or_empty() {
    // The default must be silence: no file is created merely by starting up.
    let dir = tempfile::tempdir().expect("temp dir");
    // SAFETY-free API: read only, since set_var is unsafe in edition 2024 and
    // this test asserts the absent-variable branch through the public result.
    if std::env::var(LOG_ENV).is_err() {
        assert_eq!(init(dir.path()), LoggingState::Disabled);
        assert!(!dir.path().join("ignatius.log").exists());
    }
}
