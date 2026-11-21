# Feature Specification: Editorial TUI Aesthetic, Atelier & Paper Built-In Themes

**Feature Branch**: `feat/m16-editorial-tui-aesthetic`  
**Created**: 2026-11-21  
**Status**: In Progress  
**Milestone**: M16 - Editorial High-Art TUI Polish & Atelier/Paper Themes

## Problem Statement

While Ignatius provides five accessible built-in themes (`dark`, `light`, `high-contrast`, `field-industrial`, `material`) and a four-tier surface hierarchy (`Surface`, `SurfacePane`, `SurfaceAlt`, `SurfaceOverlay`), visual inspection of the workbench and overlay suite reveals craft gaps that dilute the gallery-grade, Apple HIG + Google Material Design 3 + editorial-hipster aesthetic:

1. Header and footer keycap pills (`Theme::pill`) paint `SurfaceAlt` over a `SurfaceAlt` rail, producing zero tonal lift.
2. Secondary key-value badges inside `SurfaceOverlay` modals (`push_kv_capsule`) combine `Border` brackets and `DIM`-modified `Muted` labels, lowering legibility over `#2D2925`-class dark overlays.
3. Modal headers and footers lack a crisp horizontal hairline divider between tab strips and body panes (`modal_inner_layout` row gap) and recessed keycap badges on overlay hints.
4. Filled header capsules (`[PROD]`, `[STAGING]`, `[READ ONLY]`, `[TX ACTIVE]`, `[TX FAILED]`) lack single-space internal breathing room, and pane titles (`objects`, `editor`, `results`, `palette`) can use typographic leader accents, muted secondary metadata, and graceful ellipsis truncation instead of mid-word clipping.
5. Ignatius lacks signature curated editorial gallery themes: `atelier` (warm obsidian-espresso dark with terracotta/gold/slate accents) and `paper` (warm archival rag-paper light with Sienna/Prussian/Forest ink accents).

## User Scenarios & Testing

### User Story 1 - Curated Atelier and Paper Themes (Priority: P1)

A database engineer selects `ui.theme = "atelier"` or `ui.theme = "paper"` via config, `--theme`, or interactive theme cycling and immediately sees a warm, editorial obsidian-espresso dark workspace or archival rag-paper light workspace whose tokens satisfy WCAG AA/AAA contrast across all four surface tiers.

**Acceptance Scenarios**:
1. `ThemeChoice` accepts `"atelier"` and `"paper"` case-insensitively and serializes them as `"atelier"` and `"paper"`.
2. `Theme::atelier` and `Theme::paper` satisfy at least 4.5:1 contrast for primary text/header/focus/semantic/syntax/muted tokens across `Surface`, `SurfacePane`, `SurfaceAlt`, and `SurfaceOverlay`, and 7.0:1+ for primary `Text` and `Header` on base surfaces.
3. Interactive theme cycling (`ValidatedThemes::next()`) visits `atelier` and `paper` cleanly alongside existing built-ins and custom themes.
4. Existing `ThemeRefusal` error strings when `ui.theme` is invalid or `[theme] extends` is unrecognized remain unchanged for contract compatibility.

### User Story 2 - Elevated Keycaps, Recessed Modal Pills, and Tactile Chrome (Priority: P1)

When operating the workbench in full-screen mode across any built-in theme, ASCII mode, `NO_COLOR`, `Basic16`, or narrow terminals (`40x8`), the operator sees distinct elevated keycaps on rails, recessed high-contrast pills inside modals, clean hairline separators, right-aligned footer hints when width permits, and uncluttered pane titles.

**Acceptance Scenarios**:
1. `Theme::keycap(token)` paints bold foreground `token` over `SurfaceOverlay` at `TrueColor`/`Indexed256`, falling back cleanly to bold foreground (`Basic16`) and modifier-only (`None`/`NO_COLOR`).
2. `Theme::recessed_pill(token)` paints bold foreground `token` over `SurfaceAlt` for crisp separation inside `SurfaceOverlay` modals without dimming labels into low contrast.
3. `header.rs`, `footer.rs`, `objects.rs`, `editor.rs`, `results.rs`, `palette.rs`, and `overlays.rs` preserve existing hit-test geometry (`content_x`, `header_y`, `first_row_y`, `list_row_0_y`), delimiter tokens (`"Ignatius:"`, `" | "` toolbar separator, `[focused]`), and keyboard/mouse invariants.
4. All 14 showcase scenes (12 existing + `workbench-atelier` + `workbench-paper`) render cleanly as 2x HiDPI PNGs and pass visual inspection.

## Functional Requirements

- **FR-001**: Add `ThemeChoice::Atelier` (`"atelier"`) and `ThemeChoice::Paper` (`"paper"`) to `src/config/schema.rs` (`ACCEPTED`, `label`, `parse`).
- **FR-002**: Add `Theme::atelier(color, depth, contrast)` and `Theme::paper(color, depth, contrast)` with explicit RGB tokens and `banner_anchors` in `src/ui/theme.rs`, and wire them in `Theme::from_choice` and `src/ui/theme/custom.rs`.
- **FR-003**: Add `Theme::keycap(&self, token: Token) -> Style` and `Theme::recessed_pill(&self, token: Token) -> Style` in `src/ui/theme.rs`, respecting `ColorDepth::None`, `ColorDepth::Basic16`, `Indexed256`, and `TrueColor`.
- **FR-004**: Add `pane_block_line` in `src/ui/widgets/mod.rs` for editorial split pane titles (accent leader, bold primary title, calm muted metadata) while retaining `[focused]` text fidelity.
- **FR-005**: Upgrade `header.rs` with elevated `"Ignatius:"` badge, uniform `SurfaceAlt` bar background, and padded filled status capsules (`" [PROD] "`, etc.).
- **FR-006**: Upgrade `footer.rs` with `│` / `|` rail dividers, elevated `keycap(Token::Focus)` badges, and width-aware right-aligned shortcut hints that gracefully wrap/inline when narrow.
- **FR-007**: Upgrade `objects.rs` with multi-span breadcrumb title truncation, clean ellipsis truncation on column type/constraint metadata, and elevated empty-state starter keycaps.
- **FR-008**: Upgrade `editor.rs` with multi-span title (`✎ Editor` + muted cursor location), crisp Focus vs. Muted line numbers, and hairline rules inside completion menus.
- **FR-009**: Upgrade `results.rs` with multi-span title, full-width `SurfaceAlt` table header fill, calm row-number gutter contrast, and elevated empty-state starter keycaps while preserving `content_x`, `header_y`, `first_row_y`, and `" | "` toolbar delimiters.
- **FR-010**: Upgrade `palette.rs` with split title, full-width `SurfaceAlt` Spotlight input strip on row `y + 1`, and a 1-cell breathing gutter around the preview divider while keeping `list_row_0_y = box_area.y + 2`.
- **FR-011**: Upgrade `overlays.rs` with recessed high-contrast `push_kv_capsule` pills (no washed-out `DIM` labels), padded `push_pill_capsule`, hairline tab/body dividers in `modal_inner_layout`, and recessed keycap badges in `render_footer_hints`.
- **FR-012**: Update showcase acceptance scenes (`workbench-atelier`, `workbench-paper`, `error-diagnostics` cursor framing), `scripts/render-showcase.py` font fallback (`JetBrainsMonoNLNerdFont-*.ttf`), documentation, and `tests/documentation_matches_the_build.rs`.

## Non-Functional Requirements

- **NFR-001 (Accessibility)**: Every built-in theme (including `atelier` and `paper`) passes automated contrast assertions (`>= 4.5:1` for text/semantic tokens across all 4 surfaces) and preserves `NO_COLOR`, `ColorDepth::None`, `ColorDepth::Basic16`, `--ascii`, `reduced_motion`, and `40x8` compact fallback.
- **NFR-002 (Pure UI Layer)**: No I/O, networking, filesystem access, or async dependencies added inside `src/ui/**`.
- **NFR-003 (Sidecar Tests & Dash Hygiene)**: All unit tests live in sibling `tests.rs` files; zero em dashes or en dashes in UI copy or repository documentation; `AGENTS.md` and `CLAUDE.md` stay identical.
- **NFR-004 (Toolchain & CI Compatibility)**: Compiles and passes `clippy -- -D warnings` on MSRV Rust `1.90.0` and CI Rust `1.97.1`.

## Constitution & Quality Constraints

- Pure presentation changes in `src/ui/**` require no credential-route or threat-model changes (`secrecy::SecretString` and `diagnostics::redaction` remain untouched).
- All existing integration, contract, and unit suites must pass with `--locked --all-targets --all-features` and live fixture database running.
