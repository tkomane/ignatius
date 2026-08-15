# ADR-0008: Hand-rolled release workflow first, cargo-dist deferred

- Status: Accepted
- Date: 2026-08-15

## Context

Release identity must distinguish product version, source revision, CI build
identity, target triple, artefact checksum and signature. A rebuild must not
invent a new version.

## Decision

- Release identity is captured at build time by `build.rs` into distinct
  constants: version from `Cargo.toml`, revision and cleanliness from git, target
  from cargo, build identity from `IGNATIUS_BUILD_IDENTITY` (defaulting to
  `local`), and compiler version. `ignatius version --verbose` prints all of them
  separately.
- Distribution is phased: GitHub release archives with checksums for macOS,
  Windows and Linux first; Homebrew tap and Scoop manifest second; Winget only
  once identity and installer behaviour are stable.
- `cargo-dist` is deferred rather than rejected. Adopting it before there is a
  release to automate would mean inheriting its assumptions before knowing which
  ones matter.

## Alternatives considered

- **cargo-dist now.** Mature and would save work later. Deferred until the
  release actually exists and its signing and provenance support can be evaluated
  against a real pipeline rather than a description of one.
- **Deriving the version from git tags.** Rejected outright by principle VIII: it
  makes a rebuild produce a different product version.

## Consequences

- No claim of macOS notarisation, Windows signing, SBOM, provenance or
  package-manager availability may appear anywhere until a live release proves
  it. Nothing in this repository currently claims any of them.
- Cross-platform artefacts are produced by CI, not from the development machine,
  which has only the host target installed.

## Reversibility

High. Nothing has been released.
