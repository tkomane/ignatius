# ADR-0008: Hand-rolled release workflow first, cargo-dist deferred

- Status: Accepted baseline; provider decisions remain gated
- Date: 2026-08-15
- T031 update: 2026-08-16

## Context

Release identity must distinguish product version, source revision, build
identity, target triple and artefact checksum. A rebuild must not invent a new
version. Feature 008 also needs a dependency inventory and separate signature
and provenance evidence without turning a local rehearsal or a passing build
into a release claim.

## Decision

- Release identity is captured at build time by `build.rs` into distinct
  constants: version from `Cargo.toml`, revision and cleanliness from git, target
  from cargo, build identity from `IGNATIUS_BUILD_IDENTITY` (defaulting to
  `local`), and compiler version. `ignatius version --verbose` prints all of them
  separately.
- Distribution is phased: target-specific release archives with checksums for
  macOS, Windows and Linux first; Homebrew tap and Scoop manifest second;
  Winget only once identity and installer behaviour are stable. A target is
  covered only when its own artefact and evidence row exist; coverage is not
  inferred from another target or from compilation alone.
- The initial checksum contract is SHA-256. Each manifest entry records the
  exact artefact filename, target identity, algorithm and digest, and the
  digest is verified against the exact retrieved bytes. A checksum establishes
  byte integrity, not builder identity or trusted inputs.
- The dependency inventory is a provider-neutral release-candidate contract,
  not an inventory of this worktree. It covers direct and transitive inputs,
  including target-specific and optional inputs that are active for the
  candidate, and binds them to the source revision, product version, target,
  build identity and toolchain. Its authoritative inputs are the manifests,
  committed lockfile, declared toolchain file, build configuration and source
  identity. The minimum generation contract uses locked Cargo metadata, the
  repository's supply-chain policy checks, input digests, path
  normalisation and fail-closed privacy checks. The final machine-readable
  inventory or SBOM format and its generation owner are not selected here.
- Signature and provenance are separate evidence classes. Each future evidence
  unit must bind to the same product version, source revision, target, exact
  artefact name and declared checksum. Signature verification additionally
  requires the approved verification identity; provenance verification binds
  the builder, build type, relevant manifest and lockfile identity, verifier
  and controlled retained reference. Neither may rely on an inferred branch,
  tag, date or CI run number.
- Signature status is explicit per artefact: `not-configured`, `unsigned`,
  `verified`, `invalid` or `expired`. Only `verified` may contribute to a
  ready decision; the other states block readiness as applicable. Provenance
  status is explicit per artefact: `not-configured`, `missing`, `verified`,
  `mismatch` or `invalid`, with only `verified` contributing to readiness.
- Rehearsal evidence is labelled as rehearsal or blocked and is never called
  published. A live publication, signing or provenance workflow requires an
  owner-approved decision and retained, independently repeatable evidence.
- `cargo-dist` is deferred rather than rejected. Adopting it before there is a
  release to automate would mean inheriting its assumptions before knowing which
  ones matter.

## Alternatives considered

- **cargo-dist now.** Mature and would save work later. Deferred until the
  release actually exists and its signing and provenance support can be evaluated
  against a real pipeline rather than a description of one.
- **Deriving the version from git tags.** Rejected outright by principle VIII: it
  makes a rebuild produce a different product version.

## Decision gates

The following choices are intentionally unresolved and must remain explicit
before a live release workflow is enabled:

- the final dependency-inventory or SBOM format and generation/verification
  ownership;
- the signing provider or offline mechanism, verification identity, key
  custody, rotation, expiry, revocation, failure handling, retention location
  and independent verification command;
- the provenance format, builder identity, retention policy and independent
  verification route; and
- owner authorization for the live release and distribution route.

No provider, key service, release host or owner approval is implied by this
record. Until each applicable gate is resolved, the corresponding evidence
state is `not-configured` or another explicit blocking state, not an omitted
field and not a passing claim.

## Consequences

- The repository may document the provider-neutral inventory, signature and
  provenance contracts and their blocked states. It must not claim that an
  artefact is signed, that provenance is verified, that an inventory or SBOM
  has been generated, or that a package manager is available without the
  corresponding evidence.
- Cross-platform artefacts are produced by CI, not from the development machine,
  which has only the host target installed.
- The inventory does not include database targets, configuration, SQL, result
  data, exported files, terminal scrollback, credential stores, secrets,
  unrestricted logs or operator-specific paths. Provider outages and missing
  verification routes block readiness rather than weakening the claim.

## Evidence basis

This T031 update is limited to decisions already recorded in the Feature 008
planning and evidence contracts:

- `specs/008-release-experience/research.md` - archive-first distribution,
  SHA-256 checksums, provider-neutral inventory/signing/provenance decisions
  and open gates.
- `specs/008-release-experience/plan.md` - release scope, trust boundaries,
  ownership separation and the prohibition on unapproved signing or
  publication dependencies.
- `specs/008-release-experience/tasks.md` - T017, T018, T019, T020 and T031
  status and the non-publishing delivery boundary.
- `release-evidence/inventory.md` - dependency-inventory inputs, minimum
  record, privacy boundary and fail-closed verification rules.
- `release-evidence/README.md` - provider-neutral signature/provenance fields,
  status values, binding rules and the provider decision gate.

## Reversibility

High. Nothing has been released.
