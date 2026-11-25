# Feature Specification: Tactile Craft, Truthful Motion, and DevSecOps Hardening

**Feature Branch**: `feat/064-showcase-and-editorial-readme` (integration/showcase) following PRs #138-#142
**Created**: 2026-11-24
**Status**: Delivered in source (#138-#143)
**Milestone**: M18 - Tactile Craft, Truthful Motion & DevSecOps

## Problem Statement

After Feature 063 (`063-living-surfaces-and-truthful-motion`) established the core motion clock, hard-edge theme curtain, result settle, and static overlay recession, several safety, DevSecOps, and craft gaps remained open in `docs/status.md` Wave 68:

1. No single local `cargo xtask security` command reproduced CI's policy contracts, sidecar-test hygiene, dash checks, `actionlint`, `zizmor`, `gitleaks`, and `cargo-deny` gates from one source of truth.
2. A running query benchmark could not be cancelled mid-flight, `vendor/tokio-postgres/src/cancel_query_raw.rs` returned immediately after `shutdown()` without draining server EOF, and `ignatius bench` lacked `--allow-write` production gating parity with `ignatius query`.
3. Built-in theme unit tests checked status tokens at `3.0:1` instead of Constitution Principle XI's `4.5:1` requirement, `specs/025-gui-grade-experience/contracts/presentation.md` trailed the updated header yield order, and unused helpers (`tls_appearance`, `Glyphs::meter`) remained.
4. Editor/grid tactile details (SQL bracket highlighting, buffer-format gutter ripples, positive notice/benchmark settles, inline sort chevrons, hairline scrollbars) and overlay studio polish (exclusive plan self-time bottlenecks instead of root-biased inclusive totals, proportional 5-cell micro-bars, palette match highlighting and `SurfaceAlt` SQL cards, modal table headers, colored benchmark deltas) were not yet complete.
5. Showcase scenes, hero banners, boot-reveal motion capture, and `README.md` needed a coordinated refresh to reflect every shipped refinement cleanly.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Cancelled benchmarks wait for server EOF with bounded timeouts, report `"Cancellation requested"` while in flight, and execute `ROLLBACK` cleanly when enabled. Transitions (`FormatRipple`, `NoticeSettle`, `BenchSettle`) settle on any key and stay static on failures, warnings, and refusals.
- **Principle IV (Safe by default).** `ignatius bench` enforces `--allow-write` before opening production connections, and benchmark cancellation never blocks on `user_sql_gate`.
- **Principle VIII (One source of truth).** `cargo xtask security` parses pinned `actionlint`, `zizmor`, and `gitleaks` `@sha256:` image digests directly from `.github/workflows/ci.yml` without duplicating hashes.
- **Principle XI (Editorial craft and tactile restraint).** Every non-border token across all seven built-in palettes is pinned at `>= 4.5:1` across all four surfaces (`>= 7.0:1` in `high-contrast`), zero raw `Color::Rgb` or `Color::Indexed` literals appear in `src/ui/widgets/`, and every showcase asset renders deterministically from real widget buffers.

## User Scenarios & Testing

### User Story 1 - Reproducible Local Security Gate and Hygiene Contracts (Priority: P1)

Engineers run `cargo xtask security` locally before pushing and receive an honest `pass` / `FAIL` / `skip` summary across workflow/release/privacy/doc contracts, `AGENTS.md == CLAUDE.md`, sidecar `tests.rs` enforcement, governed dash checks, `actionlint`, `zizmor`, ANSI-stripped `gitleaks` history validation, and `cargo-deny`.

### User Story 2 - Cancellable, Gate-Safe Benchmarks and Production CLI Write Gating (Priority: P1)

An operator presses `Ctrl+C` during a running benchmark; the client drains server cancel EOF within a 5-second timeout without holding `user_sql_gate` across `.await`, skips Query B, rolls back cleanly, and leaves the session in `Autocommit` with zero committed writes. `ignatius bench` refuses non-read statements against production targets unless `--allow-write` is passed.

### User Story 3 - Truthful Motion Extensions, SQL Bracket Highlighting, and Contrast Enforcement (Priority: P2)

Formatting changes trigger `FormatRipple` down the active statement gutter and `NoticeSettle` in the footer, clean benchmark completions trigger `BenchSettle`, SQL parentheses/brackets highlight via lexer tokens outside strings/comments, sorted grid columns show inline chevrons (`▾`/`▴`) when width permits, and built-in theme tests enforce `>= 4.5:1` across all four elevation surfaces.

### User Story 4 - Exclusive Plan Bottleneck Micro-Bars, Palette Cards, and Studio Modals (Priority: P2)

`PlanNode::attention_score` uses exclusive self-time/self-cost so attention lands on the true bottleneck node rather than almost always the root, renders proportional 5-cell micro-bars while preserving unclipped `[attention]` / `[estimate mismatch]` badges, command palette highlights matched characters and paints `SurfaceAlt` SQL preview cards with divider scroll thumbs, and modal tables share `SurfaceAlt` headers with colored benchmark deltas.

### User Story 5 - Curated Showcase Refresh, Boot Reveal APNG, Hero SVGs, and Editorial README (Priority: P3)

All 14 showcase stills, 4 animated motion captures (`boot-reveal`, `run-and-settle`, `cancel-honestly`, `theme-curtain`) with reduced-motion stills, adaptive dark/light hero SVG banners, and `README.md` reflect the shipped studio instrument accurately and pass `tests/documentation_matches_the_build.rs`.

## Functional Requirements

- **FR-001**: Implement `cargo xtask security` (`xtask/src/security.rs`) parsing image digests from `.github/workflows/ci.yml`, stripping ANSI CSI escapes before checking `gitleaks` scanned-commit output with token boundaries, and add contract checks in `tests/workflow_contract.rs`.
- **FR-002**: Drain server EOF in `vendor/tokio-postgres/src/cancel_query_raw.rs`, bound `CancelHandle::cancel()` in `src/postgres/session.rs`, implement `BenchmarkCancelHandle` in `src/postgres/bench.rs`, wire `QueryBenchState::cancelling` + retryable `Ctrl+C` across `src/app/` and `src/ui/`, and add `--allow-write` gating to `BenchArgs` in `src/cli/mod.rs`.
- **FR-003**: Extend `TransitionKind` with `FormatRipple` (360 ms), `NoticeSettle` (480 ms), and `BenchSettle` (480 ms) in `src/ui/motion.rs`; implement `matching_delimiter` in `src/query/highlight.rs`; refine `editor.rs`, `results.rs`, `footer.rs`, `header.rs`, `glyphs.rs`, and `src/ui/theme/tests.rs` (`>= 4.5:1` across all 4 surfaces).
- **FR-004**: Add `exclusive_time_ms`, `exclusive_cost`, and `share_of_root` to `PlanNode` (`src/query/plan.rs`); render micro-bars, `modal_table_header_row`, colored benchmark deltas, chord keycaps, structured inspector guides, and palette SQL cards/scroll thumbs (`src/ui/widgets/overlays.rs`, `palette.rs`, `results.rs`).
- **FR-005**: Refresh showcase buffers and `motion_boot_reveal()` in `src/ui/acceptance/tests.rs`, re-render all 14 stills and 4 motion APNGs/posters via `scripts/render-showcase.py`, elevate `docs/assets/banner-dark.svg` & `banner-light.svg`, rewrite `README.md`, and update `CHANGELOG.md`, `docs/product/roadmap.md`, and `docs/status.md`.

## Non-Functional Requirements

- **NFR-001 (Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Test Hygiene)**: All unit tests reside in sidecar `tests.rs` files; `AGENTS.md` and `CLAUDE.md` stay byte-identical; zero Unicode em/en dashes in prose.
- **NFR-003 (Platform & MSRV)**: Passes `cargo --locked xtask verify` and `cargo --locked xtask security`, compiling cleanly on MSRV `1.90.0` and CI toolchains with Clippy `-D warnings` and `RUSTDOCFLAGS="-D warnings"`.
