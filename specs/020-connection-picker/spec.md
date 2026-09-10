# Feature Specification: Connection picker

**Feature Branch**: `020-connection-picker`

**Created**: 2026-09-04

**Status**: Implemented locally; locked verifier green with Unix-socket skip;
terminal hand checks pending

**Input**: The roadmap item "A connection picker, so starting is choosing
rather than typing a URI", as part of the owner's request for a first-class,
GUI-like terminal experience.

## User Scenarios & Testing

### User Story 1 - Choose before the first connection (Priority: P1)

Someone has named connections in `config.toml`. They start Ignatius without an
explicit target and see a searchable list before any database or provider work
starts. They choose a profile, or choose the default route when they want the
usual environment and service-file resolution.

**Why this priority**: A named profile is useful only when it removes the need
to remember a URI. The first screen should make the safe choice visible before
the client contacts a server.

**Independent Test**: Start with two valid profiles and no explicit route.
Assert that the picker is visible, no connection effect has started, and
choosing one profile produces exactly one connection preparation request.

### User Story 2 - Read the choice like a small connection card (Priority: P1)

Someone is choosing between development, staging and production. Each row names
the profile, safe location facts, database and role when configured, transport
mode, environment classification, read-only posture, cloud identity route and
the optional description. A profile that needs configuration attention remains
visible and says so without exposing unknown values or secrets.

**Independent Test**: Render a picker containing local, production,
read-only, cloud-authenticated and invalid profiles in Unicode and ASCII
compact layouts. Assert that the state words and safe facts survive styling
loss, while a password-shaped unknown field never appears.

### User Story 3 - Switch deliberately during a session (Priority: P1)

Someone is connected to one database and wants another named connection. They
open the connection picker from the command palette or the `Ctrl+K n` chord,
choose a profile, and see the session move through an explicit connecting state.
The old result, object tree, completion snapshot and error marker are cleared;
the editor buffer and local preferences remain available for the new session.

**Independent Test**: Start from a connected model with a retained result and a
loaded tree, choose a profile, and assert that one effect is emitted, the
buffer is unchanged, old server-derived state is gone, and the profile's auth
route controls password-prompt behaviour.

### User Story 4 - Keep every route honest (Priority: P1)

The picker uses the existing profile resolver and cloud-authentication boundary.
It never puts a password in model state, command arguments or picker text. A
profile selection resolves and authenticates once, then opens the ordinary
session and metadata paths. A missing profile, malformed profile or unavailable
provider remains a safe diagnostic and never falls back to the default route.

**Independent Test**: Select a valid profile, an invalid profile and a profile
whose provider is unavailable. Assert that each selection uses the named route,
that failures do not open a password prompt for a cloud route, and that no
secret-shaped value appears in debug or rendered picker data.

## Acceptance Scenarios

1. **Given** named profiles and no explicit target, **when** the interactive
   client starts, **then** it opens a connection picker before resolving or
   authenticating a target.
2. **Given** the picker, **when** the user types part of a profile name,
   description or safe target detail, **then** subsequence matching narrows the
   list without changing configuration or contacting PostgreSQL.
3. **Given** a valid profile, **when** it is chosen, **then** the existing
   resolver receives that profile name, the existing auth boundary is used, and
   one ordinary connection attempt begins.
4. **Given** the picker, **when** "Use default connection settings" is chosen,
   **then** the existing environment, service-file and default precedence is
   used without silently selecting a profile.
5. **Given** a connected session, **when** a different profile is chosen,
   **then** the session state says `Connecting`, old result and metadata state
   are cleared, the editor text is retained, and no old async metadata answer
   can repopulate the new session.
6. **Given** a profile with an invalid field or unavailable provider, **when** it
   is chosen, **then** the selection fails with a safe actionable diagnostic and
   does not fall back to another profile or the default route.
7. **Given** a picker row, **when** it is rendered in ASCII, no-colour,
   compact or reduced-motion presentation, **then** environment, transport,
   read-only, provider and invalid-state meaning remain in words.
8. **Given** a profile containing a secret-shaped unknown field, **when** the
   picker is built or debug-rendered, **then** only safe validation wording is
   retained and the value is never copied into the model or UI.

## Edge Cases

- No profiles exist: the current automatic connection behaviour remains
  unchanged, because there is nothing useful to choose.
- A profile table exists but is empty: the default route remains available and
  no empty modal is forced on the user.
- A profile omits host, port, database, role, TLS mode or environment: the row
  says the value comes from environment, service-file or built-in defaults
  rather than inventing a resolved value.
- A profile description or known value contains control characters: rendering
  passes through the existing display sanitiser.
- A profile name is duplicated by case or contains unusual punctuation:
  configuration parsing and the existing map semantics remain authoritative;
  the picker never creates a second interpretation.
- The connection fails after a profile is selected for want of a password: the
  existing password prompt may ask only for a non-cloud route, and the retry
  uses the already resolved profile target.
- The user opens the picker while a query, plan, metadata load or confirmation
  is active: existing modal and busy-state precedence prevents a second
  connection from being started.
- A previous connection or metadata task finishes after a switch: generation
  checks discard stale session, tree, completion and metadata messages.

## Requirements

### Functional Requirements

- **FR-2001**: The interactive client MUST expose a searchable connection
  picker when named profiles exist and no explicit connection route was given.
- **FR-2002**: The picker MUST offer every configured profile and a clearly
  labelled default-route choice, in deterministic configuration order.
- **FR-2003**: A picker row MUST show only safe profile facts: name, optional
  description, configured location hints, environment, TLS mode, read-only
  posture, provider name and configuration validity.
- **FR-2004**: The picker MUST be reachable from the command palette and the
  `Ctrl+K n` chord; printable editor input MUST remain unchanged.
- **FR-2005**: Choosing a profile MUST invoke the existing profile resolver and
  authentication boundary exactly once and MUST never merge it with another
  target or silently fall back.
- **FR-2006**: Choosing the default route MUST use the existing precedence for
  command-line, service-file, environment and built-in defaults.
- **FR-2007**: Switching connections MUST clear server-derived result, plan,
  object-tree, completion, error-location and pending connection state while
  retaining the editor buffer and local reading preferences.
- **FR-2008**: A connection switch MUST prevent stale asynchronous work from a
  prior connection from changing the new session's model.
- **FR-2009**: The normal connection, metadata, password-prompt, cloud-auth and
  connection-trust surfaces MUST remain the single runtime paths after a
  selection.
- **FR-2010**: Picker search and opening MUST be presentation-only and MUST NOT
  read metadata, run SQL, write history, write files, refresh credentials or
  contact PostgreSQL.
- **FR-2011**: Invalid profiles MUST remain identifiable and produce an
  actionable refusal when chosen; they MUST NOT be silently omitted or treated
  as the default route.
- **FR-2012**: Empty profile configuration MUST preserve the existing startup
  route and MUST NOT force an empty picker.

### Security and Compatibility Requirements

- **SEC-2001**: Passwords, cloud tokens, unknown profile values and raw
  credentials MUST NOT enter the picker summary, application model, effect
  debug output, diagnostics or rendered text.
- **SEC-2002**: Profile selection MUST preserve the existing rule that a cloud
  provider failure never turns into a password prompt.
- **SEC-2003**: A switch MUST not weaken read-only or environment classification
  rules; the selected profile is passed through the same resolver as the CLI.
- **SEC-2004**: Existing explicit target, `--profile`, plain mode, scripted
  query, history, export and machine-output routes MUST remain unchanged.
- **SEC-2005**: Safe picker summaries MUST be bounded enough for narrow
  terminals and MUST use the existing display sanitisation boundary.

## Key Entities

- **Connection profile summary**: Value-free, ephemeral display data derived
  from known profile fields; it is not a second configuration source.
- **Connection choice**: Either one profile name or the default route. The
  reducer carries only the choice name, never a resolved target or credential.
- **Picker state**: The existing searchable palette widget with a connections
  purpose, deterministic entries and explicit selection semantics.
- **Connection generation**: A runtime identity used to discard late results
  from a connection that was replaced.

## Success Criteria

- **SC-2001**: With valid named profiles and no explicit route, the first
  rendered frame shows a picker and starts zero database or provider operations
  before selection.
- **SC-2002**: A profile can be found by name, description or safe target detail
  through subsequence search and selected with the keyboard in one action.
- **SC-2003**: A profile selection produces one resolver/authentication path and
  one ordinary connection attempt, with no fallback on failure.
- **SC-2004**: A connected-session switch retains the editor buffer and local
  preferences while clearing all server-derived state and rejecting stale
  async answers.
- **SC-2005**: ASCII, no-colour, compact and reduced-motion rendering retains
  the words `environment`, `TLS`, `read-only`, `provider`, `default` and
  `configuration` where those states apply.
- **SC-2006**: Adversarial profiles containing password-shaped unknown fields
  produce no secret value in summaries, debug output or rendered picker text.
- **SC-2007**: The locked verifier and focused picker, reducer, runtime,
  documentation and CLI contract checks pass, with live database evidence
  reported separately from local picker evidence.

## Assumptions

- Profiles remain non-secret configuration and do not gain passwords, passfile
  paths or credential-store references in this slice.
- The existing `Palette` widget is the right terminal primitive: connection
  choice gains a purpose and safe entries rather than a second search control.
- The default route is a choice only when profiles exist; without profiles the
  current direct startup route is less surprising.
- A switch is allowed only when no query or confirmation is active. The runtime
  generation boundary handles already-running metadata and connection tasks.
- The first version does not edit, create, delete or persist profiles from the
  picker. Configuration remains the source of truth.
