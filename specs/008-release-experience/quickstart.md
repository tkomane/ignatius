# Quickstart: Release-readiness rehearsal

This guide validates the release contract without publishing anything. The
current repository has non-publishing release-record, archive-sidecar and
readiness commands, plus a fail-closed command for aggregating the three target
records. A hosted aggregate run, signing, provenance, platform hand evidence
and publication remain separate gates and must not be inferred from a local
rehearsal.

## Preconditions

- A checkout of the intended source revision. Use a clean checkout for a
  clean-source candidate; a modified checkout is valid only for a labelled
  local blocked-readiness rehearsal.
- Rust and the repository's normal development prerequisites.
- Docker Desktop only when the full PostgreSQL gate is required.
- Synthetic database credentials from `docker/dev.env`; never substitute a
  real credential in a command, fixture or evidence bundle.
- No release credentials, signing keys or remote publication access are needed
  for this rehearsal.

## 1. Establish the source identity

Run:

```bash
git rev-parse HEAD
git status --short
cargo --locked run --quiet -- version --verbose
```

Expected outcome:

- The revision is recorded exactly.
- The working tree is either empty or explicitly labelled modified.
- Version, source revision, source state, target, build identity and compiler
  are displayed as separate facts.
- No password, connection URI, SQL text or result value appears.

For a local rehearsal, the build identity may be made explicit without claiming
CI provenance:

```bash
IGNATIUS_BUILD_IDENTITY=local-rehearsal cargo --locked run --quiet -- version --verbose
```

## 2. Run the repository gates

For a complete local result, start the disposable services and run:

```bash
cleanup_db() { cargo --locked xtask db down || true; }
trap cleanup_db EXIT
cargo --locked xtask db up
cargo --locked xtask verify
trap - EXIT
cargo --locked xtask db down
```

Record separately:

- formatting and lint results;
- unit/layout and CLI contract counts;
- PostgreSQL integration count and server version;
- terminal or platform evidence that was not covered by the command.

If the database is not running, the integration gate is a skip, not a pass.
The exit trap tears down disposable services if the verifier fails; the final
explicit `db down` clears the trap after a successful rehearsal.

## 3. Validate the implemented candidate and sidecar contract

Obtain one exact target archive from the candidate build or set `ARCHIVE_PATH`
to an existing archive produced by that build. Set `TARGET_TRIPLE` and
`ARTEFACT_FORMAT` to the exact release-matrix values used to produce it. The
archive basename must be the canonical name derived below; the commands refuse
silent replacement of output paths:

```bash
set -euo pipefail
archive_source="${ARCHIVE_PATH:?Set ARCHIVE_PATH to the exact candidate archive}"
target="${TARGET_TRIPLE:?Set TARGET_TRIPLE to the archive target triple}"
format="${ARTEFACT_FORMAT:?Set ARTEFACT_FORMAT to tar.gz, zip or binary}"
version="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)"
test -n "$version"
test -s "$archive_source"
case "$format" in
  tar.gz) suffix=".tar.gz" ;;
  zip) suffix=".zip" ;;
  binary) suffix="" ;;
  *) echo "unsupported ARTEFACT_FORMAT: $format" >&2; exit 1 ;;
esac
name="ignatius-${version}-${target}${suffix}"
test "$(basename "$archive_source")" = "$name"

rehearsal_root="$(mktemp -d)"
trap 'rm -rf "$rehearsal_root"' EXIT
rehearsal_root="$(cd "$rehearsal_root" && pwd -P)"
staging_root="$rehearsal_root/staging"
mkdir "$staging_root"
record_dir="$rehearsal_root/record"
archive_dir="$rehearsal_root/archive"
sidecar_dir="$rehearsal_root/sidecars"
mkdir -p "$record_dir" "$archive_dir"
archive="$archive_dir/$name"
cp "$archive_source" "$archive"
size="$(wc -c < "$archive" | tr -d '[:space:]')"

cargo --locked xtask release generate \
  --output "$record_dir/release.json" \
  --target "$target" \
  --artefact-name "$name" \
  --artefact-format "$format" \
  --artefact-size "$size" \
  --notes-entry "local rehearsal for ${version}" \
  --evidence-path "release-evidence/local-rehearsal/release-record.json" \
  --build-identity local-rehearsal \
  --artefact-path "$archive"

cargo --locked xtask release validate "$record_dir/release.json"
cargo --locked xtask release manifest generate \
  --record "$record_dir/release.json" \
  --artefact-dir "$archive_dir" \
  --output-dir "$sidecar_dir"
cargo --locked xtask release manifest verify \
  --record "$record_dir/release.json" \
  --artefact-dir "$archive_dir" \
  --manifest "$sidecar_dir/release-manifest.json" \
  --checksums "$sidecar_dir/SHA256SUMS"
readiness_status=0
readiness_output="$(cargo --locked xtask release check "$record_dir/release.json" 2>&1)" || readiness_status=$?
printf '%s\n' "$readiness_output"
if test "$readiness_status" -eq 0; then
  echo "candidate unexpectedly passed the local readiness gate" >&2
  exit 1
fi

upload="$staging_root/upload"
allowlist="$staging_root/upload-scope.json"
mkdir -p "$upload/artefacts" "$upload/record" "$upload/evidence"
cp "$archive" "$upload/artefacts/$name"
cp "$record_dir/release.json" "$upload/record/release-record.json"
cp "$sidecar_dir/release-manifest.json" "$upload/evidence/release-manifest.json"
cp "$sidecar_dir/SHA256SUMS" "$upload/evidence/SHA256SUMS"
printf '{"schema_version":1,"files":["artefacts/%s","record/release-record.json","evidence/release-manifest.json","evidence/SHA256SUMS"]}\n' \
  "$name" > "$allowlist"
cargo --locked xtask release evidence-scope --root "$upload" --allowlist "$allowlist"
```

This local sequence confirms:

1. The filename contains the product version and exact target.
2. The record, archive basename and sidecars carry the same declared target;
   this does not prove that the binary inside the archive was built for that
   target.
3. The recorded SHA-256 digest matches the bytes on disk.
4. The upload root contains only the archive, release record and verified
   sidecars named by the allowlist.
5. The incomplete local evidence produces `blocked`, never `ready`.

This sequence does not prove a complete evidence bundle, dependency inventory,
signature or provenance. Those remain separate owner and provider gates until
the evidence-layout decision and corresponding evidence are supplied.

The focused release-contract suite, rather than this local sequence, covers
deliberate changed-byte, changed target, changed version, missing artefact,
missing checksum, missing inventory, invalid signature, provenance mismatch and
dirty-source scenarios. A locally
generated record is expected to remain blocked when the checkout is modified,
untagged or lacks the later evidence and owner gates; that is a truthful result,
not a failed publication.

## 4. Aggregate the three target records

After every target bundle has independently passed `release validate`,
`release manifest verify` and `release evidence-scope`, combine exactly one
record for each supported target. Set the five path variables below to the new
aggregate record and evidence paths and the three verified input records:

```bash
cargo --locked xtask release aggregate \
  --output "$aggregate_record" \
  --evidence-path "$aggregate_evidence_path" \
  --input "$macos_record" \
  --input "$windows_record" \
  --input "$linux_record"
```

The inputs must agree on version, source revision and state, build identity and
notes identity. Missing, duplicate, unsupported or mismatched targets are
refused, and the output path is create-only. Generate and verify one manifest
against the three exact archives after aggregation. A record remains blocked
when any input is blocked, and incomplete aggregate evidence never passes
`release check`.

The non-publishing workflow performs this sequence after re-verifying each
downloaded four-file target bundle and its runtime sidecar, then retains an
exact nine-file aggregate scope. A local command result proves the aggregation
contract only; it is not hosted-run, platform-runtime, signing, provenance or
publication evidence.

## 5. Rehearse installation and recovery

For macOS, Windows and Linux separately, capture the following evidence against
the exact candidate target:

1. The prerequisite check and selected artefact.
2. The integrity verification result.
3. The first-start identity output.
4. The version after upgrade from the previous known-good build.
5. What happened to configuration and user data.
6. Rollback to the previous version after an intentionally failed start.
7. The diagnostic action for an unsupported platform, architecture or missing
   prerequisite.

Do not claim hand verification from a CI build. Record the runner, operating
system, terminal, architecture and PostgreSQL server version for each evidence
item where relevant.

## 6. Review the support identity bundle

The future support bundle must contain only identity and safe diagnostics. A
reviewer should be able to answer:

- Which product version and target is installed?
- Which source/build identity produced it?
- Which operating system and terminal are involved?
- Which exit code and error category occurred?
- What is the next action?

It must not contain a password, token, full connection URI, SQL text, result
value or credential-file content. Use synthetic fixtures for negative tests.

Run one synthetic statement with `IGNATIUS_LOG=debug` and isolated
configuration/data directories. Review only the resulting test log. It must
contain an Ignatius-owned job id and statement character count, and must not
contain the SQL sentinel, result value, credential marker or a dependency trace
target such as `tokio_postgres`. Target directives in `IGNATIUS_LOG` must fail
closed. This is a release-blocking privacy check, not optional support evidence.

## Definition of done for this feature

- The release record and manifest validators reject version, target, checksum
  and evidence mismatches. The implemented catalogue validator also requires
  one exact versioned changelog heading; its focused contracts are a separate
  gate from this rehearsal.
- Every support-matrix row is covered or explicitly blocked.
- SBOM, signing and provenance status are visible and provider-neutral until
  their decisions are approved.
- Clean install, upgrade and rollback evidence exists for every supported
  platform claim.
- Release notes and compatibility claims identify their evidence class.
- A live publication is still a separately authorized operation.
