# Implementation Plan: Editorial TUI Aesthetic, Atelier & Paper Built-In Themes

**Branch**: `feat/m16-editorial-tui-aesthetic` | **Spec**: [spec.md](spec.md)

## Architecture & File Ownership

Work cleanly partitions across two non-overlapping implementation streams before final integration, showcase capture, and documentation reconciliation:

### Stream A - Theme Tokens & Contrast Architecture (`palette-craft`)
- `src/config/schema.rs`: Add `Atelier` and `Paper` variants to `ThemeChoice`, update `ACCEPTED`, `label`, and `parse`.
- `src/ui/theme.rs`:
  - Define exact curated RGB palettes for `atelier` (warm obsidian-espresso dark) and `paper` (warm archival rag-paper light) and `banner_anchors`.
  - Wire `ThemeChoice::Atelier` / `Paper` into `Theme::from_choice` and `built_in_themes`.
  - Implement `Theme::keycap(&self, token: Token) -> Style` (`SurfaceOverlay` bg on rails) and `Theme::recessed_pill(&self, token: Token) -> Style` (`SurfaceAlt` bg inside `SurfaceOverlay` modals), honoring `ColorDepth::Basic16` and `ColorDepth::None`.
- `src/ui/theme/custom.rs`: Support `atelier` and `paper` in theme selection/extension lowering and `ValidatedThemes::next()` while preserving verbatim `ThemeRefusal` error strings expected by `tests/theme_contract.rs`.
- `src/ui/theme/tests.rs` & `tests/theme_contract.rs`: Cover contrast verification across all 4 surfaces for `atelier` and `paper`, `keycap` / `recessed_pill` behavior across `TrueColor`, `Basic16`, and `None`, `ThemeChoice` parsing/cycling, and refusal stability.

### Stream B - Editorial TUI Chrome & Modal Refinement (`chrome-craft`)
- `src/ui/widgets/mod.rs`: Add `pane_block_line(Line<'static>, focused, presentation)` helper preserving `[focused]` and pane framing without changing `region_at` / `results_region` coordinates.
- `src/ui/widgets/header.rs` & `header/tests.rs`: Uniform `SurfaceAlt` rail fill, elevated badge treatment for `" ◈ Ignatius: "`, 1-space internal padding inside filled capsules.
- `src/ui/widgets/footer.rs` & `footer/tests.rs`: Crisp vertical `│` / `|` separators, elevated keycap pills, width-aware right-aligned hint rail when space permits (`used + hints_width + 2 <= area.width`).
- `src/ui/widgets/objects.rs` & `objects/tests.rs`: Multi-span truncated breadcrumb title, word-safe ellipsis truncation for long column metadata, elevated empty-state keycaps.
- `src/ui/widgets/editor.rs` & `editor/tests.rs`: Multi-span editor title with muted cursor coordinates, `Focus` vs. `Muted` gutter line numbers, hairline rules in completion popover.
- `src/ui/widgets/results.rs` & `results/tests.rs`: Multi-span results title, full-width `SurfaceAlt` header bar fill, calm row-number gutter, preserved `" | "` toolbar delimiters and `content_x`/`header_y`/`first_row_y` hitboxes, refined empty state.
- `src/ui/widgets/palette.rs` & `palette/tests.rs`: Split title, full-width `SurfaceAlt` Spotlight input strip on row `y + 1` keeping `list_row_0_y = box_area.y + 2`, 1-cell breathing gutter around vertical preview divider.
- `src/ui/widgets/overlays.rs` & `overlays/tests.rs`: High-contrast non-`DIM` `push_kv_capsule` pills with recessed surface lift, padded `push_pill_capsule`, hairline `chunks[2]` rule in `modal_inner_layout`, recessed keycaps in `render_footer_hints`.

### Stream C - Integration, Showcase Audit, Docs & Full Verification (Coordinator)
- Wire `Theme::keycap` and `Theme::recessed_pill` across Stream B call sites where Stream A added the methods.
- Extend `src/ui/acceptance/tests.rs` with `workbench-atelier` and `workbench-paper`, frame `error-diagnostics` cursor at index `20`, add font fallback in `scripts/render-showcase.py`, render all 14 2x HiDPI PNGs, and visually inspect every PNG via `read_file`.
- Reconcile `docs/design/theme-tokens.md`, `docs/support/compatibility.md`, `README.md`, `CHANGELOG.md`, `docs/product/roadmap.md`, `docs/product/agent-delivery-plan.md`, `docs/status.md`, and `tests/documentation_matches_the_build.rs`.
- Verify full local suite (`cargo --locked xtask verify`, contract tests, `actionlint`, `diff -u AGENTS.md CLAUDE.md`, zero inline tests / zero em-en dashes), stop disposable DB (`cargo --locked xtask db down`), push, wait for 100% green hosted CI/CodeQL, and merge PR to `main`.

## Reversibility & Invariants

- All changes live behind clean Rust unit/contract gates; geometry invariants (`results_region`, `palette/tests.rs` `list_row_0_y`, `"Ignatius:"`, `" | "` toolbar delimiters, `ThemeRefusal` exact error strings) remain intact.
- Visual output changes are audited directly against rendered 2x PNG screenshots before documentation updates or merge.
