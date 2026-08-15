# Feature Specification: Credential routes

**Feature Branch**: `003-credential-routes`

**Created**: 2026-08-15

**Status**: Password files and service files implemented and verified against a
real server. The OS credential store and connection profiles are not started.

**Numbering note**: third feature built. It delivers the driver-independent half
of the roadmap's "connection experience and secrets", which ADR-0009 sequences
ahead of the libpq migration because these are file formats rather than driver
capabilities. Written after the code, like Feature 002, and said so plainly.

## User Scenarios & Testing

### User Story 1 - Connect without putting a password anywhere dangerous (P1)

Someone already has a `.pgpass` because `psql` uses it. They want this client to
use it too, so the password stays out of the command line, the environment, and
their shell history.

**Independent Test**: write a password file, connect with a URI that names no
password, and confirm the connection succeeds.

**Acceptance Scenarios**:

1. **Given** a password file with a matching line, **When** no password is
   supplied any other way, **Then** it is used.
2. **Given** a password supplied explicitly, **When** a password file also
   matches, **Then** the explicit one wins.
3. **Given** a password file others can read, **When** it is consulted, **Then**
   it is not used, and the reason and the fix are stated.
4. **Given** a password file that matches nothing, **When** it is consulted,
   **Then** that is reported rather than passing silently.

### User Story 2 - Reach a database by its service name (P1)

A team shares a `pg_service.conf`. Someone wants `service=orders-prod` rather
than remembering a host, a port, a database and a user.

**Independent Test**: write a service file, connect with `service=name` alone,
and confirm every parameter came from the file.

**Acceptance Scenarios**:

1. **Given** a service file, **When** a service is named, **Then** its parameters
   are used.
2. **Given** a service and a connection string that disagree, **When** they are
   merged, **Then** the connection string wins and the service beats the
   environment.
3. **Given** a service name that does not exist, **When** it is requested,
   **Then** the error names the services that do.
4. **Given** a service carrying an unsupported security parameter, **When** it is
   used, **Then** the connection is refused exactly as it would be from a URI.

## Requirements

- **FR-201**: Password files MUST be read in PostgreSQL's `.pgpass` format,
  including `*` wildcards, `\` escapes, and first-match-wins ordering.
- **FR-202**: A socket path MUST match `localhost`, as libpq does.
- **FR-203**: Service files MUST be read in `pg_service.conf` format, selected by
  the `service` parameter or `PGSERVICE`.
- **FR-204**: Precedence MUST be arguments, connection string, service file,
  environment, defaults; and for the password, connection string, `PGPASSWORD`,
  password file.
- **FR-205**: A malformed service file MUST be an error rather than partially
  applied.

- **SEC-201**: A password file that is readable by anyone but its owner MUST NOT
  be used, and the reason and remedy MUST be stated.
- **SEC-202**: A password from a file MUST never appear in output, a note, or a
  `Debug` representation.
- **SEC-203**: Security parameters in a service file MUST be refused on the same
  terms as those in a connection string.

- **UX-201**: A password file that was found but did not match MUST say so.
- **UX-202**: An unknown service MUST list the services that exist.

## Success Criteria

- **SC-201**: A connection succeeds with a service file and a password file as
  the only source of every parameter, verified against a real server.
- **SC-202**: Loosening the password file's permissions stops it being used, and
  the resulting failure is an authentication failure with an explanation.
- **SC-203**: No test reads the developer's own home directory.

## Assumptions

- A synthetic environment never falls back to real home-directory files, which is
  enforced by a flag on the environment snapshot rather than by convention.
- `PGSYSCONFDIR` is honoured only when there is no personal service file, which
  is libpq's own order.
- The OS credential store, password prompting and connection profiles remain
  out of scope here.
