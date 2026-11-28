# Implementation Plan: Session Search Path and Idle-in-Transaction Timeout

**Branch**: `feat/m26-idle-in-tx-timeout` (PR 1A) and `feat/m26-session-search-path` (PR 1B)
**Spec**: [spec.md](spec.md)

## Architecture Decisions

1. **Reuse M23 Timeout Machinery (`src/connection/target.rs`, `src/config/schema.rs`)**:
   `idle_in_transaction_session_timeout` uses `parse_timeout_duration` and `format_timeout_duration` directly, enforcing integer milliseconds at session startup in `connect_candidate()`. Internal object-tree connections (`for_object_tree()`) clear `idle_in_transaction_session_timeout` to `None` so short metadata reads never inherit arbitrary user settings.
2. **Safe `search_path` Parsing and Quoting (`src/connection/target.rs`)**:
   `parse_search_path(raw: &str)` splits comma-separated segments, trims whitespace, rejects empty inputs/segments, NUL bytes, ASCII control characters, or more than `MAX_SEARCH_PATH_SCHEMAS` (32) schemas, and normalizes optional outer double quotes (`""` unescaping) and `$user`. `format_set_search_path_sql(schemas)` emits `SET search_path TO "schema1", "schema2"` using `crate::query::identifiers::quote_identifier`.
3. **Pure TEA Flow for Interactive `set-search-path`**:
   `Action::SetSearchPath` opens `NamePurpose::SetSearchPath` prefilled with the selected schema name (when `Focus::Objects` points at a schema node) or `SessionInfo::search_path`. Submission validates via `parse_search_path`, refuses disconnected/busy/failed-tx states, and emits `Effect::SetSearchPath { generation, schemas }`. The runtime executes `Session::set_search_path` and sends `Message::SearchPathUpdated { generation, result }`, which discards stale generations and updates `SessionInfo::search_path`.
