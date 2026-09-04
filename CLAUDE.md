# Working in this repository

Operational guidance only. The rules live in `.specify/memory/constitution.md`
and the current state lives in `docs/status.md`. Read both before changing
anything.

## Orient first

```bash
cat docs/status.md                                  # what is done, what is not
git log --oneline | head -20                        # recent work
git status                                          # uncommitted work is someone's
cat specs/001-foundation-vertical-slice/tasks.md    # task checkboxes are authoritative
```

Never assume the repository is empty or that a summary of it is current. Check.

## Plan and take an agent assignment

Read [the roadmap](docs/product/roadmap.md) for milestone order,
[the delivery plan](docs/product/agent-delivery-plan.md) for scoped work packages,
and [the agent playbook](docs/operations/agent-playbook.md) for ownership and
handoff. Use full feature slugs, including the two historical `007` directories.
The selected feature's spec/tasks remain authoritative; roadmap packages do not
replace their completion evidence or authorize external actions.

## Where authority lives

| Question | Authority |
| --- | --- |
| What may I not do? | `.specify/memory/constitution.md` |
| What are we building next? | `specs/<feature>/spec.md` and `tasks.md` |
| Why is it built this way? | `docs/architecture/decisions/` |
| What does this release actually support? | `docs/support/compatibility.md` |
| What is the state right now? | `docs/status.md` |
| What is the product called? | `src/branding.rs`, nowhere else |
| What do the exit codes mean? | `src/exit_code.rs`, mirrored in the compatibility doc |

## Validating a change

```bash
cargo xtask db up      # disposable server with synthetic fixtures
cargo xtask verify     # every gate, collecting failures, with a summary
```

`verify` runs formatting, lints, unit tests, the CLI contract and the integration
tests, continuing past a failure so one run shows every problem. It exits
non-zero if any gate failed.

Without a running database the integration tests skip and both xtask and the
tests say so. A skip is not a pass: report it as a skip.

## Handling secrets

- Never put a real credential in source, configuration, a test fixture, a commit
  message, a log, or a shell command. The container credentials in `docker/` are
  synthetic and stay that way.
- Passwords live in `secrecy::SecretString`. Anything displayable goes through
  `diagnostics::redaction`. There is one redaction implementation; do not add a
  second.
- A new credential route needs a threat-model update in the same change.

## Preserving other people's work

- Inspect before writing. Never overwrite a file you have not read.
- When two changes conflict, keep both behaviours unless one is wrong. A merge
  being inconvenient is not a reason to discard a side.
- Commits are small, imperative, and explain why. Do not push, publish, tag or
  release without the owner saying so.

## Reporting verification

Report these separately and never merge them:

- what a focused test proved
- what the full suite proved
- what live evidence proved, on which platform, against which server version
- what remains unverified

"The tests pass" is not evidence about the terminal. "It compiles for Windows" is
not evidence that it works on Windows. Say which one you have.
