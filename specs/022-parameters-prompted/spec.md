# Feature Specification: Prompted query parameters

**Feature Branch**: `022-parameters-prompted`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with semantic Unix-socket skip; terminal hand checks pending

**Input**: The experience roadmap item "Parameters prompted": a statement with
`:name` placeholders asks for the values instead of being edited each time.

## User Scenarios & Testing

### User Story 1 - Run a reusable statement without editing its template (Priority: P1)

Someone keeps a useful statement in the editor, such as
`SELECT * FROM orders WHERE customer_id = :customer_id AND status = :status`.
They run it and receive a focused, keyboard-first prompt for each distinct
parameter. The SQL remains visible and unchanged behind the prompt. They can
accept an empty text value, correct the current value before moving on, or
cancel without sending anything.

**Why this priority**: Reusing a statement should feel like filling in a small
form, not like repeatedly editing and undoing SQL. This is the central graphical
client experience promised by the roadmap.

**Independent Test**: Open a connected TUI model with repeated and distinct
`:name` placeholders, run the buffer, enter values in the displayed order, and
assert that one parameterized execution effect is produced while the editor,
template SQL and history input remain unchanged.

**Acceptance Scenarios**:

1. **Given** a connected editor containing `:customer_id` twice and
   `:status` once, **when** the run action is accepted, **then** one masked
   prompt is shown for `customer_id` followed by one for `status`.
2. **Given** a parameter prompt, **when** the user types a value and presses
   Enter, **then** the value is accepted for that name and the next distinct
   name is shown without changing the editor buffer.
3. **Given** a parameter prompt, **when** the user presses Esc or Cancel,
   **then** no execution effect is emitted, no history entry is created, and
   the SQL remains ready to run again.
4. **Given** a value containing quotes, backslashes, newlines, Unicode or SQL
   punctuation, **when** it is accepted, **then** it is treated as one literal
   value and cannot add a clause, statement boundary or comment to the query.

### User Story 2 - Use the same flow in accessible and automated modes (Priority: P1)

Someone using plain mode or a screen reader receives the same parameter names,
order, cancellation behavior and literal-value contract in words. A script using
the non-interactive `query` command supplies values through named environment
variables, so a pipeline never hangs waiting for a hidden prompt and no secret
value is placed in a process argument.

**Why this priority**: The product is a workbench as well as a full-screen
client. A feature that exists only behind terminal drawing would violate the
accessibility and scriptability promises.

**Independent Test**: Run plain-mode prompt helpers with a real terminal input
path and run `query --param-env name=VARIABLE` against a disposable database;
assert that the same template executes, missing or extra mappings fail before
connection work, and raw values do not appear in command diagnostics or output
decoration.

**Acceptance Scenarios**:

1. **Given** plain mode on an interactive terminal and a statement with two
   names, **when** the user supplies two answers, **then** the prompts are
   written to the message stream, values are not echoed by the client, and one
   execution runs with the original template recorded in history.
2. **Given** non-interactive `query` input containing `:customer_id`, **when**
   `--param-env customer_id=IGNATIUS_CUSTOMER_ID` names a set environment
   variable, **then** the query runs using that value and stdout remains only
   the requested data format.
3. **Given** non-interactive `query` input containing named placeholders,
   **when** a value is missing or a mapping is absent, **then** a usage
   diagnostic explains the safe route and the client does not resolve a target,
   open a connection or wait for input.

### User Story 3 - Keep parameter execution truthful and recoverable (Priority: P1)

A person can understand which statement ran, what happened to it, and why a
server error position may not point into the template after values were expanded.
Prompted values never become part of editor history, statement history, logs,
diagnostics, screenshots or debug representations. A failed or cancelled query
keeps the existing result, transaction, exit-code and cancellation contracts.

**Why this priority**: Parameter substitution is a boundary where convenience
can silently become data disclosure or a false editor marker. Truthful failure
and recoverability are release requirements, not polish.

**Independent Test**: Exercise successful, cancelled, failed, repeated-name,
NUL-containing and adversarial-value cases through unit, reducer, renderer,
plain, CLI-contract and disposable PostgreSQL checks, then inspect debug and
diagnostic text for the synthetic secret marker.

**Acceptance Scenarios**:

1. **Given** a successful parameterized run, **when** history is recorded,
   **then** it stores the placeholder template and never the entered values.
2. **Given** a query error after parameter expansion, **when** the server
   reports a character position, **then** the client states that the position
   belongs to the expanded request and does not move the editor caret by guess.
3. **Given** a parameter contains NUL or cannot be represented as a PostgreSQL
   text literal, **when** execution is attempted, **then** it is refused before
   the statement is sent and the refusal contains no parameter value.
4. **Given** the user cancels or the server confirms cancellation, **when** the
   run ends, **then** no retry or automatic re-execution occurs and the existing
   transaction and result boundaries remain truthful.

## Edge Cases

- A placeholder inside a single-quoted string, double-quoted identifier,
  dollar-quoted body, line comment or nested block comment is ordinary SQL text
  and is never prompted.
- PostgreSQL cast syntax such as `value::text` is not a named parameter.
- Names use an ASCII identifier shape: a letter or underscore followed by
  letters, digits or underscores. A colon followed by other punctuation is left
  for PostgreSQL to interpret.
- Distinct names are prompted once in first-use order; case differences remain
  distinct names.
- A blank answer is a valid empty text value. This slice does not invent a NULL
  shorthand; users can express NULL semantics in the SQL template itself.
- A value containing a quote, backslash, newline, tab, Unicode or terminal
  control character remains data and is escaped without terminal control output.
- A NUL-containing value is refused because it cannot cross the PostgreSQL text
  query boundary safely.
- An empty statement or a statement with no named placeholders follows the
  existing execution path and does not open a parameter prompt.
- More than the supported bounded number of distinct names is refused with a
  usage diagnostic before connection work.
- A prompt is dismissed before all values are entered: no partial binding is
  sent, no value is retained in history, and the template remains untouched.
- The connection is lost while a prompt is open: the prompt is discarded and
  no entered value can be sent after the session becomes unusable.
- A production write still asks for the existing production confirmation before
  any parameter value is requested.
- A non-interactive query never prompts. It requires complete, unique
  `--param-env NAME=VARIABLE` mappings for every named placeholder.

## Requirements

### Functional Requirements

- **FR-2201**: The client MUST recognise named placeholders only outside
  PostgreSQL strings, identifiers, dollar-quoted bodies and comments, and MUST
  exclude cast syntax.
- **FR-2202**: The interactive client MUST prompt once for each distinct named
  placeholder in first-use order before sending the statement.
- **FR-2203**: The parameter prompt MUST keep the editor template unchanged and
  MUST support insertion, backspace, Enter to accept, and Esc or Cancel to
  abandon the pending run.
- **FR-2204**: The prompt MUST show the current parameter name, progress through
  the distinct names, and an explicit explanation that values are treated as
  literal text data.
- **FR-2205**: Parameter values MUST accept empty text, preserve Unicode and
  control text as data, and refuse NUL before the statement is sent.
- **FR-2206**: Repeated occurrences of one name MUST receive the same value, and
  a value MUST never be interpreted as SQL syntax.
- **FR-2207**: Plain interactive mode MUST expose the same prompt order and
  cancellation semantics through its message stream without client-side echo.
- **FR-2208**: Non-interactive `query` MUST accept repeatable
  `--param-env NAME=VARIABLE` mappings and MUST refuse to prompt or accept raw
  parameter values in process arguments.
- **FR-2209**: Non-interactive `query` MUST refuse missing, duplicate or extra
  mappings before configuration target resolution or database work.
- **FR-2210**: Successful parameterized runs MUST record the placeholder
  template, not prompted values, in statement history.
- **FR-2211**: Prompted values MUST NOT appear in diagnostics, logs, debug
  representations, palette text, result metadata or terminal decoration.
- **FR-2212**: Parameterized errors MUST not move an editor caret using a server
  position that refers to the expanded request; the client MUST state why the
  location is unavailable.
- **FR-2213**: Parameter binding MUST preserve existing job identity,
  cancellation, transaction, row-cap, result, output and exit-code contracts.
- **FR-2214**: The implementation MUST enforce a bounded number of distinct
  names and document the limit as a compatibility contract.

### Security and Compatibility Requirements

- **SEC-2201**: Prompted values MUST use the repository's secret-value boundary
  while they are held by the model, effect or runtime, and MUST be discarded
  after the execution attempt.
- **SEC-2202**: Values MUST cross the SQL boundary only as escaped literal data;
  no value may contribute SQL keywords, identifiers, operators or statement
  delimiters.
- **SEC-2203**: Environment-backed automation MUST name variables rather than
  carrying their values in command arguments, and missing variables MUST fail
  without revealing any value.
- **SEC-2204**: The template and entered values MUST remain separate for history,
  error reporting and editor state; neither prompt cancellation nor failure may
  write the values to disk.
- **SEC-2205**: Existing unparameterized SQL, `$1` text, production confirmation,
  plain output, JSON diagnostics, cancellation and connection boundaries MUST
  remain unchanged.

## Key Entities

- **Named placeholder**: A valid `:name` occurrence in executable SQL, with a
  safe name and source span; protected SQL regions do not create one.
- **Parameter template**: The original SQL plus the distinct placeholder names
  in first-use order. It is safe to retain for editor display and history.
- **Prompted parameter value**: One ephemeral user answer associated with a
  placeholder name. It is masked in the interface and never part of the SQL
  template.
- **Environment mapping**: A non-secret `NAME=VARIABLE` declaration used only
  by non-interactive automation to obtain a prompted value from the process
  environment.
- **Parameterized execution**: One job that uses a template and its ephemeral
  values while preserving the existing result and transaction model.

## Success Criteria

### Measurable Outcomes

- **SC-2201**: A person can run a statement with up to 10 distinct names
  without editing the SQL template, with each name shown once in first-use
  order and one final acceptance starting the query.
- **SC-2202**: In automated checks, 100% of values containing quotes,
  backslashes, newlines, Unicode, SQL punctuation and empty text remain one
  literal value, while every NUL case is refused before send.
- **SC-2203**: In automated checks, no synthetic parameter secret appears in
  model or effect debug output, history entries, diagnostics, logs or machine
  output across success, failure and cancellation paths.
- **SC-2204**: Missing, duplicate and extra environment mappings return a usage
  result before target resolution in every tested non-interactive route.
- **SC-2205**: Existing unparameterized focused tests and the locked repository
  verifier remain green, with database and terminal skips reported separately.
- **SC-2206**: Plain and full-screen parameter prompts expose the same name
  order, cancellation outcome and template-preservation behavior in focused
  reducer, renderer and plain-mode checks.

## Assumptions

- Named parameters are an Ignatius interaction feature, not PostgreSQL prepared
  statement syntax. Existing `$1` text remains outside this prompt contract.
- The current simple-query execution path remains the source of server-rendered
  text values. This slice therefore sends escaped text literals and does not
  infer PostgreSQL types from prompted strings.
- `--param-env` is the automation route because process arguments are visible to
  other local processes and must not carry parameter secrets.
- The existing password prompt and terminal raw-mode boundary can be reused for
  hidden plain-mode parameter input.
- Interactive INSERT-style table selection remains outside this feature; the
  parameter prompt owns only values for the SQL statement.
- The bounded distinct-name limit is 64, chosen to keep prompts, effects and
  accidental memory use recoverable while leaving ordinary statements ample
  room.
