use super::*;
use crate::config::schema::{ColorDepthChoice, ThemeChoice};

fn temp_paths() -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = Paths::rooted_at(dir.path());
    (dir, paths)
}

#[test]
fn the_starter_file_is_one_this_build_accepts_and_agrees_with_its_defaults() {
    // The file handed to someone on their first day must be valid for the
    // build that handed it over, and must not describe behaviour the build
    // does not have.
    let parsed: Config = toml::from_str(TEMPLATE).expect("the template parses");
    assert_eq!(
        parsed,
        Config::default(),
        "every value in the starter file is this build's own default"
    );
    assert!(
        parsed.profiles.is_empty() && parsed.keys.is_empty(),
        "the examples are commented out, so a first run connects to nothing by surprise"
    );
    assert!(
        TEMPLATE.contains("No password belongs in this file"),
        "the one rule that matters is in the file itself"
    );
}

#[test]
fn writing_the_starter_file_puts_it_where_config_paths_says() {
    let (_dir, paths) = temp_paths();
    write_template(&paths).expect("write");
    assert!(paths.config_file.exists());
    assert_eq!(load(&paths).expect("load").config, Config::default());
}

#[test]
fn a_missing_file_loads_defaults_rather_than_failing() {
    let (_dir, paths) = temp_paths();
    let loaded = load(&paths).expect("defaults");
    assert_eq!(loaded.source, ConfigSource::Defaults);
    assert_eq!(loaded.config, Config::default());
    assert!(loaded.pending_migration.is_none());
    assert!(
        !paths.config_file.exists(),
        "loading must not create a file"
    );
}

#[test]
fn invalid_syntax_produces_an_actionable_diagnostic() {
    let (_dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("mkdir");
    fs::write(&paths.config_file, "schema_version = \n").expect("write");
    let err = load(&paths).expect_err("must fail");
    assert_eq!(err.kind, DiagnosticKind::Config);
    assert!(
        err.next_action.is_some(),
        "a config error must say what to do next"
    );
    assert!(err.technical.iter().any(|f| f.label == "Path"));
}

#[test]
fn out_of_range_values_are_reported_with_every_issue() {
    let (_dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("mkdir");
    fs::write(
        &paths.config_file,
        "schema_version = 1\n[query]\nmax-buffered-rows = 0\n[connection]\nconnect-timeout-seconds = 0\n",
    )
    .expect("write");
    let err = load(&paths).expect_err("must fail");
    assert!(err.headline.contains("2 setting(s)"), "{}", err.headline);
}

#[test]
fn save_is_atomic_and_leaves_no_temporary_file() {
    let (_dir, paths) = temp_paths();
    let mut config = Config::default();
    config.ui.theme = ThemeChoice::Light;
    save(&paths, &config).expect("save");

    let reloaded = load(&paths).expect("load");
    assert_eq!(reloaded.config.ui.theme, ThemeChoice::Light);
    assert_eq!(reloaded.source, ConfigSource::File);

    let leftovers: Vec<_> = fs::read_dir(&paths.config_dir)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files left behind: {leftovers:?}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&paths.config_file)
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "config must be owner-only");
    }
}

#[test]
fn loading_reports_a_pending_migration_without_rewriting_the_file() {
    let (_dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("mkdir");
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("write");

    let loaded = load(&paths).expect("load");
    assert_eq!(loaded.pending_migration, Some((0, CURRENT_SCHEMA_VERSION)));
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("read"),
        original,
        "loading must never write"
    );
}

#[test]
fn migration_is_idempotent_and_backs_up_before_changing_anything() {
    let (_dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("mkdir");
    fs::write(&paths.config_file, "[ui]\ntheme = \"light\"\n").expect("write");

    let dry = migrate(&paths, true).expect("dry run");
    assert!(!dry.no_op);
    assert!(dry.backup.is_none(), "a dry run writes nothing");
    assert_eq!(dry.to_version, CURRENT_SCHEMA_VERSION);
    assert!(
        fs::read_to_string(&paths.config_file)
            .expect("read")
            .starts_with("[ui]"),
        "dry run must not modify the file"
    );

    let first = migrate(&paths, false).expect("migrate");
    assert_eq!(first.from_version, 0);
    assert_eq!(first.to_version, CURRENT_SCHEMA_VERSION);
    let backup = first.backup.expect("backup written");
    assert!(backup.exists());
    assert_eq!(
        fs::read_to_string(&backup).expect("read"),
        "[ui]\ntheme = \"light\"\n"
    );

    let loaded = load(&paths).expect("load after migration");
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(
        loaded.config.ui.theme,
        ThemeChoice::Light,
        "settings preserved"
    );
    assert!(loaded.pending_migration.is_none());

    let second = migrate(&paths, false).expect("repeat migrate");
    assert!(second.no_op, "running migrate twice must be safe");
    assert!(second.backup.is_none(), "a no-op writes no backup");
}

#[test]
fn a_newer_schema_refuses_to_migrate_downwards() {
    let (_dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("mkdir");
    fs::write(&paths.config_file, "schema_version = 99\n").expect("write");
    let err = migrate(&paths, false).expect_err("must refuse");
    assert!(
        err.headline.contains("newer than this build"),
        "{}",
        err.headline
    );
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("read"),
        "schema_version = 99\n",
        "a refused migration changes nothing"
    );
}

#[test]
fn migrating_with_no_file_is_a_reported_no_op() {
    let (_dir, paths) = temp_paths();
    let report = migrate(&paths, false).expect("no-op");
    assert!(report.no_op);
    assert!(!paths.config_file.exists());
}

#[test]
fn the_template_carries_the_color_depth_key_with_its_default() {
    // A key the schema accepts but the starter file omits is a key nobody
    // discovers. The comparison test above keeps the whole file honest; this
    // one names the key the feature added.
    assert!(
        TEMPLATE.contains("color-depth = \"auto\""),
        "the template must show color-depth with its default"
    );
    let parsed: Config = toml::from_str(TEMPLATE).expect("the template parses");
    assert_eq!(parsed.ui.color_depth, ColorDepthChoice::Auto);
}
