# Structured cell detail

Select a cell in Results and press Enter or Ctrl+K i. When PostgreSQL provides
a supported type description, Cell Detail shows a structured tree. Use
Up/Down or PageUp/PageDown to select nodes, Left/Right to switch columns, and
r to switch between the tree and the original server text. The path below the
tree names the selected node. Shortened labels or previews end in an ellipsis.

- JSONB shows nested object keys and zero-based array indices. The tree keeps
  JSON null separate from SQL NULL. Replacements use JSON syntax, so a string
  needs double quotes.
- PostgreSQL arrays show the real subscripts, including non-one lower bounds.
  An unquoted NULL array element is distinct from the text `"NULL"`. Enter
  one PostgreSQL array element or replace the whole array at the root.
  Whitespace around elements and dimension markers is optional. To keep
  leading or trailing whitespace in a value, quote the element or escape
  the whitespace with a backslash.
- Ranges show empty ranges, inclusive/exclusive bounds and unbounded ends.
  Leave an endpoint replacement empty to make it unbounded. Select the root
  to replace the whole range and choose its brackets.
- pgvector dense `vector`, `halfvec` and `sparsevec` show coordinates when
  the type belongs to the vector extension. Sparse coordinates retain the
  indices from PostgreSQL. PostGIS `geometry` and `geography` show type,
  coordinates, dimensions and SRID from supported EWKB. Spatial coordinates
  are read-only. Select the root to supply a whole PostgreSQL spatial
  literal, such as EWKT or HEXEWKB.

The tree is limited to 64 KiB of input, 32 levels and 512 nodes. If it reaches
a limit or encounters an unsupported representation, the original retained
server text remains available. A truncated tree cannot start an edit.
Arrays and ranges with user-defined scalar input/output support inspection
and whole-value replacement. The generated SQL has a 16 KiB limit.

To edit, select a supported node and press e. Enter stages a generated
UPDATE. Review the SQL and confirm it separately to send it once. Escape
cancels. The source must be a direct table projection that includes every
primary-key column. Production-classified and read-only connections refuse
edits. The UPDATE requires an immediate live primary key, matching source
column identity and matching observed server value. It also checks
`extra_float_digits > 0` and, for arrays, `array_nulls = on` so an
unchanged value is not interpreted under lossy settings. A result retained
under lossy settings remains inspect-only. If conditions change during
review, PostgreSQL can report zero rows affected. Run the
SELECT explicitly to refresh your view. Ignatius never retries an edit.

Partial replacements keep unedited source text exactly as PostgreSQL sent it.
PostgreSQL still applies its standard input validation, constraints, dimension
rules and canonicalization. The raw value and the bound UPDATE are not
written to history by the inspector.

Reference formats checked on 2026-11-19. [PostgreSQL arrays](https://www.postgresql.org/docs/18/arrays.html#ARRAYS-IO),
[PostgreSQL ranges](https://www.postgresql.org/docs/18/rangetypes.html#RANGETYPES-IO),
[PostgreSQL JSON types](https://www.postgresql.org/docs/18/datatype-json.html),
[PostGIS HEXEWKB](https://postgis.net/docs/ST_AsHEXEWKB.html),
[pgvector](https://github.com/pgvector/pgvector).
