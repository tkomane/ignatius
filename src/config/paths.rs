//! Platform-aware locations for configuration and local state.
//!
//! Resolution order, highest first:
//!
//! 1. `IGNATIUS_CONFIG_DIR` / `IGNATIUS_DATA_DIR` - explicit override, also what
//!    the test suite uses so tests never touch a real user profile.
//! 2. XDG variables on Unix (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`).
//! 3. Platform defaults: `~/.config/ignatius` and `~/.local/share/ignatius` on
//!    Unix, `%APPDATA%\ignatius` and `%LOCALAPPDATA%\ignatius` on Windows.
//!
//! A terminal-first tool belongs where terminal users look for it, which on macOS
//! is `~/.config` rather than `~/Library/Application Support`. This is recorded in
//! ADR-0006 and shown by `ignatius config paths`.

use crate::branding::CONFIG_DIR_NAME;
use std::path::{Path, PathBuf};

/// Environment variable overriding the configuration directory.
pub const CONFIG_DIR_ENV: &str = "IGNATIUS_CONFIG_DIR";
/// Environment variable overriding the data directory.
pub const DATA_DIR_ENV: &str = "IGNATIUS_DATA_DIR";

/// Every location the product reads or writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Directory holding `config.toml`.
    pub config_dir: PathBuf,
    /// The non-secret configuration file.
    pub config_file: PathBuf,
    /// Directory holding local state.
    pub data_dir: PathBuf,
    /// Directory holding opt-in logs.
    pub log_dir: PathBuf,
    /// Directory holding saved queries as ordinary `.sql` files.
    pub queries_dir: PathBuf,
}

impl Paths {
    /// Resolves every path from the environment.
    #[must_use]
    pub fn resolve() -> Self {
        let config_dir = resolve_config_dir();
        let data_dir = resolve_data_dir();
        Self {
            config_file: config_dir.join("config.toml"),
            queries_dir: config_dir.join("queries"),
            log_dir: data_dir.join("logs"),
            config_dir,
            data_dir,
        }
    }

    /// Builds paths rooted at an explicit directory. Used by tests and by
    /// `--config-dir`, so no code path needs to mutate process environment.
    #[must_use]
    pub fn rooted_at(root: &Path) -> Self {
        Self {
            config_dir: root.to_path_buf(),
            config_file: root.join("config.toml"),
            queries_dir: root.join("queries"),
            data_dir: root.join("data"),
            log_dir: root.join("data").join("logs"),
        }
    }

    /// Labelled list for `config paths`, in a stable order.
    #[must_use]
    pub fn labelled(&self) -> Vec<(&'static str, &Path)> {
        vec![
            ("Configuration directory", self.config_dir.as_path()),
            ("Configuration file", self.config_file.as_path()),
            ("Saved queries", self.queries_dir.as_path()),
            ("Data directory", self.data_dir.as_path()),
            ("Logs", self.log_dir.as_path()),
        ]
    }
}

fn resolve_config_dir() -> PathBuf {
    if let Some(dir) = non_empty_env(CONFIG_DIR_ENV) {
        return PathBuf::from(dir);
    }
    #[cfg(unix)]
    {
        if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join(CONFIG_DIR_NAME);
        }
        home().join(".config").join(CONFIG_DIR_NAME)
    }
    #[cfg(windows)]
    {
        if let Some(appdata) = non_empty_env("APPDATA") {
            return PathBuf::from(appdata).join(CONFIG_DIR_NAME);
        }
        home().join("AppData").join("Roaming").join(CONFIG_DIR_NAME)
    }
}

fn resolve_data_dir() -> PathBuf {
    if let Some(dir) = non_empty_env(DATA_DIR_ENV) {
        return PathBuf::from(dir);
    }
    #[cfg(unix)]
    {
        if let Some(xdg) = non_empty_env("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join(CONFIG_DIR_NAME);
        }
        home().join(".local").join("share").join(CONFIG_DIR_NAME)
    }
    #[cfg(windows)]
    {
        if let Some(local) = non_empty_env("LOCALAPPDATA") {
            return PathBuf::from(local).join(CONFIG_DIR_NAME);
        }
        home().join("AppData").join("Local").join(CONFIG_DIR_NAME)
    }
}

fn home() -> PathBuf {
    directories::BaseDirs::new().map_or_else(|| PathBuf::from("."), |b| b.home_dir().to_path_buf())
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_paths_stay_inside_the_given_root() {
        let root = Path::new("/tmp/ignatius-test-root");
        let paths = Paths::rooted_at(root);
        for (_, path) in paths.labelled() {
            assert!(path.starts_with(root), "{} escaped the root", path.display());
        }
        assert_eq!(paths.config_file, root.join("config.toml"));
    }

    #[test]
    fn resolved_paths_are_absolute_and_named_for_the_product() {
        let paths = Paths::resolve();
        assert!(
            paths.config_dir.is_absolute() || non_empty_env(CONFIG_DIR_ENV).is_some(),
            "config dir should be absolute unless explicitly overridden"
        );
        assert!(paths.config_file.ends_with("config.toml"));
        assert!(paths.log_dir.starts_with(&paths.data_dir));
    }

    #[test]
    fn every_path_carries_a_human_label() {
        let paths = Paths::rooted_at(Path::new("/tmp/x"));
        let labels = paths.labelled();
        assert_eq!(labels.len(), 5);
        for (label, _) in labels {
            assert!(!label.is_empty());
        }
    }
}
