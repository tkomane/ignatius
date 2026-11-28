# Tasks: Session Search Path and Idle-in-Transaction Timeout

**Input**: Design documents from `/specs/072-session-search-path-and-idle-tx-timeout/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: PR 1A - Configurable `idle_in_transaction_session_timeout` (`feat/m26-idle-in-tx-timeout`)

- [x] T001 Add `idle_in_transaction_session_timeout` to `ConnectionArgs`, `ConnectionTarget`, `SUPPORTED_KEYS`, `resolve`, `for_object_tree`, `ConnectionConfig`, `Profile`, `PROFILE_FIELDS`, and `ConnectionOptions` (`src/connection/target.rs`, `src/config/schema.rs`, `src/cli/mod.rs`).
- [x] T002 Add `idle_in_transaction_session_timeout` to `SessionInfo`, enforce integer-ms `SET idle_in_transaction_session_timeout` in `connect_candidate()`, and surface it in `ConnectionPicker`, Connection details, `doctor`, and `docs/support/compatibility.md`.
- [x] T003 Add unit tests (`target`, `schema`, `connection_picker`, `doctor`, `cli`, `cli::interactive`, `ui::layout`) and live PostgreSQL integration test (`tests/postgres_integration.rs`) and verify with `cargo --locked xtask verify`.

## Phase 2: PR 1B - Configurable `search_path` and Interactive Switcher (`feat/m26-session-search-path`)

- [ ] T004 Implement `MAX_SEARCH_PATH_SCHEMAS`, `parse_search_path`, and `format_set_search_path_sql` in `src/connection/target.rs`, and wire `search_path` across `ConnectionArgs`, `ConnectionTarget`, `SUPPORTED_KEYS`, `Profile`, `PROFILE_FIELDS`, `ConnectionOptions`, `ConnectionPicker`, Connection details, `doctor`, and `docs/support/compatibility.md`.
- [ ] T005 Enforce startup `SET search_path TO ...` before `bootstrap()` and add `Session::set_search_path` in `src/postgres/session.rs`.
- [ ] T006 Wire `Action::SetSearchPath`, `NamePurpose::SetSearchPath`, `Effect::SetSearchPath`, and `Message::SearchPathUpdated` across `src/app/`, `src/ui/`, `src/cli/interactive.rs`, and `docs/design/keymap.md`.
- [ ] T007 Add unit tests (including SQL injection and stale generation guards) and live PostgreSQL integration tests, and verify with `cargo --locked xtask verify`.
