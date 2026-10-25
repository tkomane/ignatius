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

### Manual theme slice - 2026-10-25

Deliver FR-2513/2515/2516 under the dated theme-config contract before the
separate automatic-background protocol. T028a is the manual test subset;
T028/T028b/T032 and whole-US5 acceptance remain open until their full scope runs.
Do not change mouse capture, dependencies, query behavior or release controls.

One strict generic schema declaration may parameterise only selection/definition
leaves for a private pending parse. Prove original-source TOML syntax, duplicate
and ordinary error/span behavior before admitting it. Existing load range gates
remain; migrate/save gain theme checks only. Raw pending values have no printable
Debug route. A typed theme-only diagnostic tag preserves other human/JSON/doctor
envelopes. Serialize the actual typed definition, not its resolved cache.

Theme resolution owns canonical token names/hex grammar, the complete RGB
matrix, safe refusals and the immutable fixed-size custom cache. Active theme
identity is derived from a private built-in/custom palette variant; built-in
Theme::new stays infallible and cannot represent missing custom data. Root owns
the model/catalog pair, startup before TerminalGuard, SwitchTheme action,
owner-first handling, explicit palette discovery and actual CLI/parity evidence.
No runtime action reads files or reparses raw maps. A cache mismatch reports an
internal diagnostic without changing theme or emitting effects.

Work is bounded in two isolated lanes from accepted ffb86a1. The configuration
lane first proves parser preservation only in a private candidate. The pure-theme
lane owns theme code/tests, including the four measured Null/comment adjustments
after actual-library reds, and publishes a frozen API/source handoff. Then the
configuration lane consumes that exact handoff for schema/store/diagnostic work.
Root owns canonical integration and shared app/CLI files throughout. The sole
out-of-module theme migration is the Results sidecar's debug identity accessor;
its assertions and all T055 behavior stay unchanged. Cross-review the frozen
combined source, run strict refusal subprocesses and styled parity, then the
full verifier and normal publication gates. The existing independent-review
exception and protected-main/release approval boundaries remain unchanged.

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

### Dependency reconciliation - 2026-10-25

The US6 drag contract consumes US7's split state and keyboard split actions.
T039's keyboard/shared-geometry core is now locally verified; keep it before
T035's drag dispatch and retain final drag acceptance in both stories. This
corrects an omitted dependency rather than adding a capability.
US7 can proceed independently of US5 after the completed extraction.

Mouse-wheel integration also needs explicit viewport state because existing
arrow/page actions change selection or caret. Do not simulate viewport-only
scrolling by replaying selection actions. Keep capture disabled until the whole
verb table, keyboard parity, hit alignment and restoration are proven. Before
T035, clarify the input-event timing seam and reconcile it with existing tick
and asynchronous messages; do not let a render tick cancel a valid drag by
accident. No numerical contract or theme decision changes in this preparation.

### Density handoff - 2026-10-25

T040 now follows the verified T039/T041 core. The presentation contract pins
two-value configuration, one spacing table, the superseding 6/4 discovery cap,
notice-preserving grid rows, title-content ownership and responsive overlay
margin exceptions. No new theme, mouse, doctor or runtime-toggle behavior is
included. Keep rendering overrides authoritative and share boxes/capacities
with hit mapping and navigation. Existing constructors remain comfortable by
default; the runtime reads the optional setting once at startup.

The density implementer owns config schema/template, presentation/model,
discovery and UI widgets/layout, one runtime initialization assignment, relevant
sidecars and focused documentation/parity updates. The lead retains status,
compatibility, changelog, review and integration. Watch tests fail first for
strict parsing, template parity, each table knob, every overlay family, grid
types/notices, remapped hint bounds and minimum-size prompt visibility. Full
verification and rendered inspection follow focused checks. Keep T038 open
until its complete density test set is proven, and keep drag acceptance open
until T035.

### Selected-field correction - 2026-10-25

Before manual custom themes, T055 repairs the actual expanded Results renderer's
child-span foreground precedence. A source-bound actual Buffer probe found a
selected HighContrast header white on white and NULL grey on white, while the
Selection style specifies Surface ink. The worker owns only the existing
Results renderer and its sidecar tests; root owns contract/records/integration.
Keep the original red and prove the real renderer's selected/unselected cells,
semantic modifiers and depth/glyph/density boundaries before full and rendered
review. No RGB palette, custom parsing, runtime action, geometry or mouse change
is included. W07's independently measured alternate-buffer cursor sensor gap
remains open and is not resolved by this UI correction.

### Long-line editor view correction - 2026-10-25

W09's owned 100x30 PTY journey exposed a long SQL line being clipped while its
caret was beyond the right edge. Source inspection confirms the renderer has
only a derived vertical window; completion clamps a scalar column to the pane
edge and hit mapping treats screen cells as scalar columns. T058 first retains
an actual-Buffer failing reproduction, then shares one pure display-cell view
between those three consumers. Root owns FR-2529, the presentation contract,
records and integration; the worker owns only editor rendering/shared hit
mapping and their sidecar tests in an isolated source copy. No editor/model
state, dependency, terminal input, mouse default, SQL or effect change is
included. Existing source text, source scalar columns, styles and vertical
window remain authoritative. Independent review, rendered-cell inspection and
the same-width owned-PTY repeat precede scoped acceptance; native hand use
remains separate.

### Bounded editor-view metadata - 2026-10-25

W10's identical-harness diagnostic after T058 acceptance measures a material
test-profile allocation/latency increase at 16 KiB/1 MiB. T059 projects only
visible/caret units using the same pure mapper and one display sanitiser, with
linear source scanning and no persistent cache. Root owns FR-2530, contract,
review, integration and records; the isolated worker owns only editor view,
its three consumers and sidecar tests. Preserve the already accepted exact
styled cells and all source/edit semantics. Diagnostic observations are not
process ceilings or the release-build 100-action p95 protocol.
