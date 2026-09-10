# Data model: Explicit retained-result refresh

The feature adds no persisted entity. It adds one user intent and derives
eligibility from state already held by the interactive model.

## Retained result source

- **Owner**: `Model::last_sql` and `Model::last_execution`
- **Purpose**: identify the exact source behind the visible result
- **Rules**:
  - both a retained result set and source SQL must exist;
  - the source must split into exactly one non-empty statement;
  - the statement must be read-classified;
  - the source is never replaced by editor text or local grid state.

## Refresh eligibility

Derived, not stored. All of the following must be true:

| Fact | Required value | Why |
| --- | --- | --- |
| Focus | Results | The action belongs to the retained result surface |
| Query phase | Idle | A second job must not overlap the first |
| Plan view | Hidden | A plan is not the ordinary retained result |
| Connection | Usable | The execution boundary must be available |
| Transaction | Not failed | PostgreSQL must be able to accept the read |
| Result | A visible result set exists | There is an answer to refresh |
| Source | Exactly one read-classified statement | No ambiguous or mutating replay |

## Refresh execution

- **Identity**: the job allocated by the existing execution lifecycle
- **Input**: the retained source template
- **Optional input**: newly entered named-parameter bindings
- **Output**: the existing execution result, error, cancellation or unknown
  connection outcome
- **Persistence**: existing history rules only, with the safe template and one
  outcome per explicit request
- **State transition**:

```text
eligible retained result
        |
        v
explicit Refresh action
        |
        +--> named parameters -> masked prompt -> one execution
        |
        +--> no parameters ----> one execution
        |
        v
existing running / cancellation / outcome lifecycle
```

## Unavailable or refused state

No new durable state is created. The reducer reports a prerequisite or refusal
through the existing result/footer wording and emits no execution effect. The
editor and retained result remain unchanged.
