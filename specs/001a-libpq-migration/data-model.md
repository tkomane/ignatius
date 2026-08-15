# Data model: Enterprise PostgreSQL authentication

This is a design model for the adapter contract. It does not authorize a new
storage schema or expose native handles to the application.

## EnterpriseAuthenticationRoute

| Field | Meaning | Validation |
| --- | --- | --- |
| mechanism | The explicitly selected enterprise mechanism | Must be one approved by the owner and supported by the target platform |
| platform | macOS, Windows or Linux scope | Must match the build and server-backed evidence |
| prerequisites | Local identity, library or configuration requirements | Must be documented before the route is advertised |
| authenticated_identity | Role or identity reported by the server | Must come from the server, not a client guess |
| transport | Negotiated TLS and identity posture | Must preserve the requested TLS guarantee |

## AdapterSession

| Field | Meaning | Invariant |
| --- | --- | --- |
| lifecycle | Connecting, ready, closing, closed or lost | A closed or lost session accepts no new operation |
| operation_order | Monotonic order of submitted operations | One session has one server order |
| transaction_state | Autocommit, open, failed or unknown | The server is the authority |
| cancellation_state | None, requested, confirmed, failed or unknown | The client never calls a request confirmed before the server answer |
| notices | Server notices associated with an operation | Notices remain separate from result data and diagnostics |
| native_owner | The adapter-owned native connection and worker | No native handle crosses into `app`, `query` or `ui` |

## OperationEnvelope

| Field | Meaning | Invariant |
| --- | --- | --- |
| job | Existing job identity | A stale completion cannot overwrite a newer job |
| sql | The statement submitted by the user | Never logged or replayed automatically |
| sequence | Session-local submission order | Commands are processed serially |
| cancellation | Whether a cancel request was sent | At most one request is active for a job |
| outcome | Succeeded, failed, cancelled, connection lost or unknown | Unknown is retained when commit state cannot be established |
| diagnostic | Existing layered diagnostic, when any | Redacted before it leaves the adapter |

## NativeDependencyIdentity

| Field | Meaning | Invariant |
| --- | --- | --- |
| source | Bundled or host-provided library source | The release documentation states which one applies |
| version | Native library version or range | Verified by the build or startup check |
| architecture | CPU and operating-system target | Must match the shipped binary |
| loading_path | Trusted, documented lookup path | No silent unbounded search |
| evidence | Build, clean-install and runtime checks | No release claim without platform evidence |

## State transitions

```text
Connecting -> Ready -> Closing -> Closed
Connecting -> Failed
Ready -> Lost
Ready -> Closing

Operation submitted -> Running -> Succeeded
Operation submitted -> Running -> Failed
Running -> Cancellation requested -> Cancelled
Running -> Connection lost -> Unknown outcome
```

The adapter must not transition an operation directly from `Running` to
`Succeeded` merely because a local cancellation request was sent. The server's
final response determines the outcome.
