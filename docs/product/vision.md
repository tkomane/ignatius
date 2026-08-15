# Vision

## The problem

There are two kinds of PostgreSQL client and neither is the one you want when the
database is real.

`psql` is the reference implementation of client behaviour. It is complete,
scriptable, and always available. It is also a line-oriented REPL where a wide
result is a wall of text, where the only thing telling you which database you are
about to run `DELETE` against is your shell prompt, and where a mistyped column
gives you an error and nothing to do about it.

The full-screen clients are pleasant. They give you a grid, a buffer, and
navigation. Most of them are multi-database by design, which caps how deep the
PostgreSQL semantics can go, and almost none of them tells you what protection
your connection actually has, as distinct from what you asked for.

So people use `psql` for the connections that matter and something nicer for the
rest, which means the tool they are least fluent in is the one they use under the
most pressure.

## What this is

A terminal-native PostgreSQL workbench: a full-screen client that is pleasant
enough for daily exploration, and honest enough to point at production, in one
small local binary that is also a scriptable CLI.

The emotional target is calm confidence. Oriented, in control, slightly
delighted. Never noisy, cryptic, brittle, or smug.

## What makes it different

Not the grid, and not the editor. Harlequin and Rainfrog already do those well.
The difference is what the tool tells you about itself:

- **The transport state is the server's answer, not yours.** Read back from
  `pg_stat_ssl`. `require` and `verify-full` are never described identically,
  because they are not the same guarantee.
- **An option it cannot honour fails.** Silently ignoring `sslrootcert` gives you
  less protection than you asked for while looking like success. That is the one
  failure mode a client must never have.
- **Production is a word you declared**, not a colour inferred from a host name,
  and it survives with colour switched off.
- **Uncertainty is stated.** `Cancellation requested` until the server confirms.
  `Connection lost - query outcome unknown` when that is the truth.
- **One binary for exploring and automating**, with documented exit codes, so the
  thing you learn is the thing you script.

## What it will not become

A generic multi-database client. A migration framework. An admin console. A
visual schema editor. A BI tool. A place that sends your schema to an AI
provider. A plugin marketplace. Each of those is a different product, and taking
one on would cost the focus that makes this one worth using.

## How success will be judged

Not by stars. By whether a senior engineer reaches for this instead of `psql`
when the database is production-adjacent, and does not feel they gave anything up
to do so.
