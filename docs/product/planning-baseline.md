# Planning baseline

Inspected on **2026-09-04, Africa/Johannesburg**. This is the dated evidence
behind the [roadmap](roadmap.md), not a second live status file. Resume from
[status](../status.md) and recheck the worktree before assigning work.

**Integration boundary:** this assessment inspected the owner's feature worktree
at local HEAD `3168e7c` plus uncommitted changes. The planning-only main integration
keeps application code at `52872ac`. Implementation, task lists and detailed
verification records for the later local slices remain in the originating
worktree until W01 carries them together. A local implementation observation
below is not a claim about the code shipped in this main checkout.

## Product assessment

The [vision](vision.md), [personas](personas-and-jobs.md) and
[experience brief](experience-roadmap.md) describe one product: a local
PostgreSQL workbench that combines a discoverable terminal interface with a
reliable scripting interface. The primary customer is an infrastructure
engineer operating production-adjacent databases; application developers and
terminal-based analysts share the query and export workflows.

The two ambitions depend on each other. Trust without usable discovery leaves
people memorising commands; convenience without truthful state makes the tool
unsuitable for the databases that matter. The product therefore has to prove
both an easy daily workflow and correct behaviour during failure. A larger
feature count is insufficient evidence for either.

Most of the originally proposed experience is now present in local code. The
largest remaining delivery risks are integration across those features,
Windows-native behaviour, real cloud authentication, accessibility in use,
bounded resource consumption and a complete release trust chain. Adding a new
editor or grid before addressing those gaps would not establish the product's
claimed advantage.

The [constitution](../../.specify/memory/constitution.md) remains the constraint
on this assessment. There is no account service, hosted control plane, telemetry
programme, AI query upload, generic database layer or plugin platform to build.

## Source and evidence boundary

| Observation | Evidence inspected | What it establishes |
| --- | --- | --- |
| Local branch is `codex/013-errors-at-cursor`, HEAD `3168e7c` | `git status --short --branch`, `git log` | Feature 012 is committed locally; later application changes and specification directories are in the worktree |
| Remote repository is private `tkomane/ignatius`; `main` is `52872acd7c0e0e38c6b810723ccda5eee99a28bf` | GitHub repository and branch API read on 2026-09-04 | Remote source baseline, separate from local HEAD and uncommitted work |
| Latest inspected main CI completed successfully | [Run 33720098318](https://github.com/tkomane/ignatius/actions/runs/33720098318), job and executed-step readback | Automated OS suites, PostgreSQL 14-18 plain/TLS contracts, Linux PostgreSQL 18 socket, Rust 1.90, workflow semantics, source/evidence scans and generated-output checks at `52872ac` |
| Scheduled fixture-drift job was skipped in that run | Same run's job list; `.github/workflows/ci.yml` | That scheduled-only check did not run. It is not included in the passed gates |
| Branch protection flag is false | GitHub branch API read on 2026-09-04 | Required checks are not currently enforced by branch protection. No assumption about plan entitlement is needed |
| Advertised private vulnerability reporting is not verified | Repository private-vulnerability-reporting API returned HTTP 404 on 2026-09-04 | Availability or access remains unresolved; the response does not prove the feature is disabled. W13 must verify a usable private route before public distribution |
| Recent local verifiers are recorded for Features 013-024 | Dated entries in the originating worktree's `docs/status.md` and each available `tasks.md` | Recorded local macOS/PostgreSQL 18.4 results, with semantic Unix-socket skips and manual evidence gaps; these were not rerun for this planning assessment |
| Feature 024 completed while this assessment was running | Re-read its local spec, all 21 checked tasks and the originating status file's Feature 024 evidence entry | Focused and locked-verifier plain/TLS evidence recorded, with the socket boundary skipped. Preserve the newer completion and terminal-key follow-up records there; integration and hand-terminal evidence still remain |
| Release tooling exists; acceptance remains open | Feature 008 T033/T035, ADR-0008, `release-evidence/`, release operations guide | Packaging and a green CI run do not establish a release-ready candidate, signing, provenance or authorization |

Hosted evidence above does **not** cover the newer local features. Historical
release run `33689674455` is described in the release records; its candidate
source is `259ac76`, not this worktree. Its recorded retention deadline is
2026-09-09. Retention and downloadable bytes must be rechecked before reuse;
neither was verified by this planning pass.

## Feature inventory and remaining work

Use the full directory slug as the identifier. There are two historical `007`
directories, and the old roadmap reused `002` through `007` for broad themes.
Do not rename those directories or treat a number alone as an assignment.
Existing feature task lists remain the authority for their checkboxes.
Links below identify specifications present on main. Plain-code slugs identify
specifications still in the originating local worktree; retrieve their complete
packages during W01 instead of reconstructing them from this inventory.

| Specification | Observed delivery state | Next boundary |
| --- | --- | --- |
| [001-foundation-vertical-slice](../../specs/001-foundation-vertical-slice/spec.md) | Implemented; three task items remain open | T051 Windows hand use, T052 Linux hand use, T055a ConPTY restoration |
| [001a-libpq-migration](../../specs/001a-libpq-migration/spec.md) | Closed without implementation by ADR-0012 | Historical unchecked migration tasks are inactive; generic release work belongs to 008 |
| [002-ide-navigation](../../specs/002-ide-navigation/spec.md) | Implemented; no separate task file | Retain catalogue navigation and shared-connection fallback in integrated journeys |
| [003-credential-routes](../../specs/003-credential-routes/spec.md) | Implemented; no separate task file | Cross-platform passfile/service-file permissions, precedence and failure evidence |
| [004-result-inspection](../../specs/004-result-inspection/spec.md) | Inspection and retained-row filtering implemented | Copy delivery is tracked by 018; verify source-row identity across all grid actions |
| [005-sql-editing](../../specs/005-sql-editing/spec.md) | Task list complete; editor contracts present | Preserve undo, cursor movement and real key paths in later editing features |
| [006-statement-history](../../specs/006-statement-history/spec.md) | Implemented; no separate task file | Recheck template-only persistence, pause/clear and hostile SQL privacy |
| [007-object-definitions](../../specs/007-object-definitions/spec.md) | Implemented; no separate task file | Verify definition/dependency scope and metadata-session fallback |
| [007-plain-line-mode](../../specs/007-plain-line-mode/spec.md) | Code implemented; T032-T034 open | VoiceOver/NVDA, Windows/Linux hand evidence and final traceability |
| [008-release-experience](../../specs/008-release-experience/spec.md) | Tooling implemented; acceptance incomplete | T033 full candidate evidence and T035 acceptance/status closure |
| [009-connection-profiles](../../specs/009-connection-profiles/spec.md) | Implemented; no separate task file | Keep profile precedence and production classification through picker/switching |
| [010-asking-and-keeping](../../specs/010-asking-and-keeping/spec.md) | Implemented, specified retrospectively | Integrate config init, prompts, saved SQL and retained-result export |
| [011-cloud-identity](../../specs/011-cloud-identity/spec.md) | Mechanism implemented; no separate task file | Native Windows launcher investigation and separate live Entra/AWS/GCP evidence |
| [012-schema-completion](../../specs/012-schema-completion/spec.md) | Task list complete; in local HEAD ancestry | Integration and terminal evidence for scoped, stale and bounded completion |
| `013-errors-at-cursor` | Local task list complete | Terminal and combined formatting/parameter/error-location evidence |
| `014-workable-result-grid` | Local task list complete | Hand use, resize, sort/filter/selection/copy/update identity interactions |
| `015-guided-discovery` | Local task list complete | First-use observation; discoverability and configured-key meaning |
| `016-readable-query-plan` | Local task list complete | Real terminal use and explicit distinction between EXPLAIN and ANALYZE |
| [017-connection-trust-surface](../../specs/017-connection-trust-surface/spec.md) | On remote main; mechanism locally verified | Per-provider account evidence and accurate native-platform remedies |
| `018-clipboard-copy` | Local task list complete | Actual terminal/WSL/SSH/multiplexer acceptance; never infer readback |
| `019-format-buffer` | Local task list complete | Real keys, undo, protected SQL bytes and narrow/plain presentation |
| `020-connection-picker` | Local task list complete | Quiet-session switching, stale work and native credential routes |
| `021-intentional-exports` | Local task list complete | Retained-versus-streamed scope, file recovery and hand workflow |
| `022-parameters-prompted` | Local task list complete | Combined execution/refresh/history/error privacy and platform prompts |
| `023-cell-update` | Local task list complete | Review-to-write identity, concurrent server changes, count/outcome and terminal evidence |
| `024-retained-result-refresh` | Implemented locally; its 21-task list is reconciled after this baseline inspection | Retain the recorded plain/TLS verifier evidence, semantic Unix-socket skip and terminal hand gaps in the originating worktree's status file |

For older features without `plan.md` or `tasks.md`, do not fabricate historical
implementation tasks or test results. Before changing their behaviour, create a
scoped follow-up specification or add a clearly dated plan/task section under
the existing feature, with traceability to its requirements.

## Planning drift found and corrected

- ADR-0011 rejected the OS credential store on 2026-08-16; it is not an open
  prerequisite. ADR-0012 closed libpq and withdrew the FFI exception.
- The context diagram omitted the provider subprocess and intentional
  clipboard transfer, and still showed the rejected credential store.
- Several spec headings described implemented work as draft or unimplemented.
  Those headings can be aligned to recorded evidence without inventing new
  verification or ticking additional tasks.
- Current guidance still described PostgreSQL 15/17 and the socket CI job as
  unexercised. The inspected main CI establishes those historical hosted facts,
  while leaving the newer worktree and hand-terminal evidence unverified.
- The next-action list still proposed work already present locally. The new
  roadmap assigns integration and acceptance before more product scope.

These are documentation findings. The assessment is not a source-security audit,
a fresh full-suite pass, a usability study or a release acceptance decision.
