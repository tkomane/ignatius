# Roadmap

Order is by dependency and by risk, not by appeal. Each feature is a thin
vertical slice that leaves the product usable.

| Feature | State | What it adds |
| --- | --- | --- |
| 001 Foundation and vertical slice | **Implemented, macOS-verified** | Architecture, terminal lifecycle, connection, execution, cancellation, CLI contract, doctor |
| 002 Connection experience and secrets | Next | Profiles, first-run journey, credential store, `.pgpass`, service files, password prompting, TLS states, connection diagnostics |
| 003 Delightful query loop | Planned | Real editor, command palette, query jobs, history with privacy controls, transaction state |
| 004 Result exploration and export | Planned | Virtualised grid, type-aware cells, expanded row view, cell inspector, streaming export, partial files |
| 005 PostgreSQL object explorer | Planned | Permission-aware metadata, schemas and objects, DDL inspection, dependencies, search |
| 006 Production-aware safety | Planned | Environment policy, write protection, advisory statement classification, confirmation ergonomics, privacy sessions |
| 007 Accessibility and terminal hardening | Planned | Plain line-oriented mode, configurable keymaps, colour and Unicode overrides, terminal recovery matrix |
| 008 Release experience | Planned | Release-note catalogue, packaged artefacts, checksums, SBOM, signing and provenance, install and upgrade docs |

## Before any public release

- **Finish the naming work.** The availability check is done and the name is
  confirmed; a trademark search and a domain are outstanding. See
  `docs/product/landscape.md`.
- **Verify on Windows and Linux.** Currently authored but unverified.
- **Run the full server matrix**, PostgreSQL 14 through 18.
- **Publish a working private vulnerability reporting route.**

## Risk register

| Risk | Likelihood | Impact | Response |
| --- | --- | --- | --- |
| The pure-Rust driver's missing libpq surface (`verify-ca`, service files, client certificates) keeps growing | Medium | High | Refuse loudly rather than approximate; revisit libpq in ADR-0003 if the list stops shrinking |
| Cross-platform behaviour diverges because only macOS is exercised in practice | High | High | CI matrix on every push; treat Windows and Linux claims as unproven until it runs |
| A single-maintainer project stalls | Medium | High | Repository is the source of truth; specs, tasks and status make resumption cheap |
| Scope creep into a generic database client | Medium | High | Constitution principle II; non-goals are explicit in the landscape document |
| Full-screen interface remains hard for screen readers | High | Medium | Plain line-oriented mode is a Feature 007 requirement, not a nice-to-have |
| Terminal differences cause restoration failures in the wild | Medium | High | One abstraction, ordering test, pty evidence, automated restoration test in CI (open) |
| A dependency or release artefact is compromised | Low | High | Committed lockfile, pinned toolchain, advisories and licence checks; no signing claim until real |
