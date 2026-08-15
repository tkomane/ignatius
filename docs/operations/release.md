# Release

Nothing has been released. This describes the intended process; every claim in it
is a plan, not a fact, and no artefact may be described as signed, notarised or
published until a real release proves it.

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

- Run the naming gate in `docs/product/landscape.md`. `ignatius` is a codename.
- Publish `SECURITY.md` with a working private reporting route.
- Generate an SBOM and decide on signing and provenance. Until both are real, say
  so plainly rather than omitting the subject.

## Support and deprecation

To be defined with the first release. A breaking change to the exit codes, the
machine output formats, or the configuration schema requires a major version and
a migration note, because scripts depend on all three.
