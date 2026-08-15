# Roadmap

Order is by dependency and by risk, not by appeal. Each feature is a thin
vertical slice that leaves the product usable.

| Feature | State | What it adds |
| --- | --- | --- |
| 001 Foundation and vertical slice | **Implemented, macOS-verified** | Architecture, terminal lifecycle, connection, execution, cancellation, CLI contract, doctor |
| 002 Object navigation | **Implemented** | Object tree, command palette, chord popup, breadcrumbs, object symbolism. Built ahead of the libpq migration at the owner's request; it touches no credential route, so the two do not overlap |
| 001a libpq migration | **Next**, per ADR-0009 and its decision gates | Enterprise authentication only if GSSAPI, Kerberos or Windows SSPI remains required; `.pgpass`, service files, `verify-ca` and client certificates are already implemented natively |
| 003 Credential routes | **Password and service files done** | `.pgpass` and `pg_service.conf`, the driver-independent half of the connection experience |
| 002 Connection experience and secrets | Mostly done | Profiles, password files, service files, TLS states, connection diagnostics and a password prompt in the client are in (`specs/009-connection-profiles/spec.md`). Remaining: an OS credential store and a first-run journey |
| 003 Delightful query loop | **Implemented** | Query jobs, transaction state, the command palette, a real editor with undo and word movement, syntax colouring, and a statement history with privacy controls (`specs/005-sql-editing/spec.md`, `specs/006-statement-history/spec.md`) |
| 004 Result exploration and export | Mostly done | Streaming export, the expanded row view, the cell inspector and result filtering are in. Remaining: copying a value out, which needs a decision on the clipboard route (see `specs/004-result-inspection/spec.md`) |
| 005 PostgreSQL object explorer | **Implemented** | The tree with indexes and extensions in it, the palette, object definitions, dependency navigation, and a dedicated read-only connection for the tree (`specs/002-ide-navigation/spec.md`, `specs/007-object-definitions/spec.md`) |
| 006 Production-aware safety | **Implemented** | Environment policy, write protection the server enforces, advisory statement classification, and a confirmation that asks for the database's own name before anything destructive. Privacy sessions are in: recording can be paused for a run or a session, and a paused session says so |
| 007 Accessibility and terminal hardening | **Implemented** | Plain line-oriented mode, no-colour, ASCII and narrow layouts, colour and glyph choice from configuration or flags, and configurable key bindings. Remaining: hand-verification with a screen reader and on Windows Terminal, which is evidence rather than code |
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
| Full-screen interface remains hard for screen readers | High | Medium | Addressed: `--plain` is a full client with no full-screen interface. Still to do: hand-verification with VoiceOver and NVDA |
| Terminal differences cause restoration failures in the wild | Medium | High | One abstraction, ordering test, pty evidence, automated restoration test in CI (open) |
| A dependency or release artefact is compromised | Low | High | Committed lockfile, pinned toolchain, advisories and licence checks; no signing claim until real |
