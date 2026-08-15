# Quickstart: Release-readiness rehearsal

This guide validates the release contract without publishing anything. The
current repository has identity and verification commands, but not yet the
packaging/catalogue commands described in the later task phases. Those steps
are labelled explicitly so a rehearsal cannot be mistaken for a release.

## Preconditions

- A clean checkout of the intended source revision.
- Rust and the repository's normal development prerequisites.
- Docker Desktop only when the full PostgreSQL gate is required.
- Synthetic database credentials from `docker/dev.env`; never substitute a
  real credential in a command, fixture or evidence bundle.
- No release credentials, signing keys or remote publication access are needed
  for this rehearsal.

## 1. Establish the source identity

Run:

```bash
git rev-parse HEAD
git status --short
cargo run --quiet -- version --verbose
```

Expected outcome:

- The revision is recorded exactly.
- The working tree is either empty or explicitly labelled modified.
- Version, source revision, source state, target, build identity and compiler
  are displayed as separate facts.
- No password, connection URI, SQL text or result value appears.

For a local rehearsal, the build identity may be made explicit without claiming
CI provenance:

```bash
IGNATIUS_BUILD_IDENTITY=local-rehearsal cargo run --quiet -- version --verbose
```

## 2. Run the repository gates

For a complete local result, start the disposable services and run:

```bash
cargo xtask db up
cargo xtask verify
```

Record separately:

- formatting and lint results;
- unit/layout and CLI contract counts;
- PostgreSQL integration count and server version;
- terminal or platform evidence that was not covered by the command.

If the database is not running, the integration gate is a skip, not a pass.
Stop the disposable services after the rehearsal with `cargo xtask db down` when
they are no longer needed.

## 3. Validate the planned artefact contract

The following is the definition of the future candidate check. It is not a
currently available command until the implementation tasks create the release
manifest and validator.

For each support-matrix row, the future validation must confirm:

1. The filename contains the product version and exact target.
2. The artefact target matches the build identity.
3. The recorded SHA-256 digest matches the bytes on disk.
4. The dependency inventory, signature status and provenance status refer to
   the same source revision and target.
5. Missing or mismatched evidence produces `blocked`, never `ready`.

Deliberate negative scenarios must cover a changed byte, changed target,
changed version, missing artefact, missing inventory, invalid signature,
provenance mismatch and dirty source revision.

## 4. Rehearse installation and recovery

For macOS, Windows and Linux separately, capture the following evidence against
the exact candidate target:

1. The prerequisite check and selected artefact.
2. The integrity verification result.
3. The first-start identity output.
4. The version after upgrade from the previous known-good build.
5. What happened to configuration and user data.
6. Rollback to the previous version after an intentionally failed start.
7. The diagnostic action for an unsupported platform, architecture or missing
   prerequisite.

Do not claim hand verification from a CI build. Record the runner, operating
system, terminal, architecture and PostgreSQL server version for each evidence
item where relevant.

## 5. Review the support identity bundle

The future support bundle must contain only identity and safe diagnostics. A
reviewer should be able to answer:

- Which product version and target is installed?
- Which source/build identity produced it?
- Which operating system and terminal are involved?
- Which exit code and error category occurred?
- What is the next action?

It must not contain a password, token, full connection URI, SQL text, result
value or credential-file content. Use synthetic fixtures for negative tests.

## Definition of done for this feature

- The release record and catalogue validator reject version, target, checksum
  and evidence mismatches.
- Every support-matrix row is covered or explicitly blocked.
- SBOM, signing and provenance status are visible and provider-neutral until
  their decisions are approved.
- Clean install, upgrade and rollback evidence exists for every supported
  platform claim.
- Release notes and compatibility claims identify their evidence class.
- A live publication is still a separately authorized operation.
