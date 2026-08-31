# Dependency inventory contract

This document defines the dependency inventory required for one Ignatius
release candidate. It is a contract, not an inventory from the current
worktree. A candidate is not ready merely because this document exists.

The inventory answers four questions without requiring a cloud account:

1. Which direct and transitive build inputs produced the artefact?
2. Which source revision, product version, target and toolchain were used?
3. Which policy and integrity checks were run over those inputs?
4. Can a reviewer retain and reproduce the evidence without receiving user
   data or secrets?

## Scope and authoritative inputs

An inventory covers the exact build input set for one target-specific artefact.
The authoritative inputs are:

| Input | What it establishes | Required handling |
| --- | --- | --- |
| `Cargo.toml` and `xtask/Cargo.toml` | Direct package names, requested versions, features and workspace membership | Record the source revision and a SHA-256 digest of each manifest |
| `Cargo.lock` | Resolved direct and transitive package versions, sources and checksums | Require the committed lockfile and record its SHA-256 digest |
| `rust-toolchain.toml` | Declared compiler/toolchain channel | Record the file digest and the resolved `rustc` and `cargo` versions |
| Build configuration | Target, profile, enabled features and relevant flags | Record target triple, profile, feature set and non-secret build flags |
| Source identity | Product version, revision and working-tree state | Match the release record and artefact provenance exactly |

The inventory includes the root package and the `xtask` workspace member where
it contributes to the build or release process. It includes direct and
transitive crates, build dependencies, target-specific dependencies and
optional dependencies enabled for the candidate. A package omitted because it
was not active for a target must be identified as out of scope rather than
silently dropped.

The inventory does not include the user's database, connection target,
configuration directory, query text, result rows, exported files, terminal
scrollback or local credential stores.

## Minimum record

One inventory record is tied to one release record and one target. A future
machine-readable representation should carry at least these fields:

```json
{
  "schema_version": 1,
  "product_version": "0.1.0-preview.1",
  "source_revision": "<40-to-64-lower-case-hex-git-revision>",
  "source_state": "clean",
  "build_identity": "<ci-or-local-rehearsal-identity>",
  "target": "<rust-target-triple>",
  "toolchain": {
    "rustc": "<rustc-version>",
    "cargo": "<cargo-version>",
    "manifest_sha256": "<sha256>",
    "lockfile_sha256": "<sha256>",
    "toolchain_file_sha256": "<sha256>"
  },
  "generation": {
    "method": "cargo-metadata-and-locked-policy-check",
    "metadata_format": 1,
    "generated_at": "<utc-timestamp>",
    "generator_version": "<tool-version>"
  },
  "dependencies": [
    {
      "name": "<crate-name>",
      "version": "<resolved-version>",
      "source": "<registry-or-git-source-or-path-kind>",
      "kind": "normal",
      "target": "<target-expression-or-null>",
      "checksum": "<lockfile-checksum-or-null>"
    }
  ],
  "policy": {
    "lockfile": "verified",
    "advisories": "pass",
    "licenses": "pass",
    "bans": "pass",
    "sources": "pass"
  }
}
```

The example is a shape illustration. Angle-bracket values are placeholders and
must never occur in a candidate evidence record. A later implementation may
split the record into files, but it must preserve these facts and their
identity relationships.

`source` values are limited to dependency provenance such as a registry, a
specific git revision or a workspace path. They must not contain an access
token, signed URL, private repository credential, home-directory path or other
machine-specific secret. Workspace paths are normalised to repository-relative
paths before retention.

## Generation contract

The generator runs from the exact source revision and target build context. It
must:

1. Read the product version and source identity without inferring one fact from
   another.
2. Refuse a missing or dirty lockfile for a release candidate.
3. Run `cargo metadata --locked --format-version 1` and retain the dependency
   graph needed to account for direct and transitive inputs.
4. Record the resolved compiler, Cargo version, target triple, profile and
   enabled feature set.
5. Run the repository's locked supply-chain policy gate, currently
   `cargo deny check advisories licenses bans sources` in CI.
6. Hash the exact manifests and lockfile used, and tie those digests to the
   source revision and build identity.
7. Normalise paths and remove environment values before writing retained
   evidence.
8. Write to a new candidate evidence location. It must refuse to overwrite an
   existing record for the same version and target.

The generation method may use a provider-neutral local tool. It must not upload
the dependency graph, build logs or repository contents as a side effect.
Network access needed to resolve dependencies belongs to the build environment;
it is not a release-evidence destination.

## Verification and fail-closed rules

The inventory is acceptable only when every row below is true:

| Check | Pass condition | Blocking result |
| --- | --- | --- |
| Manifest and lockfile | Both exist, are readable, hashed and match the source revision | `blocked: missing-or-unreadable-input` |
| Locked resolution | `cargo metadata --locked` succeeds without changing the lockfile | `blocked: lockfile-drift` |
| Dependency coverage | Every active direct and transitive input appears once with source and resolved version | `blocked: incomplete-inventory` |
| Toolchain | Compiler, Cargo, target, profile and feature set are recorded | `blocked: incomplete-build-identity` |
| Source identity | Inventory revision and clean/modified state match the release record | `blocked: source-mismatch` |
| Artefact identity | Inventory target and build identity match the artefact and release record | `blocked: target-or-build-mismatch` |
| Policy | Advisories, licences, bans and sources all have explicit results | `blocked: missing-policy-result` |
| Privacy | No credential, SQL, result data, unrestricted log or secret-bearing URL is present | `blocked: sensitive-content` |
| Reproducibility | A second read of the declared inputs produces the same dependency and digest facts | `blocked: non-reproducible-inventory` |

An absent policy result is not equivalent to `pass`. A warning that changes
the resolved graph, target, source revision or integrity digest is a failure
until reviewed and recorded. A provider or signing service outage leaves the
candidate blocked; it does not become an unsigned or unverifiable ready record
by omission.

The inventory alone cannot prove a signature or build provenance. Those states
remain separate artefact evidence fields in the release-record schema and must
match the same source revision, product version and target.

## Retention and review

For a rehearsal, retain the sanitised inventory with the local evidence bundle
long enough to reproduce the review and label it `rehearsal`, never `published`.
For an authorised candidate, retain the inventory beside the immutable release
record and target artefact for the support life of that release. Do not replace
an inventory in place: issue a new record with a new source revision or mark the
candidate blocked and preserve the earlier evidence.

The reviewer must be able to answer from the inventory and its linked record:

- what was built, from which revision and for which target;
- which dependency inputs and policy checks were included;
- what is missing or unverifiable; and
- where to find the next blocked action.

The semantic readiness gate now rejects mismatched or incomplete records through
`cargo xtask release check PATH`. This contract still does not claim that
inventory generation or provider-backed evidence generation exist yet. Archive
checksum generation and exact-byte verification are implemented separately by
the non-publishing T014 sidecar commands; they do not provide an inventory or
provider-backed evidence result.

## Privacy boundary

Never retain or transmit:

- passwords, tokens, private keys, signed URLs or credential-file contents;
- SQL text, query history, result values or exported data;
- complete environment dumps, process arguments or unrestricted build logs;
- arbitrary home-directory paths that identify the operator or expose files;
- symlinks or path traversal that can resolve outside the declared evidence
  root.

If a tool emits any of these values, the generator must stop, discard the
unretained output, report a blocked evidence state and require a sanitised rerun.
Redaction is not a substitute for narrowing the generator's inputs.

This document defines T017. Executable inventory generation, provider-backed
evidence, fixtures beyond the current contract cases and CI enforcement remain
the later Feature 008 tasks listed in `specs/008-release-experience/tasks.md`.
Archive checksum production and exact-byte verification belong to T014 and are
implemented separately from inventory generation.
