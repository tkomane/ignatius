# Multi-target environment diff and fleet health

**Feature**: 061 | **Milestone**: 15 | **Specified**: 2026-11-19

Operators compare the deployed shape of PostgreSQL environments without
opening a privileged administration console or reading table rows. One run
reports which environments were reached, what differs from an explicit
baseline, and where the evidence is incomplete.

## User journeys

1. Compare the PostgreSQL 14-18 named staging profile against the named
   development baseline. Inspect schema fingerprints, extension changes,
   effective setting values with units, and server health facts.
2. Add a second candidate whose credentials or catalog access fail. Inspect
   successful comparisons alongside the unavailable target or section.
3. Use the JSON report in CI. Gate on the process result and distinguish
   complete equality, observed drift, and incomplete evidence.

## Requirements

- **FR-061-01** Select one explicit `--baseline @NAME` and one to three
  `--candidate @NAME` arguments. Names must be distinct ASCII profile slugs
  of 1-64 letters, digits, underscores, hyphens or periods. Refuse malformed,
  repeated and over-limit arguments before opening a connection. Resolve
  explicitly defined hosts, databases and users, with port 5432 as the
  profile default. Authenticate with scoped password-file entries or the
  profile's cloud provider. Use the profile or host-default TLS mode and
  allow a shared CA trust path. Keep ambient passwords and client identities
  scoped to single-target commands.
- **FR-061-02** Start no more than four probes concurrently. Each probe has a
  15-second budget covering authentication, connection and catalog reads.
  Use only fixed or parameterized catalog queries in a PostgreSQL read-only,
  repeatable-read transaction. Capture server version number, recovery
  posture, read-only confirmation and observed TLS state. Use the configured
  environment classification without exposing custom labels.
- **FR-061-03** Reuse the Feature 057 canonical snapshot and severity counts.
  An optional `--schema` applies to each schema capture, while settings and
  installed extensions keep their ordinary scope. An absent named schema,
  refused query, or bounded capture is unavailable for comparison. Report
  fingerprints and finding counts without SQL definitions or raw diagnostics.
- **FR-061-04** Compare the database's installed extensions by name and
  version from `pg_catalog.pg_extension`, to a maximum of 256 installations.
  Compare `effective_cache_size`, `jit`, `max_connections`,
  `max_parallel_workers_per_gather`, `random_page_cost`, `seq_page_cost`,
  `shared_buffers` and `work_mem` by effective value and unit from
  `pg_catalog.pg_settings`. An absent, invalid, over-limit or inaccessible
  section stays unavailable, with a bounded reason.
- **FR-061-05** Emit deterministic table, plain and versioned JSON summaries
  in input profile order, with a separate result for each candidate and
  dimension. Exit 0 for complete equality, 1 for drift or any incomplete
  evidence, 2 for wrong arguments and 3 for invalid command-wide
  configuration. A disconnected or incomplete profile cannot produce an
  equality claim. Continue collecting other targets and independent sections.
- **FR-061-06** Keep values local. Output may contain profile slugs,
  normalized health facts, schema fingerprints and counts, and bounded
  extension or effective setting differences. Exclude connection endpoints,
  raw error or notice text, settings outside the fixed allowlist, config file
  sources, and SQL/catalog definition text. Authenticate only through the
  existing profile and TLS machinery. Issue neither remediation statements
  nor mutations.

## Acceptance

- **SC-061-01** Invalid syntax and an incomplete profile cannot cause an
  environment or service fallback to another host. Missing profiles and
  refused connections remain distinguishable from available peers.
- **SC-061-02** Same-schema disposable PostgreSQL targets compare equal,
  report server-confirmed read-only and applicable TLS facts, and leave
  local history and database content untouched.
- **SC-061-03** A schema difference, extension version change or setting
  change is independently visible. An over-limit or inaccessible section
  cannot be equal even when its returned prefix matches the baseline.
- **SC-061-04** JSON and plain output are stable, bounded and free of raw
  diagnoses, URI credentials, terminal controls, and application rows.

This slice is a CLI report. Interactive overlays, saved fleet registries,
ongoing polling, synchronized snapshots across hosts, and automated repairs
are outside the slice.
