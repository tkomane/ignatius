# The experience roadmap

**Written 2026-08-16**, from the owner's brief: the differentiator is that this
is the PostgreSQL client with the best experience anyone has used, that it
demands the least expertise and the least remembering, and that it brings as
much of a graphical client's help as a terminal can carry.

This document is the argument for what to build and in what order. Each item
becomes a specification of its own before any of it becomes code.

**Execution planning updated 2026-09-04:** the originating local worktree now
contains the experience through Feature 024. Those application changes have not
been merged by this planning-only integration. Use the [product roadmap](roadmap.md)
for milestone order, the [agent delivery plan](agent-delivery-plan.md) for
assignments and the [dated baseline](planning-baseline.md) for the source boundary.
This document retains the product rationale and original sequencing.
[Product acceptance](acceptance.md) defines how to test the promised reduction
in expertise and remembering.

## Current delivery

As of 2026-09-03, Feature 017, the connection trust surface, is implemented and
pushed to `main` in commit `9f80bef`. It makes the existing cloud-auth route
legible before a query: the command palette opens a read-only view of the target,
environment, server posture, observed TLS state, provider command summary,
credential lifetime caveat and provider remedy. Entra, AWS, Google Cloud and
configured providers share the same display contract, while tokens and provider
commands are never refreshed or executed by the view.

This is a cross-cutting trust and discoverability slice, not a replacement for
the ordered experience work below. The next step is W01 integration of the
existing local chain starting with Feature 012, rather than repeating its
implementation. Live cloud-account evidence is still separate and is recorded
in the compatibility matrix when available.

## What a graphical client is actually giving people

Not beauty. Four things, and they are worth naming separately because they cost
different amounts to reproduce in a terminal:

1. **It knows the schema, so the person does not have to.** Every table name,
   column name and function is offered rather than recalled. This is the largest
   single difference between writing SQL in DataGrip and writing it in `psql`,
   and it is worth more than everything else on this list put together.
2. **It puts the answer where the question was.** A syntax error is underlined
   in the statement, not printed as a character offset. A plan is a tree with the
   expensive node marked, not 40 lines of text.
3. **It makes state visible without being asked.** Which server, which database,
   which transaction, how many rows, how long, what type this column is. In a
   terminal client most of that is a command you have to know to run.
4. **It is discoverable.** Someone who has never opened it can find things by
   looking. In a terminal, capability that nobody can find does not exist - a
   lesson this project learned expensively when five documented chords turned
   out to be bound to nothing.

What a terminal gives back is speed, an SSH session, scriptability, and no mouse
requirement. The aim is to keep all of that and take the four above.

## What constrains the answer

The constitution is not decoration here; three principles decide real questions.

- **Delight must remain truthful (I).** A completion list that offers a column
  that does not exist, or that quietly goes stale after a migration, is worse
  than no completion list. Anything derived from the catalogue says how fresh
  it is.
- **Accessible meaning survives styling loss (VI).** Every icon has a word.
  Every colour has a word. `--plain` and a screen reader get the same
  capability, expressed differently, and the ASCII glyph tier is not a
  degradation to be tolerated but a tier to be designed.
- **Keyboard-first, never keyboard-only (V).** Mouse support is welcome and is
  never load-bearing. No workflow may assume Vim.

## The original order, and why

### 1. Completion that knows the schema - Feature 012

The largest gap and the one people notice in the first minute. Keywords,
schemas, tables, views, functions, and columns scoped to the tables actually in
the statement, including through aliases. Offered as you type, accepted with one
key, dismissed with another, never inserting anything you did not choose.

It is first because everything else on this list is an improvement to a session
that is already going, and this one changes whether the session gets going at
all. The catalogue it needs is already loaded for the object tree, over a
connection that already exists.

The hard part is not the list. It is knowing what is in scope at the cursor,
which means understanding the statement well enough to know that `o.` after
`FROM orders o` means the columns of `orders`. The lexer that colours the buffer
already tokenises it; this needs a little more structure than that and much less
than a parser.

### 2. Errors that point at the problem - Feature 013

PostgreSQL returns the character position of a syntax error and the client
currently prints it. Putting the cursor there and marking the token turns a
number into an answer. Cheap, and it removes a small tax paid on every mistake.

The same applies to a failed constraint: the server names the constraint, and
the client can say which table and column that is, because it has the catalogue.

### 3. A result grid you can actually work - Feature 014

Sort by a column without rewriting the query and re-running it. Hide columns
that are in the way. Widen one. See each column's type without asking. Freeze
the first column so a wide table stays readable.

This is where a graphical client feels effortless and a terminal client feels
like a printout. It is all local work on rows already retained, so it needs no
new protocol behaviour - and the count line already established the honest
pattern for saying what is being worked on: what is retained, of what was
returned.

### 4. Finding your way without being told - Feature 015

The discoverability item, and the one that decides whether "least expertise" is
true. A first run that explains itself. A hint line that changes with focus and
says what the useful keys are here. A palette that is the answer to "what can I
do" rather than a list of commands you already knew. An empty state that
suggests the next thing rather than sitting blank.

It is fourth rather than first only because the three above give it something
worth finding.

### 5. A plan you can read - Feature 016

`EXPLAIN` and `EXPLAIN ANALYZE` rendered as a tree, with estimated against
actual rows, time per node, and the expensive node marked in words as well as
colour. Sequential scans on large tables and misestimates by an order of
magnitude are the two things people are looking for, and both can be pointed at
directly.

### 6. The rest, in no fixed order

- **Format the buffer.** One key, PostgreSQL-aware, idempotent.
- **A connection picker**, so starting is choosing rather than typing a URI.
- **Copy and export in more than one shape**: CSV, JSON, Markdown, `INSERT`
  statements. Depends on ADR-0013, which is now decided.
- **Parameters prompted.** A statement with `:name` placeholders asks for the
  values instead of being edited each time.
- **Editing a cell to generate an `UPDATE`.** The highest-value item on this
  list and the one with the most ways to hurt somebody. It is a write, invented
  by the client, against data it did not choose - so it shows the statement it
  would run and does not run it until that is confirmed, and it is refused on a
  production-classified connection like every other write.

## What is deliberately not on the list

- **Entity-relationship diagrams and schema comparison.** Real features of real
  products, and neither survives contact with an 80-column terminal.
- **A second database.** Principle II. The optimisation for PostgreSQL is the
  product.
- **Anything that uploads a query, a result or a schema.** Principle III, and it
  is not negotiable for a convenience.

## How this is judged

Not by feature count. By whether someone who knows SQL and has never seen this
can connect, find a table, write a working query against it and get the rows out
without reading documentation - and by whether someone who does that daily stops
noticing the tool. Both are hand-verification questions, and neither is answered
by a test suite.
