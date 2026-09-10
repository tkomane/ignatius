# Feature Specification: Connection trust surface

**Feature Branch**: `017-connection-trust-surface`

**Created**: 2026-09-03

**Status**: Implemented and pushed to `main` on 2026-09-03; local verification
complete; live cloud-provider evidence pending

**Input**: User request to apply Emil Kowalski UI craft principles and make
Ignatius a first-class cloud-authenticated PostgreSQL client.

## Scope

This slice makes the existing connection facts understandable at the moment
they matter. It does not add a hosted identity service, store credentials, or
claim that an unverified provider is safe. It gives Entra, AWS, Google Cloud,
and configured providers the same visible, honest interaction model.

## User Scenarios & Testing

### User Story 1 - Understand the connection before acting (Priority: P1)

Someone opens the client against a database and needs to know which database,
role, environment, transport, and authentication route they are about to use.

**Why this priority**: A database client earns trust by making high-consequence
state visible before a query is sent. Condensed header text is useful at a
glance, but a person also needs a readable detail view without memorising a
command or a provider's vocabulary.

**Independent Test**: Open the command palette, choose connection details, and
verify that the panel names the target, database, role, environment, posture,
TLS guarantee, and authentication route in a colour- and icon-independent
rendering.

**Acceptance Scenarios**:

1. **Given** a connected Entra, AWS, Google Cloud, or configured-provider
   session, **When** connection details are opened, **Then** the panel names
   the provider, the provider command, the credential lifetime caveat, and that
   the token is not retained or shown.
2. **Given** a password-authenticated session, **When** connection details are
   opened, **Then** the panel says that no cloud token was requested and names
   the supported password routes without inventing an identity claim.
3. **Given** a production or read-only session, **When** connection details are
   opened, **Then** the environment and server-enforced posture are explicit
   words, not colour-only signals.

### User Story 2 - Recover without guessing (Priority: P1)

Someone's provider is unavailable or rejects a credential and needs the next
safe action without copying a token or leaving the client to search docs.

**Why this priority**: Cloud tools fail for different reasons. A generic
"authentication failed" message sends people down the wrong path and can lead
to unsafe fallbacks. Provider-specific remedies reduce time and prevent
password prompting where a bearer token was required.

**Independent Test**: Render the details panel and provider diagnostics for each
built-in provider and a configured provider; verify that every case has a
provider-safe next action and no credential value.

**Acceptance Scenarios**:

1. **Given** a provider command is missing or not signed in, **When** its
   diagnostic is shown, **Then** the next action names the provider's own
   sign-in or identity command.
2. **Given** a provider target permits unencrypted transport, **When** a
   connection is attempted, **Then** the token is not requested and the
   diagnostic explains the TLS requirement.
3. **Given** a cloud provider connection is refused, **When** the failure is
   handled, **Then** the client does not fall back to a password prompt.

### User Story 3 - Keep the interface calm and adaptable (Priority: P2)

Someone uses a narrow, ASCII-only, no-colour, high-contrast, or reduced-motion
terminal and still needs the same information and control.

**Why this priority**: A polished interface is only first-class when its
meaning survives the user's environment and assistive technology.

**Independent Test**: Render the same connected model across all themes, glyph
tiers, colour settings, and reduced-motion settings; compare the required words
and keyboard behavior.

**Acceptance Scenarios**:

1. **Given** ASCII or no-colour presentation, **When** the panel is opened,
   **Then** every security and identity fact remains readable as text.
2. **Given** reduced motion, **When** the panel is opened or closed, **Then**
   no motion is required to understand its state.
3. **Given** the panel is open, **When** Escape is pressed, **Then** only the
   panel closes and the underlying session is unchanged.

### Edge Cases

- The provider name is custom or replaced in configuration; the panel shows
  the configured name and command without pretending it is a built-in cloud.
- The session is connecting, disconnected, or lost; the panel states what is
  known and labels unknown facts rather than filling them with defaults.
- A provider command contains hostile display text; it is escaped before it is
  rendered and is never executed by the panel.
- The terminal is too narrow for the full panel; the existing compact or
  too-small layout remains readable and gives a way out.

## Requirements

### Functional Requirements

- **FR-1301**: The interactive client MUST provide a discoverable connection
  details surface through the command palette.
- **FR-1302**: The surface MUST state target, database, role, environment,
  read/write posture, transport state, and connection state using words.
- **FR-1303**: A cloud-authenticated session MUST state the canonical provider
  label when known, its configured command without a credential value, and the
  provider's documented or configured lifetime caveat.
- **FR-1304**: A session without cloud identity MUST say that no cloud token was
  requested rather than guessing which password route succeeded.
- **FR-1305**: Provider diagnostics MUST preserve provider-specific remedies and
  MUST never include token contents, password contents, or shell execution.
- **FR-1306**: The surface MUST be usable through keyboard dismissal and MUST
  not swallow a quit request.
- **FR-1307**: The surface MUST preserve the existing ASCII, no-colour,
  high-contrast, and reduced-motion guarantees.
- **FR-1308**: Provider metadata MUST be client-side display metadata only; no
  token, password, query, result, or schema may be written or uploaded.

### Security Requirements

- **SEC-1301**: Opening or rendering connection details MUST NOT trigger a
  provider command, network request, or credential refresh.
- **SEC-1302**: Provider command text MUST be treated as untrusted display text
  and passed through the existing display sanitisation path.
- **SEC-1303**: Cloud authentication MUST continue to refuse targets whose
  transport could be unencrypted and MUST not fall back to a password prompt.

### Key Entities

- **Provider presentation**: Non-secret label, command summary, lifetime
  caveat, transport requirement, and remedy for one configured provider.
- **Connection details surface**: A read-only view of the current session's
  resolved facts and provider presentation.

## Success Criteria

### Measurable Outcomes

- **SC-1301**: A new user can identify the active cloud provider and transport
  guarantee in one palette search and one selection, without reading the
  configuration file.
- **SC-1302**: All built-in providers and configured providers render the same
  required security facts in every supported presentation tier.
- **SC-1303**: Automated tests prove that opening the surface performs zero
  credential or network operations and that no secret appears in rendered text.
- **SC-1304**: Provider failure guidance gives one concrete, provider-specific
  next action for every built-in provider.

## Assumptions

- Cloud authentication remains command-line-provider based in this release;
  direct SDK login, browser windows, and hosted identity brokering are out of
  scope.
- The server remains the authority for TLS and read-only posture; the panel
  reports what the server or resolved target says and does not elevate claims.
- The existing palette, theme, sanitisation, and terminal restoration patterns
  remain the shared interaction primitives.
