# Troubleshooting

## The terminal is broken after a crash

Run `reset`. The program installs a panic hook that restores the terminal before
printing anything, so this should not happen; if it does, it is a defect worth
reporting with the output of `ignatius version --verbose`.

## "the interactive client needs a terminal"

Standard output is redirected or piped. Use `ignatius query` for non-interactive
work; it is designed for exactly that.

## "Terminal too small"

The full layout needs 80x24. Below that a single pane is shown; below 40x8 only
the message. Resize, or use `ignatius query`.

## The connection fails and you cannot tell why

```bash
ignatius connect --check "postgres://user@host:5432/db"
```

It reports each stage separately: address resolution, TCP, PostgreSQL, TLS and
session facts. The first `FAIL` line is the real problem, and it carries a next
action. Later stages are skipped rather than repeating the same failure.

## "sslmode=verify-ca is not implemented"

It is deliberately refused rather than approximated, because approximating it
would mean claiming a verification that did not happen. Use `verify-full` for a
full identity check, or `require` to accept encryption without one. See ADR-0004.

## "security parameter(s) are not supported yet"

You passed something like `sslrootcert` or `channel_binding`. Ignoring it would
give you weaker protection than you asked for, so the connection fails instead.
`docs/support/compatibility.md` lists what is supported. Use `psql` for that
connection until the parameter is implemented.

## The connection says "TLS not negotiated"

The server did not offer TLS and your `sslmode` allowed that. The connection is
in the clear. Enable TLS on the server, or set `sslmode=require` or
`verify-full` so that an unencrypted connection fails instead of proceeding.

## "TLS state unknown"

The session could not read `pg_stat_ssl`, usually because the role lacks access.
The connection may or may not be encrypted; the program will not guess. Grant the
role access to `pg_stat_ssl` to get a definite answer.

## Results are truncated

The row cap was reached. The count shown is the true number of rows the server
sent; only what is held in memory is limited. Raise `query.max-buffered-rows` in
the configuration, or pass `--max-rows`. For large extracts, use `query` with a
machine format rather than the grid.

## A query is stuck

Ctrl+C asks the server to cancel. The status reads `Cancellation requested` until
the server confirms, because until then the statement may still be running.
Nothing is ever retried automatically. If cancellation cannot be delivered, an
administrator can use `pg_cancel_backend()` with the backend pid, which
`connect --check` prints.

## The configuration file will not load

```bash
ignatius config validate      # says what is wrong and where
ignatius config paths         # says which file it is reading
```

Every value is optional. Moving the file aside returns the program to built-in
defaults and loses nothing but your settings.

## Colour is missing or unwanted

`NO_COLOR` and `TERM=dumb` always win, whatever the configuration says. Use
`--color always` to force it on, `--plain` to turn off colour, box drawing and
decoration together, and `--ascii` to keep colour but drop Unicode borders.
