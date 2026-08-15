# Feature Specification: Named connections

**Feature Branch**: `009-connection-profiles`

**Created**: 2026-08-16

**Status**: Implemented, verified on macOS against PostgreSQL 18.4, including a
profile that pins `production` and the write it then refuses.

**Numbering note**: files are numbered in build order. This is the profiles half
of roadmap Feature 002, *Connection experience and secrets*. The credential
store is not here and is not implied by it.

## Why this exists

Not to save typing, though it does. A profile is where a database's
classification lives, so that `environment = "production"` is written down once
instead of remembered every time. Forgetting to type `--environment production`
is exactly how a write reaches the wrong database; a profile is the place that
cannot forget.

## User Scenarios & Testing

### User Story 1 - The production database is production every time (Priority: P1)

Someone connects to production several times a week. The write guard should
apply every time without being asked for.

**Independent Test**: define a profile with `environment = "production"`, run a
write through it, and be refused.

**Acceptance Scenarios**:

1. **Given** a profile with an environment, **When** it is used, **Then** the
   connection carries that classification, and a write is held back exactly as
   if the flag had been typed.
2. **Given** a profile with `read-only = true`, **When** it is used, **Then**
   the server is asked to refuse writes for the session.
3. **Given** `--read-only` on a profile that does not ask for it, **When** they
   are combined, **Then** the session is read-only: a profile can make a session
   safer, and nothing in the file can make it less safe.

### User Story 2 - Say where without saying it all (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a profile, **When** it is named as `@name` or `--profile name`,
   **Then** the two mean the same thing.
2. **Given** a profile and an explicit flag, **When** they disagree, **Then** the
   flag wins, because it was typed now.
3. **Given** a profile name that is not in the file, **When** it is used, **Then**
   the profiles that do exist are listed.
4. **Given** a profile and a connection target together, **When** they are given,
   **Then** it is a usage error: both say where to connect and they may not
   agree.

### User Story 3 - Not a place for secrets (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a profile with `password`, `pgpassword` or `sslpassword`, **When** it
   is read, **Then** it is refused with the routes that do exist, and the value
   is never repeated back.
2. **Given** a profile with any other unknown field, **When** it is read, **Then**
   it is refused with the fields that are understood.

### Edge Cases

- A database genuinely named `@something`.
- A profile whose `sslmode` or `environment` this build does not know.
- An empty `[profiles]` table.
- A profile naming a `passfile`.

## Requirements

### Precedence

Stated once, because it is what people will file bugs about:

| Source | Beats | Notes |
| --- | --- | --- |
| Command-line flags | everything below | typed now, so it wins now |
| A profile | the environment and defaults | an explicit request, like a connection string |
| A connection target | the environment and defaults | mutually exclusive with a profile |
| A service file | the environment and defaults | reached through a connection string |
| Environment variables | defaults | |
| Defaults | | |

- **FR-901**: `--profile name` and the shorthand `@name` MUST be equivalent.
- **FR-902**: A profile and a connection target together MUST be a usage error,
  unless the target is the same profile written as `@name`.
- **FR-903**: A profile MUST fill only what was not given on the command line.
- **FR-904**: `read-only` MUST be the union of the flag and the profile.
- **FR-905**: An unknown profile MUST list the profiles that exist.
- **FR-906**: A target beginning with `@` MUST always be read as a profile
  reference. A database whose name begins with `@` remains reachable through a
  full connection string.

### Security

- **SEC-901**: A profile MUST NOT hold a password under any field name, and the
  refusal MUST name the routes that exist rather than saying "unknown field".
- **SEC-902**: A refusal MUST NOT repeat the value it refused.
- **SEC-903**: A profile MUST NOT be able to weaken a session: it may add
  `read-only`, never remove it.
- **SEC-904**: A profile MUST NOT name a password file in this release. One
  place decides where password files come from, and it is not per profile.

## Success Criteria

- **SC-901**: A write through a profile classified as production is refused
  without any flag being typed.
- **SC-902**: A profile with a password in it never starts a connection.
- **SC-903**: `connect --check @name` reports on the host the profile named.

## Assumptions

- Profiles are a configuration-file feature, so `config validate` checks them as
  part of checking the file, and `config show` prints them.
- `#[serde(default)]` on the table means older files still parse. A file written
  with profiles will not be understood by an older build, which is the migration
  documentation's business rather than a reason to bump the schema version.
- The credential store, and profiles that reference a credential in it, are a
  later feature. A profile referencing a secret by name is a different design
  from a profile holding one, and only the second is refused here.
