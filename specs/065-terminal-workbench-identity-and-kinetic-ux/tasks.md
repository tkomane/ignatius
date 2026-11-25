# Tasks: Terminal Workbench Identity and Kinetic UX

**Input**: Design documents from `/specs/065-terminal-workbench-identity-and-kinetic-ux/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: Wave 1 - Identity, Kinetic Motion & Live Theme Previews (#144-#146)

- [x] T001 Reframe product identity in `src/branding.rs`, `docs/product/vision.md`, and `docs/product/personas-and-jobs.md`, and add gleaming 2-cell crest + progressive tagline reveal in `src/ui/widgets/banner.rs` (`PR #144`).
- [x] T002 Add `FocusPulse`, `TabGlide`, and `PlanSweep` transitions, luminous truecolour `ThemeCurtain` seam, Objects scrollbar & filter highlights, 5-cell phosphor comet, Results row-gutter shimmer & coordinate readout, and Editor line-count title (`PR #145`).
- [x] T003 Add live Spotlight mini-workbench theme previews (`Theme: ...`), floating overlay 1-cell architectural drop shadows (`paint_overlay_shadow`), and Plan Explorer KPI strip (`PR #146`).

## Phase 2: Wave 2 - Showcase Stills, 6 APNGs, Hero SVGs, README & Wave 70 Docs (PR 4)

- [x] T004 Update `src/ui/acceptance/tests.rs` showcase fixtures and add `motion_format_and_plan()` and `motion_palette_themes()` sequences.
- [x] T005 Re-render and visually inspect all 14 `docs/assets/screenshots/*.png` stills and all 6 animated `docs/assets/motion/*.png` + `-still.png` posters via `scripts/render-showcase.py`.
- [x] T006 Update `docs/assets/banner-dark.svg` and `docs/assets/banner-light.svg` and verify Quick Look rasterizations.
- [x] T007 Rewrite `README.md` around Ignatius's terminal PostgreSQL workbench identity with all 6 motion `<picture>` blocks, passing `tests/documentation_matches_the_build.rs`.
- [x] T008 Author Feature 065 docs and reconcile `CHANGELOG.md`, `docs/product/roadmap.md`, and `docs/status.md` (Wave 70).
- [ ] T009 Run `cargo --locked xtask verify`, commit on `feat/065-showcase-animations-and-readme`, push, and open PR 4.
