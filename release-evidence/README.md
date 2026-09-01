# Signature and provenance evidence contract

This directory describes evidence attached to a release candidate. This file
does not select a signing provider, create a key, sign an artefact, publish a
release or claim that provenance is currently available.

Checksums establish that retrieved bytes match a recorded digest. They do not
establish who built those bytes or whether the build inputs were trusted.
Signature and provenance are separate evidence classes and remain separate
from the release record's checksum and CI gate results.

## Evidence unit

One evidence unit covers exactly one artefact in one release record. A future
provider-backed evidence object must preserve at least these fields:

```json
{
  "schema_version": 1,
  "product_version": "0.1.0-preview.1",
  "source_revision": "<40-to-64-lower-case-hex-git-revision>",
  "target": "<rust-target-triple>",
  "artefact_name": "<exact-file-name>",
  "checksum_algorithm": "sha256",
  "checksum": "<artefact-digest>",
  "signature": {
    "status": "not-configured",
    "identity": null,
    "reference": null,
    "verified_at": null,
    "verifier": null,
    "failure_reason": null
  },
  "provenance": {
    "status": "not-configured",
    "source_revision": null,
    "builder_identity": null,
    "build_type": null,
    "reference": null,
    "verified_at": null,
    "verifier": null,
    "failure_reason": null
  }
}
```

This nested example is the future signature/provenance evidence object, not the
current flat release-catalogue record. The current source-controlled catalogue
keeps `checksum_status`, `signature_status`, `provenance_status`, `size_bytes`
and `build_identity` as explicit flat artefact fields in
`release-notes/catalog.schema.json`; provider selection and richer evidence
objects remain separate decision-gated work.

Angle-bracket values are placeholders and must not occur in retained evidence.
The `null` values in the example are an explicit not-configured state, not a
claim of verification.

For a complete release evidence index, these units are retained in a top-level
`artefacts` array alongside `schema_version`, `product_version`,
`source_revision` and the verified inventory result. The array must cover the
release record's artefacts exactly once. Each unit's target, basename, size,
checksum algorithm, checksum, source revision, build identity, checksum state,
signature state and provenance state must match the release
record; an absent or mismatched unit keeps the candidate blocked. If a unit
declares `format`, it must be one of the supported release formats and must
match the record; the provider-neutral shape does not require this field yet.

## Signature status

The release record uses one of these statuses for every artefact:

| Status | Meaning | Readiness consequence |
| --- | --- | --- |
| `not-configured` | No signing provider or verification identity has been approved | Block `ready` and `published` |
| `unsigned` | Signing was expected for the candidate but no signature is attached | Block `ready` and `published` |
| `verified` | The signature validates against the approved identity and exact artefact digest | May contribute to `ready`, subject to every other gate |
| `invalid` | Verification failed or the signature is malformed | Block and retain the failure reason |
| `expired` | The signature or identity is outside its validity period | Block until a valid, auditable replacement is reviewed |

`identity` identifies the approved verification identity without exposing a
private key, token or secret material. `reference` points to a controlled
evidence object and must not be a credential-bearing URL. `verified_at` is an
absolute UTC timestamp. `verifier` identifies the tool and version, not the
operator's home directory or environment dump.

## Provenance status

The release record uses one of these statuses for every artefact:

| Status | Meaning | Readiness consequence |
| --- | --- | --- |
| `not-configured` | No provenance provider or verification route has been approved | Block `ready` and `published` |
| `missing` | Provenance was required but no statement is attached | Block and name the missing item |
| `verified` | The statement binds the exact source revision, builder, target, inputs and artefact digest | May contribute to `ready`, subject to every other gate |
| `mismatch` | A statement exists but disagrees with the release record, target, source or digest | Block and preserve both identities for diagnosis |
| `invalid` | The statement cannot be parsed or its verification fails | Block and retain the failure reason |

When `verified`, provenance must identify at least:

- the exact source revision and product version;
- the target triple and artefact digest;
- the builder identity and build type;
- the relevant manifest and lockfile identity;
- the verification tool, version and timestamp; and
- the controlled reference from which a reviewer can retrieve the statement.

The builder identity is not a secret. It must not be populated with a token,
private key, full environment dump or an unredacted hosted-run URL containing
credentials.

## Binding and fail-closed rules

Signature and provenance evidence is acceptable only when all of these bindings
hold:

1. The evidence names the same product version and source revision as the
   release record.
2. The target and exact artefact filename match the target row and artefact
   entry.
3. The evidence digest uses the declared algorithm and matches the checksum
   recorded for the retrieved bytes.
4. The signature identity is the approved identity for the requested release
   action, when signing is configured.
5. Provenance identifies the builder and inputs without relying on an inferred
   branch, tag, date or CI run number.
6. Verification is independently repeatable from the retained evidence and
   does not depend on an operator's private session or an unavailable dashboard.
7. No failure, missing item or provider outage is silently converted to a
   passing or absent field.

If one binding cannot be checked, the candidate is `blocked`. A passing CI job
does not become a signature, a checksum does not become provenance, and a
provider dashboard screenshot is not a substitute for a verifiable reference.

## Evidence levels

Each review should label the evidence level in plain text:

- `declared`: the record says what is intended, with no verification result;
- `observed`: a tool produced the evidence object, but independent verification
  is still pending;
- `verified`: the exact object was checked against the release record and
  artefact;
- `blocked`: an expected object is absent, invalid or mismatched.

Only `verified` signature and provenance states may contribute to a `ready`
decision. `published` additionally requires explicit owner authorization and
live publication evidence. Local rehearsal, a clean build or a green CI run
never changes the publication state by itself.

## Candidate staging scope preparation

The local `xtask` boundary can prove the file-set contract before a shared CI
workflow is changed:

```text
cargo xtask release evidence-scope \
  --root PATH_TO_CANDIDATE_STAGING_ROOT \
  --allowlist PATH_TO_SCOPE_JSON
```

`scope.schema.json` defines the allowlist shape: schema version `1` and an
explicit, unique list of staging-root-relative portable file paths. The checker
requires the observed staging root to contain exactly those ordinary files. It
rejects missing or unexpected files, duplicate or traversal paths, symlinks,
non-regular files and paths outside the declared root.

The local command itself does not scan secrets, upload an artefact, invoke a
hosted runner or publish user data. Shared CI run `33445418179` at revision
`247d1c7e829aa5b642a5047e56e53d402e386dbb` subsequently built the synthetic
four-file root, validated that exact scope and passed the network-disabled,
digest-pinned Gitleaks scan on 2026-09-01. That is hosted T021 evidence for the
synthetic scope only. It is not a candidate archive, canonical evidence bundle,
signature, provenance statement or publication result.

Owner-authorized non-publishing run `33563497933` subsequently checked out
revision `24f1c8e7801ab5f1f6805e54e4e5f4db67ca1949` and retained one
target-specific four-file bundle for macOS, Windows and Linux, each with a
seven-day GitHub Actions retention boundary. An independent download completed
on 2026-09-02 in Africa/Johannesburg contained no symlinks or unexpected files.
All three release records validated as `blocked`; their manifests, SHA-256
sidecars and exact four-file scopes verified against the archive bytes. These
are run-scoped candidate bundles, not the canonical complete evidence tree.
Their records still declare incomplete evidence and unconfigured signatures
and provenance, and no multi-target aggregation or publication occurred.

## Canonical evidence paths and workflow transport

The `evidence_reference.path` in a release record is the semantic retained
location for that record and must remain under `release-evidence/runs/<run>/<target>/`.
The current non-publishing workflow places the checked four-file bundle inside
an `upload/` transport envelope, with separate `artefacts/`, `record/` and
`evidence/` directories. That envelope is not itself the record's canonical
evidence path.

Before an incomplete rehearsal can become a complete evidence bundle, one
coordinated change must either promote or explicitly map the scoped upload
contents into the declared canonical path before retention and validation, or
change the versioned record and scope contract to make the transport path
canonical. No current workflow step performs that promotion or mapping. Until
the choice is approved and implemented, an upload-scope pass proves only the
transport file set; it does not prove that `evidence_reference.path` is
materialised or that the candidate is ready.

## Privacy and retention

Retain evidence beside the immutable release record for the support life of the
release. Do not overwrite an evidence object in place. A correction creates a
new evidence revision linked to the same release record, while the prior
blocked or superseded object remains explainable.

Never retain or transmit:

- private keys, passwords, access tokens, signed URLs or credential-file text;
- SQL, query history, result values, exports or user configuration;
- unrestricted build logs, process arguments or complete environment dumps; or
- operator-specific paths that reveal unrelated files.

If a verifier would need secret material to repeat a check, retain the public
identity and the verification result, not the secret. A missing offline
verification route is a blocked state and must be recorded as such.

Evidence references must remain below the controlled `release-evidence/` root,
use ordinary files rather than symlinks, and name only the bounded artefact,
record and evidence paths for the candidate. They use canonical relative path
components: no empty, dot or parent components and no trailing separator. A
path that traverses outside the root or resolves through a symlink is a blocked
state, even when the referenced bytes otherwise look valid.

## Provider decision gate

Before a live signing or provenance workflow is enabled, an approved decision
must name the provider or offline mechanism, identity lifecycle, key rotation,
expiry, revocation, failure handling, retention location and independent
verification command. ADR-0008 records those gates without selecting them; the
threat model and data-handling guidance define the corresponding trust and
privacy boundary. This contract intentionally leaves provider implementation
open.

This document defines T018. Provider-backed signing/provenance generation,
independent verification tooling, provider selection and CI enforcement remain
later Feature 008 tasks in `specs/008-release-experience/tasks.md`.
