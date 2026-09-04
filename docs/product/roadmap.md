# Roadmap

Order is by dependency and by risk, not by appeal. Each feature is a thin
vertical slice that leaves the product usable.

| Feature | State | What it adds |
| --- | --- | --- |
| 001 Foundation and vertical slice | **Implemented, macOS-verified** | Architecture, terminal lifecycle, connection, execution, cancellation, CLI contract, doctor |
| 002 Object navigation | **Implemented** | Object tree, command palette, chord popup, breadcrumbs, object symbolism. Built ahead of the libpq migration at the owner's request; it touches no credential route, so the two do not overlap |
| 001a libpq migration | **Closed without implementation**, per ADR-0012 | The actual enterprise requirement is Entra token authentication over TLS, which uses the existing driver. GSSAPI, Kerberos and SSPI remain unsupported |
| 003 Credential routes | **Implemented** | `.pgpass` and `pg_service.conf`, the driver-independent half of the connection experience |
| 002 Connection experience and secrets | Mostly done | Profiles, password files, service files, TLS states, connection diagnostics and a password prompt in the client are in (`specs/009-connection-profiles/spec.md`). `ignatius config init` writes a starter file. Remaining: an OS credential store, which ADR-0011 puts to the owner rather than assuming |
| 003 Delightful query loop | **Implemented** | Query jobs, transaction state, the command palette, a real editor with undo and word movement, syntax colouring, and a statement history with privacy controls (`specs/005-sql-editing/spec.md`, `specs/006-statement-history/spec.md`) |
| 004 Result exploration and export | Mostly done | Streaming export, an export of what is on screen from the client, the expanded row view, the cell inspector and result filtering are in. Remaining: copying a value out, which needs a decision on the clipboard route (see `specs/004-result-inspection/spec.md`) |
| 005 PostgreSQL object explorer | **Implemented** | The tree with indexes and extensions in it, the palette, object definitions, dependency navigation, and a dedicated read-only connection for the tree (`specs/002-ide-navigation/spec.md`, `specs/007-object-definitions/spec.md`) |
| 006 Production-aware safety | **Implemented** | Environment policy, write protection the server enforces, advisory statement classification, and a confirmation that asks for the database's own name before anything destructive. Privacy sessions are in: recording can be paused for a run or a session, and a paused session says so |
| 007 Accessibility and terminal hardening | **Implemented** | Plain line-oriented mode, no-colour, ASCII and narrow layouts, colour and glyph choice from configuration or flags, and configurable key bindings. Remaining: hand-verification with a screen reader and on Windows Terminal, which is evidence rather than code |
| 008 Release experience | In planning, in `specs/008-release-experience/` | Release-note catalogue, packaged artefacts, checksums, SBOM, signing and provenance, install and upgrade docs |
| 012 Schema-aware completion | **Implemented on branch; focused and PostgreSQL 18.4 evidence recorded; locked verifier green with Unix-socket skip; platform hand checks pending** | One shared lexer-backed completion engine for the TUI and plain mode, catalogue snapshots with bounded visible lists, alias/schema/CTE scope, quoted one-step acceptance, truthful loading/stale/unavailable states, ASCII/no-colour output, and configurable automatic popups (`specs/012-schema-completion/spec.md`) |
| 017 Connection trust surface | **Implemented, locally verified; live provider evidence pending** | Read-only command-palette details for Entra, AWS, Google Cloud and configured providers: target, environment, server posture, observed TLS, credential lifetime caveat and safe remedy (`specs/017-connection-trust-surface/spec.md`) |

## Before any public release

- **Finish the naming work.** The availability check is done and the name is
  confirmed; a trademark search and a domain are outstanding. See
  `docs/product/landscape.md`.
- **Verify on Windows and Linux.** Currently authored but unverified.
- **Run the full server matrix**, PostgreSQL 14 through 18.
- **Exercise the built-in cloud providers live.** Entra, AWS and Google Cloud
  still need dated account and server evidence; the mechanism is tested against
  a real TLS PostgreSQL server, but that does not prove each vendor command.
- **Publish a working private vulnerability reporting route.**

## Risk register

| Risk | Likelihood | Impact | Response |
| --- | --- | --- | --- |
| A future Kerberos requirement could reopen native-driver packaging and weaken the single-binary distribution story | Low | Medium | ADR-0012 keeps the current self-contained driver; reopen the rejected migration only for a concrete requirement and reapply its packaging gates |
| FFI introduces memory-safety defects the rest of the codebase is designed to exclude | Medium | High | `unsafe` confined to the adapter behind safe wrappers, each with a stated invariant; prefer a vetted wrapper crate |
| Cross-platform behaviour diverges because only macOS is exercised in practice | High | High | CI matrix on every push; treat Windows and Linux claims as unproven until it runs |
| A single-maintainer project stalls | Medium | High | Repository is the source of truth; specs, tasks and status make resumption cheap |
| Scope creep into a generic database client | Medium | High | Constitution principle II; non-goals are explicit in the landscape document |
| Full-screen interface remains hard for screen readers | High | Medium | Addressed: `--plain` is a full client with no full-screen interface. Still to do: hand-verification with VoiceOver and NVDA |
| Terminal differences cause restoration failures in the wild | Medium | High | One abstraction, ordering test, pty evidence, automated restoration test in CI (open) |
| A dependency or release artefact is compromised | Low | High | Committed lockfile, pinned toolchain, advisories and licence checks; no signing claim until real |
