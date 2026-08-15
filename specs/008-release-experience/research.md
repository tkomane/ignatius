# Research: Governed release experience

**Status**: Repository-grounded planning research. No release, artefact,
checksum, SBOM, signature or provenance claim is being made by this document.

**Evidence checked**: 2026-08-16, against the current shared worktree at
`060380c` plus the uncommitted Claude source slice. The source slice is outside
this feature's ownership.

## Decision 1: Keep release notes and release identity separate

**Decision**: `CHANGELOG.md` remains the human-readable source for release
notes. A future structured release catalogue may index one version to its
source revision, artefacts and evidence, but it must not duplicate the full
notes text or become a second authority for product claims.

**Rationale**: The repository already describes the changelog as the single
source of release notes. A machine-readable index is still needed for
deterministic artefact and evidence validation, but duplicating notes would
create drift.

**Alternatives considered**:

- Put all release metadata in the changelog. Rejected because checksums,
  target rows and evidence status are structured data and need validation.
- Make the catalogue the release-note authority. Rejected because it would
  split the user-facing prose contract from the existing changelog.

## Decision 2: Use one release record per version

**Decision**: A release record is keyed by product version and links exactly
one source revision, one release-note entry and a set of target-specific
artefacts. A preview or rehearsal build has its own explicit status and cannot
reuse a stable release record.

**Rationale**: The constitution treats product version, source revision, build
identity, target and working-tree state as distinct facts. One record per
version makes reuse, rollback and support tracing visible.

**Alternatives considered**:

- Infer the version from a tag or CI run number. Rejected because a rebuild
  would change product identity and violate the one-source-of-truth rule.
- Maintain one mutable "latest" record. Rejected because it makes historical
  support and rollback ambiguous.

## Decision 3: Make SHA-256 the initial checksum contract

**Decision**: The first checksum manifest should record a SHA-256 digest for
  every artefact, together with the exact filename and target identity. The
  algorithm name is part of the manifest, not an assumption in the verifier.

**Rationale**: A widely available, cryptographic digest is sufficient for
  accidental corruption and ordinary distribution integrity in the first
  release workflow. The explicit algorithm field keeps future migration
  possible without changing the meaning of old records.

**Alternatives considered**:

- Use file size or a non-cryptographic hash. Rejected because neither provides
  an adequate integrity signal for a release artefact.
- Record only a digest without the target or filename. Rejected because it
  permits an otherwise valid digest to be attached to the wrong artefact.

## Decision 4: Treat target coverage as an evidence matrix

**Decision**: The support matrix is the required set of platform/architecture
  rows. Each row is either backed by a matching artefact and evidence bundle or
  is explicitly blocked with a reason. No target is inferred from another
  target's build.

**Rationale**: The compatibility document already distinguishes macOS,
  Windows and Linux evidence. CI build success is not hand-use evidence, and a
  host build cannot prove a cross-platform release.

**Alternatives considered**:

- Publish only the developer's host artefact. Rejected because it would turn an
  environment limitation into an unsupported platform claim.
- Treat a target as covered because it compiles in CI. Rejected because
  installation, terminal use and dependency behaviour need their own evidence.

## Decision 5: Keep SBOM, signing and provenance provider-neutral until approved

**Decision**: The release contract requires an inventory, signature status and
  provenance identity, but does not select a provider, key service, release
  host or inventory format in this planning package. Those choices are explicit
  decision gates before a live release workflow is enabled.

**Rationale**: The current ADR and release guide correctly state that these
  capabilities are absent. Choosing a provider without owner authorization
  would create a security and operational commitment outside the requested
  documentation slice.

**Required evidence for the later decision**:

- The inventory format captures direct and transitive build inputs and can be
  checked against the committed lockfile.
- The signing identity and verification path are independently auditable.
- Provenance identifies source revision, build inputs, target and build
  environment without exposing secrets.
- Key rotation, expiry, revocation, failure and offline verification are
  documented.

## Decision 6: Archive-first distribution remains the first release path

**Decision**: Plan first for target-specific release archives with checksums;
  package-manager manifests remain later work. A release archive is not
  considered published until the owner authorizes the live distribution action.

**Rationale**: ADR-0008 already sequences archives before Homebrew, Scoop or
  Winget. It also keeps the distribution story reversible while platform and
  signing evidence are incomplete.

## Decision 7: Keep installation, upgrade and rollback as one operator contract

**Decision**: Platform guidance must cover install, integrity verification,
  first start, upgrade, version confirmation, failure recovery and rollback in
  one traceable contract. It must state what happens to configuration and data.

**Rationale**: Separating install from recovery is how operators are left with
  an unusable binary or accidental data loss. Existing configuration migration
  and terminal restoration contracts remain prerequisites, not hidden release
  steps.

## Decision 8: Partition Feature 001a from Feature 008

**Decision**: Feature 001a owns native libpq dependency discovery, enterprise
  authentication prerequisites and libpq-specific clean-install evidence.
  Feature 008 owns generic release identity, artefact/checksum records,
  dependency inventory, signing/provenance decision gates and upgrade guidance.

**Rationale**: Both roadmaps mention installation and release, but their risks
  differ. This partition avoids two agents editing the same release authority
  or claiming that a generic archive proves a native dependency route.

## Current repository findings

- `build.rs` already separates product version, source revision, cleanliness,
  build identity, target and compiler identity, but CI does not currently set a
  non-local build identity.
- The CI workflow runs quality, platform, PostgreSQL, supply-chain and drift
  gates, but has no tag packaging, archive, checksum, SBOM, signing or
  provenance job.
- `docs/operations/release.md` describes the intended process and correctly
  says nothing has been released. It needs a later implementation task, not a
  stronger claim now.
- The repository has a configured `origin`, but that is not evidence of a
  published release. Remote release state was not queried as part of this
  local planning slice.
- `README.md` contains older limitation wording that will need a separate
  reconciliation task; changing it here would overlap the shared release
  authorities before ownership is agreed.

## Open decision gates

1. Owner authorization for a live release and distribution route.
2. SBOM format and generation/verification ownership.
3. Signing identity, key custody, verification and revocation route.
4. Provenance format, builder identity and retention policy.
5. Whether a release catalogue is needed in addition to the changelog, and if
   so its final schema and validator ownership.
6. Feature 001a's remaining enterprise authentication requirement and its
   native dependency packaging choice.

No open gate is silently resolved by this package. The tasks make each gate
visible and keep implementation work behind it.
