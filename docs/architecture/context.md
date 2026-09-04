# Context

Reconciled on **2026-09-04** against the current local feature chain. This is
the runtime trust model; [release evidence](../operations/release.md) has a
separate build and distribution trust path. Implementation does not establish
live provider or terminal support; those claims live in the compatibility matrix.

```mermaid
flowchart LR
    %% Assumption: one local user selects the target and owns configuration.
    %% Provider programs and terminals have their own caches/retention outside Ignatius.
    %% Open evidence: native provider launch, per-cloud accounts, terminal clipboard acceptance.
    user(["Engineer at a terminal"])

    subgraph process["Ignatius process trust zone"]
        app["Single local binary<br/>TUI, plain mode and CLI"]
    end

    subgraph local["User-owned host zone, outside the process"]
        config[["Configuration, service/passfile<br/>controlled history and saved SQL"]]
        outputs[["Explicit exports<br/>opt-in redacted logs"]]
        provider["Configured provider executable<br/>its own sign-in/cache policy"]
        terminal["Terminal / SSH / multiplexer<br/>optional clipboard destination"]
    end

    subgraph remote["Remote trust zone"]
        pg[("PostgreSQL<br/>all server input untrusted")]
        identity["Cloud identity service<br/>contacted by provider tool"]
    end

    user -->|"keystrokes, arguments, piped SQL"| app
    app -->|"documented data streams and exit codes"| user
    config -->|"validated settings and credential routes"| app
    app -->|"explicit config/save or controlled history write"| config
    app -->|"atomic exports; SQL/row-free support logs"| outputs
    app -->|"validated argv, no shell; only when connecting"| provider
    provider -->|"secret token on captured stdout"| app
    provider -.->|"provider-owned authentication traffic"| identity
    app -->|"sanitised frames; confirmed opt-in OSC 52 write"| terminal
    app <==>|"query + read-only metadata sessions; TCP/TLS or Unix socket"| pg

    classDef untrusted stroke-dasharray: 4 4
    class pg untrusted
```

## What crosses each boundary

**User to program.** Keystrokes, command-line arguments, environment variables,
piped SQL. Arguments and environment are visible to other local processes, which
is why a password on the command line is documented as exposed rather than
treated as safe.

**Program to user.** Rendered frames, machine-format data on stdout, diagnostics
on stderr, and an exit code. Terminal-facing server text is escaped; machine
formats preserve data under their own encoding contract and carry no UI
decoration. The generated UPDATE review deliberately displays the bound
statement before confirmation; it must not enter logs or history as bound text.

**Program to disk.** Configuration is validated at startup and written
atomically by explicit commands. Saved SQL is an explicit file operation.
Interactive history is local, scoped, pausable and erasable; parameter values
are excluded and scripted queries are not recorded. Logs exist only when
requested and exclude SQL/row values. Result data is persisted only by explicit
export. Credentials can be read from user-managed routes but are never stored
by Ignatius. ADR-0011 rejects an OS credential store.

**Program to server.** The PostgreSQL wire protocol, over TCP or a Unix socket,
with the TLS policy decided by the client from the resolved `sslmode`. The
interactive metadata connection uses the same target and a server-enforced
read-only session, with a visible shared-connection fallback. Cancellation has
its own protocol connection. No query is silently retried or replayed.

**Program to provider.** A named cloud credential route executes a validated
argument vector and captures a secret token. Encryption eligibility is checked
before acquisition; opening trust details or searching the picker does not run
a provider. The external cloud tool may contact its identity service and own a
cache. Ignatius has no token cache or direct identity HTTP client. Describing
the complete workflow as having no non-database network activity would omit
that provider boundary. There is no telemetry, update check or query upload.

**Program to clipboard path.** One selected value can leave through an explicit,
confirmed, opt-in OSC 52 write. The terminal, SSH path or multiplexer can observe
or retain it. A successful write/flush proves bytes were sent, not acceptance;
the program never reads the clipboard. This is separate from ordinary display
escaping and from disk export.

**Server to program. This is the untrusted direction.** Values, column names,
notices and error text are all attacker-controlled if anyone can write to the
database. Everything arriving here is treated as hostile and escaped before it
reaches the terminal.
