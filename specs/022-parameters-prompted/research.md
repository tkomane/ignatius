# Research: Prompted query parameters

## Decision 1: Keep user SQL on the existing simple-query path

**Decision**: Recognize named placeholders locally, then expand each answer as
an escaped PostgreSQL text literal immediately before execution. Keep the
existing simple-query result path and do not add generic extended-protocol row
decoding in this feature.

**Rationale**: ADR 0003 and `src/postgres/session.rs` make server-rendered text
the deliberate fidelity boundary. Extended protocol parameters are attractive
for injection resistance, but arbitrary `tokio-postgres::Row` values would need
type-specific decoding to retain the same behavior for numerics, timestamps,
arrays, ranges, JSON and extension types. Literal expansion preserves the
current text and streaming contracts when the value is escaped as data, and it
keeps the new secret-bearing path in one adapter boundary.

**Alternatives considered**:

- Extended protocol with `$1` parameters: rejected for this slice because it
  changes the arbitrary-type result contract and requires a larger driver
  adapter design.
- String concatenation without escaping: rejected because it would let a value
  add SQL syntax.
- A full SQL parser: rejected because the existing protected-region lexer is
  sufficient for placeholder discovery and a parser would add scope without
  improving the user contract.

## Decision 2: Recognize only `:name` outside protected SQL regions

**Decision**: A name starts with an ASCII letter or underscore and continues with
ASCII letters, digits or underscores. Scan single and double quotes,
dollar-quoted bodies, line comments, nested block comments and `::` casts as
protected or non-placeholder syntax. Distinct names retain first-use order and
repeated occurrences reuse one value. Limit distinct names to 64.

**Rationale**: This matches PostgreSQL's ordinary identifier shape closely,
avoids the most dangerous cast false positive, and keeps the prompt bounded and
predictable. Existing `statements` helpers already encode the quoting rules and
byte-safe traversal.

**Alternatives considered**:

- Prompt every colon token: rejected because strings, comments, casts and
  operators would create surprising prompts.
- Treat names case-insensitively: rejected because SQL identifiers and user
  intent can distinguish case, and silently merging values would be worse.
- Support `$1` or `?name` in the same feature: rejected to keep one explicit,
  discoverable syntax and avoid colliding with PostgreSQL's existing `$n`
  semantics.

## Decision 3: Use secret values for every prompt and environment binding

**Decision**: TUI and plain prompts hold answers in `secrecy::SecretString`.
The parameterized effect and runtime binding use a custom type with redacted
`Debug`. `--param-env NAME=VARIABLE` carries only the environment variable name
in arguments and reads the value after SQL validation, while raw `--param`
values are not added.

**Rationale**: A query value can be a password, token, personal identifier or
other sensitive data even when the SQL template is harmless. Treating every
answer as secret gives one predictable rule and avoids accidental debug,
history, screenshot or process-list disclosure.

**Alternatives considered**:

- Plain `String` prompt state: rejected by the repository's secret-handling
  constitution.
- Raw `--param NAME=VALUE`: rejected because command arguments can be observed
  by other local processes and may enter shell history.
- Prompting from non-interactive stdin: rejected because it can consume SQL or
  hang a pipeline; environment mappings are explicit and non-blocking.

## Decision 4: Do not map server positions for parameterized executions

**Decision**: Retain the server diagnostic and its position, but state in the
TUI and plain mode that the position belongs to the expanded request and no
template caret was guessed.

**Rationale**: A literal can be longer or shorter than `:name`, so applying the
server character offset to the template can mark a different token. A source
map could be built later, but a wrong marker is worse than an honest
unavailable location in this first slice.

**Alternatives considered**:

- Apply the raw position to the template: rejected as demonstrably unsafe.
- Build an expanded-to-template offset map now: deferred because it needs
  careful handling of Unicode characters, statement-relative positions and
  positions inside escaped literals; the current contract can be truthful
  without it.
