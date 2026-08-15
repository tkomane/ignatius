# 001a validation guide

This guide is a release-gate checklist. It cannot be completed from the
current macOS-only development session because enterprise authentication and
clean platform packaging require controlled environments.

## Before implementation

1. Read [ADR-0009](../../docs/architecture/decisions/0009-libpq-migration.md)
   and [ADR-0010](../../docs/architecture/decisions/0010-libpq-concurrency-model.md).
2. Record the owner's confirmation of the required enterprise route and target
   platforms in the ADR and this feature's task list.
3. Record the selected wrapper or binding, license, supported architectures,
   native dependency source and Windows packaging choice.
4. Keep the current driver as the comparison baseline until the parity gate
   passes.

## Adapter contract checks

Run the repository's normal fast gates after the adapter spike:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --test cli_contract
cargo test --test postgres_integration
```

Run the disposable services only for synthetic local checks. Do not place a
real credential in a command, file or test fixture.

## Enterprise authentication matrix

For every route the owner approves, record:

| Field | Evidence to capture |
| --- | --- |
| Platform and architecture | OS version, target and binary identity |
| Server | Controlled server version and authentication configuration |
| Local prerequisite | Identity setup or native dependency, without secret material |
| Client invocation | Redacted target and selected route |
| Result | Authenticated role, TLS state and query result |
| Failure cases | Missing prerequisite, wrong identity, unavailable route and no fallback |

The route is not complete until both success and failure evidence exist.

## Cancellation and lifecycle checks

1. Start a long synthetic statement.
2. Request cancellation once.
3. Confirm the server's final cancellation SQLSTATE and the existing user-facing
   wording.
4. Interrupt the connection during a transaction and verify unknown outcome is
   preserved.
5. Close normally and during an in-flight operation; confirm no worker or native
   handle remains and no statement is replayed.

## Clean-install checks

For macOS, Windows and Linux, from a clean environment:

1. Install the documented artifact and any explicitly documented native
   prerequisite.
2. Run `ignatius version --verbose` and record the build identity, target and
   native dependency identity.
3. Run a non-destructive connection check using a controlled server.
4. Remove or mismatch the native dependency and confirm the repairable
   diagnostic.
5. Record the exact evidence in `docs/operations/verification.md` before
   changing the roadmap or status to complete.
