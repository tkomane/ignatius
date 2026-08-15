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
