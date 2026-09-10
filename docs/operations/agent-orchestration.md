# Agent orchestration

Operational configuration for multi-agent work on Ignatius. Written 2026-09-10.

Roles are separate from model assignments: the role definitions live in
`.opencode/agent/`, the routing table below names the current model, and
replacing a model changes one line in one agent file. This document is not a
completion ledger. [docs/status.md](../status.md) remains the resumption point,
the selected feature's `spec.md` and `tasks.md` remain authoritative, and the
[agent playbook](agent-playbook.md) defines the assignment and return contract.

## Roles

| Role | Where | Responsibility | Write access |
| --- | --- | --- | --- |
| Lead | primary agent | Priorities, architecture, integration, acceptance, status | Whole tree, serialized |
| Explorer | `.opencode/agent/explorer.md` | Bounded repository or documentation investigation; findings with `file:line` | None |
| Implementer | `.opencode/agent/implementer.md` | One vertical slice in explicitly owned files | Owned files only |
| Reviewer | `.opencode/agent/reviewer.md` | Independent adversarial review of a diff and its acceptance evidence | None |

Delegation rules:

- At most three workers at once. Reduce when latency or allowance says so.
- No concurrent writes to one file. The shared files named in the playbook
  (`app/model.rs`, `app/message.rs`, `app/update.rs`, `cli/interactive.rs`,
  `ui/layout.rs`, CI, lockfile, status docs) have one writer at a time.
- Serialize `cargo xtask db up` and `cargo xtask db down`; one disposable
  database lifecycle per run.
- Every assignment carries the playbook's assignment and return block, a named
  model, and a stopping condition.
- A substantive change gets an independent reviewer on a different model family
  before the lead accepts it. A review is not a substitute for tests or live
  evidence.
- Escalate after two failed attempts at the same problem, or earlier when the
  risk warrants it, with a concise failure summary. Do not repeat a blind retry
  on the same model.
- Custom agents become selectable after an opencode restart. A session that
  predates a config change uses the built-in `explore` and `general` agents.

## Model routing

Dated 2026-09-10. Routing assumes the OpenCode Go subscription; verify the
working environment's credentials with `opencode auth list`. Do not assume a
Zen key, a direct provider key, or an approved overage or auto-reload.

| Role | Model | Variant | Rationale | Fallback |
| --- | --- | --- | --- | --- |
| Lead | `opencode-go/deepseek-v4.1-flash` | model default | Best effective value in measured use | `opencode-go/glm-5.3` for ambiguous architecture |
| Explorer | `opencode-go/deepseek-v4.1-flash` | model default | Bounded, high-volume research; cheap enough to parallelise | `opencode-go/deepseek-v4-flash` (legacy alias; DeepSeek serves it as V4.1 Flash) |
| Implementer | `opencode-go/deepseek-v4.1-flash` | model default | Bounded, specified slices | `opencode-go/glm-5.3` after a failure summary |
| Reviewer | `opencode-go/glm-5.3` | model default | Independent family for adversarial review | `opencode-go/kimi-k2.7-code` |

Notes:

- opencode supports provider-specific reasoning effort through `variant`
  (`opencode run --variant` describes values such as `high`, `max`, `minimal`).
  The exact variant names for these Go models are unverified, so none is set.
  Calibrate on a real task before assigning a variant.
- DeepSeek V4 Pro is not used for escalation: from 2026-09-14 06:00 SAST
  DeepSeek routes `deepseek-v4-pro` requests to V4.1 Flash and bills at the
  Flash price, so it cannot provide a stronger second opinion.
- Do not route repository content through a provider that was not already
  authorised for the work. Free-tier models are outside this routing unless the
  operator authorises them.
- Validated 2026-09-10: `explorer`, `implementer` and `reviewer` each loaded
  from `.opencode/agent/` and answered a smoke prompt, and the reviewer route
  resolved `opencode-go/glm-5.3`.

Fallback policy:

1. Prefer the model with remaining monthly bucket headroom for the task class.
2. On a limit warning, checkpoint the work, record the exact limit and where
   work stopped in `docs/status.md`, and continue on the fallback model in the
   same provider. Resume the preferred model when allowance returns.
3. Never enable Zen balance overflow, auto-reload or a new provider without the
   owner's approval. A quota stop is a checkpoint, not a reason to spend.

## Dated offer observations

Checked 2026-09-10. Sources: `https://opencode.ai/docs/go/` (page timestamp
2026-09-10), `https://opencode.ai/go`, `https://opencode.ai/docs/zen/` (Last
updated 2026-09-10), `https://api-docs.deepseek.com/quick_start/pricing/`, and
the two X posts named by the owner. USD is authoritative. ZAR figures are
converted at an assumed 16.30 ZAR/USD, the most recent verified daily close in
hand (2026-08-06, YCharts); the rate was not re-verified on 2026-09-10.

| Item | Observation | Status |
| --- | --- | --- |
| Subscription | OpenCode Go, $10/month, one plan, no annual option | Official |
| Monthly buckets | Per-model dollar limits: V4.1 Flash $15, V4 Flash $30, GLM-5.3 $15, GLM-5.2 and GLM-5.1 $60 each, Kimi K2.7 Code $60, Kimi K3 $15, Qwen3.8 Max $15 | Official table |
| Stacked windows | $12 per rolling 5 hours, $30 per week, $60 per month | Third-party reports; not shown on the official page in this check |
| Overage | Usage stops at the limit; Zen balance overflow only if opted in | Third-party; unverified. Do not opt in |
| V4.1 Flash rate | Off-peak $0.15 in / $0.60 out / $0.003 cached read per MTok; peak $0.30 / $1.20 / $0.006 | Official |
| Peak windows | 01:00-04:00 and 06:00-10:00 UTC Mon-Fri = 03:00-06:00 and 08:00-12:00 SAST Mon-Fri; all other hours and weekends are off-peak | Official |
| Promotion | "DeepSeek Flash v4.1 is now available in OpenCode Go", quoted with "day 0 support we've bumped it up to 4x usage for the next 72 hours" (posted 2026-09-10; displayed time 05:37; timezone unconfirmed) | X post; expiry about 2026-09-13 05:37 if the displayed time is SAST |
| Promotion | "GLM 5.3 Flash now gets twice the limits on OpenCode Go" (posted 2026-09-09; displayed time 22:59; timezone unconfirmed) | X post; permanence unverified |
| Zen | Pay-as-you-go at listed per-token rates, no subscription; card fee 4.4% + $0.30; auto-reload defaults to $20 when the balance falls below $5 | Official; not authenticated here, so unavailable without owner approval |
| Direct DeepSeek | V4.1 Flash at the same off-peak/peak rates as Go, 1M context, thinking mode default, 2500 concurrency | Official; would be a new billing arrangement, not authorised |
| ZAR equivalent | $10 = R163.00; a $60 bucket = R978.00; a $15 bucket = R244.50 | Indicative, at the assumed FX |

Accounting distinction: a subscription allowance is not a cash charge. Model
usage is metered in dollars against included allowance buckets; it becomes cash
only if an overage route is deliberately enabled. Zen and direct API pricing
are per-token cash arrangements, not alternative views of the same allowance.

Refresh procedure: at session start, at a promotion expiry, or on a material
availability change, re-open the sources above, record the date and any change
in a new dated section here, and update the routing table only when a bucket,
promotion or model retirement changes the value calculation. A promotion
ending does not by itself change the routed model.

## Current ownership

Dated 2026-09-10. Recorded from this session; update at handoff.

| Item | State |
| --- | --- |
| Package | W03 (verify query, parameter and reviewed-write semantics) |
| Slices | 1. Repair `FROM ONLY` misresolution in Feature 023 source analysis. 2. Live reviewed-write evidence (composite quoted key, concurrent change, restricted role, review-to-execution identity) with accepted concurrency semantics documented |
| Owned files | `src/query/update.rs`, `specs/023-cell-update/`, `tests/postgres_integration.rs`, test module of `src/app/update.rs`, `docs/support/compatibility.md`, `CHANGELOG.md` |
| Status | Both slices accepted after independent review; `cargo --locked xtask verify` green at the final source on 2026-09-10 (995 workspace tests, 53 live PostgreSQL integration tests, one Unix-socket semantic skip); fixture torn down |
| Evidence | `docs/status.md` section "W01 reconciliation and W03 reviewed-write evidence - 2026-09-10" |
| Next candidates | W02 runtime interleavings; `standard_conforming_strings` decision; W04 privacy probes |
| Blockers | Live cloud-provider evidence needs an account; Windows/WSL hand checks need a Windows host; release evidence needs owner authority |

## Local container runtime note

On macOS workstations this project uses a CLI-only container runtime (colima
with the Homebrew `docker` CLI) rather than Docker Desktop. Do not depend on
Desktop-only tooling.

- The daemon runs only while the VM is started. Check with `colima status` and
  start with `colima start` before any docker command; retry once if a command
  fails while it is down.
- The repository compose fixture works through the standard context with
  `cargo xtask db up`; no `DOCKER_HOST` or `DOCKER_CONFIG` override is needed.
- If `credsStore: osxkeychain` is configured, install the
  `docker-credential-helper` formula, or every image pull fails even for
  public images.
- Do not use a removed `desktop-linux` context or a dangling default socket.
  Stale shells should run `hash -r` and `unset DOCKER_HOST`.
- `cargo xtask db down` is `docker compose down -v` scoped to this repository's
  own disposable fixture, which is the prescribed teardown. Never run global
  volume prunes, and never run `down -v` against another project.

