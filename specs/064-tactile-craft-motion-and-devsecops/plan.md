# Implementation Plan: Tactile Craft, Truthful Motion, and DevSecOps Hardening

**Spec**: [spec.md](spec.md)

## Delivery Waves

Feature 064 executes across two coordinated waves and six focused pull requests so independent subsystems never collide on shared files:

### Wave 1 - Independent Code & Hardening PRs (#138-#142)

1. **PR #138 (`ci/064-devsecops-hardening-and-local-audit`)**:
   - `xtask/src/security.rs`, `xtask/src/security/tests.rs`, `xtask/src/tests.rs`, `xtask/src/main.rs`: `cargo xtask security` gate parsing `.github/workflows/ci.yml` digests directly.
   - `tests/workflow_contract.rs`: sidecar test enforcement, `AGENTS.md == CLAUDE.md`, and zero duplicate digests in `xtask/src/security.rs`.
   - `specs/025-gui-grade-experience/contracts/presentation.md`, `docs/operations/local-development.md`, `docs/operations/verification.md`: reconciled header yield order and ruleset `22835887` status.
2. **PR #139 (`feat/064-cancellable-safe-benchmarks`)**:
   - `vendor/tokio-postgres/src/cancel_query_raw.rs` + `src/postgres/session.rs`: server EOF drain and 5s timeout on cancel.
   - `src/postgres/bench.rs`, `src/app/{model,message,discovery,update}.rs`, `src/cli/interactive.rs`, `src/cli/mod.rs`: `BenchmarkCancelHandle`, retryable `Ctrl+C`, unconditional `ROLLBACK` on `57014`, `--allow-write` production CLI gating on `BenchArgs`.
   - `tests/bench_contract.rs`, `tests/bench_session_contract.rs`, `docs/design/{keymap,interaction-principles}.md`, `docs/support/compatibility.md`.
3. **PR #140 (`fix/064-xtask-gitleaks-ansi`)**:
   - `xtask/src/security.rs` + `xtask/src/security/tests.rs`: `strip_ansi` CSI escape stripping for `gitleaks` zerolog output validation.
4. **PR #141 (`feat/064-living-surfaces-motion-and-editor-craft`)**:
   - `src/ui/motion.rs`, `src/app/update.rs`: `FormatRipple`, `NoticeSettle`, `BenchSettle`.
   - `src/query/highlight.rs`, `src/ui/widgets/{editor,results,footer,header}.rs`, `src/ui/glyphs.rs`, `src/ui/theme/tests.rs`: SQL bracket matching, statement gutter ripple, hairline scrollbar, inline sort chevron, `4.5:1` contrast pins across all 7 themes, unused helper cleanup.
   - `docs/design/{theme-tokens,icons-and-motion}.md`.
5. **PR #142 (`feat/064-editorial-overlays-plan-and-palette`)**:
   - `src/query/plan.rs`: `exclusive_time_ms`, `exclusive_cost`, `share_of_root`, self-time `attention_score`.
   - `src/ui/widgets/{overlays,palette,results,mod}.rs`, `src/ui/layout.rs`: proportional 5-cell plan micro-bars with unclipped badges, palette match highlight, `SurfaceAlt` SQL preview cards, scroll thumb, `modal_table_header_row`, colored benchmark deltas, `BenchSettle` hook, chord keycaps, structured cell inspector guides, and review rail emphasis.

### Wave 2 - Showcase, Editorial README & Feature 064 Docs (PR 5)

Branched from `origin/main` (`da82fe6`, containing PRs #138-#142):

- `src/ui/acceptance/tests.rs`: refined showcase buffers (`workbench-atelier`, `workbench-paper`, `plan-explorer`, `palette-preview`) and `motion_boot_reveal()`.
- `docs/assets/screenshots/*.png` (14 scenes) & `docs/assets/motion/*.png` (4 animated APNGs + 4 `-still.png` posters) via `scripts/render-showcase.py`.
- `docs/assets/banner-dark.svg` & `docs/assets/banner-light.svg`: studio specification matrix + keycap rail, Quick Look verified.
- `README.md`, `specs/064-tactile-craft-motion-and-devsecops/{spec.md,plan.md,tasks.md}`, `CHANGELOG.md`, `docs/product/roadmap.md`, `docs/status.md`.

## Verification Gates

- `cargo --locked xtask verify` against live disposable PostgreSQL 18.7 plain (`127.0.0.1:55432`) and verified-TLS (`localhost:55433`) fixtures.
- `cargo --locked xtask security` across policy contracts, repo hygiene, `actionlint`, `zizmor`, `gitleaks`, and `cargo-deny` (honestly reporting any environment skips).
- Visual audit of all 14 rendered screenshot PNGs, all 4 motion still/sweep frames, and both Quick Look rasterized SVG banners.
