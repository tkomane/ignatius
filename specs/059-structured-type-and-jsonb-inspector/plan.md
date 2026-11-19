# Implementation Plan: Structured Type and JSONB Inspector (059)

## Architecture and trust boundaries

- **FR-5901** Attach aligned, budgeted structured metadata during
  `Session::describe_columns` for workbench results. Reuse driver type kind
  and column origin. Under the user SQL gate, probe relevant settings around
  single direct table projections, so a retained rounded result is inspect-only
  even if the settings later change. Ordinary CLI reads keep their current path.
  Where needed, read array element delimiters or extension membership in one
  bounded catalog query. The primary types and raw text result remain usable
  when this metadata is unavailable.
- **FR-5902, FR-5903** Parse the retained text in `src/structured/` with one
  bounded preorder tree and byte spans. Keep the result set as the only owner
  of the raw value. Derive labels and paths from exact source spans and
  sanitize for display. The pure reducer stores only display mode, a node
  index and the small edit intent carried through the existing metadata lookup.
- **FR-5904** Validate a replacement for the selected span and splice bytes.
  JSON syntax validation ignores numeric values during parsing to avoid
  conversion to binary floats. Wire and document the raw fallback. Inspect
  spatial EWKB using server bytes and expose whole-value replacement.
- **FR-5905, FR-5906** Reuse the existing update candidate, lookup, value prompt,
  review and sending effect. Add typed-origin predicates and the observed-text
  comparison to the same UPDATE as the primary-key predicate. Store the old
  value and the replacement through existing secret wrappers and bind at the
  current PostgreSQL execution boundary. Leave ordinary updates unchanged.
  Only the PostgreSQL executor performs I/O. No replay or background write.
- **FR-5907** Add parser/patch unit tests and narrow/ASCII visual-buffer and
  reducer tests. Extend `postgres_integration` with live losslessness and
  concurrency contracts. Verify with the workspace and PostgreSQL fixtures
  available for the candidate. Record native terminal and extension evidence
  separately when available.

The tree has fixed byte, depth and node ceilings so hostile text cannot drive
unbounded recursive work in the UI. The server text is preserved for export
and raw inspection. Source identity uses the existing post-execution
parse/describe contract, which matches the returned shape and never replays
the query. It has no claim of observing an intermediate change that had
already returned to the same state before the guarded UPDATE.

Set `IGNATIUS_TEST_PG_POSTGIS_URI` and `IGNATIUS_TEST_PG_VECTOR_URI` to
disposable servers with their respective extensions already enabled to run
the spatial and vector round-trip contracts. When absent, the stock
PostgreSQL suite reports an explicit skip for each optional fixture.
