# Data model: Governed release experience

This model describes the records needed to make release claims auditable. It
does not define a database schema and does not imply that any record is already
generated.

## Release Record

One immutable identity entry for a product version.

| Field | Meaning | Invariant |
| --- | --- | --- |
| `schema_version` | Version of the release-record shape | A reader rejects newer schemas rather than guessing |
| `product_version` | User-visible semantic version | Comes from the product manifest and is never inferred from a date or run number |
| `source_revision` | Exact source revision used | Must identify one revision and match the artefact evidence |
| `source_state` | `clean`, `modified`, `detached` or `unknown` | A stable release requires `clean`; other states are visibly non-release |
| `build_identity` | CI or local build identity | `local` is not represented as a production release identity |
| `target_rows` | Supported platform/architecture rows | Each row is covered, blocked with a reason, or explicitly out of scope |
| `notes_reference` | The matching changelog entry | Exactly one release-note entry maps to the version |
| `evidence_reference` | Evidence bundle for the candidate | The bundle must use the same source revision and product version |
| `status` | `draft`, `candidate`, `blocked`, `ready`, `published` or `withdrawn` | A record cannot become `published` without owner authorization and live evidence |
| `authorization_reference` | Review or owner approval reference | Required only for live publication, signing or distribution |

## Artefact

One platform-specific distributable associated with a release record.

| Field | Meaning | Invariant |
| --- | --- | --- |
| `name` | Exact archive or binary filename | Unique within a release record and includes target identity |
| `target` | Platform and architecture | Must match the support-matrix row and build metadata |
| `format` | Archive or binary format | Must be documented in the installation contract |
| `size_bytes` | Size at packaging time | Recorded for support and completeness checks; not an integrity substitute |
| `checksum_algorithm` | Digest algorithm name | Explicit and supported by the verification guide |
| `checksum` | Digest of the exact artefact bytes | Recomputed before publication and after retrieval |
| `build_identity` | Identity of the build that produced it | Must match the release record and provenance evidence |
| `signature_status` | `not-configured`, `unsigned`, `verified`, `invalid` or `expired` | Missing or invalid evidence blocks readiness |
| `provenance_status` | `not-configured`, `missing`, `verified`, `mismatch` or `invalid` | Must identify the source and builder when verified |

## Evidence Bundle

The set of facts a reviewer uses to decide whether a candidate is ready.

- Release record reference.
- Gate results with command, environment, date and evidence class.
- Dependency inventory covering direct and transitive build inputs.
- Artefact list, target rows, sizes and checksums.
- Signature and provenance status for every artefact.
- Clean-install, first-start, upgrade and rollback evidence references.
- Known limitations and explicitly unverified claims.
- Reviewer decision and next action when blocked.

The evidence bundle must not contain credentials, tokens, SQL text, result data
or unrestricted build logs. It may link to a controlled record without copying
secret material into the release package.

## Installation Contract

One operator-facing contract per supported platform.

- Platform and architecture prerequisites.
- Artefact selection and integrity-verification command.
- Installation location and permission expectations.
- First-start and identity-confirmation steps.
- Upgrade procedure and preserved configuration/data statement.
- Failure repair and rollback procedure.
- Uninstall or cleanup guidance where relevant.
- Unsupported platform/architecture response.
- First diagnostic action and privacy-safe support identity command.

## Support Identity Bundle

The minimum facts needed to identify an installation without exporting sensitive
content.

- Product version.
- Source/build identity and target.
- Working-tree state when relevant to a local build.
- Operating system and terminal context supplied by the user.
- PostgreSQL server version and connection posture when the user chooses to
  provide them.
- Error category, exit code and next action.

It excludes passwords, tokens, SQL text, result values, full connection URIs and
credential-file contents by default.

## State transitions

```text
draft -> candidate -> blocked
draft -> candidate -> ready -> published
candidate -> withdrawn
ready -> withdrawn
published -> withdrawn
```

- `draft`: record is being assembled and cannot be distributed.
- `candidate`: identity and artefact set exist, but review is incomplete.
- `blocked`: a required gate or evidence item is missing or mismatched.
- `ready`: all required evidence is present and owner authorization is recorded
  for the action being requested; this is still not proof of live publication.
- `published`: the exact live release action has been observed and recorded.
- `withdrawn`: the record is retained for support and must not be presented as a
  current install choice.

## Cross-entity invariants

1. A `Release Record` has exactly one product version and notes reference.
2. Every `Artefact` belongs to exactly one release record and one target row.
3. A checksum mismatch changes the record to `blocked` and cannot be hidden by
   replacing only the manifest entry.
4. A source revision or target mismatch between record, artefact, inventory,
   signature or provenance evidence is a blocked state.
5. `published` requires live publication evidence and explicit authorization;
   passing CI or building locally is insufficient.
6. Removing or replacing an artefact never removes the release record or
   support evidence needed to explain the change.
7. Support identity data is safe to share by default and cannot contain secret
   fields by schema or redaction convention.
