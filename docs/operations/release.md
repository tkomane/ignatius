# Release

No release has been published. This document defines the intended process and
labels local rehearsal evidence separately. No artefact may be described as
signed, notarised or published until a real release proves it.

## Publishing options

Nothing has been published. The repository is public source at `tkomane/ignatius`
since 2026-09-11; these are the remaining routes in increasing order of
commitment. Each row assumes the ones above it.

| Option | Effort | Ongoing cost | Reversible | Reaches | Needs first |
| --- | --- | --- | --- | --- | --- |
| **A. Stay local** | None | None | Fully | You | Nothing |
| **B. Private GitHub remote** | Minutes | None | Fully | You, and anyone you invite | A repository |
| **C. Public source, no artefacts** | Minutes | Issues arrive | Awkward: forks and clones persist | Anyone who builds it | Green CI on all three platforms |
| **D. Tagged releases with binaries and checksums** | Hours | Every release, plus platform bug reports | Artefacts can be deleted, but people have them | Anyone who downloads | C, plus a signing decision |
| **E. crates.io** | Under an hour | Versions are permanent and cannot be unpublished, only yanked | **No.** A published version is forever | Rust users, `cargo install` | D, plus a stable public API or a binary-only crate |
| **F. Homebrew tap and Scoop** | Half a day each | A manifest update per release, forever | Yes: delete the tap | macOS and Windows users who expect a package manager | D, and a release cadence you can sustain |
| **G. Homebrew core and Winget** | Days, plus review | Their standards, their timelines | Hard | Everyone | F, plus a user base that justifies it |

**Current distribution is C (public source).** The repository became public
on 2026-09-11. Post-merge CI and CodeQL passed on `3ca6811`, including the
Linux, macOS and Windows jobs. [CI run](https://github.com/tkomane/ignatius/actions/runs/44656868966)
[CodeQL run](https://github.com/tkomane/ignatius/actions/runs/44656868960)

The [compatibility matrix](../support/compatibility.md#release-target-matrix)
records archive and runtime checks and remaining native hand use. Distribution
requires the approved signing and provenance identities, canonical release
evidence, independently verified sidecars and owner authorization for the exact
release. [ADR-0008](../architecture/decisions/0008-packaging-and-release.md#live-release-gates)

## Release identity

Six facts, none derived from another:

| Fact | Source |
| --- | --- |
| Product version | `Cargo.toml`, and nothing else |
| Source revision | `git rev-parse HEAD`, captured by `build.rs` |
| Working tree state | clean or modified, captured by `build.rs` |
| Build identity | `IGNATIUS_BUILD_IDENTITY`, set by CI, `local` otherwise |
| Target triple | cargo's `TARGET` |
| Artefact checksum | Computed at packaging time |

`ignatius version --verbose` prints them separately. A rebuild of the same source
produces the same product version: the version is never inferred from a date, a
branch, or a CI run number.

## Candidate archive and checksum contract

This section defines the identity and integrity fields candidate tooling must
produce. The local commands below do not build, sign, upload or publish an
archive.

### Archive names and target identity

Every distributable archive or binary uses this basename shape:

```text
ignatius-<product_version>-<target><format_suffix>
```

For example:

```text
ignatius-0.1.0-preview.1-aarch64-apple-darwin.tar.gz
ignatius-0.1.0-preview.1-x86_64-pc-windows-msvc.zip
```

`<product_version>` comes from `Cargo.toml`. `<target>` is the exact Rust
target triple recorded by the build, including architecture, operating system
and environment where present. It is not shortened to a platform label and it
must not be inferred from the machine performing verification. `<format>` is
the actual archive or binary format recorded in the release record; the exact
format-to-suffix mapping is defined immediately below.

Names are canonical portable basenames: they start with an ASCII letter or
digit and contain only ASCII letters, digits, dots, underscores, hyphens and a
plus sign when that plus sign is the SemVer build-metadata separator. They
contain no adjacent dots, path separators or spaces, and duplicate names are
not allowed in one candidate. The target row, artefact name and build target
must agree. A supported platform without a built artefact is recorded as
`blocked` with a reason rather than represented by another platform's archive.

The product-version token is copied byte-for-byte from Cargo.toml and uses the
release schema's semantic-version grammar: numeric major, minor and patch with
optional ASCII alphanumeric, hyphen and dot prerelease/build identifiers, with
`+` separating build metadata. No escaping, normalisation or date suffix is
allowed. The target token uses only
ASCII letters, digits, hyphens, underscores and periods, begins with an
alphanumeric character, and contains no path separator, whitespace or `..`.
The format mapping is exact: `tar.gz` ends in `.tar.gz`, `zip` ends in `.zip`,
and `binary` has no suffix. The suffix is never selected from the host
operating system.

The set of artefacts must equal the set of target rows whose coverage is
`covered`, with exactly one artefact for each target. Rows marked `blocked` or
`out-of-scope` have no artefact entry and retain a non-empty reason.
Before a record can be `ready` or `published`, its target rows must include
every supported target in the release matrix. A target may be covered by an
artefact or explicitly blocked/out-of-scope with its reason; an omitted target
is a readiness failure, not an implicit skip.

### Structured manifest and checksum list

The candidate evidence directory contains two related files:

- `release-manifest.json` is UTF-8 JSON with `schema_version`,
  `product_version`, `source_revision`, `build_identity` and an `artefacts`
  array. Each array item has `name`, `target`, `format`, `size_bytes`,
  `checksum_algorithm` and `checksum`. The array is sorted by exact target
  triple, then exact basename. The identity fields must match the release
  record; no extra or duplicate artefact is permitted.
- `SHA256SUMS` is a UTF-8 text file with one line per artefact in the same
  basename order. Each line has exactly 64 lower-case hexadecimal characters,
  two spaces and the exact archive basename:

```text
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  ignatius-0.1.0-preview.1-aarch64-apple-darwin.tar.gz
```

The two files must cover exactly the same artefacts, once each. The manifest
files are evidence, not distributable artefacts, and are excluded from their
own checksum list. A parser rejects malformed JSON, non-canonical ordering,
unknown fields, missing entries, extra entries, duplicate names or duplicate
targets rather than guessing.

The canonical serialization is UTF-8 without a BOM, one final LF, no blank
lines, no trailing spaces and no insignificant JSON whitespace. Object keys
are sorted lexicographically at every level, integers use base-10 notation
without leading zeroes, and artefact arrays use target-triple then basename
order. Neither release-manifest.json nor SHA256SUMS is an artefact or may
appear as a line in SHA256SUMS. The manifest set must also equal the covered
target-row set; blocked and out-of-scope rows are excluded.

The corresponding release-record fields are `name`, `target`, `size_bytes`,
`checksum_algorithm`, `checksum_status`, `checksum`, `source_revision` and
`build_identity`. A `present` checksum must be a lower-case 64-character
SHA-256 digest. `missing` and `mismatch` are explicit blocked states; a
missing or mismatched digest is never replaced silently by a new manifest
entry. The manifest, release record and evidence bundle must all name the same
basename, target, product version, source revision and build identity.

### Verification rules

Recompute the digest from the downloaded bytes before extraction, then compare
it with both `SHA256SUMS` and the structured manifest. Also compare the byte
count with `size_bytes`. Verification fails closed when the archive is absent,
empty, renamed, listed more than once, represented by a non-SHA-256 digest, has
an extra or missing manifest entry, or produces a digest or size that does not
match the exact archive bytes and basename.

The digest and byte count prove properties of the exact bytes and basename
only. A separate metadata comparison must bind product version, target, source
revision and build identity across the manifest, release record and evidence
bundle. The platform commands below are hash and size primitives; the future
verifier must compare their values, reject missing or extra entries, and exit
non-zero before extraction when any comparison fails.

The platform-native commands are:

```bash
# macOS
shasum -a 256 -- "<archive>"
wc -c < "<archive>"

# Linux
sha256sum -- "<archive>"
wc -c -- "<archive>"
```

```powershell
# Windows PowerShell 7
$ErrorActionPreference = 'Stop'
$artifact = "<archive>"
(Get-FileHash -Algorithm SHA256 -LiteralPath $artifact).Hash.ToLowerInvariant()
(Get-Item -LiteralPath $artifact).Length
```

A passing checksum proves integrity of the bytes only; it does not prove
signing, provenance, publication or platform hand verification. The
non-publishing sidecar commands are:

~~~text
cargo --locked xtask release manifest generate \
  --record PATH --artefact-dir PATH --output-dir PATH
cargo --locked xtask release manifest verify \
  --record PATH --artefact-dir PATH \
  --manifest PATH --checksums PATH
~~~

`manifest generate` reads only the exact artefacts named by the one-record
catalogue, hashes their bytes and writes canonical `release-manifest.json` and
`SHA256SUMS` files into a new output directory without overwriting existing
evidence. The pair becomes visible together. `manifest verify` must
pass before `cargo --locked xtask release check PATH` is accepted for a candidate: it
re-reads the exact archive bytes, size, filename, target, source revision,
build identity and both sidecars, and fails closed before extraction. The
record check remains the metadata, evidence and publication-readiness gate; it
does not imply byte verification by itself.

The verifier also refuses a symlinked archive, manifest or checksum sidecar
before opening it. The archive boundary is therefore an ordinary-file check as
well as a digest check; a symlink cannot redirect byte verification to a
different file.

The `--output-dir` path must not already exist; this prevents a rerun from
silently replacing an evidence pair. The check uses symlink metadata, so a
dangling symlink at that path is also rejected rather than replaced, and a
symlinked parent is rejected before any evidence is written.

The candidate record written by `release generate` follows the same
non-overwriting boundary: its output parent must already exist and must not be
a symlink. This prevents a record path from redirecting writes outside the
intended staging root. Release validation and manifest operations also refuse a
symlinked catalogue input rather than following it as if it were the ordinary
release record.

The `--artefact-size` value passed to `release generate` is a declared
expectation until `manifest generate` re-reads the exact file and confirms it.
`cargo --locked xtask release generate` records a checksum as `present` only when
`--artefact-path` was supplied and the exact bytes were hashed. A caller-supplied
`--checksum` without that path is rejected. T013 now provides the separate
non-publishing `.github/workflows/release.yml` rehearsal: it builds one
target-specific archive per matrix row with the repository-owned deterministic
archive helper, generates and verifies these sidecars,
stages only the archive, record and verified sidecars under an exact upload
root, checks that root with `cargo --locked xtask release evidence-scope`, and retains it
as a run-scoped workflow artifact. The hardened workflow first freezes one
source revision and runs release-note validation, the dependency policy, a
full-history secret scan and the complete Linux verifier before retaining any
target bundle. Each matrix job emits a per-target record with one shared
run-attempt build identity. A final job downloads the three bundles, rechecks
each five-file scope, verifies each archive and inventory against its manifest,
and uses
`cargo --locked xtask release aggregate` to create a canonical three-target
record. It then generates and verifies one combined manifest, collects one
runtime sidecar per target, scans each retained target bundle and the final
package, and retains only the exact
three archives, aggregate record, combined manifest, checksum list, three
runtime sidecars and three CycloneDX inventories. Missing, duplicate,
unsupported or identity-mismatched inputs fail closed. Earlier rehearsals and
their retained file sets are recorded below. The readiness assertion and byte
reproducibility evidence remain part of the rehearsal.

The run-attempt identity intentionally prevents mixing target bytes from partial
reruns. If one archive job needs retrying, use **Re-run all jobs** or start a
fresh full dispatch. A partial rerun leaves the aggregate without three bundles
from one attempt and fails with an actionable recovery instruction instead of
assembling mixed-attempt evidence.

The Windows reproducibility pair uses independent clean compiler outputs on
one hosted Windows VM. It records and checks the selected native inputs around
the two builds. This controls for image revisions that ship different compiler
and linker bytes under the same version labels. The comparator still compares
both Windows archives exactly, along with the independent Linux and macOS
repeat pairs. Reproducibility across different Windows images remains open.

The archive helper refuses an existing output and a symlinked output parent. It
creates the completed archive in the destination directory and installs it
with a same-directory hard link, so a concurrent destination cannot be
silently replaced or redirected through a directory symlink.

### Windows live archive diagnostic

Use the run ID and attempt of a successful non-publishing rehearsal dispatched
on `main`, and the full source revision in its retained manifest.

~~~sh
gh workflow run windows-archive-runtime-diagnostic.yml --ref main \
  -f candidate_run=RUN_ID -f candidate_attempt=ATTEMPT \
  -f source_revision=FULL_SOURCE_REVISION
~~~

The diagnostic verifies the retained aggregate's archive bytes and then
extracts its Windows ZIP. The packaged client connects to a disposable native
PostgreSQL cluster over loopback using `verify-full` and a read-only session.
The check also requires a typed TLS refusal for the numeric address and
value-free client debug logging. The job logs a metadata receipt with the
binary hash, runner image, database version, result and fixture cleanup.
Windows Terminal interaction, signing and provenance verification have their
own acceptance evidence.

### Inventory and trust sidecars

The owner trust choices are Sigstore keyless signing, a CycloneDX SBOM, and
GitHub OIDC/SLSA provenance ([ADR-0008](../architecture/decisions/0008-packaging-and-release.md)).
The local commands check inventory reproducibility and sidecar binding.
The approved independent verifiers check cryptographic trust.

~~~text
cargo --locked xtask release sbom generate \
  --lockfile=PATH --out=PATH
cargo --locked xtask release sbom verify \
  --lockfile=PATH --sbom=PATH
~~~

`sbom generate` derives a CycloneDX 1.5 inventory deterministically from
`Cargo.lock`: no network, no clock, no randomness, so the same lockfile
always yields the same bytes. Its parser understands only `[[package]]`
name/version entries and refuses unknown keys, duplicates, or unclosed lists
rather than silently dropping crates. `sbom verify` regenerates and
byte-compares, naming the first differing offset. An SBOM proves which
dependencies the source resolves, not what they do.

~~~text
cargo --locked xtask release trust verify \
  --manifest-dir=DIR --bundle-dir=DIR
~~~

For every manifest artefact, `trust verify` requires a signature digest in
`<name>.sigstore.json` and a provenance document in `<name>.intoto.jsonl`.
The signature's `messageDigest` must carry the manifest SHA-256. The
provenance document must identify the SLSA v1 predicate and bind the exact
archive name and SHA-256 in one subject. See the [accepted provenance
formats](#trust-bundle-sidecar-verification).

1. Run `manifest verify` to check the actual archive bytes against
   `release-manifest.json` and `SHA256SUMS`, then run `sbom verify` and
   `trust verify`.
2. Use the independent Sigstore verifier to check each signature against the
   certificate identity approved in the release record and verify its
   transparency log evidence.
3. Use the [independent provenance verifier](#slsa-provenance-verification)
   for the approved producer. Verify the SLSA predicate, builder and source
   identity against the exact candidate.
4. Every step must pass before the candidate can meet its trust gate.

## Native dependency boundary

The generic release contract above proves candidate identity, exact archive
bytes, checksums, provider-neutral evidence states and recoverable installation
steps. It does not prove the presence, version, trust or runtime loadability of
a native database dependency.

ADR-0012 closed Feature 001a without implementation: the required Entra route
uses a bearer token as the password over the existing TLS connection and needs
no libpq dependency. The archive therefore has no libpq discovery, bundling or
fallback path to diagnose. If a real GSSAPI, Kerberos or SSPI requirement later
reopens that decision, the new feature must own library and TLS identity,
architecture and runtime closure, loader search-path trust, clean-install
behaviour and repairable missing-dependency diagnostics. Its evidence would
have to bind back to the same product version, source revision, target, build
identity and archive basename recorded here.

The generic workflow must not grow a speculative native-library check. It
packages the current `tokio-postgres` build and keeps its existing driver and
TLS failure rules unchanged.

## Platform installation and first start

This is the operator contract for an authorized archive once a real release has
passed the readiness gate. The repository currently contains only a
non-publishing packaging workflow, so the target rows below are defined
selection rules, not proof that a hosted archive can be installed. There is no
installer, package-manager manifest, automatic update check or published
download URL.

Select an archive by exact target triple and format. Do not choose by a broad
platform label or extract an archive for another architecture.

| Host requirement | Target triple | Archive | Local tools |
| --- | --- | --- | --- |
| Apple silicon macOS | `aarch64-apple-darwin` | `tar.gz` | `shasum`, `tar`; `jq` for structured-sidecar review |
| x86_64 Windows | `x86_64-pc-windows-msvc` | `zip` | PowerShell `Get-FileHash`, `Expand-Archive` |
| x86_64 Linux with compatible GNU libc | `x86_64-unknown-linux-gnu` | `tar.gz` | `sha256sum`, `tar`; `jq` for structured-sidecar review |

The current release matrix does not define an x86_64 macOS, Windows ARM64,
Linux ARM64 or Linux musl archive. Those hosts are outside the release claim
until a target row and evidence are added. `uname -m` is a useful architecture
check on Unix. The Windows target row requires an x64 operating system, since
an emulated process can report a different architecture. Compare the host
architecture with the exact target and runtime evidence in the release record.

Check each Unix tool with `command -v`. In PowerShell, check
`Get-Command Get-FileHash, Expand-Archive -ErrorAction Stop`. If a required
checksum or extraction tool is missing, restore it through the host's normal
setup or use a compatible host with those tools before continuing. See the
structured-manifest alternatives below if `jq` is unavailable.

### Wrong architecture or missing host runtime

A wrong-architecture binary cannot run far enough to produce an Ignatius
diagnostic, so this check belongs before first start. Do not work around a
loader failure by trying an archive from another target row.

- On Apple silicon, `uname -m` must print `arm64`. `Bad CPU type in executable`
  means the selected archive does not match the host. This release matrix has
  no Intel macOS archive.
- On Linux, `uname -m` must print `x86_64`. For an `Exec format error`, recheck
  the host and archive targets. If the verified executable exists and the
  launcher reports `No such file or directory`, check for the ELF interpreter
  named by `readelf -lW ./ignatius`. For a missing `GLIBC_` version, compare the
  error with `getconf GNU_LIBC_VERSION`. Record the loader error and use an
  x86_64 host with a compatible GNU runtime. [Linux execve errors](https://man7.org/linux/man-pages/man2/execve.2.html#ERRORS)
- On Windows, run
  `[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture` in
  PowerShell and require `X64`. `This app can't run on your PC` or a bad-image
  error is a target mismatch unless the retained platform evidence proves a
  different prerequisite failure. This matrix has no Windows ARM64 archive.

If `ignatius version --verbose` starts, record its exact target and compiler
identity. If it does not start, record the host architecture, archive basename,
digest result and the operating system's loader message. Do not include an
environment dump, configuration file, connection URI or credential in the
support evidence.

### Verify before extraction

Keep the archive, `SHA256SUMS`, structured manifest and release record together.
Use the exact basename and target named by the record. Each example checks the
selected archive's checksum and manifest entry before extraction, so a download
for one target can use the full release-wide `SHA256SUMS` unchanged. A checksum
match establishes the integrity of the archive bytes. Signing, notarisation
and platform acceptance have separate evidence.

On macOS and Linux, set `archive_path` and `target` to the selected archive
and its exact target triple. Run the block from the sidecar directory:

```bash
(
  set -eu
  archive_path='<verified-archive>'
  target='<target-triple>'
  name="${archive_path##*/}"
  expected="$(awk -v name="$name" '
    substr($0, 65) == "  " name {
      digest = substr($0, 1, 64)
      count++
    }
    END {
      if (count != 1 || digest !~ /^[0-9a-f]+$/) exit 1
      print digest
    }
  ' SHA256SUMS)" || {
    printf 'SHA256SUMS must contain exactly one SHA-256 entry for %s\n' "$name" >&2
    exit 1
  }
  if [ "$(uname -s)" = Darwin ]; then
    checksum="$(shasum -a 256 < "$archive_path")"
  else
    checksum="$(sha256sum < "$archive_path")"
  fi
  digest="${checksum%% *}"
  if [ "$digest" != "$expected" ]; then
    printf 'SHA-256 mismatch for %s\n' "$name" >&2
    exit 1
  fi
  size="$(wc -c < "$archive_path" | tr -d '[:space:]')"
  printf 'SHA-256 verified: %s\nBytes: %s\n' "$name" "$size"
  jq -es --arg name "$name" --arg target "$target" \
    --arg digest "$digest" --argjson size "$size" '
    length == 1 and (
      [.[0].artefacts[] | select(.name == $name or .target == $target)] |
      length == 1 and .[0].name == $name and .[0].target == $target and
      .[0].checksum_algorithm == "sha256" and .[0].checksum == $digest and
      .[0].size_bytes == $size
    )
  ' release-manifest.json
)
```

On Windows, run this block in PowerShell from the sidecar directory:

```powershell
$ErrorActionPreference = 'Stop'
$archive = '<verified-archive>'
$target = 'x86_64-pc-windows-msvc'
$name = Split-Path -Leaf $archive
$lines = @(Get-Content -LiteralPath '.\SHA256SUMS' |
    Where-Object { $_.Length -gt 64 -and $_.Substring(64) -ceq "  $name" })
if ($lines.Count -ne 1 -or $lines[0] -cnotmatch '^[0-9a-f]{64}  ') {
    throw "SHA256SUMS must contain exactly one SHA-256 entry for $name"
}
$expected = $lines[0].Substring(0, 64)
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
if ($actual -cne $expected) { throw "SHA-256 mismatch for $name" }
$bytes = (Get-Item -LiteralPath $archive).Length
Write-Output "SHA-256 verified: $name"
Write-Output "Bytes: $bytes"
$manifest = Get-Content -LiteralPath '.\release-manifest.json' -Raw | ConvertFrom-Json
$entries = @($manifest.artefacts | Where-Object { $_.name -ceq $name -or $_.target -ceq $target })
if ($entries.Count -ne 1 -or $entries[0].name -cne $name -or $entries[0].target -cne $target -or
    $entries[0].checksum_algorithm -cne 'sha256' -or
    $entries[0].checksum -cne $actual -or $entries[0].size_bytes -ne $bytes) {
    throw "Manifest must contain one matching SHA-256 and byte count for $name"
}
Write-Output "Manifest verified: $name"
```

On Unix the structured manifest check succeeds when `jq` prints `true` and
exits zero. Without `jq`, use a trusted JSON reader to compare the basename,
target, SHA-256 and byte count with the one matching entry.
The repository's maintainer-level verifier is the exact
`cargo --locked xtask release manifest verify` command documented above; it additionally
rejects malformed or non-canonical sidecars, missing or extra entries and
identity mismatches. If the structured manifest check cannot be completed,
record the checksum result and stop before extraction.

### macOS and Linux

After verification, extract into a new temporary directory and confirm that the
archive contains the one expected binary at its root. The workflow creates that
shape and does not include configuration or data files:

```bash
stage="$(mktemp -d)" &&
  tar -xzf "<verified-archive>" -C "$stage" &&
  test -f "$stage/ignatius" &&
  "$stage/ignatius" version --verbose
```

For a per-user installation, keep the binary under a user-owned directory and
put that directory on `PATH`. A versioned binary or directory is preferable to
overwriting the previous one. The repository's `cargo --locked xtask install` command
builds from a source checkout; it is not the installer for a release archive.
For example, after the first-start check, a Unix user can keep releases
side-by-side and point a stable per-user link at the selected one:

```bash
version='<product-version>'
target='<target-triple>'
install_root="$HOME/.local/lib/ignatius/$version/$target"
mkdir -p "$install_root" "$HOME/.local/bin"
install -m 0755 "$stage/ignatius" "$install_root/ignatius"
ln -sfn "$install_root/ignatius" "$HOME/.local/bin/ignatius"
```

The install root is separate from `~/.config/ignatius` and
`~/.local/share/ignatius`, which hold user state. Add `$HOME/.local/bin` to
`PATH` if it is not already there.
On macOS, signing and notarisation are not configured by this project. Do not
turn off platform security controls as an installation step; a blocked launch
must remain an explicit release-owner or signing decision.

On Linux, the `gnu` target requires a compatible GNU libc runtime. A successful
archive extraction is not a libc or first-run test. If the binary cannot start,
retain the diagnostic and use the target-specific support boundary rather than
substituting an archive from another row.

### Windows

After verification, extract to a new version-specific directory. Do not unpack
over an existing installation while diagnosing a candidate:

```powershell
$ErrorActionPreference = 'Stop'
$versionDir = Join-Path $env:LOCALAPPDATA 'Programs\Ignatius\<product-version>\x86_64-pc-windows-msvc'
New-Item -ItemType Directory -Force -Path $versionDir | Out-Null
Expand-Archive -LiteralPath '<verified-archive>' -DestinationPath $versionDir
$binary = Join-Path $versionDir 'ignatius.exe'
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "archive did not contain ignatius.exe at its root"
}
& $binary version --verbose
$env:Path = "$versionDir;$env:Path"
```

The temporary `PATH` change is for the current PowerShell process only. Add a
version directory to the user PATH through the normal Windows environment
settings only after its first-start checks pass. The `Programs` install root is
separate from `%APPDATA%\ignatius` and `%LOCALAPPDATA%\ignatius`, which hold
user state. The archive is a binary-only bundle; it does not install a service,
write configuration, or move database data.

### First-start checks

Run these checks from the installed binary before opening the full-screen
client or asking a user to connect:

```text
ignatius version --verbose
ignatius doctor --json
ignatius config paths
ignatius config validate
```

The first run may have no configuration file and should report built-in
defaults. `ignatius config init` is optional and writes a starter file; it does
not create a connection, store a password or contact a server. If a database
check is needed, use a target with no password in the command text:

```text
ignatius connect --check "postgres://user@host:5432/database"
```

Supply credentials through the supported password-file, environment or prompt
routes, not by placing them in an archive command or shell history. Record the
binary's verbose identity and the exact target row used in the installation
evidence. Do not record a successful first start as platform hand evidence until
the binary has actually been opened on that platform.

Before accepting a candidate, repeat one synthetic statement with
`IGNATIUS_LOG=debug` and isolated configuration/data directories. The new log
bytes must contain the internal job id and statement character count, and must
not contain the statement sentinel, its result value, a credential marker or a
dependency trace target such as `tokio_postgres`. Use only the disposable local
database for this check. Any SQL or result value in the log rejects the candidate
even when installation and connection checks pass.

## Configuration, data preservation and rollback

An archive upgrade replaces a binary, not the user's configuration or local
state. Before changing the active binary, run `ignatius config paths` and copy
the exact configuration and data directories it reports to a protected,
user-controlled backup location. Treat the backup as sensitive: statement
history contains SQL text even though credentials are filtered heuristically,
and logs or saved queries may contain environment-specific information.

The paths are normally:

| Content | Unix default | Windows default |
| --- | --- | --- |
| Configuration and saved queries | `~/.config/ignatius` | `%APPDATA%\ignatius` |
| Data, history and logs | `~/.local/share/ignatius` | `%LOCALAPPDATA%\ignatius` |

`IGNATIUS_CONFIG_DIR`, `IGNATIUS_DATA_DIR` and the Unix XDG variables can
override these locations. Never back up only the defaults when
`ignatius config paths` reports an override.

### Side-by-side upgrade

1. Close running Ignatius processes and preserve the currently working binary
   and its verbose identity. Do not overwrite it yet.
2. Back up the configuration and data paths, including saved queries, history,
   logs and any migration backups that already exist. On Unix, preserve file
   permissions; on Windows, keep the backup in a location accessible only to
   the intended user.
3. Verify the new archive and extract it into a new version-specific location
   using the installation procedure above.
4. Run `version --verbose`, `doctor --json`, `config validate` and a non-secret
   `connect --check` from the new binary. Keep the old binary available until
   these checks and the required platform evidence pass.
5. If the new binary reports an older configuration schema, run
   `config migrate --dry-run` first. Review the named steps, then run
   `config migrate` only after the backup exists. A real migration creates a
   timestamped backup beside `config.toml`; a dry run writes nothing.
6. Switch the PATH entry or per-user launcher to the new version only after
   the checks pass. Do not delete the old binary or backup as part of the
   upgrade.

Configuration writes are atomic. A crash during a write leaves the previous
file intact, and a migration refuses to proceed when the file is invalid or is
from a newer schema than the binary understands. A missing configuration file
is valid and uses defaults. There is no result cache to migrate; result rows
are kept in memory unless the user explicitly exports them.

### Failed upgrade repair

If the new binary fails to start, reports a configuration error or fails its
non-secret connection check, keep the new version directory and diagnostics for
the evidence record. Switch the PATH or launcher back to the old binary. Do not
delete or edit the only copy of the configuration or data directory while
diagnosing the failure. If a migration already ran, restore the timestamped
pre-migration configuration backup before starting the older binary. If local
state is implicated, restore the separately backed-up data directory only
after first moving the failed state aside so it remains recoverable.

For a syntax or validation error with no known-good backup, run
`ignatius config paths`, copy the file aside, and use `config validate` to guide
the repair. Starting with defaults is a last-resort recovery path, not an
upgrade step. Do not use `config init --force` until the existing file has been
copied somewhere safe.

### Rollback

Rollback means returning both the binary and the configuration schema to the
last known-good pair:

1. Stop the new binary and keep its version directory and logs.
2. Restore the previous PATH entry or launcher without overwriting the new
   binary.
3. If configuration was migrated, move the current `config.toml` aside and
   restore the matching timestamped migration backup or external backup.
4. Restore data only if the old binary cannot read the current state; keep the
   current data under a dated recovery name rather than deleting it.
5. Run the old binary's `version --verbose`, `config validate` and
   `connect --check`, then record the rollback reason and the exact identities.

If the old binary reports that the configuration schema is newer, the binary
and configuration pair do not match; restore the older configuration backup or
use a newer binary. Never make a downgrade appear successful by silently
discarding a schema or state file. A rollback rehearsal is not complete until
the prior binary starts, the configuration validates and the recovery outcome
is recorded for the platform.

### Uninstallation and clean state removal

To completely remove Ignatius or clean up local state:

1. **Binary and staged releases**:
   - macOS and Linux: remove `$HOME/.local/bin/ignatius` and any staged version
     directories under `$HOME/.local/lib/ignatius`.
   - Windows: remove `%LOCALAPPDATA%\Programs\Ignatius` and remove the binary
     directory from the user `PATH` environment variable.
2. **Configuration**:
   - macOS and Linux: remove `$XDG_CONFIG_HOME/ignatius` (defaults to
     `~/.config/ignatius`).
   - Windows: remove `%APPDATA%\ignatius`.
3. **Local query history and state**:
   - Query history can be cleared while keeping the tool installed by running
     `ignatius history clear --yes`.
   - To remove all local data, query history, and logs entirely, remove
     `$XDG_DATA_HOME/ignatius` (defaults to `~/.local/share/ignatius`) on macOS/Linux
     and `%LOCALAPPDATA%\ignatius` on Windows.

## Non-publishing workflow dry run and authorization gate

The local dry run on 2026-08-31 exercised the implemented packaging path for
`aarch64-apple-darwin` at source revision
`f84afda2eb6b332c2b9659ef5b83671fbbdc4f74`, explicitly labelled modified. A
locked release build with identity `local-rehearsal-2026-08-31` produced the
root-only archive `ignatius-0.1.0-aarch64-apple-darwin.tar.gz`, 3,010,634 bytes,
with SHA-256
`c7ba67b568ea803d9f6f7d8e1de802e25cdf539d1377618c76825d1683aa380d`.
The repository validator accepted the blocked record, generated and verified
the one-artefact manifest and checksum list, and accepted the exact four-file
upload scope. Repeating the archive helper with the same binary and inputs
produced identical bytes.

The archive extracted to one root binary. Its identity, configuration paths
and configuration validation passed; `doctor --json` reported seven checks OK,
three expected skips and no failures. The network-disabled, digest-pinned
Gitleaks v8.30.0 container scanned only the checked upload root and reported no
leaks. These are local macOS and Docker results, not hosted workflow or
cross-platform installation evidence. The temporary files were not retained as
release evidence.

The first hosted attempt, run `33561901626` on 2026-09-01, checked out exact
revision `247d1c7e829aa5b642a5047e56e53d402e386dbb`. All three declared target
builds passed. All three then failed before archive creation because the common
step parsed one exact Cargo package-ID spelling for the version and received no
match under the pinned runner toolchain. No candidate archive, checksum sidecar
or run-scoped evidence bundle was retained.

The repair reads the product version directly from `Cargo.toml`, which is its
authority, and gives each pre-archive refusal an actionable message. It was
committed and pushed as
`24f1c8e7801ab5f1f6805e54e4e5f4db67ca1949`. Hosted CI run `33563478021`
passed all 10 jobs at that exact revision.

The owner authorized only the scoped commit, push and non-publishing workflow
rerun for product version `0.1.0`, that exact revision and the three declared
targets. Run `33563497933` completed across 2026-09-01 and 2026-09-02 in
Africa/Johannesburg. Every target job built one binary, created its root-only
archive, verified its record, manifest, checksum and blocked readiness result,
checked the exact four-file upload scope and retained that scope as a GitHub
Actions artifact for seven days.

The three downloaded bundles were independently inspected. They contained no
symlinks or unexpected files; record validation, manifest verification,
SHA-256 verification and the four-file evidence-scope check passed for every
target. Each archive contained exactly one executable with the declared binary
format and architecture. The downloaded Apple-silicon binary also passed
isolated first-start identity, configuration-path, configuration-validation and
diagnostic checks on an arm64 Mac. The Windows and Linux binaries were not run
on their target platforms, so the workflow result is packaging evidence rather
than cross-platform installation, upgrade or rollback evidence.

Run `33563497933` predates the current aggregate job and embedded a
target-suffixed build identity in each binary, so its retained records are
historical target-specific evidence and are not retroactively aggregatable.
The corrected workflow was pushed at
`fc79120b5229a1724a5a589dee4337acf2daa36b`. Hosted CI run `33614417589`
passed all ten jobs, and owner-authorized non-publishing release run
`33614446587` passed all three target archive jobs and its aggregate job.

Run `33614446587` used one shared attempt identity, `ci/33614446587-1`, for
all three targets while retaining target as a separate fact. Its final job
re-verified each downloaded four-file bundle, produced one canonical aggregate
record, verified one combined manifest and retained an exact six-file aggregate
scope. The aggregate artifact and all three target artifacts are bound to the
pushed SHA and retained through 2026-09-09. The downloaded aggregate record
validated, all three archive checksums matched and the combined manifest
verified locally.

The aggregate readiness check exited 1 as required and named eight issues:
incomplete and unmaterialised evidence, detached and untagged source, a
non-publishable state, and absent verified signature and provenance evidence.
The candidate must remain blocked until complete evidence, clean and correctly
tagged source, dependency inventory, verified signatures, verified provenance,
platform installation and recovery evidence, and publication authorization all
exist. A checksum match, successful build or successful aggregate job does not
satisfy those gates.

The corrected workflow run `33689674455` checked out the full source revision
`259ac76de6477719b8a3515777f3623b24237dd6` and completed all archive,
repeat-build, comparison, runtime and aggregate jobs. Every target produced
byte-identical repeat archives, and the three runtime jobs passed identity,
diagnostic, configuration, non-interactive refusal, side-by-side validation,
controlled invalid-configuration exit 3 and known-good rollback checks. The
run uses `/Brepro` for Windows reproducibility and is retained through
2026-09-09 with shared build identity `ci/33689674455-1`.

After download, the aggregate contained exactly nine ordinary files: three
target archives, one aggregate record, one combined manifest, `SHA256SUMS` and
three target runtime JSON sidecars. The local allowlist, record validation,
combined manifest and independent archive checksums passed. The runtime
sidecars are hosted automated smoke evidence only: they do not establish a
database query or direct `IGNATIUS_LOG=debug` privacy audit, full-screen
Windows/Linux terminal behaviour, or a published upgrade. The aggregate
readiness check remained blocked by eight named issues, including the
unmaterialised canonical evidence path, detached and untagged source, and
missing verified signature and provenance.

The recorded authorization for runs `33614446587` and `33689674455` covers only
the scoped commit, repository push, workflow dispatch and seven-day, run-scoped
non-publishing GitHub Actions artifacts. It does not
authorize a tag, signing operation, GitHub Release, package-registry upload,
package-manifest change or promotion into the canonical evidence tree. Before
any of those future operations, the owner must record new explicit
authorization naming the exact product version, source revision, target set,
artefact destinations and approved evidence retention boundary. This rehearsal
performed none of those publication operations.

## Trust verification and governance runbook (W11 and W13)

This runbook specifies operator and consumer verification procedures for release
trust artefacts (CycloneDX SBOM, Sigstore keyless signatures, SLSA provenance)
and the dependency vulnerability triage process.

### CycloneDX SBOM generation and verification

The software bill of materials is derived deterministically from `Cargo.lock`
using the pinned CycloneDX 1.5 schema without network access:

```bash
# Generate the deterministic CycloneDX SBOM from Cargo.lock
cargo xtask release sbom generate \
  --lockfile=Cargo.lock \
  --out=release-evidence/ignatius.cdx.json

# Verify the stored SBOM reproduces the exact locked crate inventory
cargo xtask release sbom verify \
  --lockfile=Cargo.lock \
  --sbom=release-evidence/ignatius.cdx.json
```

If any dependency is added, removed, or changed in version without regenerating
the SBOM, `sbom verify` exits non-zero and reports the exact byte offset where
the inventory diverges from the stored file.

### Candidate dependency inventories

The release workflow builds with pinned Cargo and captures the compiler records
for the archive's native target build and the `xtask` packaging build. It
compares the emitted package identities against locked Cargo metadata and binds
the sidecar to the exact archive manifest. Reproduction uses the same build
flags, source, policy checks and build inputs. The two raw Cargo traces and
metadata file stay in runner scratch storage.

Each retained `evidence/<target>.cdx.json` includes the observed crate inputs
and their Cargo features, the inactive lockfile entries, and the release,
source, build, toolchain and archive identities. The workflow runs
`release sbom candidate verify-build` before upload and
`release sbom candidate bind` after the download. The aggregate also compares
the built executable's checksum with the checksum observed by the runtime job
after extraction.

The local `tokio-postgres` 0.7.18 patch appears as a library with
`ignatius:cargo:source=repository-patch`,
`ignatius:cargo:source-manifest=vendor/tokio-postgres/Cargo.toml`, and
`ignatius:cargo:source-tree-sha256`. The digest covers the regular files in the
package, sorted by slash-separated package-relative path. Each digest input
line contains the lowercase SHA-256 of one file, two spaces, its relative
path, and an LF. Both candidate verification commands reconstruct the digest
from the checked-out source.

CI and release preflight check the patched workspace and use the same pinned
Cargo Deny tool and policy to check the original crates.io release for
advisories and yanks. The latter check uses locked Cargo metadata generated
from a temporary workspace with the registry identity restored.

To verify a downloaded sidecar's binding, use the frozen source checkout and
first verify the archive manifest and checksums. Then run the following command.

~~~sh
cargo --locked xtask release sbom candidate bind \
  --target="$TARGET" --manifest="$AGGREGATE/evidence/release-manifest.json" \
  --sbom="$AGGREGATE/evidence/$TARGET.cdx.json"
~~~

### Trust bundle sidecar verification

Before publishing or distributing release archives, operators verify that the
Sigstore signature bundle and SLSA in-toto provenance statement bind the exact
SHA-256 archive checksums declared in `release-manifest.json`:

```bash
# Verify that sidecars bind the manifest checksums across manifest and bundle directories
cargo xtask release trust verify \
  --manifest-dir=<manifest-dir> \
  --bundle-dir=<bundle-dir>

# For a unified release distribution directory (e.g., dist/ or release-bundle/):
cargo xtask release trust verify \
  --manifest-dir=dist \
  --bundle-dir=dist
```

In operator wrapper tooling, `--dir <bundle-dir>` maps directly to passing the
bundle directory to both `--manifest-dir` and `--bundle-dir`.

Each `<archive>.intoto.jsonl` contains one JSON document. The dry binding
check accepts a bare in-toto v1 statement, a standalone DSSE envelope, or
a [Sigstore bundle containing a DSSE envelope](https://docs.sigstore.dev/about/bundle/#dsse-envelope).
Bundles must use the Sigstore v0.3 media type or a legacy version-parameter
media type listed in the [Sigstore bundle schema](https://github.com/sigstore/protobuf-specs/blob/main/protos/sigstore_bundle.proto).
An envelope contains a base64-encoded statement with an in-toto JSON
payload type and nonempty signature fields. The statement must specify `https://slsa.dev/provenance/v1`
as its predicate type. One subject must pair the exact manifest archive
name with its SHA-256 digest. Missing sidecars, duplicate keys, multiple
documents, ambiguous envelopes and mismatches block the check.

Run `manifest verify` to check the archive bytes and `SHA256SUMS`.
Cryptographic verification uses the signed envelope or bundle from the
approved producer.

### Independent cryptographic verification

Consumer verification of Sigstore signatures and SLSA provenance statements
does not depend on repository tooling. Users verify downloaded archives using
official standalone verifiers.

#### Sigstore keyless signature verification (cosign)

Verify the archive signature against the repository workflow identity:

```bash
# Verification using individual signature and certificate sidecars:
cosign verify-blob \
  --certificate <archive>.pem \
  --signature <archive>.sig \
  --certificate-identity-regexp '^https://github\.com/tkomane/ignatius/\.github/workflows/release\.yml@refs/tags/v.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  <archive>

# Verification using the Sigstore bundle sidecar:
cosign verify-blob \
  --bundle <archive>.sigstore.json \
  --certificate-identity-regexp '^https://github\.com/tkomane/ignatius/\.github/workflows/release\.yml@refs/tags/v.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  <archive>
```

#### SLSA provenance verification

For provenance produced by GitHub Artifact Attestations, pass the retained
Sigstore bundle and the approved signer workflow, source ref, and source
revision from the release record. [GitHub Actions attestation output](https://github.com/actions/attest#outputs), [GitHub CLI verification](https://cli.github.com/manual/gh_attestation_verify).

```bash
gh attestation verify "$ARCHIVE" \
  --bundle "$PROVENANCE_SIDECAR" \
  --repo tkomane/ignatius \
  --signer-workflow "$APPROVED_SIGNER_WORKFLOW" \
  --source-ref "$APPROVED_SOURCE_REF" \
  --source-digest "$SOURCE_REVISION" \
  --predicate-type https://slsa.dev/provenance/v1
```

For a supported SLSA GitHub generator with standalone DSSE output, verify
using its approved builder identity and source tag. Compare the source
revision in the verified output to the release record. [SLSA verifier](https://github.com/slsa-framework/slsa-verifier#verification-for-github-builders).

```bash
slsa-verifier verify-artifact "$ARCHIVE" \
  --provenance-path "$PROVENANCE_SIDECAR" \
  --builder-id "$APPROVED_BUILDER_ID" \
  --source-uri github.com/tkomane/ignatius \
  --source-tag "$APPROVED_VERSION_TAG" \
  --print-provenance
```

### Dependency vulnerability triage and emergency patch releases

All dependencies are continuously checked against RustSec and supply-chain
policies.

#### Vulnerability scanning

Run advisory and policy checks locally or in CI:

```bash
# Advisory database audit
cargo audit

# Comprehensive advisory, license, ban, and source policy validation
cargo deny check advisories bans licenses sources
```

#### Triage protocol

When an advisory is reported:

1. **Classify dependency scope**:
   - Shipped runtime code (`src/` and core production dependencies).
   - Test or build tooling only (`tests/`, `xtask/`, or `dev-dependencies`).
2. **Evaluate reachability**:
   - Determine whether the vulnerable function, type, or code path is reachable
     from user input, network input, or configuration in `ignatius`.
   - If unreachable or isolated to dev-dependencies, document the rationale in
     the security tracker and schedule an update in the next routine cycle.
   - If reachable in shipped code, treat the advisory as an emergency blocker.
3. **Remediation**:
   - Bump the dependency version in `Cargo.toml` to a patched release.
   - Run `cargo update -p <crate_name>` to update `Cargo.lock`.
   - Run `cargo xtask verify` to ensure tests, formatting, and lints pass.
   - Run the [per-target inventory rehearsal](#candidate-dependency-inventories)
     and verify the retained archive binding.

#### Emergency patch release procedure

1. Record the CVE or RUSTSEC ID and patch summary in `CHANGELOG.md` under a new
   patch version section (for example `## [0.1.1] - YYYY-MM-DD`).
2. Increment the patch version in `Cargo.toml` (for example `version = "0.1.1"`).
3. Validate locally:
   ```bash
   cargo xtask verify
   ```
4. Run the non-publishing release rehearsal against the exact patched source.
   It produces archives and CycloneDX inventories for macOS, Windows, and Linux.
5. Obtain the owner's authorization for the exact tag and release actions.
   The tag-triggered workflow also stops at candidate preparation. Follow
   the [trust gates](#before-any-public-release) for signing, provenance,
   independent verification and publication.

### Trust-key revocation and rotation rehearsal (W15)

Maintainers rehearse signing-identity revocation and trust-bundle rotation to
ensure compromised identities cannot publish or validate releases:

1. **Rehearsal protocol**:
   - Prepare synthetic signature and provenance sidecars with missing or
     mismatched digests to exercise structural check failures.
   - Run trust verification to confirm the invalid sidecars are rejected:
     ```bash
     cargo --locked xtask release trust verify --manifest-dir=DIR --bundle-dir=DIR
     ```
     Verification must fail with non-zero exit code and explicitly name the
     offending sidecar.
   - Run release readiness check against the candidate catalog:
     ```bash
     cargo --locked xtask release check catalog.json
     ```
     `release check` must exit with code 1, reporting `Release readiness: blocked`
     and citing `signatures_and_provenance_verified` or missing verified signature/provenance
     evidence as blockers.
2. **Rotating GitHub Actions OIDC workflow references**:
   - When a workflow or signing identity is rotated, update the expected OIDC
     issuer and SAN URI in the release verification workflow and repository secrets.
   - Use the approved independent verifier to check that signatures produced
     under older or revoked workflow identities are rejected.
3. **Operator rollback guidance**:
   - If a published release candidate is revoked or defective, mark its catalogue
     status as `withdrawn` or `blocked`.
   - Publish a security advisory instructing operators to roll back to the last
     verified build following the rollback runbook.
   - Ensure operator rollback preserves configuration files and statement history
     intact without data loss.

## Steps

1. Update `CHANGELOG.md` under a new version heading. The changelog is the single
   source of release notes, and `New`, `Improved`, `Fixed`, `Security` and
   `Known limitations` are generated from it.
2. Update the version in `Cargo.toml` and commit the lockfile change.
3. Confirm every gate in `docs/operations/verification.md` passes on all three
   platforms.
4. Confirm `docs/support/compatibility.md` matches reality, especially the tested
   server versions and the unsupported parameter list.
5. Tag the commit. One tag, one version, one changelog entry.
6. Build artefacts in CI for each target from that tagged commit.
7. Re-verify all three target bundles, aggregate their records and exact bytes,
   and inspect the retained twelve-file scope, including one inventory per target.
8. Execute the packaged-archive J8 and W04 rehearsal probes on each extracted archive
   before recording candidate validation:
   - J8 Identity Probe: run `version --verbose` and verify Cargo.toml version, git revision,
     target triple, and build identity.
   - J8 Diagnostics Probe: run `doctor --json` and verify status and checks without credential
     disclosure.
   - J8 Invalid Configuration Refusal Probe: run `doctor` with a malformed `config.toml` and
     verify exit code 3 (`InvalidConfiguration`).
   - J8 Preservation and Rollback Probe: verify existing configuration and `history.jsonl`
     bytes remain 100% intact after candidate rehearsal and rollback.
   - W04 Runtime Privacy Sentinel Probe: run candidate binary with `IGNATIUS_LOG=debug`
     and `--plain` against an unreachable local endpoint with test statement stdin,
     confirming passwords, cell payloads, query tokens, and driver internal targets
     are never logged or displayed.
   The hosted archive smoke captures these checks in the target runtime JSON.
   It keeps stdout, stderr, history, and the debug log inside the disposable
   runtime directory. Live database privacy checks are recorded separately.
9. Publish checksums alongside the artefacts only after every remaining gate
   and the exact publication authorization are recorded.
10. Only then update any package manifest.

## Phased distribution

1. GitHub release archives with checksums for macOS, Windows and Linux.
2. A Homebrew tap and a Scoop manifest.
3. Winget, once identity and installer behaviour are stable.
4. Anything else only when there is someone to maintain it.

## Before any public release

- Finish the naming work in `docs/product/landscape.md`: the name is confirmed,
  but the dated research does not establish trademark clearance or registrar
  availability. Obtain the owner's domain/distribution decision before acting.
- Verify `SECURITY.md` and the private reporting route. The setting was enabled
  on 2026-10-25; submission, notification and response handling remain untested.
- Use ADR-0008's selected CycloneDX, Sigstore keyless and GitHub OIDC/SLSA
  mechanisms. Complete the outstanding verification identity, failure/revocation
  handling, retention and independent-verification details, then generate and
  validate the actual candidate evidence. Provider selection is not proof that
  an artifact is signed or provenance is verified; live actions still require
  the owner's explicit authority.

## Support and deprecation

To be defined with the first release. A breaking change to the exit codes, the
machine output formats, or the configuration schema requires a major version and
a migration note, because scripts depend on all three.
