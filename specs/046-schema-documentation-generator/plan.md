# Implementation Plan: Schema Documentation Generator (046)

## 1. Domain (`src/dbdoc/`)

- `SchemaDocument` (`version`, `database`, `schema`, `generated_at`,
  `relations`, `truncated`).
- `RelationDocument` (`schema`, `name`, `kind`, `comment`, `estimated_rows`,
  `rls_enabled`, `rls_forced`, `columns`, `indexes`, `constraints`,
  `policies`).
- `ColumnDocument` (`ordinal`, `name`, `data_type`, `nullable`, `default`,
  `primary_key`, `references`, `comment`).
- `IndexDocument`, `ConstraintDocument`, `PolicyDocument`.
- `render_markdown`, `render_json`, `render_mermaid`.
- Escaping helpers for Markdown table cells and Mermaid labels.
- Stable ordering and truncation accounting.

## 2. Catalog read

- Add `Session::schema_documentation(schema)` in `src/postgres/session.rs` or a
  dedicated `src/postgres/dbdoc.rs`.
- Read relations, columns, constraints, indexes, RLS and comments with
  parameterized catalog queries bounded by `LIMIT`. Use `obj_description` and
  `col_description`; do not read table data.
- Reuse the existing metadata error mapping. A missing schema is a usage error,
  not an internal failure.

## 3. CLI

- `Command::Docs(DocsArgs)`:
  - `target: Option<String>`
  - `--schema` default `public`
  - `--format markdown|json|mermaid` default `markdown`
  - `--output`, `--force`
  - connection options flattened as on other commands.
- `run_docs` resolves the target, connects read-only, reads the catalog, renders
  and writes atomically. `--format mermaid` bypasses the Markdown wrapper.
- Machine output has no colour or progress decoration.

## 4. Tests

- `src/dbdoc/tests.rs`: Markdown/JSON/Mermaid shape, escaping, ordering,
  empty schema, truncation wording.
- `tests/dbdoc_contract.rs`: live PostgreSQL catalog for a probe schema with a
  table, FK, index, check constraint, comment and RLS policy; CLI output,
  `--force`, JSON parse, Mermaid content.
- `tests/documentation_matches_the_build.rs`: README/CLI documentation parity.

## 5. Documentation

- README feature section, CLI block and roadmap.
- CHANGELOG, status.
- A short note that the document is generated from catalog metadata and omits
  row data.

## 6. Verification

- focused dbdoc tests and the live contract against plain and TLS PostgreSQL
- `cargo xtask verify`
