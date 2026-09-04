# Data model: Connection picker

## ConnectionProfileSummary

Ephemeral safe display data derived from one known `Profile`:

| Field | Meaning | Safety rule |
| --- | --- | --- |
| `name` | Configuration map key | Rendered through display sanitisation |
| `description` | Optional user description | Never treated as a credential; bounded in the row detail |
| `location` | Host and port hints | Uses only configured values and explicit fallback words |
| `database` | Configured database hint | Never resolves environment values here |
| `user` | Configured role hint | Never resolves environment values here |
| `transport` | Configured TLS mode or default wording | Does not claim observed TLS |
| `environment` | Classification or unclassified wording | Never inferred from host |
| `read_only` | Server read-only request | Preserves the profile's safety fact |
| `auth` | Provider name, when configured | Name only, never token or command output |
| `provider_presentation` | Safe trust-surface facts, when known | Contains no token or arbitrary argument vector |
| `validation` | Safe configuration issue, when invalid | Contains no unknown field value |

## ConnectionChoice

The reducer and palette carry only:

- `None`: use command-line, service-file, environment and built-in defaults;
- `Some(profile_name)`: resolve exactly this named profile.

No resolved host, password, token or target object enters the application
model or effect.

## Picker state

The existing `Palette` carries the picker state:

| Field | Value for connections |
| --- | --- |
| `purpose` | `Connections` |
| `entries` | One default choice followed by profile summaries |
| `query` | User's subsequence search |
| `selected` | Bounded row index |
| `context_note` | Default precedence and no-password reminder |

## Connection generation

The runtime maintains a monotonically increasing identity outside `Model`:

```text
generation N
    |
    +-- select profile/default --> increment to N+1, clear session slots
                                  start resolution/authentication
    +-- target ready ------------> start connection tasks tagged N+1
    +-- old task response -------> discard when tag != current generation
```

The generation is runtime coordination state, not product data and is never
rendered or persisted.

## State transitions

```text
startup with no profiles or explicit route
    -> resolve existing route -> Connecting -> Connected or Failed

startup with profiles and no explicit route
    -> Connections picker, no connection effect
    -> default choice --------> resolve existing default route
    -> profile choice --------> resolve named profile
    -> target/auth ready ------> Connecting -> Connected or Failed

connected session
    -> open picker ------------> Connections palette, no effect
    -> choose choice ----------> clear server-derived state, Connecting
    -> target/auth failure ----> Failed, no fallback
    -> connection success ----> Connected, metadata/completion reloads
```
