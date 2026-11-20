# ADR-0008: Hand-rolled release workflow first, cargo-dist deferred

- Status: Accepted. The rehearsal implements per-target inventories and
  trust-sidecar binding. Signing, verified provenance and distribution remain
  owner-gated.
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
  normalisation and fail-closed privacy checks. Release candidates use the
  CycloneDX format and per-target generation procedure below.
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

## Live release gates

The dated trust choices below establish the providers and formats. A live
release still requires:

- an approved signing identity, key lifecycle and failure handling policy;
- an approved provenance builder identity and retention policy;
- retained candidate evidence with independently verified archive signatures
  and provenance using the approved identities; and
- owner authorization for the exact live release and distribution route.

The candidate record retains an explicit blocking state for each unresolved
gate.

## Owner trust choices - 2026-09-30

The owner chose the trust providers; implementation and dry validation follow
under W11, and the live release authorization gate below stays blocking:

- Signing: Sigstore keyless (ephemeral OIDC keys, no long-term secrets).
- SBOM: CycloneDX, matching the tooling already pinned for `xtask`.
- Provenance: GitHub OIDC/SLSA from CI-hosted builds.
- Still open: verification identity details, rotation/expiry/revocation
  handling, retention location, the independent verification command, and
  the live release and distribution authorization itself.

## Trust verification and governance specification (W11 and W13)

Under W11 and W13, the trust contract establishes deterministic SBOM generation,
binding verification of Sigstore and SLSA sidecars, and independent verification
procedures.

### Operator trust commands

- **Lockfile inventory**
  ```bash
  cargo xtask release sbom generate --lockfile=Cargo.lock --out=release-evidence/ignatius.cdx.json
  cargo xtask release sbom verify --lockfile=Cargo.lock --sbom=release-evidence/ignatius.cdx.json
  ```
  The generation process derives the component list directly from `Cargo.lock`
  without network or timestamp nondeterminism. Verification enforces byte-exact
  reproducibility. Release candidates use a per-target inventory bound to the
  observed compilation and exact archive. [Candidate inventory procedure](../../operations/release.md#candidate-dependency-inventories)

- **Trust bundle sidecar binding verification**:
  ```bash
  cargo xtask release trust verify --manifest-dir=<dir> --bundle-dir=<dir>
  # For unified release bundles:
  cargo xtask release trust verify --manifest-dir=dist --bundle-dir=dist
  ```
  Wrapper tooling accepts `--dir <bundle-dir>` mapping to identical manifest and
  bundle directories. The task checks the signature digest in
  `<archive>.sigstore.json`, then checks for an SLSA v1 subject that pairs the
  exact archive name and SHA-256 in `<archive>.intoto.jsonl` against
  `release-manifest.json`. The provenance sidecar holds one in-toto v1 statement,
  directly or inside a DSSE envelope. GitHub Artifact Attestations retain
  that envelope inside a Sigstore bundle. `manifest verify` independently
  checks the archive bytes against the manifest and `SHA256SUMS`. [GitHub
  attestation output](https://github.com/actions/attest#outputs),
  [Sigstore DSSE bundle](https://docs.sigstore.dev/about/bundle/#dsse-envelope).

### Independent consumer verification

Consumers verify downloaded release archives without repository dependencies
using standard ecosystem verifiers:

- **Sigstore keyless verification (cosign)**. Use `cosign verify-blob` against
  the exact archive and signature bundle. Pass the owner-approved workflow
  certificate identity with `--certificate-identity` and the GitHub Actions
  OIDC issuer with `--certificate-oidc-issuer`. The approved identity must
  include the exact workflow reference. [Release operations commands](../../operations/release.md#sigstore-keyless-signature-verification-cosign)

- **SLSA provenance verification**. For a GitHub Artifact Attestations bundle,
  use `gh attestation verify --bundle` with the approved signer workflow,
  exact source ref and source digest, and the SLSA v1 predicate. For a
  supported SLSA GitHub generator's standalone DSSE envelope, use
  `slsa-verifier verify-artifact` with the approved builder and source tag,
  then compare the verified source revision to the release record.
  Follow the [release operations guide](../../operations/release.md#slsa-provenance-verification)
  for the commands. The live signing and provenance identities remain
  approval gates.

### Dependency vulnerability triage and emergency patch-release runbook

Supply-chain governance enforces continuous auditing and fail-closed response:

- **Automated scanning**:
  ```bash
  cargo audit
  cargo deny check advisories bans licenses sources
  ```
- **Triage classification**:
  Advisories are triaged by reachability: shipped runtime paths (`src/`) vs
  test/xtask-only paths (`tests/`, `xtask/`). Reachable vulnerabilities in shipped
  code block candidate release readiness immediately.
- **Emergency patch release**:
  Patches are applied via minimal dependency updates in `Cargo.toml`/`Cargo.lock`,
  validated with `cargo xtask verify` and recorded in `CHANGELOG.md`. Rehearse
  the exact source with the non-publishing workflow. An owner-authorized tag
  triggers candidate preparation. Obtain approval for the exact signing and
  publication actions separately.

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
