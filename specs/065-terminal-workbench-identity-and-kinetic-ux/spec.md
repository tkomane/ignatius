# Feature Specification: Terminal Workbench Identity and Kinetic UX

**Feature Branch**: `feat/065-showcase-animations-and-readme` following PRs #144-#146
**Created**: 2026-11-25
**Status**: Delivered in source (#144, #145, #146, and Wave 2 PR 4)
**Milestone**: M19 - Terminal Workbench Identity & Kinetic UX

## Problem Statement

After Feature 064 (`064-tactile-craft-motion-and-devsecops`) hardened DevSecOps, benchmark cancellation, exclusive plan micro-bars, and contrast pins, the product identity, live kinetic transitions, floating overlay presentation, and public showcase needed a cohesive upgrade:

1. Product copy across `src/branding.rs`, `src/ui/widgets/banner.rs`, `docs/product/vision.md`, `docs/product/personas-and-jobs.md`, `README.md`, and hero SVG banners still framed Ignatius around `"a quiet instrument for PostgreSQL"` / `"unhurried thinking"` rather than its true identity as a fast, full-powered terminal PostgreSQL workbench.
2. Startup boot reveal lacked a gleaming 2-cell `Token::Focus` crest behind the decode frontier and progressive tagline reveal, while pane focus switches (`Tab`), tab switches, and plan loads had no dedicated bounded transitions (`FocusPulse`, `TabGlide`, `PlanSweep`).
3. `ThemeCurtain` lacked a luminous leading seam at truecolour, Objects lacked an overflow scrollbar and fuzzy filter match highlights, Results lacked a 5-cell phosphor comet tail, row-gutter settle shimmer, and selected-cell coordinate readout (`row/total · col_name`), and Editor titles lacked line counts alongside `[modified]`/`[focused]` badges.
4. Floating overlays lacked a 1-cell architectural drop shadow, `Theme: ...` entries in the Spotlight palette lacked live mini-workbench theme previews, and `render_plan` lacked a summary KPI strip.
5. Showcase stills, animated APNGs (adding `format-and-plan` and `palette-themes`), hero SVGs, and `README.md` needed to reflect all PR #144-#146 capabilities deterministically.

## Principles This Feature Must Keep

- **Principle I (Delight must remain truthful).** Every transition (`FocusPulse`, `TabGlide`, `PlanSweep`, `ThemeCurtain`, `ResultSettle`, `FormatRipple`) settles on any key, never delays input, stays completely still under `ui.reduced-motion = true` / ASCII / 16-colour / `NO_COLOR`, and keeps safety rails unchanged.
- **Principle VIII (One source of truth).** `src/branding.rs` owns `IDENTITY_LINE` (`"the PostgreSQL workbench for your terminal"`) and `TAGLINE` (`"A full-powered PostgreSQL workbench for the terminal"`).
- **Principle XI (Editorial craft and tactile restraint).** All colours flow through `Theme` and `Token` with zero raw `Color::Rgb` or `Color::Indexed` literals in `src/ui/widgets/`, and all 14 showcase stills and 6 animated APNGs render deterministically from real Ratatui buffers.

## User Scenarios & Testing

### User Story 1 - Reframed Identity and Gleaming Boot Reveal (Priority: P1)

Users opening Ignatius see `"the PostgreSQL workbench for your terminal"` revealed progressively beneath a gleaming 2-cell `Token::Focus` crest during the bounded 1.1 s boot reveal (`PR #144`), matching `docs/product/vision.md` and CLI `--help`.

### User Story 2 - Kinetic Transitions, Luminous Curtain Seam, and Workbench Craft (Priority: P1)

Pane focus changes pulse smoothly (`FocusPulse` 240 ms), editor tab switches glide (`TabGlide` 260 ms), `EXPLAIN` plan micro-bars sweep into place (`PlanSweep` 420 ms), `ThemeCurtain` sweeps a luminous truecolour leading edge, `Objects` displays hairline overflow scrollbars and highlighted filter matches, `Results` renders a 5-cell phosphor comet, row-gutter settle shimmer, and right-aligned `row/total · col_name` coordinate readout, and `Editor` titles report buffer line counts (`PR #145`).

### User Story 3 - Live Palette Theme Previews, Floating Modal Shadows, and Plan KPI Strip (Priority: P1)

Browsing `Theme: ...` entries in the `Ctrl+P` Spotlight palette previews each theme as a live mini-workbench card with header, SQL, grid rows, and semantic swatches; floating overlays cast a 1-cell architectural drop shadow inside the body bounds; and the Plan Explorer displays a top KPI strip (`Nodes`, `Planning`, `Execution`, `Hotspot`) (`PR #146`).

### User Story 4 - Showcase Refresh, 6 Animated APNGs, Hero SVGs, and Punchy README (Priority: P2)

All 14 stills under `docs/assets/screenshots/*.png` and 6 looping APNGs + `-still.png` posters under `docs/assets/motion/*.png` (including new `format-and-plan` and `palette-themes`), updated `banner-dark.svg` / `banner-light.svg`, and the rewritten `README.md` reflect the complete shipped workbench and pass all documentation and acceptance contracts (`PR 4`).

## Functional Requirements

- **FR-001**: Reframe `src/branding.rs`, `src/cli/mod.rs`, `src/lib.rs`, `docs/product/vision.md`, `docs/product/personas-and-jobs.md`, and `src/ui/widgets/banner.rs` (`is_gleam_col` + progressive tagline reveal) (`#144`).
- **FR-002**: Add `FocusPulse` (240 ms), `TabGlide` (260 ms), and `PlanSweep` (420 ms) in `src/ui/motion.rs` and `src/app/update.rs`; add luminous truecolour curtain seam in `src/ui/layout.rs`; refine `objects.rs`, `results.rs`, `editor.rs`, and `overlays.rs` (`#145`).
- **FR-003**: Add `PaletteCommand::SelectTheme(ThemeChoice)`, live mini-workbench `render_theme_preview` in `src/ui/widgets/palette.rs`, `paint_overlay_shadow` in `src/ui/layout.rs`, and `render_plan_kpi_strip` in `src/ui/widgets/overlays.rs` (`#146`).
- **FR-004**: Update `src/ui/acceptance/tests.rs` (overflowing Objects scrollbar assertions, KPI strip & coordinate assertions, `motion_format_and_plan()`, `motion_palette_themes()`), re-render all 14 stills and 6 APNGs + 6 `-still.png` posters via `scripts/render-showcase.py`, update `docs/assets/banner-{dark,light}.svg`, rewrite `README.md`, and reconcile `CHANGELOG.md`, `docs/product/roadmap.md`, and `docs/status.md` (`PR 4`).

## Non-Functional Requirements

- **NFR-001 (Token Discipline)**: Zero raw `Color::Rgb(` or `Color::Indexed(` literals in `src/ui/widgets/`.
- **NFR-002 (Test Hygiene)**: All unit tests live in sidecar `tests.rs` files; `AGENTS.md` and `CLAUDE.md` stay byte-identical; zero Unicode em dashes (`\u{2014}`) or en dashes (`\u{2013}`) in prose.
- **NFR-003 (Verification)**: Passes `cargo --locked xtask verify`, `tests/documentation_matches_the_build.rs`, and visual inspection of every rendered still, motion poster, and rasterized hero SVG banner.
