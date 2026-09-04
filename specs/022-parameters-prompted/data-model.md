# Data Model: Prompted query parameters

## NamedPlaceholder

Represents one executable `:name` occurrence in the original SQL.

- `name`: ASCII identifier, case-sensitive.
- `start`, `end`: UTF-8 byte span in the original template.

Occurrences inside strings, quoted identifiers, dollar bodies, comments and
cast syntax are not entities.

## ParameterTemplate

The safe reusable query view produced by discovery.

- `sql`: original SQL, unchanged.
- `names`: distinct names in first-use order.
- `occurrences`: all named-placeholder spans, including repeats.

The distinct-name count is bounded at 64. Empty or comment-only SQL has no
template names.

## ParameterBindings

Ephemeral values for one execution attempt.

- ordered `(name, SecretString)` pairs matching a `ParameterTemplate`;
- no display or serialization of values;
- complete and unique before binding;
- dropped after the execution task finishes or is cancelled.

Values are always literal text data. Empty text is valid. NUL is invalid. No
NULL shorthand or type inference is part of this feature.

## ParameterPrompt

Interactive state while values are collected.

- safe template SQL and original statement source;
- safe ordered name list;
- active name index;
- current typed answer as `SecretString`;
- accepted answers as secret values;
- custom `Debug` that exposes names and progress only, never answer contents.

State transitions:

```text
Run requested
  -> production confirmation, when required
  -> no named names: ordinary execution
  -> ParameterPrompt(active = first name)
       -> type/backspace: edit current secret answer
       -> Esc/Cancel: discard all answers, return idle
       -> Enter: store current answer
            -> another name: ParameterPrompt(active = next name)
            -> last name: ParameterizedExecution effect
```

## EnvironmentMapping

Non-secret automation declaration:

- `name`: one discovered placeholder name;
- `variable`: environment variable name, never its value.

Every discovered name needs exactly one mapping. Duplicate, extra, malformed or
missing mappings fail before target resolution or connection work.

## ParameterizedExecution

One ordinary query job with an additional ephemeral `ParameterBindings` value.
The job identity, template SQL, statement count, transaction state, result cap,
cancellation and history outcome follow the existing execution model. History
receives `ParameterTemplate.sql`, never the expanded text.
