# Feature Specification: Structured Type and JSONB Inspector (059)

**Created** 2026-11-19

**Authority** Milestone 15, Feature 059

## Operator journeys

1. Select a retained result cell and open Cell Detail with Enter or Ctrl+K i.
   A described JSONB, array, range, PostGIS geometry/geography or pgvector
   value opens as a scrollable tree. Choose a node with Up/Down, inspect its
   path, switch to the intact server text with r, or use Left/Right to inspect
   an adjacent cell. Ordinary, SQL NULL and undescribed cells retain their
   existing view.
2. Select an existing node and press e. Ignatius offers a literal replacement
   using the node's type syntax. Enter first stages a reviewed UPDATE. A second
   explicit confirmation sends it once, only if the row, source column and
   observed value still match. Escape on either screen discards the edit.
3. Inspect a wide, deeply nested, malformed or unsupported value. Cell Detail
   states the limitation and preserves access to the server text. Only a
   complete supported tree can start a structured edit.

## Requirements

- **FR-5901** Derive type structure from PostgreSQL's aligned column
  description. Never infer a type from text contents. Verify that optional
  spatial/vector types belong to their extensions. Genuine arrays use their
  element's delimiter. Unavailable metadata leaves the raw view usable.
- **FR-5902** Show JSONB nested object keys and zero-based indices, PostgreSQL
  array dimensions and actual lower-bound indices, range endpoints and
  inclusivity, spatial type/SRID/dimensions and supported EWKB coordinates,
  and pgvector dense or sparse coordinates. Distinguish SQL NULL, JSON null,
  null array elements, quoted text, empty values and unbounded range ends.
- **FR-5903** Retain the raw view and state when structure or a shown label or
  preview was shortened. Structure parsing is bound to 64 KiB, 32 levels and
  512 nodes. All terminal content uses existing display sanitization and
  textual labels usable in ASCII and no-color modes.
- **FR-5904** Replace a selected JSON value, array element, range endpoint or
  vector coordinate while preserving bytes outside the selected value span
  before PostgreSQL applies its usual type canonicalization. Whole structured
  values can also be replaced with a PostgreSQL input literal. Spatial
  coordinates are inspectable and spatial edits replace the whole value.
  Arrays and ranges with user-defined scalar input/output support inspection
  and whole-value replacement. There is no local numeric coercion in the
  replacement path.
- **FR-5905** Reuse the existing live metadata lookup, direct-projection and
  complete primary-key requirements, sensitive parameter handling, SQL byte
  limit and two-step reviewed UPDATE. Structured edits are unavailable on
  production or read-only connections. Result job, source row and selected
  column must be current. At send time the statement matches the retained
  column's relation, attribute, type and typmod and compares the exact old
  server text. A missing or changed row/value can affect zero rows and is
  reported as such. An immediate valid primary key must still match the
  reviewed columns. PostgreSQL must use round-trippable float text and enable
  NULL array input for array edits when retaining the writable result and
  when sending the update. A failed edit does not rerun the source
  SELECT or retry.
- **FR-5906** Keep structure and replacement input in memory. Do not put
  result text, replacement, expanded SQL or key values into history or
  diagnostics. Preserve current COPY/export and ordinary cell-update behavior.
- **FR-5907** Exercise the feature on synthetic PostgreSQL via the existing
  integration target across versions 14-18. Exercise unavailable extension
  behavior when stock fixtures lack the optional extensions. Only claim live
  extension evidence on an extension-enabled fixture.

## Scope

This feature replaces existing nodes, including a whole document or array
root. Adding or deleting child nodes individually, writing through views or
derived columns, spatial reprojection, and new shortcuts or CLI flags are
outside the slice. PostgreSQL remains responsible for domain constraints,
numeric range, geometry validity and storage canonicalization.
