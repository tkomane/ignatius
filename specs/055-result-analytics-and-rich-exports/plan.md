# Implementation Plan: Result Analytics & Rich Exports (055)

## 1. Pure Domain & Rich Export Engine (`src/analytics/mod.rs`)

### Part A: Zero-Query Column Profiler & Sparklines
- **Constants**:
  - `HISTOGRAM_BUCKETS: usize = 8`
  - `MAX_DISTINCT_TRACKED: usize = 10_000`
- **Types**:
  - `ColumnProfile`: per-column cardinality, NULL/empty counts, min/max,
    numeric count, Welford mean/population stddev, interpolated `p50`/`p95`,
    8-bucket `histogram_counts`, `sparkline_ascii`, `sparkline_unicode`, and
    top-5 `(value, count)` pairs.
  - `ResultAnalytics`: dataset-level `row_count`, `rows_seen`, `truncated`,
    `column_count`, `columns`, plus `total_nulls()`, `render_table()`,
    `render_table_with_glyphs(unicode)`, `render_plain()`, and `render_json()`.
- **Algorithm**:
  - `profile_result_set(set: &ResultSet) -> ResultAnalytics` performs one pass
    over retained rows per column.
  - Respects `ResultSet::is_available()`: returns empty `columns` with
    `truncated = true` and `column_count = set.source_column_count()` when
    unavailable.
  - Distinguishes `Cell::Null` vs `Cell::Text("")` vs `Cell::Text("NULL")`
    explicitly (`null_count` vs `empty_count` vs non-null text).
  - Parses finite `f64` values when `column_type` is `None` or not an explicit
    PostgreSQL non-numeric type (`text`, `varchar`, `bool`, `date`, `timestamp`,
    `uuid`, `json`, `jsonb`, `bytea`, etc.).
  - Computes 8-bucket numeric histogram (or character-length histogram when
    `numeric_count == 0`) and maps counts into `" .:-=+*#%"` (ASCII) and
    `[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']` (Unicode).

### Part B: Field Industrial HTML5 Report Exporter
- `escape_html(input: &str) -> String`: escapes `&`, `<`, `>`, `"`, `'`.
- `render_html_report(set: &ResultSet, title: &str, analytics: &ResultAnalytics) -> String`:
  - Complete HTML5 document with inline `<style>` using Field Industrial tokens
    (`#16181E`, `#1C1F27`, `#232732`, `#3E465B`, `#F4F3EF`, `#FF6A00`,
    `#FBBF24`, `#24C992`) and monospace typography.
  - Zero external resources, zero `<script>` tags.
  - KPI header + explicit truncation banner if `set.is_truncated()`.
  - Column profile summary table with inline CSS percentage-height bar
    sparklines.
  - Scrollable result grid with sticky `<thead>`, distinguishing `[null]`,
    `"" (empty)`, and literal `"NULL"`.

### Part C: Streaming Pure-Rust Apache Parquet Binary Writer
- **Internal `CompactWriter`** (Thrift `TCompactProtocol`):
  - ZigZag varints for `i32` / `i64`, ULEB128 unsigned varints, struct field-ID
    delta stack (`struct_begin`, `struct_end`, `field_begin`), `list_begin`,
    `write_i32`, `write_i64`, `write_binary`, `write_string`.
- **Parquet Layout (`write_parquet` & `export_parquet_bytes`)**:
  - Writes `b"PAR1"` header magic.
  - Refuses `!set.is_available()` with `io::ErrorKind::InvalidInput`.
  - When `set.rows.is_empty()` or `set.columns.is_empty()`, writes 0 RowGroups.
  - When `set.rows.len() > 0` and `set.columns.len() > 0`, streams 1 `RowGroup`
    column-by-column:
    - Selects `BOOLEAN`, `INT64`, `DOUBLE`, or `BYTE_ARRAY` (`UTF8`/`STRING`)
      per column based on described PG type and clean cell parsing.
    - Encodes `OPTIONAL` definition levels (`bit_width = 1`) using 4-byte LE
      length prefix + RLE run (for uniform columns) or LSB-first bit-packed
      runs (chunked up to 1024 groups per run) for mixed columns.
    - Appends `PLAIN`-encoded non-null values.
    - Writes Thrift `PageHeader` (`DATA_PAGE`, `UNCOMPRESSED`) followed by the
      page payload directly to `out: &mut W`.
  - Serializes `FileMetaData` (`version = 1`, `SchemaElement` tree, `num_rows`,
    `RowGroup`/`ColumnChunk`/`ColumnMetaData`, `created_by`), writes it to `out`,
    followed by 4-byte LE footer length and `b"PAR1"`.

## 2. Sidecar Unit Tests (`src/analytics/tests.rs`)

- Zero-query profiling tests: NULL vs `""` vs `"NULL"`, numeric statistics
  (mean, population stddev, `p50`, `p95`), 8-bucket numeric & string-length
  histograms, ASCII & Unicode sparklines, `MAX_DISTINCT_TRACKED` cap, `top_values`
  determinism, truncation & unavailable `ResultSet` handling, and table/plain/JSON
  renderers.
- HTML report tests: standalone HTML5 structure, Field Industrial color palette,
  distinct `[null]` / empty / `"NULL"` rendering, truncation warning, and XSS
  payload escaping across `title`, column names, and cell values.
- Parquet writer tests: `PAR1` header/footer magic and LE footer length invariant,
  0-row schema-only container, `Unavailable` refusal, typed columns (`INT64`,
  `BOOLEAN`, `DOUBLE`, `BYTE_ARRAY`), Thrift `TCompactProtocol` tag & metadata
  decoding verification, and RLE/bit-packed definition level round-trip decoding
  with mixed NULLs.

## 3. Coordinator Integration (Deferred to Coordinator)

- Wire `pub mod analytics;` in `src/lib.rs`.
- Connect CLI export formats (`html`, `parquet`) and interactive TUI analytics
  overlay in `src/cli/*` and `src/app/*`.
