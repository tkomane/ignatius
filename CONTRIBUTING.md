# Contributing

## Before you write code

Read `.specify/memory/constitution.md`. It is short and it is binding. Then read
`docs/status.md` for where the work actually is, and the relevant
`specs/<feature>/spec.md` for what is being built.

Behaviour changes start with a specification, not a pull request. If what you
want is not in a spec, propose the spec change first; it is a cheaper
conversation.

## Getting set up

See `docs/operations/local-development.md`. In short: a Rust toolchain, Docker
for the disposable PostgreSQL server, and:

```bash
cargo xtask db up          # disposable PostgreSQL with synthetic fixtures
cargo xtask verify         # every gate, with a summary
cargo xtask install-hooks  # run those gates before every push
```

Main is protected: changes arrive as a pull request, and its required CI checks
must pass before it can merge. The pre-push hook runs the same gates locally
first, so a failure is found in seconds rather than after a push.

## What a good change looks like

- One coherent thing per commit, with an imperative message that explains why
  rather than restating the diff.
- Tests beside the behaviour they prove. A test name should read as the claim it
  is making.
- Documentation and release notes updated in the same commit as the behaviour.
- Nothing that weakens a constitutional principle. If a principle is genuinely
  wrong, amend it explicitly rather than routing around it.

## What will be sent back

- A new dependency without a reason, or a second way to do something that already
  has one implementation. Redaction in particular has exactly one home.
- Colour or an icon as the only carrier of meaning.
- A status message that claims more than the code knows.
- A silent fallback: a retry, a downgrade, an ignored parameter, an implicit
  migration.
- A test that asserts a screenshot without asserting the meaning in it.
- An em dash in prose. Use a normal hyphen.

## Reporting results

Separate what a focused test proved, what the full suite proved, what live
evidence proved and on which platform, and what remains unverified. Skipped tests
are skips, not passes.
