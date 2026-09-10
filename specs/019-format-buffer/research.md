# Research: Format the SQL buffer

## Decision: Use a conservative lexical formatter

The formatter will scan PostgreSQL lexical boundaries rather than introduce a
generic SQL parser or invoke an external process. The repository already owns
the quote, nested-comment, dollar-body, and statement-boundary rules in
`query::statements`; the formatter reuses those helpers and preserves each
protected slice exactly.

Rationale: formatting is an editor convenience, not a second SQL execution
engine. A parser dependency would add dialect and maintenance risk, while a
generic formatter could rewrite PostgreSQL-specific syntax it does not
understand. Refusing ambiguous input is more truthful than guessing.

## Decision: Normalize layout, not SQL meaning

Outside protected regions, the formatter collapses incidental whitespace,
places major clauses and boolean predicates on readable lines, gives eligible
top-level comma lists two-space continuation indentation, and keeps operators,
casts, dots, brackets, and parameters lexically separated. It does not change
keyword spelling, identifier spelling, literal contents, comments, statement
order, or semicolons.

Rationale: the user asked for a more graphical editing experience, not a query
rewriter. A deterministic style with a small rule set is inspectable and
idempotent.

## Decision: Fail closed at lexical uncertainty and 1 MiB

Unterminated strings, quoted identifiers, dollar bodies, or block comments leave
the source unchanged and report a safe kind and source location. Source above
1 MiB of UTF-8 bytes is refused before a derived output is built. Empty and
comment-only buffers are also unchanged.

Rationale: a partly edited buffer is common in an editor. Formatting it by
guessing where a protected region ends could move or alter text the user did
not intend to touch. The bound keeps work and memory predictable.

## Decision: Map the cursor through token identity

The formatter keeps the byte span of every lexical token while it emits output.
If the source cursor is inside a token, the output cursor is the same byte
offset inside that unchanged token. A cursor in collapsed whitespace maps to the
nearest output boundary, and a cursor after the buffer maps to the formatted
end. Every returned cursor is clamped to a UTF-8 character boundary.

Rationale: line and column alone are not stable when clauses become lines. Token
identity is the smallest useful anchor and remains valid for quoted and Unicode
text because protected bytes are copied unchanged.

## Decision: Make formatting one editor edit

The reducer calls the pure formatter, applies the result as a whole-buffer
replacement through the existing editor history, then restores the mapped
cursor. A changed result creates one undo entry; unchanged and refused results
create none. A changed buffer uses the existing editor mutation path so old
server error locations are invalidated consistently.

## Decision: Keep capability parity in plain mode

Plain mode accepts `\\format` while SQL is being accumulated, prints the
formatted pending buffer to the message stream, and leaves it pending for a
later semicolon. It never places the formatted preview in result stdout and
never executes the buffer as part of formatting.

Rationale: `--plain` is the accessible, line-oriented form of the same client;
formatting must remain useful without a full-screen renderer.

## Decision: Expose both a common shortcut and a chord

The default direct shortcut is `Ctrl+Shift+F`, which is familiar to users of
graphical editors and can be configured through `format-buffer`. The existing
chord prefix also offers `Ctrl+K q` as a discoverable fallback. Both routes
resolve to the same reducer action and the command palette lists one command.

Rationale: terminals differ in which modified keys they deliver. A direct
shortcut reduces friction; a chord keeps the action visible alongside the rest
of the workbench controls.
