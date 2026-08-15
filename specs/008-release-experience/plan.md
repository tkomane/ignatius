# Implementation Plan: Governed release experience

**Branch**: `008-release-experience` | **Date**: 2026-08-16 | **Spec**:
[spec.md](./spec.md)

**Input**: Feature specification from
`/specs/008-release-experience/spec.md`

## Status and delivery boundary

This package defines the release contract and implementation order. It does not
publish, sign or distribute a release. Any task that changes CI publication,
signing credentials, remote release state or production distribution requires a
separate owner-approved handoff.

## Summary

Create a source-controlled release evidence chain that ties one version to its
source revision, build identity, platform artefacts, checksums, dependency
inventory, signing/provenance status, installation contract and support notes.
The first useful increment is a reviewable release-readiness record and
validation guide; packaging automation follows only after the evidence formats
and trust boundaries are approved.

## Technical Context

**Language/Version**: Rust 2024 project with the repository's pinned stable
toolchain; release documentation and manifests are source-controlled text.

**Primary Dependencies**: Existing `cargo xtask verify`, CI matrix, build
identity output and committed lockfile. No new runtime dependency is assumed.
Packaging, SBOM, signing and provenance tools remain explicit decision gates.

**Storage**: Source-controlled release records and evidence manifests, plus
immutable release artefacts when an authorized release exists. User
configuration and database data are not release storage.

**Testing**: Requirements-quality review, deterministic manifest validation,
checksum mismatch tests, `cargo xtask verify`, CI matrix evidence and clean
install/upgrade rehearsals on macOS, Windows and Linux.

**Target Platform**: macOS, Windows and Linux as named by the compatibility
record. Any additional architecture needs an explicit support-matrix entry and
artefact evidence.

**Project Type**: Local-first command-line PostgreSQL client with a
source-controlled release process.

**Performance Goals**: A reviewer can decide ready, blocked or not-yet-published
status in under 5 minutes. A first-time operator can install and verify a
supported artefact in under 15 minutes, excluding download time.

**Constraints**: No secret in source or artefacts; no automatic network activity;
no silent overwrite, downgrade or replay; no claim stronger than its evidence;
publication and signing require owner authorization; current Claude source WIP
in `src/app/`, `src/cli/`, `src/config/`, `src/ui/`,
`tests/cli_contract.rs`, `src/lib.rs` and `src/history.rs` is out of scope for
this package.

**Scale/Scope**: One release record per version, one evidence bundle per
candidate, one artefact/checksum entry per supported target, and one installation
contract per supported platform for the first release workflow.

## Constitution Check

| Principle | Gate result and application |
| --- | --- |
| I. Delight must remain truthful | Pass. Release states distinguish ready, blocked, not-yet-published and unverified evidence. |
| II. PostgreSQL correctness | Pass. The release contract preserves the existing compatibility and server-support claims rather than changing database behaviour. |
| III. Local-first and private | Pass. Local validation is possible without an account; packaging adds no telemetry, update check or query/result upload. |
| IV. Safe by default | Pass. Integrity mismatch, missing evidence and failed upgrade block or explain the next safe action. |
| V. Keyboard-first | Pass. The feature does not alter the client workflow; commands and paths in guidance remain copyable and discoverable. |
| VI. Meaning survives styling loss | Pass. Release status and recovery guidance use explicit words and platform labels. |
| VII. Cross-platform release criterion | Pass. macOS, Windows and Linux are separate evidence rows, not inferred from one runner. |
| VIII. One source of truth | Pass. Version, changelog, compatibility, build identity and release evidence each retain one authority and are cross-checked. |
| IX. Evidence over confidence | Pass. Automated, hand, server, packaging and live publication evidence are separate classes. |
| X. Recoverability | Pass. Upgrade failure and rollback are first-class requirements; existing configuration/data are not silently deleted. |

No constitutional exception is requested. This plan does not authorize a
publication, signing-key operation, remote release mutation or production
change.

## Phase 0: Research and decisions

1. Reconcile the current release prose, changelog, build identity and
   compatibility authorities into one traceability map.
2. Compare release-record formats and select the smallest source-controlled
   representation that can validate one version-to-artefact mapping without
   duplicating the changelog.
3. Define the checksum manifest, artefact naming and target-coverage contract;
   use a standard cryptographic digest and record the algorithm explicitly.
4. Compare dependency-inventory formats and signing/provenance evidence models;
   record the selected format or the owner decision gate without pretending a
   provider is already configured.
5. Define the clean-install, upgrade, rollback and missing-prerequisite evidence
   matrix for macOS, Windows and Linux.
6. Partition Feature 001a's libpq-specific packaging evidence from this
   feature's generic release contract so neither feature owns the same gate.

Research decisions and unresolved owner gates are recorded in
[research.md](./research.md); none authorizes an implementation dependency.

## Phase 1: Design artifacts

1. Model release records, artefacts, evidence bundles, installation contracts
   and support identity bundles in [data-model.md](./data-model.md).
2. Define the review and validation scenarios in
   [quickstart.md](./quickstart.md), including deliberate checksum, target,
   version, evidence and rollback failures.
3. Generate the ordered implementation tasks in [tasks.md](./tasks.md).
4. Keep the requirements-quality checklist separate from implementation status.

## Proposed ownership and source structure

This planning package owns only `specs/008-release-experience/` until an
explicit implementation handoff. The following are future authorities and must
not be edited by this package's documentation-only slice without a new scoped
task:

```text
CHANGELOG.md                         # release-note source
Cargo.toml                           # product version source
build.rs, src/branding.rs            # build identity contract
docs/support/compatibility.md        # support matrix and claims
docs/operations/release.md           # operator release procedure
docs/operations/verification.md      # evidence and merge gates
.github/workflows/ci.yml             # CI execution and release automation
release-notes/                        # future release catalogue, if approved
artifacts/                            # future local/CI packaging workspace only
```

The current implementation work in `src/app/`, `src/cli/`, `src/config/`,
`src/ui/`, `tests/cli_contract.rs`, `src/lib.rs` and `src/history.rs` is
explicitly excluded from this package.

## Architecture and trust boundaries

```mermaid
flowchart LR
    source["Source zone\nversion, changelog, lockfile, commit"]
    build["Build zone\nCI or local rehearsal"]
    evidence["Evidence zone\nidentity, checksums, inventory, provenance"]
    artifact["Artefact zone\nplatform archives and binaries"]
    operator["Operator zone\ninstall, verify, upgrade, rollback"]
    support["Support zone\nprivacy-safe identity bundle"]

    source -->|declared revision and version| build
    build -->|targeted output and build facts| artifact
    build -->|logs and metadata, no secrets| evidence
    artifact -->|digest and target verification| evidence
    evidence -->|review decision| operator
    operator -->|safe identity only| support

    %% Trust zones: source and evidence are repository-controlled; build,
    %% artefacts, signing services and target machines are separate trust zones.
    %% Open question: the signing/provenance provider and release host remain
    %% owner decisions and must not be inferred from this diagram.
```

The evidence index is the control plane for truthfulness. A passing build is
not a signed release, and a checksum is not provenance. Each claim must point
to the evidence class that proves it.

## Traceability

| Spec area | Design evidence | Planned validation |
| --- | --- | --- |
| FR-801 to FR-805, SC-801 to SC-803 | Release record, artefact and checksum entities | Deterministic manifest and mismatch scenarios |
| FR-806 to FR-808, SEC-801 to SEC-802 | Evidence bundle and publication gate | Missing inventory/signature/provenance and tampered digest scenarios |
| FR-809 to FR-810, SC-804 to SC-805 | Installation contract and platform matrix | Clean install, upgrade and rollback rehearsals per platform |
| FR-811 to FR-813, UX-801 to UX-802, SC-806 to SC-807 | Support identity and evidence-class rules | Documentation review and secret-safe diagnostic captures |
| FR-814 to FR-815 | Ownership and overwrite controls | Review of authorization and idempotency gates before live release |

## Dependency and collision gates

- Feature 001a owns only libpq-specific native dependency discovery,
  enterprise-authentication packaging and its clean-install evidence. Feature
  008 owns the generic release record, checksums, inventories, signing/
  provenance decision gate and upgrade contract.
- No task in this package may add a native library, change the adapter, or edit
  Claude's active source paths.
- Shared authorities such as the changelog, release procedure, verification
  record and CI workflow are deferred until ownership is explicitly agreed and
  the relevant task is selected.
- The Spec Kit feature pointer is temporary tooling state only and must be
  removed after generation so another active feature cannot be redirected.

## Complexity Tracking

No complexity exception is approved. A separate release host, package manager,
signing provider or catalogue generator requires a decision record and a
revised scope before it is introduced.
