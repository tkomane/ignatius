# Verification

## The full command

```bash
cargo xtask verify
```

It runs every gate below in the order that fails fastest, continues past a
failure so one run surfaces every problem, prints a summary, and exits non-zero
if any gate failed. Underneath it is exactly this:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo test --test cli_contract
cargo test --test postgres_integration     # needs IGNATIUS_TEST_PG_URI
```

Run them in that order: the fast gates fail fastest. Without
`IGNATIUS_TEST_PG_URI` the integration tests skip and say so on stderr. **A skip
is not a pass.** Report it as a skip.

## Required gates before merging

| Gate | Command | Blocking |
| --- | --- | --- |
| Formatting | `cargo fmt --check` | Yes |
| Lints | `cargo clippy --all-targets -- -D warnings` | Yes |
| Unit and layout tests | `cargo test --lib` | Yes |
| CLI contract | `cargo test --test cli_contract` | Yes |
| PostgreSQL integration | `cargo test --test postgres_integration` | Yes |
| Compilation on every target | `cargo check --target ...` in CI | Yes |
| Dependency advisories and licences | `cargo deny check` in CI | Yes |
| Secret scan | gitleaks in CI | Yes |
| Documentation and release notes updated | Review | Yes, when behaviour changes |

## Evidence recorded on 2026-08-16

macOS 26.6.1 on Apple silicon, rustc 1.97.1 (Homebrew), against both the plain
and TLS disposable services from `postgres:18.4-alpine`, at committed HEAD
`6541610`. CI ran the same suites on Linux against PostgreSQL 14, 16 and 18, and
built and tested on Windows.

**Automated**: formatting clean, `cargo clippy --all-targets -- -D warnings`
clean, 494 library tests, 38 CLI contract tests, 38 PostgreSQL integration tests
and 5 pseudo-terminal tests, all passing. Two of the pty tests are new and prove
the password prompt by using it: the password is typed into a real terminal, the
session opens, and the transcript is read back to confirm the password is not in
it. That run covers the editor and its
undo, syntax colouring, the expanded row view and cell inspector, result
filtering, the statement history and its refusals, object definitions and
dependencies, the object tree's own read-only connection, connection profiles,
the password prompt in both surfaces, and configurable key bindings.

**Run directly rather than through `cargo xtask verify`.** The workspace's xtask
crate was being edited concurrently by another tool at the time, so the five
gates were run against the `ignatius` package individually. The gates are the
same commands the xtask runs; what is missing is only the convenience of one
invocation.

**Terminal lifecycle, captured from a real pseudo-terminal.** The client was run
under `script`, sent Ctrl+Q, and the byte stream captured. In order:

```text
entry:  ESC[?1049h   enter alternate screen
        ESC[?2004h   enable bracketed paste
        ESC[?25l     hide cursor
exit:   ESC[?25h     show cursor
        ESC[?2004l   disable bracketed paste
        ESC[?1049l   leave alternate screen
```

Every mode enabled was disabled, in reverse order, with the alternate screen left
last. No mouse capture sequence appears, because mouse capture is off by default.
The process exited 0 and the shell was usable afterwards.

**Interactive execution.** With the pty sized to 110x30, the client rendered the
header (`[PROD] [read-write]`, the target, `TLS not negotiated`), the editor with
its starter query, an empty-state message in the results pane, and the footer
hints. Sending F5 produced `Completed`, `1 row`, and the value `ignatius_demo` in
the grid. Ctrl+Q exited cleanly.

**Live CLI behaviour.** `connect --check` reported five passing stages and warned
truthfully that the connection was not encrypted. A query against a missing
relation reported SQLSTATE 42P01 with a cause and a next action, exited 7, and
wrote nothing to stdout. A value containing `ESC[2J` was rendered as `\x1B[2J`.
NULL, an empty string and the text `NULL` were each displayed distinctly. The
plain-mode subprocess checks also proved line-oriented input,
production-write confirmation, server-confirmed cancellation, failed-
transaction recovery wording and screen-reader-safe output.

## Release candidate identity and reproducibility evidence

The non-publishing release workflow in `.github/workflows/release.yml` is an
automated packaging contract, not live publication evidence. Each matrix job
builds one explicit target, writes one per-target release record, generates and
verifies `release-manifest.json` and `SHA256SUMS`, then proves that the
readiness gate refuses the incomplete candidate before retaining the bundle as
a run-scoped workflow artifact.

| Fact | Evidence source | State on 2026-08-16 |
| --- | --- | --- |
| Product version | `Cargo.toml`, read by `cargo pkgid`; archive basename and record must match | Automated contract present; no release version is published |
| Source revision and state | `cargo xtask release generate` captures `HEAD` and cleanliness, and checks exact tag identity | Automated blocking is present; the local rehearsal was correctly blocked by the shared modified and untagged checkout |
| Build identity | `IGNATIUS_BUILD_IDENTITY=ci/<run>-<attempt>/<target>` is injected before the target build and copied into the record | Automated contract present; rerun attempts are identity-distinct; not provenance evidence |
| Target identity | Workflow matrix and exact Rust target triple in the archive name, record and sidecars | Matrix defined for Linux `x86_64-unknown-linux-gnu`, macOS `aarch64-apple-darwin` and Windows `x86_64-pc-windows-msvc`; a hosted run is not recorded here |
| Archive size and exact bytes | `release generate --artefact-path`, `manifest generate`, `manifest verify` and the 71-case release contract suite | Local aarch64 macOS rehearsal passed generation and verification; byte mutation, sidecar mismatch, symlinked archive/sidecar inputs, symlinked archive-output parent, dangling-output-symlink, symlinked-record-parent, symlinked-catalogue-input, schema-basename and conflicting evidence-alias cases fail closed |
| Readiness | `cargo xtask release check` after sidecar verification | The workflow requires a non-zero blocked result; signing, provenance, inventory and owner authorization remain absent |
| Reproducibility | Repository-owned archive helper plus repeat archive comparison from the same source and inputs | The local helper produced identical `tar.gz` and `zip` bytes twice for the same synthetic binary; hosted repeat-build and cross-runner evidence remain open |

The release-contract checks and the local rehearsal prove the identity and
integrity rules on this checkout. They do not prove that the hosted macOS,
Windows or Linux jobs have completed, that the archives install successfully,
or that a multi-target record has been aggregated. Those are separate evidence
classes and remain before any release can be called ready or published.

## Installation, upgrade and rollback evidence

The release procedure defines how to perform these checks; this table records
the evidence still required for each target. The packaging workflow and a
checksum match are not installation or recovery evidence. Each completed row
must retain the exact candidate record, archive basename and target identity,
the command outcome and the next action for any failure, without retaining
credentials, SQL text or result data.

| Platform and target | Scenario | Required evidence | Current state on 2026-08-16 |
| --- | --- | --- | --- |
| macOS Apple silicon, `aarch64-apple-darwin` | Clean install and first start | Verify the `tar.gz` bytes before extraction, run `version --verbose`, `doctor --json`, `config paths`, `config validate` and a reviewed non-secret `connect --check` | Partial local archive evidence: extraction and first-start identity/configuration checks passed; hosted archive and reviewed connection evidence remain open |
| macOS Apple silicon, `aarch64-apple-darwin` | Upgrade and preservation | Install beside the known-good binary, confirm configuration, saved queries, history and logs remain at the paths reported by `config paths`, and record migration output if applicable | Not run; no platform upgrade evidence retained |
| macOS Apple silicon, `aarch64-apple-darwin` | Failed upgrade and rollback | Force a documented startup or validation failure, return to the prior binary/configuration pair, validate it and retain the failed state for diagnosis | Not run; rollback remains a documented contract |
| Windows x86_64, `x86_64-pc-windows-msvc` | Clean install and first start | Verify the `zip` bytes with PowerShell, extract `ignatius.exe` to a new version directory, run the identity and diagnostic commands, and record the PATH outcome | Not run; workflow row is defined but no hosted archive result is retained |
| Windows x86_64, `x86_64-pc-windows-msvc` | Upgrade and preservation | Keep the prior version directory, confirm `%APPDATA%\ignatius` and `%LOCALAPPDATA%\ignatius` are preserved, and record any schema migration backup | Not run; no platform upgrade evidence retained |
| Windows x86_64, `x86_64-pc-windows-msvc` | Failed upgrade and rollback | Restore the previous PATH or launcher and matching configuration/data backup without deleting the failed version or state | Not run; rollback remains a documented contract |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Clean install and first start | Verify the `tar.gz` bytes before extraction, confirm the GNU libc target boundary, run the identity and diagnostic commands, and record the PATH outcome | Not run from a hosted release archive; no Linux archive result retained |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Upgrade and preservation | Keep the prior binary available, confirm XDG or explicit configuration/data paths are preserved, and record any schema migration backup | Not run; no platform upgrade evidence retained |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Failed upgrade and rollback | Restore the prior binary and matching configuration/data backup, validate it, and preserve the failed state for diagnosis | Not run; rollback remains a documented contract |

Until these rows have platform-specific evidence, the release target matrix is
an automated packaging contract rather than a supported installation claim.

## Feature 008 release rehearsal evidence

This table records the local non-publishing rehearsal run on 2026-08-31 on
macOS 26.6.1 (`arm64`). It binds all local results to revision
`f84afda2eb6b332c2b9659ef5b83671fbbdc4f74` and labels the worktree modified.
Hosted CI, platform hand verification and publication remain separate evidence
classes.

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Source and build identity | Pass. The packaged binary reported product version `0.1.0`, the full source revision above, modified source state, build identity `local-rehearsal-2026-08-31`, target `aarch64-apple-darwin` and Homebrew rustc 1.98.0. | Local modified checkout only; not a release tag or CI provenance. |
| Full repository gate | Pass. With disposable PostgreSQL services running, `cargo --locked xtask verify` passed formatting, lints, 513 unit/layout tests, 38 CLI contract tests and 38 PostgreSQL integration tests. Teardown removed both containers, their network and generated data; `cargo --locked xtask db status` then reported `Not running`. | PostgreSQL 18.4 plain/TLS synthetic fixtures; no production server evidence. |
| Focused release contracts | Pass. The final focused run passed 3 archive-helper, 1 native-boundary, 71 release-contract, 7 release-notes and 1 release-schema tests. The repository catalogue also validated as one blocked record with one exact changelog entry. | Local automated contracts only; no hosted runner evidence. |
| macOS archive and sidecars | Pass. The repository helper produced a root-only `ignatius-0.1.0-aarch64-apple-darwin.tar.gz`, 3,010,634 bytes, with SHA-256 `c7ba67b568ea803d9f6f7d8e1de802e25cdf539d1377618c76825d1683aa380d`. Record validation, manifest generation, exact-byte verification and the four-file evidence-scope check passed. | Temporary local Apple silicon rehearsal; no hosted archive or retained release evidence. |
| Deterministic archive helper | Pass. Rebuilding the same `tar.gz` from the same binary and declared inputs produced byte-identical output. | Same local Python and compression runtime; hosted repeat-build and cross-runner evidence remain open. |
| Scoped secret scan | Pass. The CI-pinned Gitleaks v8.30.0 container, addressed by digest and run with no container network, scanned the verified four-file upload root and reported no leaks. | Local Docker execution of the new CI command; the shared CI job has not run on a hosted runner. |
| Extracted first start | Partial pass. The root-only archive extracted cleanly; `version --verbose`, `config paths` and `config validate` passed. `doctor --json` reported seven checks OK, three skipped for terminal, terminal size and connection, and no failures. | Isolated local first start only; no reviewed server connection, upgrade or rollback was performed. |
| Readiness gate | Pass as a blocking test. `cargo --locked xtask release check` exited 1 and named 13 issues, including modified source, unmaterialised and incomplete evidence, absent Windows/Linux artefacts, inventory, signatures, provenance, platform recovery evidence and owner authorization. | Expected blocked result; no readiness or publication claim. |
| Hosted target matrix | Not verified. | Ubuntu, hosted macOS and Windows jobs require a hosted workflow run. |
| Platform installation, upgrade and rollback | Incomplete. | The local macOS first-start subset above does not satisfy macOS upgrade/rollback or any Windows/Linux hand-evidence row. |
| Inventory, signing and provenance | Not materialised or verified. | The provider-neutral contracts exist; provider and retention decisions remain owner gates. |
| Owner authorization | Absent. | No version, revision, target set, destination or retention boundary has been authorized for publication. |
| Publication | Not requested and not performed. | No tag, upload, release or distribution operation is authorized by this package. |

Earlier 2026-08-16 runs remain historical evidence for their recorded
revisions. They do not supersede this recorded modified-worktree rehearsal or close any
hosted, cross-platform, installation, supply-chain or authorization gate.

## What has not been verified

- **Windows and Linux by hand.** CI builds and tests both platforms, but nobody
  has opened the full-screen client there. The same is true of Warp's renderer
  and a live terminal resize.
- **PostgreSQL 15 and 17.** CI exercises 14, 16 and 18; the local server is 18.4.
- **Windows terminal restoration.** The Unix pty test passes; the equivalent
  ConPTY test remains open work.
- **Panic and signal restoration as a process-level claim.** The RAII and panic
  paths are covered in code and unit tests; only the normal Unix pty path is
  exercised end to end here.

These are tracked in `docs/status.md`.

## Adding a test

Name it as the claim it makes: `a_stale_result_cannot_overwrite_a_newer_query`
rather than `test_update_3`. A failure message should tell someone what broke
without them reading the test body.

Assert meaning, not pixels. A layout test checks that the production marker and
the row count are present, not that a 3 KB buffer is byte-identical, because the
second kind passes while the interface silently loses its meaning.
