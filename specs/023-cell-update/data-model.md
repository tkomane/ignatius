# Feature 023 data model

## Pure query model

### `RelationReference`

- optional schema, for a bare or schema-qualified source relation
- exact PostgreSQL relation name after SQL identifier folding/unquoting

### `UpdateSource`

- `relation: RelationReference`
- `projected_columns: Vec<String>` in result order
- `target_column: String`
- `selected_result_column: usize`

The source contains names and positions only. It contains no row values.

### `UpdateColumn`

- `name: String`
- `primary_key: bool`

This is the small safe metadata shape the pure planner needs from the
PostgreSQL adapter.

### `UpdatePlan`

- target column and ordered primary-key names
- generated SQL template with quoted identifiers and private named parameters
- secret `ParameterBindings` for the replacement and key values

Its debug representation exposes names and the template shape but not bindings
or values.

## Live adapter model

### `UpdateRelation`

- resolved schema and relation name
- `ObjectKind`
- `readable: bool`
- `writable: bool`
- ordered `ColumnInfo` values, including primary-key membership

The adapter returns this from a bound catalogue query and never copies a
password, token or process credential.

## Application state

### `UpdateCandidate`

- `result_job: JobId`
- retained source row index
- source result column index
- source SQL snapshot

It is the stale-state guard for both the value prompt and final confirmation.

### `UpdateLookup`

- monotonically allocated request identity
- `UpdateCandidate`
- parsed `UpdateSource`

It exists while live relation metadata is loading.

### `UpdatePrompt`

- candidate and source
- resolved `UpdateRelation`
- selected source column
- secret replacement input

Only the prompt renderer may expose the typed replacement, and only because the
user is actively entering it. Its debug output is redacted.

### `PendingUpdate`

- candidate identity
- resolved relation metadata
- `UpdatePlan`

It is the review state. Enter performs one final identity and posture check,
then transfers the template and bindings to `ExecuteParameterized`.

## State transitions

```text
ready result
  -> GenerateCellUpdate
  -> UpdateLookup + LoadUpdateTarget
  -> UpdatePrompt
  -> PendingUpdate review
  -> ExecuteParameterized
  -> ordinary ExecutionFinished lifecycle
```

Every path out of `UpdateLookup`, `UpdatePrompt` or `PendingUpdate` clears the
ephemeral state. No path reruns the source `SELECT` automatically.
