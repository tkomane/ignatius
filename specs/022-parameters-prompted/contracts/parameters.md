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

- The template is recorded after a completed job when history is enabled.
- Parameter values never appear in history, debug, logs, diagnostics, palette
  entries, notices or machine output.
- A query position from expanded SQL is retained as server technical data but is
  not mapped into the template. The UI and plain mode say why.
- Cancellation and connection-loss outcomes reuse the existing server-confirmed
  behavior and never retry.
