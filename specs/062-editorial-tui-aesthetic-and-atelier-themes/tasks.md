# Tasks: Editorial TUI Aesthetic, Atelier & Paper Built-In Themes

**Input**: Design documents from `/specs/062-editorial-tui-aesthetic-and-atelier-themes/`
**Prerequisites**: `spec.md`, `plan.md`

## Phase 1: Specification & Swarm Planning

- [x] T001 Author Feature 062 `spec.md`, `plan.md`, and `tasks.md` and commit base specification on `feat/m16-editorial-tui-aesthetic`.

## Phase 2: Theme Architecture (`palette-craft`)

- [x] T002 Add `ThemeChoice::Atelier` and `ThemeChoice::Paper` (`"atelier"`, `"paper"`) in `src/config/schema.rs` (`ACCEPTED`, `label`, `parse`).
- [x] T003 Define `Theme::atelier` and `Theme::paper` RGB token palettes and `banner_anchors` in `src/ui/theme.rs` with >= 4.5:1 WCAG contrast across all 4 surface tiers (`Surface`, `SurfacePane`, `SurfaceAlt`, `SurfaceOverlay`).
- [x] T004 Implement `Theme::keycap` (`SurfaceOverlay` bg on `SurfaceAlt` rails) and `Theme::recessed_pill` (`SurfaceAlt` bg inside `SurfaceOverlay` modals) in `src/ui/theme.rs` with clean `Basic16` and `None` fallbacks.
- [x] T005 Support `atelier` and `paper` in `src/ui/theme/custom.rs` selection/extends/cycling while preserving existing `ThemeRefusal` contract strings.
- [x] T006 Add unit and contract tests in `src/ui/theme/tests.rs` and `tests/theme_contract.rs` verifying `atelier`/`paper` 4-surface contrast, `keycap`/`recessed_pill`, parsing, and cycling.

## Phase 3: Editorial TUI Chrome & Modal Polish (`chrome-craft`)

- [x] T007 Add `pane_block_line` in `src/ui/widgets/mod.rs` and keep all hit-test and pane frame invariants intact.
- [x] T008 Upgrade `src/ui/widgets/header.rs` and `header/tests.rs` (elevated `" ◈ Ignatius: "` badge, continuous `SurfaceAlt` bar, padded filled status capsules).
- [x] T009 Upgrade `src/ui/widgets/footer.rs` and `footer/tests.rs` (vertical dividers, elevated keycaps, width-aware right-aligned shortcut hint rail with narrow inline fallback).
- [x] T010 Upgrade `src/ui/widgets/objects.rs` and `objects/tests.rs` (split truncated breadcrumb title, word-safe ellipsis truncation for column metadata, calm empty-state spacing and starter keycaps).
- [x] T011 Upgrade `src/ui/widgets/editor.rs` and `editor/tests.rs` (split title with muted cursor position, Focus vs. Muted line numbers, hairline dividers in completion popup).
- [x] T012 Upgrade `src/ui/widgets/results.rs` and `results/tests.rs` (split title, full-width `SurfaceAlt` header bar fill, calm row gutter, elevated empty state, preserved `" | "` and geometry invariants).
- [x] T013 Upgrade `src/ui/widgets/palette.rs` and `palette/tests.rs` (split title, full-width `SurfaceAlt` Spotlight input strip on `y + 1` keeping `list_row_0_y = box_area.y + 2`, 1-cell breathing gutter around preview divider).
- [x] T014 Upgrade `src/ui/widgets/overlays.rs` and `overlays/tests.rs` (high-contrast non-`DIM` `push_kv_capsule` pills, padded `push_pill_capsule`, hairline `chunks[2]` rule in `modal_inner_layout`, recessed keycap footer hints, plus width-aware `render_capsule_bar`).

## Phase 4: Integration, 2x Showcase Render & Visual Audit

- [x] T015 Integrate `palette-craft` and `chrome-craft` changes on `feat/m16-editorial-tui-aesthetic`, wire `Theme::keycap` and `Theme::recessed_pill` across header/footer/empty states/overlays, and verify `ui::`, `config`, and `theme_contract` suites.
- [x] T016 Extend `src/ui/acceptance/tests.rs` (`workbench-atelier`, `workbench-paper`, `error-diagnostics` cursor `11`) and `scripts/render-showcase.py` font fallback (`JetBrainsMonoNLNerdFont-*.ttf`), then capture and render all 14 2x HiDPI PNG screenshots in `docs/assets/screenshots/`.
- [x] T017 Visually inspect every one of the 14 PNG screenshots with `read_file` and refine any visual blemishes before finalizing (caught and fixed `schema-snapshot.png` `[INFO: 1]` right-edge clipping via `render_capsule_bar`, re-rendered and re-verified).

## Phase 5: Documentation, Contract Verification & Merge

- [x] T018 Update `docs/design/theme-tokens.md`, `docs/support/compatibility.md`, `README.md`, `CHANGELOG.md`, `docs/product/roadmap.md`, `docs/product/agent-delivery-plan.md`, `docs/status.md`, and `tests/documentation_matches_the_build.rs`.
- [x] T019 Run full `cargo --locked xtask verify`, contract tests, `actionlint`, `diff -u AGENTS.md CLAUDE.md`, sidecar/dash checks, and shut down disposable DB (`cargo --locked xtask db down`).
- [x] T020 Land the Atelier and Paper theme slice and verify the merged source.
  [PR #123](https://github.com/tkomane/ignatius/pull/123) passed
  [PR CI](https://github.com/tkomane/ignatius/actions/runs/44949834697) and
  all three [PR CodeQL analyses](https://github.com/tkomane/ignatius/actions/runs/44949834690).
  The separate aggregate CodeQL check reported neutral. PR head
  `52941dbc167cae56d904f1c683f92618e9c84ef6` and merged
  `a59690d5ab391d3d56f6dff34cbfcedc80c7ddd1` have the same Git tree.
  [Main CI](https://github.com/tkomane/ignatius/actions/runs/44950588920) and
  [main CodeQL](https://github.com/tkomane/ignatius/actions/runs/44950588862)
  passed at that revision. The scheduled fixture-drift job skipped.
