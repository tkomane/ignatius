# Research: Schema-aware completion

## Existing seams

- `src/query/statements.rs` already owns statement boundaries and the safe
  single-, double-, dollar-quoted, line-comment, and block-comment skips.
- `src/query/highlight.rs` already walks the same byte-oriented SQL and produces
  token spans. Completion must reuse it for literal/comment exclusion rather
  than create a second lexer.
- `src/postgres/metadata.rs` owns catalogue reads, `quote_identifier`, object
  kinds, schema summaries, relation objects, and columns. `src/cli/interactive.rs`
  already opens a read-only metadata session and loads the object tree through an
  effect.
- `src/app/update.rs` is a pure reducer. Async responses carry request or job
  identity where a late result could otherwise overwrite newer state.
- `src/app/editor.rs` stores whole-buffer snapshots and already distinguishes
  replacement edits. A completion accept can therefore be one normal
  non-coalesced range replacement and one undo step.
- `src/cli/plain.rs` is line-oriented and treats a leading backslash as a
  meta-command only when no SQL statement is being accumulated. This gives an
  explicit completion command a safe, discoverable place without changing SQL
  accumulation.

## Catalogue query decision

Load one snapshot on initial object-tree load and on explicit reload. The query
must be fixed SQL using `pg_catalog.pg_class`, `pg_catalog.pg_namespace`,
`pg_catalog.pg_attribute`, and `pg_catalog.pg_proc`, with system schemas omitted
using the same rule as the object tree. Return relation kind, schema, name,
readability, function return detail, and relation columns/type. No query is
issued for a typed character. A single snapshot also means candidates do not
change underneath a person because a list was refreshed on every key.

The snapshot retains the rows read from the server without silently dropping
database objects; the visible candidate menu is bounded. Candidate matching
reports both the number matched and the true total in the snapshot. The catalogue remains a read-only
metadata operation and is allowed to fail independently of the object tree: the
UI still offers keywords and says why objects are unavailable.

## Scope and ranking decision

The first version uses prefix matching, case-insensitive for bare words, with no
fuzzy matching. The analyzer scans tokens before the cursor tolerantly rather
than requiring a complete parse, which keeps useful results when a statement has
an error earlier in the buffer. It recognizes:

- a relation position after `FROM`, `JOIN`, `UPDATE`, `INTO`, or `REFERENCES`;
- a schema-qualified position such as `public.`;
- an alias-qualified position such as `o.` after `FROM orders o`;
- relation columns in ordinary expression positions when relations are in
  scope;
- CTE names declared by `WITH`, and subquery aliases without inventing columns;
- no candidates inside strings, dollar-quoted bodies, or comments.

Candidates sort by prefix match, then kind priority, then case-insensitive
display name, while retaining schema/source/type detail. The quoted insertion
text is computed from the exact stored name with the existing identifier quoting
rule, including names containing quotes, spaces, reserved words, or mixed case.

## Interaction decision

Automatic completion opens only when enabled and a two-or-more-character typed
prefix has matching candidates. It stays out of the way when the prefix already
is one exact, unambiguous candidate. `Ctrl+Space` explicitly opens the list even
with an empty prefix, while the catalogue is loading, or while automatic
completion is off.
The menu is a normal reducer mode: printable keys edit and refresh it, arrows
move selection, Enter accepts the selected candidate, and Esc dismisses it.
No candidate is pre-inserted and no key not mapped to acceptance causes an
insertion. The popup is anchored near the editor caret in both full and compact
layouts and has no animation, because it is frequent keyboard feedback rather
than an attention-seeking transition.

Plain mode does not pretend to have a cursor popup. `\complete [prefix]` is
explicit, prints an ASCII list with number, name, kind, schema, and type/detail,
then accepts one following `\use <number|exact-name>` command. A cancelled or
invalid choice leaves the SQL input untouched. The same analyzer and quoted
replacement function are used by both modes.

## Configuration decision

`[ui] completion = true` controls automatic menus only. It is additive and can
be omitted from existing files because serde defaults it to true. Explicit
`Ctrl+Space` remains available when false, so a person who disables popups does
not lose the discoverable feature entirely.

## Verification decision

Pure tests prove scope, ordering, quoting, literals/comments, CTEs, aliases,
bounded counts, and syntax-error tolerance. Reducer/editor tests prove one-step
undo, dismissal identity, stale catalog handling, and the automatic-off
property. CLI tests prove the plain transcript stays ASCII and does not run a
statement while completing. PostgreSQL tests prove the snapshot is read from a
real disposable server and that hostile identifiers remain safe to accept and
execute. Full release verification remains distinct from these focused gates.
