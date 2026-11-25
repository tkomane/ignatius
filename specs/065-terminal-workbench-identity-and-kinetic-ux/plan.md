# Implementation Plan: Terminal Workbench Identity and Kinetic UX

**Spec**: [spec.md](spec.md)

## Delivery Waves

Feature 065 executes across two coordinated waves and four focused pull requests:

### Wave 1 - Independent Code & Visual PRs (#144-#146)

1. **PR #144 (`feat/065-product-vision-and-identity`)**:
   - `src/branding.rs`, `src/branding/tests.rs`, `src/cli/mod.rs`, `src/lib.rs`: reframed `TAGLINE` and `IDENTITY_LINE` (`"the PostgreSQL workbench for your terminal"`).
   - `src/ui/widgets/banner.rs`, `src/ui/widgets/banner/tests.rs`: 2-cell gleaming `Token::Focus` crest and progressive tagline reveal.
   - `docs/product/{vision,personas-and-jobs}.md`, `docs/design/{icons-and-motion,interaction-principles}.md`, `tests/documentation_matches_the_build.rs`.
2. **PR #145 (`feat/065-kinetic-ui-and-living-motion`)**:
   - `src/ui/motion.rs`, `src/ui/motion/tests.rs`, `src/app/update.rs`, `src/app/update/tests.rs`: `FocusPulse` (240 ms), `TabGlide` (260 ms), `PlanSweep` (420 ms), `OutCubic`, reduced-motion stillness.
   - `src/ui/layout.rs`, `src/ui/layout/tests.rs`: luminous truecolour `ThemeCurtain` leading seam.
   - `src/ui/widgets/{objects,results,editor,overlays,mod}.rs`: Objects overflow scrollbar and filter substring highlights, 5-cell phosphor comet, Results row-gutter settle shimmer and `row/total · col_name` coordinate readout, Editor line-count titles and `[modified]`/`[focused]` badges.
3. **PR #146 (`feat/065-studio-theme-previews-and-overlays`)**:
   - `src/app/palette.rs`, `src/app/update.rs`, `src/ui/widgets/palette.rs`: `SelectTheme(ThemeChoice)` entries and live mini-workbench `render_theme_preview`.
   - `src/ui/layout.rs`: `paint_overlay_shadow` 1-cell architectural drop shadow for floating overlays.
   - `src/ui/widgets/overlays.rs`: `render_plan_kpi_strip` (`Nodes`, `Planning`, `Execution`, `Hotspot`) and studio overlay polish.

### Wave 2 - Showcase, Animations, Hero Banners & Punchy README (PR 4)

Branched from `origin/main` (`ced9be8`, containing PRs #144-#146):

- `src/ui/acceptance/tests.rs`: overflowing Objects tree in `showcase_base_model`, assertions for line count, coordinate readout, Objects scrollbar, and KPI strip, plus `motion_format_and_plan()` and `motion_palette_themes()`.
- `docs/assets/screenshots/*.png` (14 stills) and `docs/assets/motion/*.png` (6 animated APNGs + 6 `-still.png` posters) via `scripts/render-showcase.py`.
- `docs/assets/banner-dark.svg` & `docs/assets/banner-light.svg`: `"The PostgreSQL workbench for your terminal."` + 2x2 workbench capability pills, Quick Look rasterized and inspected.
- `README.md`, `specs/065-terminal-workbench-identity-and-kinetic-ux/{spec.md,plan.md,tasks.md}`, `CHANGELOG.md`, `docs/product/roadmap.md`, `docs/status.md`.

## Verification Gates

- `cargo --locked xtask verify` against live disposable PostgreSQL 18.7 plain (`127.0.0.1:55432`) and verified-TLS (`localhost:55433`) fixtures.
- `cargo --locked test --test documentation_matches_the_build` confirming all 14 screenshot paths, 56 `CHORDS`, valid `[profiles.production-orders]` TOML, M6 slugs, and zero em/en dashes.
- Visual audit via `read_file` of all 14 screenshot PNGs, all 6 motion `-still.png` posters, and both Quick Look rasterized hero SVG banners.
