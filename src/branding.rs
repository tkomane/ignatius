//! Single source of truth for product identity.
//!
//! The name is Ignatius, confirmed by the owner on 2026-08-15 after the
//! availability check recorded in `docs/product/landscape.md`.
//!
//! Every user-visible occurrence still resolves through this module. That is not
//! about renaming any more; it is so that the binary name, the display name, the
//! configuration directory, the keyring service and the `application_name`
//! reported to PostgreSQL can never drift apart.

/// Name of the executable, as typed by the user.
pub const BINARY_NAME: &str = "ignatius";

/// Display name used in prose, headers and help output.
pub const PRODUCT_NAME: &str = "Ignatius";

/// One-sentence description shown on first run and in `--help`.
pub const TAGLINE: &str = "A terminal-native PostgreSQL workbench";

/// Directory name used under the platform configuration and data roots.
pub const CONFIG_DIR_NAME: &str = "ignatius";

/// Service name used when a credential is stored in the OS credential store.
///
/// Reserved for Feature 002; recorded here so the identifier has one owner.
/// Changing it after release would orphan every stored credential.
pub const KEYRING_SERVICE: &str = "ignatius";

/// Default `application_name` reported to PostgreSQL, visible in `pg_stat_activity`.
pub const DEFAULT_APPLICATION_NAME: &str = "ignatius";

/// Semantic product version, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Source revision, injected by `build.rs`. `unknown` when built outside a checkout.
pub const SOURCE_REVISION: &str = env!("IGNATIUS_SOURCE_REVISION");

/// Whether the working tree was clean at build time.
pub const SOURCE_CLEAN: &str = env!("IGNATIUS_SOURCE_CLEAN");

/// Target triple the binary was compiled for.
pub const TARGET_TRIPLE: &str = env!("IGNATIUS_TARGET_TRIPLE");

/// Build identity supplied by CI, or `local` for a developer build.
pub const BUILD_IDENTITY: &str = env!("IGNATIUS_BUILD_IDENTITY");

/// Rust compiler version used for the build.
pub const RUSTC_VERSION: &str = env!("IGNATIUS_RUSTC_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_populated() {
        assert!(!BINARY_NAME.is_empty());
        assert!(!VERSION.is_empty());
        assert!(!TARGET_TRIPLE.is_empty());
        // A rebuild must not invent a product version: the version is the one in
        // Cargo.toml and nothing else may synthesise it.
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
