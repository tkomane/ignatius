# Feature 061 tasks

- [x] T061-01 Define command arguments and explicit named-profile resolution
  in `src/cli/fleet.rs` (FR-061-01, SC-061-01).
- [x] T061-02 Implement bounded read-only probes and health facts in
  `src/postgres/fleet.rs` and the session adapter (FR-061-02, SC-061-02).
- [x] T061-03 Thread fleet wire bounds through the shared schema extractor
  and preserve the existing snapshot route (FR-061-03, SC-061-03).
- [x] T061-04 Capture and validate the installed-extension and effective
  setting sections with independent unavailable states (FR-061-04,
  SC-061-03, SC-061-04).
- [x] T061-05 Build deterministic comparison/output and stable exit behavior
  in `src/fleet/mod.rs` and the command (FR-061-03, FR-061-05, FR-061-06,
  SC-061-03, SC-061-04).
- [x] T061-06 Verify malformed/duplicate inputs, ordered mixed results,
  PostgreSQL equality and TLS, bounded snapshots, and display privacy with
  sidecar and subprocess tests (FR-061-01 through FR-061-06, SC-061-01
  through SC-061-04).
- [ ] T061-07 Integrate documentation, hosted fixtures and acceptance evidence
  (FR-061-05, FR-061-06, SC-061-02, SC-061-04).

Local evidence on 19 November 2026 from the feature working tree based on
`28dcc4b`, macOS arm64, Rust 1.100.0 and disposable PostgreSQL 18.7
on separate plain and verify-full TLS endpoints:

- `cargo test --locked --lib fleet` passed 4 tests, including a read-only
  catalog check that exercised the over-limit definition path.
- `fleet_contract` passed 5 tests. The same-run mixed transport probe and
  the separate same-server equality probes exercised both servers.
- `snapshot_contract` passed 1 test for the unchanged ordinary snapshot
  path. `documentation_matches_the_build` passed 22 tests.
- Formatting, workspace Clippy with warnings denied, and whitespace passed.

Hosted PostgreSQL and cross-platform checks are tracked with T061-07.
