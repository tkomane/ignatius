## What changed, and why

<!-- Why over what. The diff already says what. -->

## Evidence

Report these separately. A skipped test is a skip, not a pass.

- [ ] `cargo xtask verify` passes locally
- [ ] Integration tests ran against a real server, rather than skipping
- [ ] Live evidence, if the change touches the terminal or the wire protocol:

<!-- platform, server version, what you observed -->

## Checks

- [ ] Documentation and `CHANGELOG.md` updated in this change, if behaviour changed
- [ ] `specs/` and `docs/status.md` updated, if this completes or opens a task
- [ ] No new colour-only or icon-only meaning
- [ ] No secret in source, fixtures, logs or commit messages
- [ ] No silent fallback added: no retry, downgrade, ignored parameter or implicit migration
