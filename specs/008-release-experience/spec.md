# Feature Specification: Governed release experience

**Feature Branch**: `008-release-experience`

**Created**: 2026-08-16

**Status**: Draft

**Input**: Roadmap item for release-note catalogue, packaged artefacts,
checksums, SBOM, signing and provenance, installation and upgrade guidance.

## Scope note

This feature defines the release evidence and user-facing support contract for
Ignatius. It covers preparing and rehearsing a release candidate, not publishing
an external release without the owner's explicit authorization. A local or CI
result must never be described as signed, notarised, published or production
ready unless the corresponding live evidence exists.

## User Scenarios & Testing

### User Story 1 - Prepare an identifiable release candidate (Priority: P1)

As a maintainer, I want every candidate to carry one consistent version and
complete build identity so that an artefact can be traced back to exactly what
was built.

**Why this priority**: A release cannot be supported or rolled back safely when
its version, source, target or working-tree state is ambiguous.

**Independent Test**: Prepare a candidate from a clean revision and inspect its
release record and artefacts without publishing them.

**Acceptance Scenarios**:

1. **Given** a clean source revision and a release version, **When** a candidate
   is prepared, **Then** its record identifies the product version, source
   revision, cleanliness, build identity, target and checksum as separate facts.
2. **Given** the same source revision and build inputs, **When** the candidate is
   prepared twice, **Then** the product version and release-note identity remain
   identical and any difference in artefact content is reported rather than
   silently accepted.
3. **Given** a modified or untagged revision, **When** a release candidate is
   requested, **Then** it is labelled as a non-release build and cannot be
   represented as the corresponding stable version.

### User Story 2 - Review supply-chain evidence before publication (Priority: P1)

As a release reviewer, I want the dependency inventory, signing status and build
provenance to be visible so that I can distinguish a verified release from one
that still needs a gate.

**Why this priority**: Missing or overstated supply-chain evidence creates a
security and trust failure even when the binary itself works.

**Independent Test**: Review a candidate with complete evidence, then review one
with a deliberately missing evidence item, and compare the resulting release
decision.

**Acceptance Scenarios**:

1. **Given** a candidate with an inventory, checksum, signature status and
   provenance record, **When** a reviewer opens the release evidence, **Then**
   each item is linked to the exact candidate and its verification state is
   explicit.
2. **Given** a candidate missing a signature, provenance record or inventory,
   **When** a reviewer evaluates it, **Then** the missing item is named and the
   candidate remains not ready for publication.
3. **Given** evidence from a different source revision or target, **When** it is
   attached to a candidate, **Then** the mismatch is rejected or clearly marked
   rather than combined into a misleading release record.

### User Story 3 - Install and upgrade with a recoverable path (Priority: P1)

As an operator, I want platform-specific installation, upgrade and recovery
instructions so that I can use a supported build and return to the previous
known-good version when an upgrade is unsuitable.

**Why this priority**: A release is not usable if installation depends on tribal
knowledge or if a failed upgrade leaves an operator without a recovery route.

**Independent Test**: Follow the documented install, upgrade, verification and
rollback journey for each supported platform using a candidate artefact.

**Acceptance Scenarios**:

1. **Given** a supported macOS, Windows or Linux environment, **When** an
   operator follows the installation guide, **Then** the prerequisites,
   artefact choice, verification step and first diagnostic action are
   unambiguous.
2. **Given** an existing installation and a newer candidate, **When** the
   operator upgrades, **Then** the guide states what is preserved, how the
   installed version is confirmed and how to return to the previous version.
3. **Given** an unsupported platform, architecture or missing prerequisite,
   **When** an operator attempts installation, **Then** the support boundary and
   the next safe action are stated without implying compatibility.

### User Story 4 - Support a released version with truthful notes (Priority: P2)

As a user or support engineer, I want release notes and compatibility claims to
match the exact artefact so that I can understand changes, limitations and the
information needed for a useful support request.

**Why this priority**: Clear notes reduce avoidable support work and prevent a
known limitation from being mistaken for a regression.

**Independent Test**: Select one candidate and trace its version, notes,
compatibility entry, diagnostic command and rollback reference end to end.

**Acceptance Scenarios**:

1. **Given** a candidate version, **When** a user reads its notes, **Then** new,
   improved, fixed, security and known-limitation changes are separated and
   linked to the same release identity.
2. **Given** a documented limitation or unsupported route, **When** a user
   encounters it, **Then** the compatibility and troubleshooting guidance name
   the boundary and a practical next action.
3. **Given** a support request, **When** the user follows the diagnostic guide,
   **Then** the collected identity is sufficient to distinguish product version,
   source/build identity, platform and server context without exposing secrets
   or SQL text.

### Edge Cases

- A release version is reused with a different source revision or changelog
  entry.
- One platform artefact is missing, has the wrong target, or has a checksum that
  does not match the release evidence.
- A candidate is built from a dirty working tree, a detached revision or a
  source revision that has not passed all required gates.
- Signing or provenance is unavailable, expired, invalid or refers to a
  different candidate.
- An upgrade fails after installation but before the first successful start.
- A user has an older configuration or data directory that the new version
  cannot migrate safely.
- A release note describes a capability that is only proven in CI or locally,
  not on the platform or server version being claimed.
- A user requests an artefact for an architecture that has not been built.

## Requirements

### Functional Requirements

- **FR-801**: The release process MUST maintain one authoritative release record
  for each version, with one matching notes entry and source revision.
- **FR-802**: A release record MUST keep product version, source revision,
  working-tree state, build identity, target, artefact name and checksum as
  separate facts.
- **FR-803**: The process MUST produce or explicitly mark unavailable an
  artefact for every platform and architecture included in the support matrix.
- **FR-804**: Every distributable artefact MUST have a cryptographic checksum
  recorded beside the artefact and tied to the exact release record.
- **FR-805**: The process MUST detect a mismatch between an artefact, its
  checksum, its target, its source revision and its release record before
  publication.
- **FR-806**: Each release candidate MUST have a dependency inventory that names
  the build inputs and can be retained with the release evidence.
- **FR-807**: The release record MUST state signing and provenance status for
  each artefact, including an explicit not-ready state when either is absent.
- **FR-808**: The process MUST refuse or clearly block publication when required
  verification gates, artefacts or evidence are missing.
- **FR-809**: Installation documentation MUST identify prerequisites, supported
  targets, artefact selection, integrity verification, first start and repair
  actions for macOS, Windows and Linux.
- **FR-810**: Upgrade documentation MUST state configuration/data preservation,
  incompatibility handling, version confirmation and rollback to a known-good
  release.
- **FR-811**: Release notes and compatibility claims MUST distinguish automated
  evidence, hand verification, server coverage and unverified assumptions.
- **FR-812**: Support documentation MUST provide a privacy-safe identity bundle
  that does not include passwords, tokens, SQL text or result data by default.
- **FR-813**: The release process MUST preserve a reproducible path from the
  source revision to the release evidence without requiring a cloud account for
  local validation.
- **FR-814**: No release step MUST silently overwrite an existing version,
  artefact, checksum, configuration or user data.
- **FR-815**: Actual publication, signing and distribution MUST require explicit
  owner authorization and MUST not be implied by a local build or passing CI.

### Security and Privacy Requirements

- **SEC-801**: Secrets used by CI or signing MUST be injected by the execution
  environment and MUST never appear in source, release notes, logs, artefacts or
  support bundles.
- **SEC-802**: Integrity verification MUST fail closed when a checksum or
  provenance identity is absent or mismatched.
- **SEC-803**: Release evidence MUST not upload queries, result rows, credentials
  or telemetry as a side effect of packaging or update guidance.

### Quality and Accessibility Requirements

- **UX-801**: Release, installation and recovery guidance MUST use plain words,
  explicit platform labels and copyable commands or paths where appropriate.
- **UX-802**: Every release state MUST be understandable without colour, icons or
  a signing-provider dashboard.
- **NFR-801**: A maintainer MUST be able to identify the release status and next
  blocked action from one evidence index without searching unrelated logs.

### Key Entities

- **Release Record**: The authoritative version entry linking notes, source
  revision, build identity, supported targets and release status.
- **Artefact**: A platform-specific distributable with target identity, size,
  checksum and verification state.
- **Evidence Bundle**: The dependency inventory, build provenance, signature
  status, gate results and review references for one release record.
- **Installation Contract**: The supported prerequisites, install, upgrade,
  verification, rollback and troubleshooting guidance for a platform.
- **Support Identity Bundle**: The safe diagnostic facts a user can share to
  identify a build and environment without sharing sensitive content.

## Success Criteria

### Measurable Outcomes

- **SC-801**: For every release candidate, 100% of artefacts listed in the
  support matrix have a matching target, checksum and release-record entry, or
  an explicit blocked reason.
- **SC-802**: A reviewer can determine ready, blocked or not-yet-published
  status and the next action for a candidate in under 5 minutes using only the
  evidence index and linked documents.
- **SC-803**: Two preparations from the same source revision and declared inputs
  produce identical release identity and matching artefact checksums, or report
  a reproducibility failure before publication.
- **SC-804**: A first-time operator can install and verify the supported artefact
  on each primary platform in under 15 minutes by following the guide, excluding
  download time.
- **SC-805**: A failed upgrade has a documented recovery path that returns the
  operator to the previous known-good version without deleting user configuration
  or data.
- **SC-806**: 100% of published release notes and compatibility claims identify
  their evidence level and do not claim signing, provenance, notarisation or
  publication without corresponding live evidence.
- **SC-807**: A support identity bundle contains no password, token, SQL text or
  result data in representative diagnostic scenarios.

## Assumptions

- The first distribution route remains release archives with checksums for the
  three primary platforms; package-manager distribution is later work.
- The product manifest, changelog, compatibility record and build identity
  output remain authoritative for their respective facts until a separately
  approved design changes them.
- CI can build the supported targets, but CI evidence and hand verification are
  recorded as different evidence classes.
- A release may be rehearsed locally or in CI without being published.
- Existing configuration migrations, terminal restoration and database safety
  contracts remain prerequisites rather than being redesigned by this feature.

## Out of Scope

- Publishing a real public release, signing key creation, notarisation or
  production distribution without explicit owner authorization.
- Choosing a package manager, cloud release host or signing/provenance provider
  beyond documenting the decision gate and required evidence.
- Adding telemetry, automatic update checks, query upload or result upload.
- Replacing the PostgreSQL adapter, connection profiles or the editor.
