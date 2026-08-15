# Release

Nothing has been released. This describes the intended process; every claim in it
is a plan, not a fact, and no artefact may be described as signed, notarised or
published until a real release proves it.

## Publishing options

Nothing has been published and no remote is configured. These are the routes, in
increasing order of commitment. Each row assumes the ones above it.

| Option | Effort | Ongoing cost | Reversible | Reaches | Needs first |
| --- | --- | --- | --- | --- | --- |
| **A. Stay local** | None | None | Fully | You | Nothing |
| **B. Private GitHub remote** | Minutes | None | Fully | You, and anyone you invite | A repository |
| **C. Public source, no artefacts** | Minutes | Issues arrive | Awkward: forks and clones persist | Anyone who builds it | Green CI on all three platforms |
| **D. Tagged releases with binaries and checksums** | Hours | Every release, plus platform bug reports | Artefacts can be deleted, but people have them | Anyone who downloads | C, plus a signing decision |
| **E. crates.io** | Under an hour | Versions are permanent and cannot be unpublished, only yanked | **No.** A published version is forever | Rust users, `cargo install` | D, plus a stable public API or a binary-only crate |
| **F. Homebrew tap and Scoop** | Half a day each | A manifest update per release, forever | Yes: delete the tap | macOS and Windows users who expect a package manager | D, and a release cadence you can sustain |
| **G. Homebrew core and Winget** | Days, plus review | Their standards, their timelines | Hard | Everyone | F, plus a user base that justifies it |

**Recommendation: A now, B when convenient, C once CI is green on all three
platforms.** Publishing source before Windows and Linux have ever been run would
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

This section defines the identity and integrity fields a future candidate job
must produce. It does not build, sign, upload or publish an archive.

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

Names are basenames only: no path separators, spaces or duplicate names are
allowed in one candidate. The target row, artefact name and build target must
agree. A supported platform without a built artefact is recorded as `blocked`
with a reason rather than represented by another platform's archive.

The product-version token is copied byte-for-byte from Cargo.toml and uses the
release schema's semantic-version grammar: numeric major, minor and patch with
optional ASCII alphanumeric, hyphen and dot prerelease/build identifiers. No
escaping, normalisation or date suffix is allowed. The target token uses only
ASCII letters, digits, hyphens, underscores and periods, begins with an
alphanumeric character, and contains no path separator, whitespace or `..`.
The format mapping is exact: `tar.gz` ends in `.tar.gz`, `zip` ends in `.zip`,
and `binary` has no suffix. The suffix is never selected from the host
operating system.

The set of artefacts must equal the set of target rows whose coverage is
`covered`, with exactly one artefact for each target. Rows marked `blocked` or
`out-of-scope` have no artefact entry and retain a non-empty reason.

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
signing, provenance, publication or platform hand verification. The current
`cargo xtask release check PATH` command validates the declared record and
evidence relationships. It does not generate archives, read these sidecars or
recompute their bytes yet. T013/T014 own the future archive workflow, byte
hashing, sidecar parsing, size re-read and fail-closed exit behavior.

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
7. Publish checksums alongside the artefacts.
8. Only then update any package manifest.

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
