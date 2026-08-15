# Data Model: Plain line-oriented terminal mode

The feature introduces no persisted data. These are the transient concepts that
must remain coherent across input, execution and output.

## Input transcript

| Field | Meaning | Rules |
| --- | --- | --- |
| buffered lines | Lines entered for the current SQL statement | Retained until a PostgreSQL-aware boundary is complete; never sent as a partial statement |
| continuing | Whether the buffer contains non-whitespace input | Selects the continuation prompt |
| command | A supported session command at the start of a statement | `\\?`, `\\c` and `\\q` are handled locally; unknown commands explain themselves |
| end-of-input | Ctrl+D or closed stdin | Exits successfully without waiting for more input |

## Connection summary

| Field | Meaning | Display rule |
| --- | --- | --- |
| database | Database reached by the session | Always named in the connection summary and prompt |
| user | PostgreSQL role | Displayed as existing connection information allows |
| server version | Server identity | Displayed in the connection summary |
| environment | Local, non-production or production classification | Production is written as `PROD` or its existing word label |
| posture | Read-only or read-write session posture | Written as words |
| TLS state | What the server reports about encryption and verification | Never inferred from the requested mode |

## Production confirmation

| Field | Meaning | Rules |
| --- | --- | --- |
| impact | Advisory classification of the pending SQL | Existing classifier is the authority |
| required answer | Database name for destructive writes, `yes` for lesser writes | Exact database name is required for destructive writes |
| answer | Text entered by the user | Whitespace is trimmed; any non-accepted answer cancels |
| decision | Accepted or cancelled | SQL is sent only after acceptance |

## Plain execution outcome

| Field | Meaning | Rules |
| --- | --- | --- |
| result data | Rows and command summaries | Written to the data stream using the existing output contract |
| notices | Server notices | Written to the message stream, never mixed into result data |
| diagnostic | Layered failure explanation | Headline, attempted action, likely cause, next action and optional technical detail |
| status | Completed, failed, cancelled or connection outcome unknown | Wording must match what the server confirmed |
| transaction state | Autocommit, open, failed or unknown | Comes from the existing server-backed execution result |
| elapsed time | Client-observed execution duration | Reported as existing outcome text |

## Relationships

- One input transcript produces zero or one session command, or one plain
  execution outcome when a statement completes.
- One connection summary governs every prompt in the session.
- A production confirmation belongs to one pending execution and is discarded
  after acceptance or cancellation.
- One execution outcome can carry multiple statement results, notices and one
  transaction state, as defined by the existing result model.
