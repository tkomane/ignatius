# Roadmap

Order is by dependency and by risk, not by appeal. Each feature is a thin
vertical slice that leaves the product usable.

| Feature | State | What it adds |
| --- | --- | --- |
| 001 Foundation and vertical slice | **Implemented, macOS-verified** | Architecture, terminal lifecycle, connection, execution, cancellation, CLI contract, doctor |
| 002 Object navigation | **Implemented** | Object tree, command palette, chord popup, breadcrumbs, object symbolism. Built ahead of the libpq migration at the owner's request; it touches no credential route, so the two do not overlap |
| 001a libpq migration | **Next**, per ADR-0009 | Client certificates, `.pgpass`, service files and enterprise authentication, which the pure-Rust driver cannot provide |
| 003 Credential routes | **Password and service files done** | `.pgpass` and `pg_service.conf`, the driver-independent half of the connection experience |
| 002 Connection experience and secrets | After 001a | Profiles, first-run journey, credential store, `.pgpass`, service files, password prompting, TLS states, connection diagnostics |
| 003 Delightful query loop | Partly done | Query jobs and transaction state are in, and the command palette landed early in 002. Remaining: a real editor and history with privacy controls |
| 004 Result exploration and export | **Export done** | Streaming export with partial files is in. Remaining: expanded row view, cell inspector, filtering, copy |
| 005 PostgreSQL object explorer | Partly done in 002 | Remaining: DDL inspection, dependency navigation, indexes and extensions in the tree, a dedicated metadata connection |
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
| The libpq migration weakens the single-binary distribution story, especially on Windows | High | Medium | ADR-0009 requires the packaging answer before the migration ships, not after |
| FFI introduces memory-safety defects the rest of the codebase is designed to exclude | Medium | High | `unsafe` confined to the adapter behind safe wrappers, each with a stated invariant; prefer a vetted wrapper crate |
| Cross-platform behaviour diverges because only macOS is exercised in practice | High | High | CI matrix on every push; treat Windows and Linux claims as unproven until it runs |
| A single-maintainer project stalls | Medium | High | Repository is the source of truth; specs, tasks and status make resumption cheap |
| Scope creep into a generic database client | Medium | High | Constitution principle II; non-goals are explicit in the landscape document |
| Full-screen interface remains hard for screen readers | High | Medium | Plain line-oriented mode is a Feature 007 requirement, not a nice-to-have |
| Terminal differences cause restoration failures in the wild | Medium | High | One abstraction, ordering test, pty evidence, automated restoration test in CI (open) |
| A dependency or release artefact is compromised | Low | High | Committed lockfile, pinned toolchain, advisories and licence checks; no signing claim until real |
