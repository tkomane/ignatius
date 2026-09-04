# Research: Connection picker

## Existing repository facts

1. `Config.profiles` is a deterministic `BTreeMap<String, Profile>` and
   `Profile` already separates safe known fields from an `other` map that is
   refused by `Profile::validate`. The picker can copy only known fields and
   leave unknown values behind.
2. `resolve_target_and_profile` already owns profile precedence, `@name`,
   `--profile`, unknown-profile wording, environment classification and the
   read-only union. A picker must submit a profile name to that function rather
   than construct a target itself.
3. `authenticate_target` and `connection::cloud::authenticate` are the existing
   credential boundaries. Picker selection must not duplicate provider lookup,
   token handling or transport checks.
4. `Palette` already provides subsequence search, deterministic ordering,
   keyboard navigation, modal precedence and compact rendering. A new
   `Connections` purpose and typed selection commands are smaller than a second
   picker widget.
5. `ConnectionTarget` deliberately is not `Clone` because it may hold a
   password. Runtime state must therefore hold an `Arc<ConnectionTarget>` only
   outside the application model and move resolved targets through the existing
   connection task boundary.
6. Metadata and completion responses already carry request identities, but the
   root schema and metadata-connection messages do not carry a connection
   identity. A runtime connection generation is required to prevent old session
   messages from reaching a newly selected profile.

## Decisions

### Reuse the palette widget

The picker is a palette purpose, not a new modal type. This preserves the
existing search scoring, arrow-key behavior, Enter selection, Escape dismissal,
ASCII/no-colour rendering and command-palette discoverability.

### Default route is an explicit row

The picker appears only when profiles exist. It therefore always offers a
non-profile escape hatch named `Use default connection settings`, which keeps
environment and service-file users from being trapped in profile configuration.

### Summaries are allowlisted

Known profile fields are copied into a small display summary. `other` is never
copied, even though it may contain a password-shaped value. Invalidity is
represented by a safe headline only. Display text still passes through the
existing renderer sanitiser.

### Runtime owns target preparation

The reducer emits only `ConnectProfile { profile }`. The interactive runtime
resolves the profile, obtains a cloud credential when named, stores the resolved
target only in its connection slot, and starts the ordinary session task. This
keeps secrets out of `Model` and prevents a second resolution algorithm.

### Generation guards switchable async work

Selecting a connection increments a runtime generation, clears session slots,
and tags all new connection and metadata tasks. A task checks the generation
while holding its destination lock before writing or sending a message. Old
responses are dropped rather than displayed as facts about the new target.

## Rejected alternatives

- **Always auto-connect and show a picker after connection**: contacts the
  server before the person chooses and does not satisfy the start-up promise.
- **Put `ConnectionTarget` in `Model`**: makes a secret-bearing state object
  easy to debug, clone or render accidentally.
- **Resolve the profile in the reducer**: violates the pure reducer boundary and
  would make provider execution or file/environment reads part of UI state
  transitions.
- **Add a separate profile editor now**: increases persistence and validation
  scope without improving the first connection choice.
