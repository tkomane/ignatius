# Plain mode contract

This is the user-visible contract for the opt-in line-oriented client.

## Invocation

```text
ignatius --plain connect [connection-target]
ignatius --plain
```

The target and connection options use the existing resolution and configuration
rules. The default invocation without `--plain` remains the full-screen client.

## Streams

| Stream | Carries |
| --- | --- |
| stdout | Result data and existing result formatting |
| stderr | Product greeting, connection summary, prompts, continuation prompts, notices, diagnostics, confirmations and outcome lines |

Neither stream may contain alternate-screen, raw-mode, cursor-addressing or
other terminal-control sequences. Plain-mode decoration and status markers are
ASCII and textual.

## Input and commands

| Input | Behaviour |
| --- | --- |
| SQL lines | Accumulate until the PostgreSQL-aware statement boundary is complete |
| `\\?` | Print the supported command and usage help |
| `\\c` | Print the current connection summary |
| `\\q` | Exit successfully |
| Ctrl+D or closed stdin | Exit successfully |
| Ctrl+C while executing | Ask the server to cancel once and report its confirmed outcome |
| Unknown `\\` command at statement start | Explain the unknown command and point to `\\?` |
| `\\` inside SQL | Remains SQL input |

## Safety and state

- A production-classified write is described before it is sent.
- A destructive production write requires the exact database name; a lesser
  write requires `yes`.
- Empty or incorrect confirmation sends no SQL and reports cancellation.
- The prompt includes the database, production classification when applicable
  and server-reported transaction state.
- A failed transaction names ROLLBACK as the recovery action.
- Connection loss is reported as an unknown query outcome where the existing
  execution contract requires it.
- Passwords and other secrets remain covered by the existing redaction contract.

## Exit meanings

Plain mode preserves the existing documented exit meanings: success, usage,
configuration, connection, authentication, TLS, query, cancellation,
interrupted export where applicable and internal failure. It does not introduce
a second exit-code vocabulary.
