# libpq adapter contract

This contract describes the boundary the proposed native adapter must satisfy.
It is intentionally expressed in product terms so a wrapper choice cannot
silently change the application behaviour.

## Connect

Input: the resolved connection target, existing TLS policy, timeout and
configuration context.

Output: a session with database identity, authenticated identity, server
version, negotiated TLS state, read-only posture and environment classification.

Rules:

- Every selected authentication and TLS requirement is either honoured or
  refused explicitly.
- A missing native dependency or unsupported route is a connection diagnostic,
  not a fallback to a weaker route.
- Credentials and native handles do not appear in `Debug`, logs or diagnostics.

## Execute

Input: one SQL string, existing row cap and job identity.

Output: the existing `Execution` contract, including result sets, notices,
elapsed time, transaction state, status and layered diagnostic.

Rules:

- One session processes operations in submission order.
- Server text and SQLSTATE remain authoritative.
- A failed or unknown operation is not replayed automatically.
- Results stay within the existing memory and stream contracts.

## Cancel

Input: one active job identity.

Output: a server-confirmed cancelled execution, a cancellation failure, or an
unknown outcome if the connection is lost before the answer is known.

Rules:

- A second request for the same job is suppressed.
- Cancellation does not terminate the process or claim success prematurely.
- The final transaction state and diagnostic remain visible to the caller.

## Close

Input: normal shutdown, connection loss, timeout or adapter failure.

Output: all native resources released and no new operation accepted.

Rules:

- Close is safe to request more than once.
- Worker, command queue and native handles have a bounded shutdown path.
- An in-flight operation is reported according to the existing unknown-outcome
  rule; it is not described as committed or rolled back without evidence.

## Compatibility matrix

| Existing contract | Required migration result |
| --- | --- |
| Target precedence | Same meaning and same diagnostic for conflicts |
| Password and service files | Same routes and redaction |
| TLS modes | Same guarantee, no downgrade |
| Result formats | Same bytes for the same server result where the format contract allows |
| Notices and errors | Same stream and layered fields |
| Transaction state | Server-confirmed state, including failed and unknown |
| Cancellation | Server-confirmed outcome and existing exit meaning |
| Pure application layers | No native-driver dependency |
