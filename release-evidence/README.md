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
machine-readable record must preserve at least these fields:

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

Angle-bracket values are placeholders and must not occur in retained evidence.
The `null` values in the example are an explicit not-configured state, not a
claim of verification.

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

## Provider decision gate

Before a live signing or provenance workflow is enabled, an approved decision
must name the provider or offline mechanism, identity lifecycle, key rotation,
expiry, revocation, failure handling, retention location and independent
verification command. That decision belongs in the release architecture and
threat-model authorities; this contract intentionally leaves it open.

This document defines T018. Executable generation, verification, mismatch
tests, provider selection and CI enforcement remain later Feature 008 tasks in
`specs/008-release-experience/tasks.md`.
