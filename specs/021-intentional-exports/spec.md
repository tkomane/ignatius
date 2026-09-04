# Feature Specification: Intentional export shapes

**Feature Branch**: `021-intentional-exports`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip; terminal hand checks pending

**Input**: The experience roadmap item "More export shapes": let a person
choose a useful file shape instead of remembering an option, and let a script
request explicit SQL `INSERT` statements when that is genuinely what it needs.

## User Scenarios & Testing

### User Story 1 - Choose the file shape before naming the file (Priority: P1)

Someone has a retained and possibly filtered result on screen. They press the
existing export shortcut and see a searchable list of CSV, TSV, JSON, NDJSON
and Markdown shapes, with the retained-row count and truncation boundary stated
before any file path is requested. They choose one, type a path, and the file
uses exactly that shape.

**Why this priority**: A graphical client asks "how should I save this?" at the
moment of saving. Making the format explicit removes extension guessing and
lets a person choose a useful representation without remembering flags.

**Independent Test**: Open the export-format palette over a filtered result,
choose JSON, confirm the path prompt names JSON and the next effect carries the
chosen format without writing until the path is accepted.

### User Story 2 - Generate SQL only when the destination is named (Priority: P1)

A script needs result rows as SQL. It runs `query --format insert
--insert-table orders`, and Ignatius emits quoted identifiers, NULL literals
and escaped text literals as data. If the destination table is missing or
ambiguous, the command refuses before connecting rather than inventing a table
or silently producing a different format.

**Why this priority**: SQL output is executable data. The table name, duplicate
column labels, multiple result sets and literal rules need a deliberate
contract, not an attractive but unsafe guess.

**Independent Test**: Render a result containing quotes, backslashes, control
text, Unicode, empty text and NULL; assert that the output is deterministic and
safe, and that missing table or duplicate-column input is refused.

### User Story 3 - Keep large exports bounded and scriptable (Priority: P1)

A script streams a large result to a file with `--output`, using CSV, TSV,
NDJSON or SQL `INSERT`. The destination is still written through the existing
partial-file and atomic-finish boundary. The data stream contains no progress,
colour or diagnostic decoration, and an interrupted export keeps the existing
partial-file evidence.

**Independent Test**: Compare buffered and streamed output for every streamable
format, exercise an overwrite refusal and an interrupted export, and assert
that the result view, history and diagnostics do not alter machine output.

## Acceptance Scenarios

1. **Given** retained result rows, **when** `Ctrl+K e` is pressed, **then** a
   searchable format palette opens and no file, network, history or clipboard
   effect occurs.
2. **Given** a filtered or truncated result, **when** the format palette opens,
   **then** it states that the export is the retained rows currently on screen
   and preserves the existing count boundary.
3. **Given** JSON, TSV, CSV, NDJSON or Markdown is chosen, **when** a path is
   accepted, **then** one export effect carries both the path and the selected
   shape, and the existing no-overwrite boundary remains in force.
4. **Given** `--format insert` without `--insert-table`, **when** the command
   is parsed or validated, **then** it returns a usage diagnostic before target
   resolution or database work.
5. **Given** `--format insert --insert-table orders`, **when** a result is
   rendered, **then** every result set is a separate deterministic `INSERT`
   statement with a quoted table and column list, SQL NULL for NULL and safely
   escaped text literals for non-NULL cells.
6. **Given** duplicate column labels, an empty table name or an unsupported
   `--no-header` combination, **when** INSERT output is requested, **then** it
   refuses rather than generating ambiguous SQL.
7. **Given** an existing destination or an interrupted stream, **when** INSERT
   output is written with `--output`, **then** the existing partial-file,
   overwrite and exit-code contracts remain unchanged.
8. **Given** table, CSV, TSV, JSON, NDJSON or Markdown output, **when** result
   view metadata or local grid shaping changes, **then** scripted output remains
   in server order with server values and no interactive decoration.

## Edge Cases

- A retained result has zero rows: the selected format writes its truthful empty
  representation; INSERT writes no statement and reports zero rows.
- A result has multiple result sets: CSV, TSV, JSON, Markdown, table and INSERT
  keep explicit set boundaries; NDJSON continues to refuse ambiguity.
- A result has duplicate column labels: ordinary formats preserve them, while
  INSERT refuses because a column list would not be unambiguous.
- A cell contains a single quote, backslash, newline, tab, Unicode or terminal
  control text: output escaping preserves data without emitting terminal
  control sequences; NUL is refused because PostgreSQL text literals cannot
  carry it.
- A table name contains spaces, quotes or SQL punctuation: it is quoted as one
  identifier and never interpolated as executable syntax.
- A path has no familiar extension: the explicitly chosen interactive format
  wins; the client does not guess from the path.
- A user dismisses the format palette or path prompt: no file is created and
  the retained result remains unchanged.
- The user opens export controls while a query, plan, confirmation or picker is
  active: existing modal and busy-state precedence prevents a second action.

## Requirements

### Functional Requirements

- **FR-2101**: Interactive result export MUST open a searchable format choice
  before requesting a destination path.
- **FR-2102**: The interactive choice MUST offer CSV, TSV, JSON, NDJSON and
  Markdown in deterministic order and state that it exports retained visible
  rows.
- **FR-2103**: The selected interactive format MUST remain in the model until
  the path is accepted and MUST be carried in the export effect.
- **FR-2104**: Interactive export MUST preserve filtering, retained-row counts,
  no-overwrite, partial-file and safe path expansion behavior.
- **FR-2105**: The CLI MUST accept `--format insert` and MUST require a
  non-empty `--insert-table` value for it.
- **FR-2106**: INSERT output MUST quote the table and column identifiers and
  MUST escape text values without treating cell contents as SQL syntax.
- **FR-2107**: INSERT output MUST render NULL as SQL `NULL`, empty text as an
  empty text literal, and refuse NUL-containing text.
- **FR-2108**: INSERT output MUST refuse duplicate column labels and an
  unsupported header-suppression combination before writing output.
- **FR-2109**: Buffered and streamable INSERT output MUST use the same encoding
  contract and deterministic row order.
- **FR-2110**: Existing CSV, TSV, JSON, NDJSON, Markdown and table output MUST
  retain their current byte and error contracts.
- **FR-2111**: Export format selection and validation MUST not execute SQL,
  contact PostgreSQL, write history, transfer clipboard data or add telemetry.

### Security and Compatibility Requirements

- **SEC-2101**: Output values MUST never reach diagnostics, palette text,
  history or debug output as a side effect of choosing a format.
- **SEC-2102**: Table names and column names MUST be quoted through the single
  identifier helper; raw names MUST never be concatenated into SQL syntax.
- **SEC-2103**: A file is never replaced accidentally. Existing destinations
  and partial exports retain their current explicit recovery behavior.
- **SEC-2104**: Machine output MUST contain only the selected data shape, with
  no colour, progress, status banner or terminal control sequence.
- **SEC-2105**: Interactive format choice MUST remain outside plain, JSON,
  NDJSON, history, clipboard and ordinary scripted query routes.

## Key Entities

- **Export format**: An application-level choice for a retained interactive
  result or a CLI output contract; it is not inferred from a file extension.
- **INSERT output options**: An explicit destination table plus the existing
  header and null options, validated before connection or writing.
- **Format palette**: The existing searchable palette used as a small GUI-like
  "save as" choice before the path prompt.

## Success Criteria

- **SC-2101**: A person can choose an interactive export shape, then a path,
  without remembering a flag and without any effect before path acceptance.
- **SC-2102**: The chosen interactive shape produces the same retained rows and
  filter boundary as the screen, without guessing from the extension.
- **SC-2103**: INSERT output containing adversarial text, NULL, empty text and
  Unicode is deterministic, quoted and accepted by the stated contract, with
  unsafe or ambiguous cases refused.
- **SC-2104**: Buffered and streamed CSV, TSV, NDJSON and INSERT output share
  byte-identical row encoding where both paths are available.
- **SC-2105**: Existing destinations, interrupted exports, multi-result NDJSON,
  plain mode, JSON diagnostics and machine-output boundaries remain green.
- **SC-2106**: Focused unit, reducer, renderer, CLI-contract, documentation and
  locked verifier checks pass, with live database evidence reported separately.

## Assumptions

- The existing `Export` partial-file machinery remains the only file-write
  boundary.
- Interactive INSERT is deferred in this slice because it needs a second
  deliberate table-name choice; the CLI contract is available immediately for
  scripts and can be surfaced in a future table-destination dialog.
- PostgreSQL values arrive as server-rendered text. INSERT therefore emits
  escaped text literals rather than pretending to infer exact source types.
- The existing `quote_identifier` helper is the only identifier quoting route.
