use super::*;

#[test]
fn density_template_carries_its_comfortable_default() {
    assert!(TEMPLATE.contains("density = \"comfortable\""));
    let config: Config = toml::from_str(TEMPLATE).unwrap();
    assert_eq!(config, Config::default());
}
use crate::config::schema::{ColorDepthChoice, ThemeChoice};

#[test]
fn config_save_preserves_a_stale_temporary_from_an_interrupted_write() {
    let (_dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("config directory");
    let stale = paths.config_dir.join(".config.toml.tmp");
    fs::write(&stale, "earlier state").expect("stale temporary");
    save(&paths, &Config::default()).expect("save current state");
    assert_eq!(fs::read_to_string(stale).unwrap(), "earlier state");
    assert_eq!(load(&paths).unwrap().config, Config::default());
}

#[cfg(unix)]
#[test]
fn config_save_never_follows_a_stale_temporary_symlink() {
    use std::os::unix::fs::symlink;
    let (dir, paths) = temp_paths();
    fs::create_dir_all(&paths.config_dir).expect("config directory");
    let outside = dir.path().join("outside");
    fs::write(&outside, "untouched").expect("outside file");
    let stale = paths.config_dir.join(".config.toml.tmp");
    symlink(&outside, &stale).expect("stale link");
    save(&paths, &Config::default()).expect("save current state");
    assert_eq!(fs::read_to_string(&outside).unwrap(), "untouched");
    assert!(fs::symlink_metadata(stale).unwrap().is_symlink());
    assert_eq!(load(&paths).unwrap().config, Config::default());
}

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

#[cfg(unix)]
#[test]
fn migration_backup_is_private_even_when_the_source_was_world_readable() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, paths) = temp_paths();
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("legacy config");
    fs::set_permissions(&paths.config_file, fs::Permissions::from_mode(0o644))
        .expect("permissive existing source");

    let report = migrate_at(&paths, false, "20261025T090000Z").expect("migration");
    let backup = report.backup.expect("backup");
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    assert_eq!(
        fs::metadata(backup).unwrap().permissions().mode() & 0o777,
        0o600,
        "a migration backup must not inherit permissive source permissions"
    );
}

#[cfg(unix)]
#[test]
fn migration_backup_skips_ordinary_and_dangling_link_collisions() {
    use std::os::unix::fs::symlink;
    let (root, paths) = temp_paths();
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("legacy config");
    let first = paths
        .config_file
        .with_extension("toml.20261025T090000Z.bak");
    let second = paths
        .config_file
        .with_extension("toml.20261025T090000Z-1.bak");
    let expected = paths
        .config_file
        .with_extension("toml.20261025T090000Z-2.bak");
    fs::write(&first, "earlier backup").expect("ordinary collision");
    let outside = root.path().join("not-created");
    symlink(&outside, &second).expect("dangling collision");

    let report = migrate_at(&paths, false, "20261025T090000Z").expect("migration");
    assert_eq!(report.backup.as_deref(), Some(expected.as_path()));
    assert_eq!(fs::read_to_string(first).unwrap(), "earlier backup");
    assert!(fs::symlink_metadata(second).unwrap().is_symlink());
    assert!(!outside.exists(), "a backup followed the dangling link");
    assert_eq!(fs::read_to_string(expected).unwrap(), original);
}

#[test]
fn migration_backup_uses_the_parsed_bytes_and_is_private_before_its_first_write() {
    let (_root, paths) = temp_paths();
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("legacy config");
    let called = std::cell::Cell::new(false);

    let report = migrate_with_backup_writer(&paths, false, "20261025T090000Z", |file, contents| {
        called.set(true);
        assert_eq!(file.metadata()?.len(), 0, "no bytes precede the writer");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(contents, original.as_bytes());
        // Simulate another source write after parsing. The backup must use
        // the parsed snapshot rather than reopening this changed file.
        fs::write(&paths.config_file, "[ui]\ntheme = \"dark\"\n")?;
        file.write_all(contents)
    })
    .expect("migration from the parsed snapshot");

    assert!(
        called.get(),
        "the real backup writer boundary was exercised"
    );
    assert_eq!(
        fs::read_to_string(report.backup.unwrap()).unwrap(),
        original
    );
    assert_eq!(load(&paths).unwrap().config.ui.theme, ThemeChoice::Light);
}

#[test]
fn a_partial_backup_write_failure_keeps_the_config_and_cleans_only_its_reservation() {
    let (_root, paths) = temp_paths();
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("legacy config");
    let previous = paths
        .config_file
        .with_extension("toml.20261025T090000Z.bak");
    let partial = paths
        .config_file
        .with_extension("toml.20261025T090000Z-1.bak");
    fs::write(&previous, "earlier backup").expect("existing reservation");
    let called = std::cell::Cell::new(false);

    let error = migrate_with_backup_writer(&paths, false, "20261025T090000Z", |file, contents| {
        called.set(true);
        file.write_all(&contents[..5])?;
        Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "synthetic partial backup write",
        ))
    })
    .expect_err("an incomplete backup must prevent migration");

    assert!(called.get());
    assert_eq!(
        error.headline,
        "could not write a backup, so nothing was migrated"
    );
    assert_eq!(fs::read_to_string(&paths.config_file).unwrap(), original);
    assert_eq!(fs::read_to_string(previous).unwrap(), "earlier backup");
    assert!(
        fs::symlink_metadata(partial).is_err(),
        "the owned partial backup remains"
    );
    assert_eq!(fs::read_dir(&paths.config_dir).unwrap().count(), 2);
}

#[test]
fn exhausted_backup_reservations_leave_the_config_and_all_existing_entries_unchanged() {
    let (_root, paths) = temp_paths();
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("legacy config");
    let candidate = |index| {
        paths.config_file.with_extension(if index == 0 {
            "toml.20261025T090000Z.bak".to_owned()
        } else {
            format!("toml.20261025T090000Z-{index}.bak")
        })
    };
    for index in 0..128 {
        fs::write(candidate(index), format!("existing backup {index}"))
            .expect("existing reservation");
    }

    let error = migrate_at(&paths, false, "20261025T090000Z")
        .expect_err("candidate reservation is bounded at 128 attempts");
    assert_eq!(
        error.headline,
        "could not write a backup, so nothing was migrated"
    );
    assert!(error.likely_cause.unwrap().contains("128 attempts"));
    assert_eq!(fs::read_to_string(&paths.config_file).unwrap(), original);
    for index in 0..128 {
        assert_eq!(
            fs::read_to_string(candidate(index)).unwrap(),
            format!("existing backup {index}")
        );
    }
    assert!(!candidate(128).exists(), "a 129th candidate was attempted");
    assert_eq!(fs::read_dir(&paths.config_dir).unwrap().count(), 129);
}

#[test]
fn a_completed_backup_remains_available_when_config_replacement_fails() {
    let (_root, paths) = temp_paths();
    let original = "[ui]\ntheme = \"light\"\n";
    fs::write(&paths.config_file, original).expect("legacy config");
    let preserved_source = paths.config_dir.join("preserved-config.toml");
    let backup = paths
        .config_file
        .with_extension("toml.20261025T090000Z.bak");

    let error = migrate_with_backup_writer(&paths, false, "20261025T090000Z", |file, contents| {
        file.write_all(contents)?;
        // A real destination-type conflict makes the later atomic rename
        // fail on every platform, without relying on permission changes.
        fs::rename(&paths.config_file, &preserved_source)?;
        fs::create_dir(&paths.config_file)?;
        fs::write(paths.config_file.join("keep"), "existing destination entry")
    })
    .expect_err("the directory must block configuration replacement");

    assert_eq!(error.headline, "could not write the migrated configuration");
    assert!(
        error
            .next_action
            .unwrap()
            .contains(backup.to_str().unwrap())
    );
    assert_eq!(fs::read_to_string(backup).unwrap(), original);
    assert_eq!(fs::read_to_string(preserved_source).unwrap(), original);
    assert_eq!(
        fs::read_to_string(paths.config_file.join("keep")).unwrap(),
        "existing destination entry"
    );
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
