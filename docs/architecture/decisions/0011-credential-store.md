# ADR-0011: Where a stored credential would live

- Status: Proposed. Owner confirmation is required before implementation.
- Date: 2026-08-16
- Related: ADR-0006 (configuration and state), ADR-0007 (secret handling),
  `specs/009-connection-profiles/spec.md`

## Context

Four credential routes exist and are tested: a connection string, `PGPASSWORD`,
a `.pgpass` password file, and a prompt in both surfaces. Connection profiles
name a database and deliberately refuse to hold a password.

What does not exist is a way to say "remember this password for this profile"
without writing it into a file the user manages themselves. That is the last
item of roadmap Feature 002, and it is the first feature in this product that
would put a secret somewhere the product chose.

The decision is gated because it is not reversible in the way a UI choice is:
once a build has written a secret into an operating-system store, uninstalling
the build does not remove it, and a later change of format leaves credentials
stranded under a name nothing reads.

## The options

**A. Do not store credentials at all.** The four existing routes stay the whole
answer. `.pgpass` is already the tool most PostgreSQL users have, it is shared
with `psql`, and the file's permissions are checked and reported. Cost: a
password typed at the prompt is typed again next session.

**B. The operating system's own store, through the `keyring` crate.** macOS
Keychain, Windows Credential Manager, and Secret Service or the kernel keyring
on Linux. A profile would gain a *reference* - the account name under which to
look - never a secret. Cost: a new dependency with real platform surface; on
Linux, Secret Service needs a session bus, which a container or an SSH session
often does not have, so the failure mode has to be a clear fallback rather than
a crash. Also: what this build writes, this build must be able to delete, which
means an explicit `credential forget` and documentation of exactly what is
stored, under which service name.

**C. An encrypted file of our own.** Rejected before it is weighed. A key has to
live somewhere, and every honest answer to where is either the operating
system's store, which is option B with extra steps, or a passphrase the user
types, which is the prompt with extra steps.

## Decision proposal

Option B, with these conditions, or option A if the owner would rather not carry
the dependency:

1. **Opt-in per profile, never automatic.** Nothing is stored because a password
   was typed. Storing is an explicit action with its own command.
2. **A profile holds a reference, not a secret.** That keeps the existing
   refusal in `specs/009-connection-profiles/spec.md` true rather than carving
   an exception into it.
3. **What is stored is listable and removable** by this build: `credential list`
   shows service and account names and never values, and `credential forget`
   removes one.
4. **An unavailable store is a stated fallback, never a failure.** No session
   bus, a locked keychain, a headless container: the client says the store could
   not be reached and falls back to the routes that do not need it.
5. **The store is never consulted for a target the user did not name.** A
   lookup keyed by host and user alone would hand a credential to whatever
   happened to answer on that address.

## Consequences

- One more dependency, with three platform backends and their own failure modes,
  each of which needs an integration test that can run headless in CI.
- `docs/security/data-handling.md` gains a row that is materially different from
  every other row in it: a secret this product put somewhere.
- The threat model needs a paragraph on what an attacker with the user's session
  can do, which is: read the store, exactly as they could read `.pgpass`.

## What the owner is being asked

Whether a credential store is wanted at all, given that `.pgpass` already exists
and is shared with `psql`. If it is, whether the `keyring` dependency and its
Linux caveat are acceptable. Nothing is implemented until that answer exists.
