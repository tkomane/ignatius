# Implementation plan for Feature 061

## Input and execution

```text
ignatius fleet --baseline @dev --candidate @staging --candidate @prod --schema public --format json
```

`src/cli/fleet.rs` owns argument validation, profile resolution and the
async runner. Keep command registration small. Read the existing config and
capture the process environment once. Apply the selected profiles with
`resolve_target_and_profile` and the shared connection resolver. Default
an omitted profile port to 5432. Only password-file entries matched to the
target and profile cloud credentials can authenticate across the fleet. Keep
the CA trust path and filter out ambient route, TLS mode and identity
overrides. A missing or incomplete named profile produces a result for that
profile without touching the network. Start bounded concurrent probes and
preserve input order. Timeout drops the session and the existing provider
process if authentication is still active. There is no retry.

## Capture and trust boundaries

`src/postgres/fleet.rs` reads fixed fields through a narrow
`Session::fleet_probe` adapter. Verify read-only from the existing session
bootstrap and in the report transaction. Use a read-only repeatable-read
catalog transaction per target and savepoints to retain independent results
after catalog errors. Each statement has a 2-second server timeout under
the 15-second probe budget. The same catalog transaction covers the schema
and extension sections. Effective settings describe this connection, and
are compared as reported with their units.

Pass the optional schema as a bind parameter and check it exists. Use the
Feature 057 extractor's ordered row caps. For fleet probes, bound large
deparsed definitions and nested catalog arrays at the PostgreSQL boundary
and mark a capped snapshot truncated. Use at most 4 KiB per retained
definition and 128 entries per enum-label or policy-role array. An
over-limit snapshot is unavailable for equality and supplies no partial
fingerprint. The ordinary Feature 057 route retains its earlier limits.

Query only extension names and versions, with a 257-row probe for overflow.
Admit only bounded simple ASCII identifiers and versions. Query only the
eight numeric or Boolean settings in the specification by bind parameter,
and validate their values, units and complete presence. Version/recovery
facts are typed. Machine output receives only fixed failure codes, so
vendor error messages, connection parameters and notice details stay out
of every renderer.

## Comparison and output

`src/fleet/mod.rs` is pure. Reduce schema differences to the canonical
fingerprint and three severity counts. Compare extension and setting maps
in key order, preserving both values on difference. Each unavailable side
carries a reason for that dimension. Produce format version 1 JSON with
ordered targets and comparisons. Plain text and table expose the same
outcomes. Keep status (`complete`, `partial`, `unavailable`) distinct
from comparison (`match`, `drift`, `unavailable`). A TLS observation of
unknown remains unknown; it does not imply either cleartext or verified TLS.

Test the pure model in `src/fleet/tests.rs` and subprocess/live behavior
in `tests/fleet_contract.rs`. Integration owner extends hosted fixture
coverage and updates README, compatibility, status and release notes.
There are no new dependencies, credentials, network destinations, TUI
actions or configuration fields.

## Source contracts

- [Feature 057 snapshot](../057-schema-snapshots-and-drift-guard/spec.md)
- [PostgreSQL 14 transaction access and isolation](https://www.postgresql.org/docs/14/sql-set-transaction.html)
- [PostgreSQL 14 effective runtime settings](https://www.postgresql.org/docs/14/view-pg-settings.html)
- [PostgreSQL 18 installed extensions](https://www.postgresql.org/docs/18/catalog-pg-extension.html)
