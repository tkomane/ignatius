# Data model: A plan you can read

## Plan document

One bounded response for the statement under the cursor.

| Field | Meaning | Validation |
| --- | --- | --- |
| `analyzed` | Whether PostgreSQL executed the target statement while collecting metrics | Set by the request, never inferred from optional fields |
| `root` | Top-level plan node | Required for a successful document |
| `planning_time_ms` | Server-reported planning time | Optional; absent stays unavailable |
| `execution_time_ms` | Server-reported total execution time | Optional; normally present for analyzed output |
| `node_count` | Number of retained nodes | At most 500 |
| `truncated` | Whether response size, depth, or node count limited the view | Drives explicit UI wording |
| `attention_path` | Derived path to the largest-cost or slowest node | Empty when the relevant metric is unavailable |

## Plan node

| Field | Meaning | Validation |
| --- | --- | --- |
| `node_type` | PostgreSQL operation name | Required, non-empty, display-sanitised at render time |
| `relation` | Relation name when supplied | Optional |
| `index` | Index name when supplied | Optional |
| `startup_cost`, `total_cost` | Planner cost units | Optional numeric values |
| `plan_rows`, `plan_width` | Estimated rows and row width | Optional numeric values |
| `actual_startup_time`, `actual_total_time` | Per-loop observed milliseconds | Optional, only supplied by ANALYZE |
| `actual_rows`, `actual_loops` | Per-loop observed rows and loop count | Optional, only supplied by ANALYZE |
| `facts` | Bounded known supplementary fields such as conditions, sort keys, and buffers | Safe strings and values, no credentials |
| `children` | Ordered child nodes | Recursively bounded by document limits |

## Derived node interpretation

- `estimate_ratio = actual_rows / plan_rows` when both are positive and actual
  metrics exist.
- `estimate_mismatch` is true when the ratio is at least 10 or at most 0.1.
- `attention_basis` is `estimated cost` for a plain plan and `measured time`
  for an analyzed plan.
- A missing, zero, or non-finite denominator produces `not comparable`, not a
  fabricated ratio.

## Plan session

The application-owned view state for one request:

| State | Meaning |
| --- | --- |
| `Hidden` | No plan is being shown |
| `Loading` | A plan request is in flight with a job identity |
| `Ready` | A valid bounded plan document is available |
| `Failed` | The request or parser failed; headline and next action are retained |

Ready state also holds the selected node path and collapsed node paths. These
are local presentation state and are reset when a new plan request starts or
the plan is dismissed.

## Analysis confirmation

A one-shot in-memory decision containing the source statement, its advisory
impact classification, and whether the request is `EXPLAIN` or `EXPLAIN
ANALYZE`. It is removed on confirmation or cancellation and is never written to
history or configuration.
