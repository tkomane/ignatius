# ADR-0006: TOML configuration in XDG-style locations, SQLite deferred

- Status: Accepted
- Date: 2026-08-15

## Context

Configuration must be hand-editable, versioned, validated, migratable, and
written without a chance of leaving a truncated file. Query history has different
needs and does not exist yet.

## Decision

- **Format:** TOML via Serde, with `deny_unknown_fields` so a typo is an error
  with a name rather than a setting that silently does nothing.
- **Locations:** `IGNATIUS_CONFIG_DIR` and `IGNATIUS_DATA_DIR` override
  everything; then XDG variables on Unix; then `~/.config/ignatius` and
  `~/.local/share/ignatius` on Unix, and `%APPDATA%` and `%LOCALAPPDATA%` on
  Windows.
- **Writes:** temporary file in the same directory, write, flush, `fsync`,
  rename. Owner-only permissions where the platform enforces them.
- **Migration:** explicit `schema_version`, never migrated implicitly. Loading
  reports that a migration is pending; `config migrate` performs it after taking
  a timestamped backup, and `--dry-run` writes nothing.
- **Query history:** deferred to Feature 003 along with the choice of SQLite.

## Alternatives considered

- **`directories` platform defaults throughout.** On macOS that means
  `~/Library/Application Support`, which is correct for a GUI application and
  wrong for where terminal users look. The `directories` crate is still used to
  find the home directory.
- **YAML or JSON.** YAML has too many ways to mean the same thing; JSON has no
  comments, which a hand-edited configuration file needs.
- **Adopting SQLite now.** No user of it yet. Adding a storage engine before the
  data it stores exists is how packaging cost gets paid for nothing.

## Consequences

- The path choice is deliberate and visible through `ignatius config paths`.
- No secret value may appear in the configuration file; profiles will store
  references. Enforced by review and by the threat model, not by the type system.

## Reversibility

High for format and location. Migration steps are permanent once released.
