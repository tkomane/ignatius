# Implementation Plan: GUI-grade experience

**Branch**: `025-gui-grade-experience` | **Date**: 2026-09-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/025-gui-grade-experience/spec.md`

## Summary

Make the interactive client read as a designed product: paint every frame on
a three-level surface ladder at a detected colour depth, let the user own
and switch themes safely, name every connecting step under a fully painted
shell, make the grid, panes and overlays workable by mouse and keyboard
alike, and stop dropping pasted SQL. A serialized foundational refactor
extracts the 6,553-line renderer into per-pane widget modules, adds pure
hit-region functions and moves presentation into the model; every
behavioural story then lands as an independent vertical slice on top of it.
The task list is decomposed for the repository's multiagent implementation
setup: explicit owned paths per slice, an integrator lane for the
single-writer files, and pinned-value contracts so an implementer never
guesses.

## Technical Context

**Language/Version**: Rust 2024 edition, minimum Rust 1.90, local verifier
currently using Rust/Cargo 1.98.1

**Primary Dependencies**: Existing Ratatui 0.30 and Crossterm 0.29, the
existing reducer, theme, glyph, keymap, palette and discovery layers. No
new dependency: the colour quantizer, hit regions and the one-shot terminal
background query are hand-rolled at existing boundaries.

**Storage**: None beyond configuration. New keys under `[ui]` and a new
`[theme]` table in the existing TOML store; split, zoom and runtime theme
choice are session-local and never persisted.

**Testing**: Render-to-string layout tests including the extended
meaning-parity matrix (built-in themes plus a valid and a hostile fixture
theme, four colour depths, three glyph tiers), pure quantizer and
hit-region unit tests, reducer tests for paste, mouse dispatch, splits,
zoom, theme switching and connecting steps, configuration refusal tests,
keymap reachability, documentation parity, and the locked verifier. The
behaviour-preserving extraction is gated by the existing layout tests
moving unchanged.

**Target Platform**: macOS, Linux and Windows terminal clients. Interactive
TUI only; plain mode, machine output and scripted `query` are unchanged.
Windows mouse and paste arrive by the console API route Crossterm already
owns.

**Project Type**: Terminal-native interactive workbench and scriptable CLI.

**Performance Goals**: Rendering stays a single-pass buffer paint; the
surface fill adds one styled clear per frame. Hit-testing is arithmetic
over the same layout rects the renderer computes. The background query is
one write and one bounded read at startup with a 100 ms deadline. The
existing p95 input-to-frame target of 100 ms or less continues to apply.

**Constraints**: No new network call of any kind; the background query
speaks only to the local terminal. No new persistence. No animation beyond
the existing spinner and meter. No right-click bindings, no context menus,
no theme plugin system. Colour quantization happens at the single existing
Rgb conversion point. Ratatui's documented truecolor caveat means reduced
depths must emit indexed colours rather than raw RGB.

**Scale/Scope**: One foundational refactor plus nine user stories across
roughly ten source modules and one new widgets directory. The task list is
sized for at most three concurrent implementing workers with one integrator
lane.

## Constitution Check

*GATE: Pass before Phase 0 research and after Phase 1 design.*

- Delight remains truthful (I): the connecting display names only steps
  that ran, `auto` theme detection reports an unanswered query instead of
  claiming detection, reduced colour depths are described as
  approximations, zoom and splits state what they did, and a refused paste
  or theme names its reason.
- PostgreSQL correctness before abstraction (II): nothing in this feature
  touches SQL semantics; grid sorting, previews and refresh-adjacent
  surfaces remain local views over retained data with zero execution
  effects.
- Local-first and private (III): no telemetry, no network beyond the
  database connection, no new persisted state; session layout and runtime
  theme choice die with the session; previews render only retained safe
  data through the existing redaction boundary.
- Safe by default with escape hatches (IV): invalid configuration is
  refused before the terminal is taken with exit 3; mouse capture is
  documented, defaulted deliberately and cleanly disabled; nothing replays
  or retries.
- Keyboard-first, never keyboard-only (V): every mouse verb dispatches to
  an action the keyboard already reaches; the mouse-off test run proves no
  capability loss; no Vim assumption is introduced.
- Accessible meaning survives styling loss (VI): the parity matrix extends
  to colour depths and fixture themes; badges, stripes, elevation and
  previews reinforce words that carry the meaning alone; colour-off stays
  modifier-only for every theme source.
- Cross-platform is a release criterion (VII): input arrives through
  Crossterm's existing platform routes; terminal restoration order gains
  no new modes beyond those already restored; Windows behaviour remains a
  named evidence gap until hand-verified, and is reported as such.
- One source of truth (VIII): the theme tokens, keymap actions, exit codes
  and configuration template each keep their single authority; the design
  documents are amended in the same change as the behaviour they describe,
  under the existing parity tests; the mouse default flip and the
  colour-depth note are declared amendments, not silent contradictions.
- Evidence over confidence (IX): the extraction is proven
  behaviour-preserving by existing tests moving unchanged; each story
  carries focused tests written to fail first; focused, full-suite, live
  and hand evidence stay separate, and the one-session friction evidence
  is recorded as sample size one.
- Recoverability (X): every exit path disables mouse capture and bracketed
  paste in the existing reverse restoration order; a drag interrupted by
  resize re-clamps; an invalid theme can never take the terminal.

**Gate result**: PASS. The feature repaints and rearranges presentation
over existing state and effects; it adds input message types and
configuration keys but no new effect category, storage route, network
route or permission boundary.

## Project Structure

### Documentation (this feature)

```text
specs/025-gui-grade-experience/
├── plan.md              # This file
├── research.md          # Phase 0 output: sourced design decisions
├── data-model.md        # Phase 1 output: state ownership and rules
├── quickstart.md        # Phase 1 output: validation commands and hand checks
├── contracts/
│   ├── presentation.md  # palettes, elevation, quantization, depth detection
│   ├── mouse.md         # verb table with keyboard equivalents
│   ├── theme-config.md  # [theme] grammar, validation wording, OSC 11 facts
│   └── grid.md          # wording shapes, sort cycle, bounds
├── checklists/
│   └── requirements.md  # specification quality gate
└── tasks.md             # Phase 2 output: worker-decomposed task list
```

### Source Code (repository root)

```text
src/
├── app/
│   ├── model.rs         # presentation ownership, splits, connecting step
│   ├── message.rs       # Mouse and Pasted messages; theme, split, zoom intents
│   ├── update.rs        # new reducer arms; mouse dispatch through hit regions
│   ├── editor.rs        # paste insertion as one undoable edit
│   ├── grid.rs          # header sort, column widths, wheel paging
│   ├── palette.rs       # preview sources; switch-theme entry
│   └── discovery.rs     # hints for the new actions
├── connection/
│   └── service.rs       # named connecting-step reporting
├── cli/
│   ├── mod.rs           # --color-depth flag; presentation resolution
│   └── interactive.rs   # Mouse/Paste event arms; background query; model-owned presentation
├── config/
│   ├── schema.rs        # ui.color-depth, ui.density, ui.theme auto/custom, [theme], mouse default
│   └── store.rs         # configuration template additions (same change as schema)
├── diagnostics/
│   └── doctor.rs        # colour depth and background-query reporting
└── ui/
    ├── layout.rs        # shrinks to orchestrator; split-aware geometry
    ├── widgets/         # NEW: mod, header, footer, editor, results, objects, palette, overlays
    ├── theme.rs         # ColorDepth, quantizer, elevation levels, custom themes, capsule set
    ├── glyphs.rs        # capsule ends and any new icons across all three tiers
    ├── keymap.rs        # new bindable actions and defaults
    └── terminal.rs      # COLORTERM detection; mouse-capture default

tests/
└── documentation_matches_the_build.rs   # parity updates in the Polish phase only
```

**Structure Decision**: The foundational phase moves render functions
wholesale into `src/ui/widgets/` and adds the pure hit-region function
beside them, so hit-testing and drawing can never disagree. Presentation
moves into the model so a theme switch is an ordinary message. Everything
else extends existing layers in place: the reducer stays pure, the runtime
translates terminal events into messages, and widgets keep asking the
theme for meaning rather than colours. The five single-writer files
(`app/model.rs`, `app/message.rs`, `app/update.rs`, `cli/interactive.rs`,
`ui/layout.rs`) are touched together only in the foundational phase; each
later story routes its narrow single-writer change through the integrator
lane named in tasks.md.

## Complexity Tracking

No constitution violations. Two structural costs are accepted knowingly:

| Cost | Why needed | Simpler alternative rejected because |
|------|------------|--------------------------------------|
| Widget-module extraction before any visual change | 6,553-line single file makes parallel implementation impossible and hit regions unverifiable | Patching in place would serialize every story through one file and let hit-testing drift from drawing |
| Presentation moves into `Model` | Runtime theme switching and depth-aware rendering must be reducible, testable state | Mutating a shared reference in the event loop would put presentation outside the reducer's evidence boundary |

## Implementation Phases

1. **Foundational (serialized, integrator only)**: extract
   `src/ui/widgets/`, add `region_at` hit regions, move presentation into
   the model, add inert `Mouse` and `Pasted` message arms. Behaviour
   identical; existing layout and parity tests move and pass unchanged.
2. **P1 slices**: paste insertion (US1); painted surfaces and colour depth
   (US2); grid range wording, sort, resize and wheel actions with keyboard
   routes (US3); connecting steps and the useful first frame (US4).
3. **P2 slices**: theme configuration, validation, runtime switch and
   `auto` (US5); mouse verb dispatch and the capture default flip (US6);
   splits, zoom and density (US7).
4. **P3 slices**: quiet chrome and the capsule set (US8); overlay preview
   geometry (US9).
5. **Polish**: amend the design documents and compatibility, changelog and
   parity assertions in the same change as the final behaviour; run the
   focused checks, then the locked verifier with the database fixture, and
   record focused, full, live and unverified evidence separately in
   `docs/status.md`.
