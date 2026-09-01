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

| Fact | Evidence source | State on 2026-09-02 |
| --- | --- | --- |
| Product version | `Cargo.toml`, read directly by the workflow; archive basename and record must match | Hosted run `33563497933` recorded `0.1.0` consistently for all three targets; no release version is published |
| Source revision and state | `cargo xtask release generate` captures `HEAD` and cleanliness, and checks exact tag identity | All hosted records identify `24f1c8e7801ab5f1f6805e54e4e5f4db67ca1949`, detached and untagged, and remain blocked |
| Build identity | `IGNATIUS_BUILD_IDENTITY=ci/<run>-<attempt>/<target>` is injected before the target build and copied into the record | Automated contract present; rerun attempts are identity-distinct; not provenance evidence |
| Target identity | Workflow matrix, executable header and exact Rust target triple in the archive name, record and sidecars | Run `33563497933` completed Linux `x86_64-unknown-linux-gnu`, macOS `aarch64-apple-darwin` and Windows `x86_64-pc-windows-msvc`; independent header inspection matched every row |
| Archive size and exact bytes | `release generate --artefact-path`, `manifest generate`, `manifest verify` and the 72-case release contract suite | All three hosted archives passed record, manifest, SHA-256 and exact-scope verification after download; mutation, sidecar, symlink and conflicting-alias cases fail closed |
| Readiness | `cargo xtask release check` after sidecar verification | The workflow requires a non-zero blocked result; signing, provenance, inventory and owner authorization remain absent |
| Reproducibility | Repository-owned archive helper plus repeat archive comparison from the same source and inputs | The local helper produced identical `tar.gz` and `zip` bytes twice for the same synthetic binary; hosted repeat-build and cross-runner evidence remain open |

The release-contract checks, local rehearsal and hosted rerun prove the identity
and integrity rules for the three target-specific candidate bundles. They do
not prove that the Windows or Linux archives run on their target platforms,
that any target can be upgraded and rolled back, or that a multi-target record
has been aggregated. Those are separate evidence classes and remain before any
release can be called ready or published.

## Installation, upgrade and rollback evidence

The release procedure defines how to perform these checks; this table records
the evidence still required for each target. The packaging workflow and a
checksum match are not installation or recovery evidence. Each completed row
must retain the exact candidate record, archive basename and target identity,
the command outcome and the next action for any failure, without retaining
credentials, SQL text or result data.

| Platform and target | Scenario | Required evidence | Current state on 2026-09-02 |
| --- | --- | --- | --- |
| macOS Apple silicon, `aarch64-apple-darwin` | Clean install and first start | Verify the `tar.gz` bytes before extraction, run `version --verbose`, `doctor --json`, `config paths`, `config validate` and a reviewed non-secret `connect --check` | Direct hosted-archive pass on macOS 26.6.1 arm64: installed into a version-specific directory, exact identity and configuration passed, PostgreSQL 18.4 passed over localhost plain transport and TLS 1.3 with `verify-full`, and the full-screen client entered and left a real pseudo-terminal with `Ctrl+Q`. The same candidate then failed the logging privacy audit below, so this row is runtime evidence, not release acceptance. |
| macOS Apple silicon, `aarch64-apple-darwin` | Upgrade and preservation | Install beside the known-good binary, confirm configuration, saved queries, history and logs remain at the paths reported by `config paths`, and record migration output if applicable | Direct source-revision replacement pass: known-good `247d1c7` and hosted `24f1c8e` binaries were installed side by side and selected through one launcher. Configuration, saved query, history and log files were byte-identical after first candidate start. Both binaries report product version `0.1.0`, and the prior build was not a published release, so this is not semantic-version or published-upgrade evidence. |
| macOS Apple silicon, `aarch64-apple-darwin` | Failed upgrade and rollback | Force a documented startup or validation failure, return to the prior binary/configuration pair, validate it and retain the failed state for diagnosis | Direct pass with the same boundary: schema version 999 caused a controlled configuration failure with exit 3 and a repair action. The invalid file was retained, the valid backup and prior launcher were restored, the prior binary reconnected, and the complete configuration/data tree matched its pre-upgrade backup. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Clean install and first start | Verify the `zip` bytes with PowerShell, extract `ignatius.exe` to a new version directory, run the identity and diagnostic commands, and record the PATH outcome | Hosted archive, record, sidecars and PE32+ x86-64 header verified after download; no Windows runtime, PowerShell or PATH evidence exists |
| Windows x86_64, `x86_64-pc-windows-msvc` | Upgrade and preservation | Keep the prior version directory, confirm `%APPDATA%\ignatius` and `%LOCALAPPDATA%\ignatius` are preserved, and record any schema migration backup | Not run; no platform upgrade evidence retained |
| Windows x86_64, `x86_64-pc-windows-msvc` | Failed upgrade and rollback | Restore the previous PATH or launcher and matching configuration/data backup without deleting the failed version or state | Not run; rollback remains a documented contract |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Clean install and first start | Verify the `tar.gz` bytes before extraction, confirm the GNU libc target boundary, run the identity and diagnostic commands, and record the PATH outcome | Hosted archive, record, sidecars and ELF x86-64 GNU/Linux header verified after download; no Linux runtime or installed-path evidence exists |
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
| Scoped secret scan | Pass. The CI-pinned Gitleaks v8.30.0 container, addressed by digest and run with no container network, scanned the verified four-file upload root and reported no leaks. | Local Docker execution of the new CI command; the later hosted follow-up is recorded below. |
| Extracted first start | Partial pass. The root-only archive extracted cleanly; `version --verbose`, `config paths` and `config validate` passed. `doctor --json` reported seven checks OK, three skipped for terminal, terminal size and connection, and no failures. | Isolated local first start only; no reviewed server connection, upgrade or rollback was performed. |
| Readiness gate | Pass as a blocking test. `cargo --locked xtask release check` exited 1 and named 13 issues, including modified source, unmaterialised and incomplete evidence, absent Windows/Linux artefacts, inventory, signatures, provenance, platform recovery evidence and owner authorization. | Expected blocked result; no readiness or publication claim. |
| Hosted target matrix | Not part of this local run. | The later hosted result is recorded below. |
| Platform installation, upgrade and rollback | Incomplete. | The local macOS first-start subset above does not satisfy macOS upgrade/rollback or any Windows/Linux hand-evidence row. |
| Inventory, signing and provenance | Not materialised or verified. | The provider-neutral contracts exist; provider and retention decisions remain owner gates. |
| Owner authorization | Absent. | No version, revision, target set, destination or retention boundary has been authorized for publication. |
| Publication | Not requested and not performed. | No tag, upload, release or distribution operation is authorized by this package. |

### Hosted follow-up and repair on 2026-09-01 to 2026-09-02

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Shared CI | Pass. Run `33445418179` completed all 10 jobs at committed revision `247d1c7e829aa5b642a5047e56e53d402e386dbb`, including macOS, Windows, Ubuntu, PostgreSQL 14/16/18, release-evidence scope, network-disabled scoped Gitleaks, cargo-deny and generated-artefact drift. | Hosted CI and synthetic evidence-scope result only; not release archive, installation or publication evidence. |
| First hosted archive run | Fail. Run `33561901626` checked out the same exact revision. All three target builds passed; all three jobs then exited in the common packaging step before the archive helper ran or any candidate evidence was uploaded. | No hosted archive, checksum sidecar or retained candidate bundle exists from this run. |
| Failure diagnosis and repair | Pass. Commit `24f1c8e7801ab5f1f6805e54e4e5f4db67ca1949` makes the workflow read `Cargo.toml` directly and name missing-version, missing-binary and symlinked-binary failures. | The repair is committed on `main`, pushed and exercised by the successful hosted rerun below. |
| Focused release contracts after repair | Pass. 3 archive-helper, 1 native-boundary, 72 release-contract, 7 release-note and 1 release-schema tests passed; the six workflow-focused cases include the new version-authority contract. | Local automated contract evidence only. |
| Full repository gate after repair | Pass. `cargo --locked xtask verify` passed formatting, lints, 513 unit/layout tests, 38 CLI contracts and 38 PostgreSQL integration tests against disposable PostgreSQL 18.4 plain/TLS services. Teardown removed the containers, network and data; final status was `Not running`. | Local macOS and synthetic PostgreSQL evidence. The earlier sandboxed verifier run skipped integration and is not counted as the full result. |
| Repair CI | Pass. Run `33563478021` completed all 10 jobs at the repaired revision, including macOS, Windows, Ubuntu, PostgreSQL 14/16/18, release-evidence scope and secret, dependency, licence and generated-artefact gates. | Hosted source and synthetic integration evidence; separate from candidate packaging and hand verification. |
| Second hosted archive run | Pass. Owner-authorized non-publishing run `33563497933` checked out the repaired revision. All three jobs built, packaged, verified the deliberately blocked readiness result and uploaded one exact four-file target bundle with seven-day retention. | Target-specific run artifacts only; no aggregation, signing, provenance, canonical evidence promotion or publication. |
| Downloaded candidate bundles | Pass. Each bundle contained only its archive, record, manifest and `SHA256SUMS`, with no symlinks. All three records validated as `blocked`; all manifests, independent SHA-256 checks and four-file scope checks passed. The archives contained exactly one executable with the declared Mach-O arm64, PE32+ x86-64 or ELF x86-64 GNU/Linux format. | Independent packaging and byte-integrity inspection on arm64 macOS; executable headers are not Windows/Linux runtime evidence. |
| Hosted archive identities | Pass. macOS: 3,016,587 bytes, SHA-256 `ce5c9364615009e5516edc394b41aef2c33e66113a26491996d0f5e7927536f2`; Windows: 3,068,482 bytes, `cc0e30a71527a2ba5298f1e3066d080b60712d5e251aced11a722304ec82d056`; Linux: 3,318,764 bytes, `90512e79e4d14ac310cfddffe79fef8383f748601505f159d27f296dbf6353ef`. | Exact run `33563497933` attempt 1 bytes, retained by GitHub Actions for seven days. |
| Downloaded macOS first start | Partial pass. On arm64 macOS the extracted hosted binary reported version `0.1.0`, exact source revision, clean source, build identity `ci/33563497933-1/aarch64-apple-darwin` and the expected target. Isolated `config paths` and `config validate` passed; `doctor --json` reported seven OK, three skipped and no warnings or failures. | Direct execution of the hosted macOS archive only; no reviewed connection, installed-path, upgrade or rollback evidence. |
| Native macOS installation and recovery | Pass with a version-boundary limitation. The exact hosted binary passed installed-path identity, configuration, plain and verified-TLS PostgreSQL 18.4 checks, full-screen pseudo-terminal start and clean quit. Side-by-side launcher replacement preserved configuration, saved query, history and log bytes. A synthetic schema-999 start failed with exit 3; restoring the backup and prior launcher returned all state to the pre-upgrade bytes and reconnected successfully. | Direct arm64 macOS evidence. Both binaries are version `0.1.0`, the prior build was not published, and this does not cover Windows or Linux. |
| Logging privacy audit | **Fail, release-blocking.** With a fresh isolated profile and synthetic statement, `IGNATIUS_LOG=debug` in the exact hosted macOS binary wrote the full SQL through `tokio_postgres::simple_query`. The log did not contain the synthetic password, but SQL persistence violates the documented no-SQL logging contract. | Exact run `33563497933` macOS artifact. The candidate is rejected even though its packaging, runtime and recovery subsets passed. |
| Local logging remediation | Focused pass. The modified working tree accepts only simple log levels, excludes dependency targets and prefix collisions, redacts each complete internal event and records only `job=1 statement_chars=<n>`. Five logging unit tests and one live CLI contract against PostgreSQL 18.4 passed; newly appended bytes contained no SQL marker, result label, synthetic credential marker or dependency trace. | Local modified build identity `local-t033-log-fix` only. It is not a clean, hosted or retained candidate and does not supersede the failed artifact. |
| Full repository gate after logging remediation | Pass. The exact current tree passed formatting, clippy with warnings denied, 515 unit/layout tests, 39 CLI contracts and 38 PostgreSQL integration tests against the disposable PostgreSQL 18.4 plain/TLS services, with no skips. | Local macOS and synthetic PostgreSQL evidence. Teardown removed the containers, network and data; final status was `Not running`. |
| Focused release and documentation contracts after logging remediation | Pass. 3 archive-helper, 6 documentation/build, 1 native-boundary, 72 release-contract, 7 release-notes and 1 release-schema tests passed. The catalogue wrapper validated one blocked record against its exact changelog heading. | 90 local focused tests plus the read-only catalogue wrapper; no hosted or platform-runtime claim. |
| Readiness and authorization | Correctly blocked. Every per-target check exited 1 with eight issues, including incomplete evidence, detached and untagged source, absent verified signature and provenance, and non-publishable state. The later logging audit independently rejects the candidate. Owner authorization covered only commit, push and this seven-day run-scoped rehearsal. | T034 is complete. T033 remains open; no tag, signing, GitHub Release, package publication or canonical evidence promotion was authorized or performed. |

Earlier 2026-08-16 runs remain historical evidence for their recorded
revisions. They do not supersede the later local and hosted evidence or close
any cross-platform installation, supply-chain or authorization gate.

## What has not been verified

- **A corrected hosted candidate.** The logging remediation has exact-tree local
  evidence only. No hosted CI or replacement archive run exists for it, and no
  further workflow run is authorized by the earlier run-scoped approval.
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
