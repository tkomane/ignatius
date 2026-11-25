# Tasks: Tactile Craft, Truthful Motion, and DevSecOps Hardening

**Input**: Design documents from `/specs/064-tactile-craft-motion-and-devsecops/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: Wave 1 - DevSecOps, Benchmark Safety, Motion & Editorial Overlays (#138-#142)

- [x] T001 Implement `cargo xtask security`, workflow/sidecar/AGENTS contract tests, and Wave 68 doc drift fixes (`PR #138`).
- [x] T002 Drain server EOF on cancel, bound cancel timeout, implement `BenchmarkCancelHandle`, interactive retryable cancel + rollback safety, and `ignatius bench --allow-write` (`PR #139`).
- [x] T003 Strip ANSI CSI sequences in `gitleaks_output_scanned_history` with zerolog regression tests (`PR #140`).
- [x] T004 Add `FormatRipple`, `NoticeSettle`, `BenchSettle`, SQL-aware `matching_delimiter`, hairline scrollbar, inline sort chevron, `4.5:1` built-in theme contrast pins across all 4 surfaces, and unused helper cleanup (`PR #141`).
- [x] T005 Switch query plan attention scoring to exclusive self time/cost, add proportional 5-cell micro-bars, palette match highlighting + `SurfaceAlt` SQL preview cards + scroll thumb, `modal_table_header_row`, colored benchmark deltas, and modal/inspector rail polish (`PR #142`).

## Phase 2: Wave 2 - Showcase Refresh, Hero SVGs, Editorial README & Docs (PR #143)

- [x] T006 Refine showcase fixtures (`workbench-atelier`, `workbench-paper`, `plan-explorer`, `palette-preview`, `query-bench`) and add `motion_boot_reveal()` in `src/ui/acceptance/tests.rs`.
- [x] T007 Re-render and visually inspect all 14 `docs/assets/screenshots/*.png` and all 4 animated `docs/assets/motion/*.png` + `-still.png` posters via `scripts/render-showcase.py`.
- [x] T008 Elevate `docs/assets/banner-dark.svg` and `docs/assets/banner-light.svg` with the studio specification matrix and inspect Quick Look rasterizations.
- [x] T009 Rewrite `README.md` with understated editorial studio prose and all 4 motion `<picture>` showcases, passing `tests/documentation_matches_the_build.rs` and `tests/session_recovery_contract.rs`.
- [x] T010 Author Feature 064 `spec.md`, `plan.md`, `tasks.md` and reconcile `CHANGELOG.md`, `docs/product/roadmap.md`, and `docs/status.md`.
- [x] T011 Run the final verification and security gates and verify the merge.
  [PR #143](https://github.com/tkomane/ignatius/pull/143) records the successful
  local verifier with PostgreSQL 18.7 plain and verified TLS. The local Unix
  socket case skipped. The local security gate passed its executed checks,
  with Gitleaks and `cargo-deny` skipped at that checkout. The PR passed
  [CI](https://github.com/tkomane/ignatius/actions/runs/45475112873) and
  [CodeQL](https://github.com/tkomane/ignatius/actions/runs/45475112912).
  PR head `c296c16428c0d18bb01dcc41a16cd1d0d7ca9ce3` and merged
  `b65a099d139c3cace9c78ad73db1984a9a548010` have the same Git tree. At that
  revision, [main CI](https://github.com/tkomane/ignatius/actions/runs/45477541967)
  and [main CodeQL](https://github.com/tkomane/ignatius/actions/runs/45477541824)
  passed, including hosted socket, dependency and repository secret scans.
  The scheduled fixture-drift job skipped.
