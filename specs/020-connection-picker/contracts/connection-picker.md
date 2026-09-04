# Contract: Connection picker

## Startup

- Profiles are considered for the interactive client only when the user did
  not give a target, `--profile`, host, port, database, user, TLS mode,
  environment, read-only flag or cloud provider.
- When at least one profile exists, the first rendered model has a
  `Connections` palette and no session or provider task has started.
- The palette contains `Use default connection settings` followed by profile
  rows in configuration order.
- With no profiles, existing automatic startup resolution is unchanged.

## Picker rows

- Rows expose name, configured location/database/user hints, description,
  environment, TLS wording, read-only wording, provider name and invalid
  configuration wording.
- Missing values are described as coming from environment, service-file or
  defaults. The picker never claims those values were resolved until a target
  has been selected.
- Unknown profile values, including secret-shaped ones, are not copied into
  summary state or rendered text.

## Selection

- `Ctrl+K n`, the command palette's Choose a connection action, and the
  connection picker Enter action all use the same reducer transition.
- The reducer emits `Effect::ConnectProfile { profile }`, where `profile` is
  only the selected name or `None` for defaults.
- The runtime calls the existing profile resolver and cloud authentication
  boundary. It never combines the selection with another target and never
  falls back after failure.
- A switch clears server-derived results, plans, metadata, completion,
  diagnostics and pending connection state, but retains editor text and local
  result-reading preferences.
- A switch is unavailable while a query or confirmation is active.

## Output and privacy

- Opening or searching the picker produces no SQL, network request, metadata
  read, history entry, file write, clipboard write or telemetry.
- Passwords and cloud tokens remain outside the model and picker. A cloud
  authentication failure never opens the password prompt.
- Picker wording passes through the same sanitisation boundary as other UI text
  and retains meaning in ASCII, no-colour, compact and reduced-motion modes.
- Late messages from an earlier connection generation are discarded before they
  can populate session, tree, completion or metadata state.
