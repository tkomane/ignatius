# Parameter Contracts

## Placeholder discovery

Input: arbitrary UTF-8 SQL text.

Output: either a bounded `ParameterTemplate` or a usage error.

Rules:

- `:name` is recognized only when `name` begins with `[A-Za-z_]` and continues
  with `[A-Za-z0-9_]*`.
- The scanner skips single-quoted strings, double-quoted identifiers,
  dollar-quoted bodies, line comments and nested block comments.
- A colon participating in `::` is not a placeholder.
- Names are case-sensitive, distinct names preserve first-use order, and repeats
  share one value.
- At most 64 distinct names are accepted.

## Literal binding

Input: a template and complete secret values.

Output: SQL sent to PostgreSQL through the existing simple-query adapter.

- Every non-NUL value is represented as one PostgreSQL escape string literal.
- Backslash, quote, newline, carriage return, tab, backspace, formfeed and
  ASCII controls are escaped; Unicode remains Unicode data.
- No value can add an identifier, keyword, operator, comment or statement
  boundary.
- Empty text is represented as an empty literal.
- NUL returns a usage diagnostic before send.
- Binding failures do not include the value in the text.

## TUI contract

- The prompt opens only after any required production confirmation.
- The prompt title names the current `:name` and progress, and the body states
  that the answer is literal text data.
- Enter accepts empty text and advances; Esc, Cancel and Quit have their normal
  modal semantics.
- The editor, template SQL, history and result state do not change while the
  prompt is open.
- Completion emits one `ExecuteParameterized` effect; no automatic second run
  is possible.

## Plain and CLI contract

- Plain mode uses the hidden terminal input boundary for each name and writes
  prompts only to its message stream.
- If plain mode cannot establish interactive input, it refuses to run the
  parameterized statement and explains the environment-backed route.
- `query` accepts repeatable `--param-env NAME=VARIABLE` declarations.
- The non-interactive command rejects missing, duplicate, extra or malformed
  mappings, and an unset variable, before configuration target resolution.
- No raw parameter-value command-line option exists.
- stdout remains only the selected data format; prompts and diagnostics remain
  on stderr.

## Error and history contract

Runtime binding requires a fresh `SHOW standard_conforming_strings` value of
exactly `on`; otherwise the request is refused with value-free query diagnostic
wording and advice to restore `on` separately. The client never changes the
setting. Buffered execution checks every separately sent statement and retains
a successful prefix once. Streaming keeps its single Query message and initial
check. User-SQL methods on one session serialize the check/bind/send operation;
cancellation remains independent. CLI mapping validation remains pre-connection.

The distinct boundaries follow [PostgreSQL 18 message flow](https://www.postgresql.org/docs/18/protocol-flow.html),
section 54.2.2.1, checked 2026-10-25: initial parsing covers the complete Query
message, and splitting such a message changes its implicit transaction scope.
The string-setting semantics are documented in [PostgreSQL 18 compatibility settings](https://www.postgresql.org/docs/18/runtime-config-compatible.html#GUC-STANDARD-CONFORMING-STRINGS),
checked the same date. An earlier connection-time snapshot is insufficient
because SQL can change the setting later in the session.

- The template is recorded after a completed job when history is enabled.
- Parameter values never appear in history, debug, logs, diagnostics, palette
  entries, notices or machine output.
- Parameterized execution/export diagnostics retain classification, validated
  SQLSTATE, statement ordinal and numeric server position; server-controlled
  prose/object fields are omitted with an explicit privacy explanation. Safe
  cause/action text comes from the built-in SQLSTATE guidance. This is omission
  of untrusted text, not an attempt to recognise every transformation of a value.
- From the first parameterized send until disconnect, notices contain fixed
  privacy wording and validated SQLSTATE only. The policy is session-scoped
  because delayed notices cannot be reliably associated with a request. Query
  and export rows remain intentional output; unparameterized errors retain
  their existing detailed diagnostic fields.
- A query position from expanded SQL is retained as server technical data but is
  not mapped into the template. The UI and plain mode say why.
- Cancellation and connection-loss outcomes reuse the existing server-confirmed
  behavior and never retry.
