//! Reading, validating, migrating and atomically writing configuration.
//!
//! Rules that hold for every write:
//!
//! - Writes go to a temporary file in the same directory, are flushed and synced,
//!   then renamed over the target. A crash mid-write leaves the old file intact.
//! - The file is restricted to its owner where the platform allows it.
//! - A migration takes a timestamped backup before it changes anything.
//! - Nothing is migrated implicitly. Loading reports that a migration is needed;
//!   the user runs `config migrate` when they choose to.

use crate::config::paths::Paths;
use crate::config::schema::{CURRENT_SCHEMA_VERSION, Config};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Where the loaded configuration came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// No file exists; built-in defaults are in use.
    Defaults,
    /// Values were read from `config.toml`.
    File,
}

/// A successfully loaded configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The effective configuration.
    pub config: Config,
    /// Whether it came from a file or from defaults.
    pub source: ConfigSource,
    /// Set when the file's schema is older than this build understands.
    pub pending_migration: Option<(u32, u32)>,
}

/// Loads configuration, or returns a diagnostic explaining exactly what is wrong.
///
/// A missing file is not an error: the product works on first run with no
/// configuration at all.
pub fn load(paths: &Paths) -> Result<Loaded, Diagnostic> {
    let path = &paths.config_file;
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Loaded {
                config: Config::default(),
                source: ConfigSource::Defaults,
                pending_migration: None,
            });
        }
        Err(err) => {
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                format!("could not read {}", path.display()),
                "loading configuration",
            )
            .likely_cause(err.to_string())
            .next_action(format!(
                "check the file's permissions, or move it aside to start from defaults: {}",
                path.display()
            ))
            .technical("Path", path.display().to_string()));
        }
    };

    let config: Config = toml::from_str(&text).map_err(|err| {
        let mut diagnostic = Diagnostic::new(
            DiagnosticKind::Config,
            "configuration file is not valid",
            "parsing configuration",
        )
        .likely_cause(err.message().to_owned())
        .next_action(format!(
            "fix the file and re-run `{} config validate`, or move it aside to start from defaults",
            crate::branding::BINARY_NAME
        ))
        .technical("Path", path.display().to_string());
        if let Some(span) = err.span() {
            let line = text[..span.start.min(text.len())].lines().count();
            diagnostic = diagnostic.technical("Line", line.to_string());
        }
        diagnostic
    })?;

    let pending_migration = (config.schema_version < CURRENT_SCHEMA_VERSION)
        .then_some((config.schema_version, CURRENT_SCHEMA_VERSION));

    let issues = config.validate();
    if !issues.is_empty() {
        let mut diagnostic = Diagnostic::new(
            DiagnosticKind::Config,
            format!("{} setting(s) are out of range", issues.len()),
            "validating configuration",
        )
        .likely_cause(format!("{}: {}", issues[0].path, issues[0].message))
        .next_action(issues[0].suggestion.clone())
        .technical("Path", path.display().to_string());
        for issue in &issues {
            diagnostic = diagnostic.technical(issue.path.clone(), issue.message.clone());
        }
        return Err(diagnostic);
    }

    Ok(Loaded {
        config,
        source: ConfigSource::File,
        pending_migration,
    })
}

/// What a migration did, or would do under `--dry-run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    /// Schema version found in the file.
    pub from_version: u32,
    /// Schema version after migration.
    pub to_version: u32,
    /// Human-readable steps applied, in order.
    pub steps: Vec<String>,
    /// Backup written before the change, when one was needed.
    pub backup: Option<std::path::PathBuf>,
    /// True when nothing was written because nothing needed to change.
    pub no_op: bool,
    /// True when this was a dry run.
    pub dry_run: bool,
}

/// Migrates the configuration file to the current schema version.
///
/// Safe to run repeatedly: an already-current file is reported as a no-op and
/// nothing is written.
pub fn migrate(paths: &Paths, dry_run: bool) -> Result<MigrationReport, Diagnostic> {
    let path = &paths.config_file;
    if !path.exists() {
        return Ok(MigrationReport {
            from_version: CURRENT_SCHEMA_VERSION,
            to_version: CURRENT_SCHEMA_VERSION,
            steps: vec!["no configuration file exists; defaults are already current".into()],
            backup: None,
            no_op: true,
            dry_run,
        });
    }

    let loaded_text = fs::read_to_string(path).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Config,
            format!("could not read {}", path.display()),
            "migrating configuration",
        )
        .likely_cause(err.to_string())
        .next_action("check the file's permissions")
    })?;

    let mut config: Config = toml::from_str(&loaded_text).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Config,
            "configuration file is not valid, so it cannot be migrated",
            "migrating configuration",
        )
        .likely_cause(err.message().to_owned())
        .next_action(format!(
            "fix the syntax first: `{} config validate`",
            crate::branding::BINARY_NAME
        ))
    })?;

    let from_version = config.schema_version;
    if from_version == CURRENT_SCHEMA_VERSION {
        return Ok(MigrationReport {
            from_version,
            to_version: CURRENT_SCHEMA_VERSION,
            steps: vec![format!(
                "already at schema version {CURRENT_SCHEMA_VERSION}"
            )],
            backup: None,
            no_op: true,
            dry_run,
        });
    }
    if from_version > CURRENT_SCHEMA_VERSION {
        return Err(Diagnostic::new(
            DiagnosticKind::Config,
            format!(
                "file uses schema version {from_version}, newer than this build understands ({CURRENT_SCHEMA_VERSION})"
            ),
            "migrating configuration",
        )
        .likely_cause("the file was written by a newer version of the product")
        .next_action("upgrade Ignatius; downgrading a configuration file is not supported")
        .technical("Path", path.display().to_string()));
    }

    let mut steps = Vec::new();
    // Step 0 -> 1: a file written by hand has no schema_version. Stamping it makes
    // future migrations possible without guessing.
    if config.schema_version == 0 {
        config.schema_version = 1;
        steps.push("stamped schema-version = 1 on a file that had none".into());
    }

    if dry_run {
        return Ok(MigrationReport {
            from_version,
            to_version: config.schema_version,
            steps,
            backup: None,
            no_op: false,
            dry_run,
        });
    }

    let backup = backup_path(path);
    fs::copy(path, &backup).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Config,
            "could not write a backup, so nothing was migrated",
            "backing up configuration before migration",
        )
        .likely_cause(err.to_string())
        .next_action("free space or fix permissions in the configuration directory, then retry")
        .technical("Backup", backup.display().to_string())
    })?;

    let rendered = toml::to_string_pretty(&config).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            "could not render the migrated configuration",
            "migrating configuration",
        )
        .likely_cause(err.to_string())
        .next_action("report this with the backup file's contents")
    })?;

    write_atomic(path, &rendered).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Config,
            "could not write the migrated configuration",
            "migrating configuration",
        )
        .likely_cause(err.to_string())
        .next_action(format!(
            "the previous file is intact at {}; restore it if needed",
            backup.display()
        ))
    })?;

    Ok(MigrationReport {
        from_version,
        to_version: config.schema_version,
        steps,
        backup: Some(backup),
        no_op: false,
        dry_run,
    })
}

/// Writes the configuration file atomically, creating the directory if needed.
pub fn save(paths: &Paths, config: &Config) -> Result<(), Diagnostic> {
    let rendered = toml::to_string_pretty(config).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Internal,
            "could not render configuration",
            "saving configuration",
        )
        .likely_cause(err.to_string())
    })?;
    write_atomic(&paths.config_file, &rendered).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Config,
            format!("could not write {}", paths.config_file.display()),
            "saving configuration",
        )
        .likely_cause(err.to_string())
        .next_action("check permissions and free space in the configuration directory")
    })
}

/// Writes `contents` to `path` atomically: temp file, flush, sync, rename.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let _ = crate::platform::restrict_to_owner(dir);

    let temp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config")
    ));
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        // Durability before the rename, so a crash cannot leave a truncated file.
        file.sync_all()?;
    }
    crate::platform::restrict_to_owner(&temp)?;
    fs::rename(&temp, path)?;
    Ok(())
}

/// Builds a timestamped backup path that never overwrites an existing backup.
fn backup_path(path: &Path) -> std::path::PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut candidate = path.with_extension(format!("toml.{stamp}.bak"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = path.with_extension(format!("toml.{stamp}-{counter}.bak"));
        counter += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ThemeChoice;

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = Paths::rooted_at(dir.path());
        (dir, paths)
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
}
