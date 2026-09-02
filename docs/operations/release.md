# Release

No release has been published. This document defines the intended process and
labels local rehearsal evidence separately. No artefact may be described as
signed, notarised or published until a real release proves it.

## Publishing options

Nothing has been published. The repository has a private GitHub remote; these
are the remaining routes in increasing order of commitment. Each row assumes
the ones above it.

| Option | Effort | Ongoing cost | Reversible | Reaches | Needs first |
| --- | --- | --- | --- | --- | --- |
| **A. Stay local** | None | None | Fully | You | Nothing |
| **B. Private GitHub remote** | Minutes | None | Fully | You, and anyone you invite | A repository |
| **C. Public source, no artefacts** | Minutes | Issues arrive | Awkward: forks and clones persist | Anyone who builds it | Green CI on all three platforms |
| **D. Tagged releases with binaries and checksums** | Hours | Every release, plus platform bug reports | Artefacts can be deleted, but people have them | Anyone who downloads | C, plus a signing decision |
| **E. crates.io** | Under an hour | Versions are permanent and cannot be unpublished, only yanked | **No.** A published version is forever | Rust users, `cargo install` | D, plus a stable public API or a binary-only crate |
| **F. Homebrew tap and Scoop** | Half a day each | A manifest update per release, forever | Yes: delete the tap | macOS and Windows users who expect a package manager | D, and a release cadence you can sustain |
| **G. Homebrew core and Winget** | Days, plus review | Their standards, their timelines | Hard | Everyone | F, plus a user base that justifies it |

**Current state: B. Move to C only once CI is green on all three platforms.**
Publishing source before Windows and Linux have ever been run would
invite issues about platforms nobody has tested, which is the fastest way to
spend a weekend on someone else's environment.

D onwards should wait for a real 0.1.0 with the compatibility matrix filled in.
E in particular is one-way: a published crates.io version can be yanked but never
removed, so the name and the first public API arrive together and permanently.

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
as a run-scoped workflow artifact. Each matrix job emits a per-target record
with one shared run-attempt build identity. A final job downloads the three
bundles, rechecks each four-file scope, re-verifies each archive against its
manifest, and uses `cargo --locked xtask release aggregate` to create a
canonical three-target record. It then generates and verifies one combined
manifest and retains only the exact three archives, aggregate record and two
sidecars. Missing, duplicate, unsupported or identity-mismatched inputs fail
closed. This aggregate path has local contract evidence but no hosted run.
The workflow is not a release, signing or publication gate; T015 provides the
readiness assertion and T016 still owns reproducibility evidence.

The archive helper refuses an existing output and a symlinked output parent. It
creates the completed archive in the destination directory and installs it
with a same-directory hard link, so a concurrent destination cannot be
silently replaced or redirected through a directory symlink.

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
check on Unix, and Windows must report an x64 process/OS architecture; neither
check replaces the exact target and runtime evidence in the release record.

### Wrong architecture or missing host runtime

A wrong-architecture binary cannot run far enough to produce an Ignatius
diagnostic, so this check belongs before first start. Do not work around a
loader failure by trying an archive from another target row.

- On Apple silicon, `uname -m` must print `arm64`. `Bad CPU type in executable`
  means the selected archive does not match the host. This release matrix has
  no Intel macOS archive.
- On Linux, `uname -m` must print `x86_64`. `Exec format error` means the binary
  does not match the host architecture. A message naming `GLIBC_` or a dynamic
  loader while the file exists means the GNU runtime is absent or too old for
  that candidate. Retain the exact message and treat that host as unsupported
  until a compatible target row is built and evidenced.
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
Use the exact basename named by the record, run the platform-native digest
command from the candidate section, and compare both digest and byte count
before extracting anything. A checksum match proves archive bytes only. It does
not prove that the archive is signed, notarised, published or safe for an
unverified platform.

The standard checksum-list checks are:

```bash
# macOS
shasum -a 256 -c SHA256SUMS

# Linux
sha256sum --strict --check SHA256SUMS
```

On Windows, compare the one exact basename line and the file length with
PowerShell rather than accepting a digest copied from a different target:

```powershell
$ErrorActionPreference = 'Stop'
$archive = '<verified-archive>'
$name = Split-Path -Leaf $archive
$lines = @(Get-Content -LiteralPath '.\SHA256SUMS' |
    Where-Object { $_ -match ('^[0-9a-fA-F]{64}  ' + [regex]::Escape($name) + '$') })
if ($lines.Count -ne 1) { throw "SHA256SUMS must contain exactly one entry for $name" }
$expected = ($lines[0] -split '\s+', 2)[0].ToLowerInvariant()
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
if ($actual -cne $expected) { throw "SHA-256 mismatch for $name" }
Write-Output "SHA-256 verified: $name"
Write-Output "Bytes: $((Get-Item -LiteralPath $archive).Length)"
```

The structured manifest must also have exactly one entry for the basename and
target, with the same lower-case digest and `size_bytes`. Reviewers with `jq`
can run the following after calculating `digest` and `size` from the exact file:

```bash
archive_path='<verified-archive>'
name="${archive_path##*/}"
target='<target-triple>'
if command -v shasum >/dev/null 2>&1; then
  digest="$(shasum -a 256 "$archive_path" | awk '{print $1}')"
else
  digest="$(sha256sum -- "$archive_path" | awk '{print $1}')"
fi
size="$(wc -c < "$archive_path" | tr -d '[:space:]')"
jq -e --arg name "$name" --arg target "$target" \
  --arg digest "$digest" --argjson size "$size" '
  [.artefacts[] | select(.name == $name and .target == $target
    and .checksum == $digest and .size_bytes == $size)] | length == 1
' release-manifest.json
```

The repository's maintainer-level verifier is the exact
`cargo --locked xtask release manifest verify` command documented above; it additionally
rejects malformed or non-canonical sidecars, missing or extra entries and
identity mismatches. If neither that verifier nor a trusted structured-JSON
review is available, record only the standard checksum result and do not call
the candidate's full evidence bundle verified.

### macOS and Linux

After verification, extract into a new temporary directory and confirm that the
archive contains the one expected binary at its root. The workflow creates that
shape and does not include configuration or data files:

```bash
stage="$(mktemp -d)"
tar -xzf "<verified-archive>" -C "$stage"
test -f "$stage/ignatius"
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
The current local workflow instead gives all three target builds one
run-attempt identity while retaining target as a separate fact. Its final job
re-verifies the downloaded inputs and produces an exact six-file aggregate
scope. Four aggregation-focused and seven workflow-focused contracts pass, but
the aggregate job has not run on GitHub Actions and no aggregate artifact has
been retained.

The earlier local aggregate readiness check exited 1 and named 13 blockers.
Each hosted per-target record also exited 1 as required and retained eight
blockers: incomplete and unmaterialised evidence, detached and untagged source,
a non-publishable state, and absent verified signature and provenance evidence.
The candidate must remain blocked until complete evidence, clean and correctly
tagged source, the full aggregated target set, dependency inventory, verified
signatures, verified provenance, platform installation and recovery evidence,
and publication authorization all exist. A checksum match or successful build
does not satisfy those gates.

The recorded authorization for run `33563497933` ends at the seven-day,
run-scoped GitHub Actions artifacts. It does not authorize a tag, signing
operation, GitHub Release, package-registry upload, package-manifest change or
promotion into the canonical evidence tree. Before any of those future
operations, the owner must record new explicit authorization naming the exact
product version, source revision, target set, artefact destinations and approved
evidence retention boundary. This rehearsal performed none of those publication
operations.

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
   and inspect the retained six-file scope.
8. Publish checksums alongside the artefacts only after every remaining gate
   and the exact publication authorization are recorded.
9. Only then update any package manifest.

## Phased distribution

1. GitHub release archives with checksums for macOS, Windows and Linux.
2. A Homebrew tap and a Scoop manifest.
3. Winget, once identity and installer behaviour are stable.
4. Anything else only when there is someone to maintain it.

## Before any public release

- Finish the naming work in `docs/product/landscape.md`: the name is confirmed
  and the availability check is done, but no trademark search has been run.
- Publish `SECURITY.md` with a working private reporting route.
- Generate an SBOM and decide on signing and provenance. Until both are real, say
  so plainly rather than omitting the subject.

## Support and deprecation

To be defined with the first release. A breaking change to the exit codes, the
machine output formats, or the configuration schema requires a major version and
a migration note, because scripts depend on all three.
