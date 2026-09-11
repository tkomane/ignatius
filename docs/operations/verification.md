# Verification

## The full command

```bash
cargo xtask verify
```

It runs every gate below in the order that fails fastest, continues past a
failure so one run surfaces every problem, prints a summary, and exits non-zero
if any gate failed. Underneath it is exactly this:

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --doc --all-features
```

Run them in that order: the fast gates fail fastest. When the disposable plain
and TLS services are ready, xtask injects both test URIs into the complete
suite. Without them the database-backed cases skip and say so. The Unix-socket
case separately requires `IGNATIUS_TEST_PG_SOCKET_URI`. **A skip is not a
pass.** The verifier lists each missing live boundary after the ordinary gate
results.

## Required gates before merging

| Gate | Command | Blocking |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | Yes |
| Lints | locked workspace clippy across all targets and features | Yes |
| API documentation | locked workspace `cargo doc`, warnings denied | Yes |
| Complete auto-discovered test suite | locked workspace `cargo test --all-targets --all-features` | Yes |
| Documentation examples | locked workspace `cargo test --doc --all-features` | Yes |
| CLI, PostgreSQL, prompt and cloud-token integration | live plain and TLS services in CI | Yes |
| Unix socket integration | focused live socket fixture on Linux in CI | Yes |
| Compilation on every primary OS | locked workspace build on Linux, macOS and Windows | Yes |
| Minimum supported Rust | locked workspace check with Rust 1.90 | Yes |
| Dependency advisories and licences | `cargo deny check` in CI | Yes |
| Secret scan | gitleaks in CI | Yes |
| Release evidence scope | allowlist validation and scoped gitleaks scan in CI | Yes |
| Code scanning | CodeQL advanced setup in CI, test sources excluded | Yes |
| Documentation and release notes updated | Review | Yes, when behaviour changes |

## CI execution contract prepared on 2026-09-02

The hardened workflow makes failures complete and actionable: stale branch runs
are canceled, every job has an explicit timeout, checkout credentials are
removed before repository code runs, and Cargo commands that consume
dependencies use the committed lockfile. Dependency auditing, repository secret
scanning and release-evidence scanning are independent blocking jobs.

Pull requests run PostgreSQL coverage on the oldest and newest supported majors.
Pushes to `main`, manual runs and the weekly scheduled run cover every supported
major from 14 through 18. A separate PostgreSQL 18 fixture exposes a real Unix
socket to the Linux runner. The cross-platform suite uses Cargo auto-discovery,
so a newly added contract runs by default.

**Hosted readback on 2026-09-04:**
[main CI run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318)
completed successfully at `52872acd7c0e0e38c6b810723ccda5eee99a28bf`.
The executed jobs include PostgreSQL 14-18 plain/TLS contracts, PostgreSQL 18
Unix socket, native OS suites, Rust 1.90, workflow semantics, dependency/licence
checks, source/evidence scans and generated-output checks. The scheduled-only
fixture-drift job was skipped and is not included in those passes. This is
historical main-source CI evidence, not verification of the newer local
feature chain or of the separate release preflight and candidate artifacts.

## Code scanning contract added on 2026-09-11

CodeQL uses advanced setup rather than GitHub's default setup, because default
setup cannot be configured for a user-owned repository and the analysis must
keep test sources out. `.github/codeql/codeql-config.yml` ignores `tests/**` and
`**/tests.rs`: Rust has no inline alert suppression yet (github/codeql#21637),
and the tests deliberately format password-like values to prove they never
reach output, which the cleartext-logging query reports as high severity.
Production sources keep the full default query set. Unit test modules live in a
`tests.rs` file beside their module, never inline in a production file.

## Local CI-hardening evidence recorded on 2026-09-02

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Workflow syntax and policy | Actionlint and independent YAML parsing passed for all workflow files and the local fixture action. Seven workflow contracts and 80 release contracts passed. | Isolated local integration branch only. |
| Security controls | Cargo-deny 0.20.2, digest-pinned Gitleaks source/history scans and the Rust 1.90 locked check passed. | Local containers and tools; no hosted claim. |
| Full verifier | Formatting, lints, documentation, workspace tests and PostgreSQL 18.4 plain/TLS integration passed. | Disposable local services; the verifier's Unix-socket case explicitly skipped because its URI was not configured. |
| Unix-socket focused attempt | Not counted as a pass. With the URI supplied, the macOS Docker bind mount rejected PostgreSQL's socket-permission change before the server could start. | The CI job is Linux-only and must be verified on an Ubuntu runner; no local socket success is claimed. |
| Release boundary | The candidate remains non-publishing and fail-closed. No tag, signature, provenance, release or distribution was created. | Local implementation evidence only. |

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

| Fact | Evidence source | State on 2026-09-03 |
| --- | --- | --- |
| Product version | `Cargo.toml`, read directly by the workflow; archive basename and record must match | Hosted run `33689674455` recorded `0.1.0` consistently for all three targets; no release version is published |
| Source revision and state | `cargo xtask release generate` captures `HEAD` and cleanliness, and checks exact tag identity | The latest aggregate identifies `259ac76de6477719b8a3515777f3623b24237dd6`, detached and untagged, and remains blocked |
| Build identity | The workflow injects shared `IGNATIUS_BUILD_IDENTITY=ci/<run>-<attempt>` before all three target builds and keeps target separate in each record | Hosted run `33689674455` used `ci/33689674455-1` for all three targets; this is an identity fact, not provenance evidence |
| Target identity | Workflow matrix, executable header and exact Rust target triple in the archive name, record and sidecars | Run `33689674455` completed Linux `x86_64-unknown-linux-gnu`, macOS `aarch64-apple-darwin` and Windows `x86_64-pc-windows-msvc`; each package, runtime smoke and aggregate verification step passed |
| Archive size and exact bytes | `release generate --artefact-path`, `manifest generate`, `manifest verify` and the 77-case release contract suite | The aggregate artifact from run `33689674455` contains three archives of 2,773,854, 2,806,839 and 3,053,954 bytes; downloaded SHA-256 and combined-manifest checks passed |
| Readiness | `cargo xtask release check` after sidecar verification | The workflow requires a non-zero blocked result; signing, provenance, inventory and owner authorization remain absent |
| Reproducibility | Repository-owned archive helper plus repeat archive comparison from the same source and inputs | Hosted run `33689674455` compared two fixed-identity builds per target; macOS, Windows and Linux byte comparisons passed. Windows builds use `/Brepro` to remove PE linker timestamps. |

The release-contract checks, local rehearsal and hosted runs prove the identity
and integrity rules for the three target-specific candidate bundles. Run
`33689674455` also exercised the shared build identity, compared two exact
archives for each target, ran the target runtime smoke and rollback checks, and
retained and inspected the nine-file aggregate scope. The runtime smoke is
automated hosted evidence, not full-screen terminal hand evidence or a
published-version upgrade. Signing, provenance, dependency inventory and the
remaining hand and server-version evidence remain before any release can be
called ready or published.

## Installation, upgrade and rollback evidence

The release procedure defines how to perform these checks; this table records
the evidence still required for each target. The packaging workflow and a
checksum match are not installation or recovery evidence. Each completed row
must retain the exact candidate record, archive basename and target identity,
the command outcome and the next action for any failure, without retaining
credentials, SQL text or result data.

| Platform and target | Scenario | Required evidence | Current state on 2026-09-03 |
| --- | --- | --- | --- |
| macOS Apple silicon, `aarch64-apple-darwin` | Clean install and first start | Verify the `tar.gz` bytes before extraction, run `version --verbose`, `doctor --json`, `config paths`, `config validate` and a reviewed non-secret `connect --check` | Direct hosted-archive pass on macOS 26.6.1 arm64: installed into a version-specific directory, exact identity and configuration passed, PostgreSQL 18.4 passed over localhost plain transport and TLS 1.3 with `verify-full`, and the full-screen client entered and left a real pseudo-terminal with `Ctrl+Q`. The same candidate then failed the logging privacy audit below, so this row is runtime evidence, not release acceptance. |
| macOS Apple silicon, `aarch64-apple-darwin` | Upgrade and preservation | Install beside the known-good binary, confirm configuration, saved queries, history and logs remain at the paths reported by `config paths`, and record migration output if applicable | Direct source-revision replacement pass: known-good `247d1c7` and hosted `24f1c8e` binaries were installed side by side and selected through one launcher. Configuration, saved query, history and log files were byte-identical after first candidate start. Both binaries report product version `0.1.0`, and the prior build was not a published release, so this is not semantic-version or published-upgrade evidence. |
| macOS Apple silicon, `aarch64-apple-darwin` | Failed upgrade and rollback | Force a documented startup or validation failure, return to the prior binary/configuration pair, validate it and retain the failed state for diagnosis | Direct pass with the same boundary: schema version 999 caused a controlled configuration failure with exit 3 and a repair action. The invalid file was retained, the valid backup and prior launcher were restored, the prior binary reconnected, and the complete configuration/data tree matched its pre-upgrade backup. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Clean install and first start | Verify the `zip` bytes with PowerShell, extract `ignatius.exe` to a new version directory, run the identity and diagnostic commands, and record the PATH outcome | Hosted run `33689674455` extracted and executed the exact candidate. `version --verbose`, `doctor --json`, `config paths`, `config init`, `config validate` and non-interactive refusal passed; PowerShell PATH and full-screen hand evidence remain open. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Upgrade and preservation | Keep the prior version directory, confirm `%APPDATA%\ignatius` and `%LOCALAPPDATA%\ignatius` are preserved, and record any schema migration backup | Hosted side-by-side candidate validation passed with isolated configuration/data state and unchanged preservation digest; this is not a published semantic-version upgrade. |
| Windows x86_64, `x86_64-pc-windows-msvc` | Failed upgrade and rollback | Restore the previous PATH or launcher and matching configuration/data backup without deleting the failed version or state | Hosted controlled invalid-configuration and known-good rollback checks passed; ConPTY and operator hand evidence remain open. |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Clean install and first start | Verify the `tar.gz` bytes before extraction, confirm the GNU libc target boundary, run the identity and diagnostic commands, and record the PATH outcome | Hosted run `33689674455` extracted and executed the exact candidate. `version --verbose`, `doctor --json`, `config paths`, `config init`, `config validate` and non-interactive refusal passed; installed-path and full-screen hand evidence remain open. |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Upgrade and preservation | Keep the prior binary available, confirm XDG or explicit configuration/data paths are preserved, and record any schema migration backup | Hosted side-by-side candidate validation passed with isolated configuration/data state and unchanged preservation digest; this is not a published semantic-version upgrade. |
| Linux x86_64 GNU, `x86_64-unknown-linux-gnu` | Failed upgrade and rollback | Restore the prior binary and matching configuration/data backup, validate it, and preserve the failed state for diagnosis | Hosted controlled invalid-configuration and known-good rollback checks passed; operator hand evidence remains open. |

The hosted smoke results close the automated runtime subset for all three
targets, but do not replace platform hand checks, terminal restoration, PATH
review or a published semantic-version upgrade. The release target matrix is
therefore still an evidence-backed rehearsal, not a ready or published release.

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
| Deterministic archive helper | Pass. Rebuilding the same `tar.gz` from the same binary and declared inputs produced byte-identical output. | Same local Python and compression runtime; the corrected hosted repeat-build evidence is recorded in the later 2026-09-03 section. |
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
| Clean local correction archive | Pass. Commit `b2ac0a547957409514a2400202cd0207e8acfa65` reported clean source and reproduced the same 2,764,785-byte arm64 archive twice with SHA-256 `197690aceabecc45ad43dc61ebf9b998bd6c37eddbb511465279b4f3dc55b353`. Record, manifest, exact-byte checksum and four-file scope checks passed. The extracted binary's 76-byte mode-0600 debug log contained `job=1 statement_chars=66`; the SQL sentinel, result label, synthetic credential and `tokio_postgres` target were absent. An invalid target directive created no log path. | Direct local macOS and synthetic PostgreSQL 18.4 evidence. Readiness remained blocked by six named gates. This is not hosted, retained workflow or publication evidence. |
| Full repository gate after logging remediation | Pass twice. The modified remediation tree and clean commit `b2ac0a5` each passed formatting, clippy with warnings denied, 515 unit/layout tests, 39 CLI contracts and 38 PostgreSQL integration tests against the disposable PostgreSQL 18.4 plain/TLS services, with no skips. | Local macOS and synthetic PostgreSQL evidence. Each teardown removed the containers, network and data; final status was `Not running`. |
| Focused release and documentation contracts after logging remediation | Pass. 3 archive-helper, 6 documentation/build, 1 native-boundary, 72 release-contract, 7 release-notes and 1 release-schema tests passed. The catalogue wrapper validated one blocked record against its exact changelog heading. | 90 local focused tests plus the read-only catalogue wrapper; no hosted or platform-runtime claim. |
| Multi-target aggregation implementation | Focused pass. Four aggregation contracts cover canonical three-target output, combined-manifest verification, blocked-state union, create-only output and missing, duplicate, unsupported or identity-mismatched refusals. Seven workflow contracts cover the shared run-attempt identity, exact target matrix, action pins, per-target re-verification, aggregate command and six-file retained scope. The complete focused slice passed 3 archive-helper, 6 documentation/build, 1 native-boundary, 77 release-contract, 7 release-notes and 1 release-schema tests; the catalogue wrapper also passed. | 95 local tests plus the read-only wrapper. Hosted run `33614446587` independently re-verified all three target bundles, aggregated their records and retained the exact six-file scope. |
| Hosted aggregate artifact | Pass. The retained artifact contains exactly three archives, one aggregate record, one combined manifest and one `SHA256SUMS` file, with no symlinks; all files are bound to `fc79120b5229a1724a5a589dee4337acf2daa36b`. | GitHub Actions run `33614446587`, artifact retention through 2026-09-09; downloaded archive checksums and combined manifest verified locally. |
| Full repository gate with aggregation | Pass. `cargo --locked xtask verify` passed formatting, clippy with warnings denied, 515 unit/layout tests, 39 CLI contracts and 38 PostgreSQL integration tests. | Exact committed tip `fc79120b` with disposable PostgreSQL 18.4 plain/TLS services, with no skips. Teardown removed both containers, their network and synthetic data; final status was `Not running`. |
| Readiness and authorization | Correctly blocked. The aggregate readiness check exited 1 with eight issues, including incomplete evidence, detached and untagged source, absent verified signature and provenance, and non-publishable state. The historical `33563497933` logging audit remains a separate release-blocking result; the corrected run has not had an equivalent runtime privacy audit. Owner authorization covered commit, push and this seven-day run-scoped rehearsal. | T034 is complete. T033 remains open; no tag, signing, GitHub Release, package publication or canonical evidence promotion was authorized or performed. |

### Corrected hosted runtime and reproducibility run on 2026-09-03

Owner-authorized, non-publishing workflow run `33689674455` checked out the
exact pushed source revision
`259ac76de6477719b8a3515777f3623b24237dd6`. The workflow completed all archive,
repeat-build, comparison, runtime and aggregate jobs successfully. The run is
retained by GitHub Actions through 2026-09-09 and is bound to build identity
`ci/33689674455-1`.

| Evidence class | Result | Boundary |
| --- | --- | --- |
| Target archive jobs | Pass. macOS, Windows and Linux each built one declared target, generated and re-verified its record, manifest and checksum, and passed the blocked readiness assertion. | Hosted packaging and integrity evidence; no signature, provenance or publication claim. |
| Repeat-build reproducibility | Pass. Two fixed-identity builds for each target produced byte-identical archives; the comparison job passed for macOS, Windows and Linux. Windows uses `/Brepro` to remove PE linker timestamps. | Same source revision and declared CI inputs; not a signed or published release. |
| Runtime install and rollback smoke | Pass. Each exact archive extracted and executed its target binary. Identity, diagnostics, configuration initialisation and validation, non-interactive refusal, side-by-side candidate validation, controlled invalid-configuration exit 3 and known-good rollback validation all passed. | Hosted automated smoke evidence only. It does not replace Windows/Linux full-screen, PATH, ConPTY or hand-terminal checks, and it does not exercise a database query or the debug logging privacy boundary. |
| Aggregate evidence | Pass. The final artifact contains exactly three archives, one aggregate record, one combined manifest, `SHA256SUMS` and three runtime sidecars. The nine-file allowlist, record validation, combined manifest and independent archive checksums passed locally after download. | Run-scoped transport artifact; the record's canonical `evidence_reference.path` is still not materialised. |
| Readiness | Correctly blocked. The downloaded aggregate readiness check named eight issues: incomplete and unmaterialised evidence, detached and untagged source, non-publishable state, and absent verified signature and provenance. | This is expected fail-closed behaviour, not a release approval. |

The aggregate and runtime JSON sidecars record observed or verified evidence
without credentials, SQL text, result data or publication actions. The corrected
candidate therefore closes the hosted packaging, reproducibility and automated
runtime subset, while the remaining hand, privacy, inventory, signature,
provenance, canonical-path and authorization gates stay open.

Earlier 2026-08-16 runs remain historical evidence for their recorded
revisions. They do not supersede the later local and hosted evidence or close
any cross-platform installation, supply-chain or authorization gate.

## What has not been verified

- **Hosted runtime privacy correction.** The corrected target archives passed
  hosted packaging, reproducibility, aggregation and runtime smoke, but the
  replacement binaries have not yet had the direct logging privacy audit that
  rejected the historical candidate. The smoke jobs do not execute a database
  query or enable `IGNATIUS_LOG=debug`.
- **Canonical evidence and supply-chain records.** The retained nine-file
  aggregate is a run-scoped transport artifact. The canonical evidence path is
  not materialised, and no dependency inventory, verified signature or verified
  provenance record is attached.
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
