# Feature Specification: Result Analytics & Rich Exports (055)

**Feature Branch**: `feat/m13-crud-plandiff-analytics`
**Created**: 2026-11-18
**Status**: Phase 1 Implemented (Library Module & Sidecar Tests)
**Authority**: Roadmap Milestone 13 (M13: Live Data CRUD, Plan Diff & Analytics)

## 1. Context and Problem Statement

After running a SQL query, engineers routinely want a quick statistical profile of
the returned columns (NULL rate, distinct cardinality, min/max, mean/stddev,
`p50`/`p95` percentiles, distribution shape, and top frequent values) and the
ability to share or archive results in rich formats beyond plain CSV/JSON:
a self-contained interactive-style HTML report and a columnar Apache Parquet file
for downstream DuckDB, Polars, PyArrow, or Spark analysis.

Running extra `COUNT(DISTINCT ...)` or `percentile_cont(...)` queries behind the
user's back would violate Ignatius's core truthfulness and zero-replay principles
(Constitution I, III, IV), because re-querying a table could return different data
or incur unexpected load. Likewise, adding heavy C/C++ or multi-crate Parquet/Arrow
dependencies would bloat builds and supply-chain surface.

Feature 055 introduces a pure, zero-query analytics and rich-export engine in
`src/analytics/mod.rs`:

1. Zero-query single-pass profiling over retained `ResultSet` rows with
   8-bucket histograms, ASCII and optional Unicode sparklines, and deterministic
   table/plain/JSON renderers.
2. Self-contained Field Industrial HTML5 report rendering with strict HTML entity
   escaping and zero external assets or scripts.
3. Pure-Rust, zero-dependency, streaming Apache Parquet binary writer using an
   internal Thrift `TCompactProtocol` encoder and `RLE`/bit-packed definition
   level framing.

## 2. User Scenarios

### Scenario 1: Zero-query column profiling and distribution sparklines

An operator inspects a retained `ResultSet` in the terminal or CLI without
sending any additional SQL query to PostgreSQL. Every column reports total
retained rows, `null_count`, `empty_count`, `non_null_count`, `null_pct`,
bounded `distinct_count` (`distinct_capped` flagged when exceeding
`MAX_DISTINCT_TRACKED`), `min_value`, `max_value`, numeric `mean`/`stddev`/`p50`/
`p95` when numeric values are present, an 8-bucket histogram (`histogram_counts`),
strictly plain-ASCII `sparkline_ascii` (plus optional `sparkline_unicode`), and
up to 5 `top_values` ordered by frequency. If `ResultSet` was truncated at the
client row cap or retained-result limit, `ResultAnalytics.truncated` and the
rendered summaries state that clearly.

### Scenario 2: Exporting a self-contained Field Industrial HTML report

An engineer exports query results as an offline HTML5 report (`render_html_report`).
The document uses the Field Industrial palette (`#16181E` matte slate, `#1C1F27`
surface, `#232732` card, `#F4F3EF` warm bone text, `#FF6A00` signal orange
accent, `#FBBF24` phosphor amber NULL badge, `#24C992` alpine sage stats),
displays KPI summary cards, truncation warnings when `set.is_truncated()`,
column profiles with inline CSS histogram bars, and a scrollable result table
that cleanly distinguishes SQL NULL (`[null]`), empty string (`""`), and literal
`"NULL"` text. All titles, headers, metadata, and cell contents are strictly
HTML-entity escaped so hostile values such as `<script>alert(1)</script>` cannot
execute.

### Scenario 3: Streaming Apache Parquet binary export

A user exports a `ResultSet` to `.parquet` (`write_parquet` /
`export_parquet_bytes`). Columns are streamed one `ColumnChunk` at a time to
any `std::io::Write` sink (such as `Export::writer()`) without external crate
dependencies, producing a spec-compliant Apache Parquet container (`PAR1` header,
uncompressed `DATA_PAGE` with `RLE`/bit-packed `OPTIONAL` definition levels and
`PLAIN`-encoded `BOOLEAN`, `INT64`, `DOUBLE`, or `BYTE_ARRAY` / `UTF8` values,
Thrift `TCompactProtocol` `FileMetaData` footer, 4-byte little-endian footer
length, and trailing `PAR1` magic). Empty (`0`-row) `ResultSet` inputs produce a
valid 0-row Parquet container preserving the column schema.

## 3. Functional Requirements

- **FR-5501**: `src/analytics/mod.rs` defines `HISTOGRAM_BUCKETS = 8` and
  `MAX_DISTINCT_TRACKED = 10_000`.
- **FR-5502**: `ColumnProfile` captures `name`, `type_name`, `total_rows`,
  `null_count`, `empty_count`, `non_null_count`, `null_pct`, `distinct_count`,
  `distinct_capped`, `min_value`, `max_value`, `numeric_count`, `mean`,
  `stddev`, `p50`, `p95`, `histogram_counts`, `sparkline_ascii`,
  `sparkline_unicode`, and `top_values` (up to 5 entries).
- **FR-5503**: `ResultAnalytics` captures `row_count`, `rows_seen`, `truncated`,
  `column_count`, and `columns`, and implements `render_table()`,
  `render_table_with_glyphs(unicode)`, `render_plain()`, and `render_json()`.
- **FR-5504**: `profile_result_set(set: &ResultSet) -> ResultAnalytics` executes
  zero SQL or network operations, handles `ResultRetention::Unavailable` safely,
  distinguishes `Cell::Null`, `Cell::Text("")`, and `Cell::Text("NULL")`, uses
  Welford's algorithm for mean/population stddev and linear interpolation for
  `p50`/`p95`, builds an 8-bucket value histogram for numeric columns or an
  8-bucket string-length histogram for non-numeric columns, and generates
  8-character sparklines using ` .:-=+*#%` (plain ASCII) and `▁▂▃▄▅▆▇█`
  (Unicode).
- **FR-5505**: `render_html_report(set: &ResultSet, title: &str, analytics: &ResultAnalytics) -> String`
  produces a self-contained HTML5 document with inline CSS in the Field
  Industrial palette, zero external network references, zero `<script>` tags,
  truncation warning disclosure, KPI summary, profile table, and scrollable data
  table.
- **FR-5506**: Strict HTML entity escaping (`&`, `<`, `>`, `"`, `'`) is applied
  to `title`, column names, column types, statistics, truncation labels, and
  every table cell.
- **FR-5507**: `write_parquet<W: std::io::Write>(set: &ResultSet, out: &mut W) -> std::io::Result<()>`
  and `export_parquet_bytes(set: &ResultSet) -> std::io::Result<Vec<u8>>` write
  valid Apache Parquet containers (`PAR1` header/footer magic, 4-byte LE footer
  length, Thrift `TCompactProtocol` `PageHeader` and `FileMetaData`, `OPTIONAL`
  definition levels with `RLE`/bit-packed framing, and `PLAIN` values).
- **FR-5508**: `write_parquet` maps described PostgreSQL integer columns
  (`int2`/`int4`/`int8`/`smallint`/`integer`/`bigint`/`oid`) to `INT64`, float
  columns (`float4`/`float8`/`real`/`double precision`) to `DOUBLE`, boolean
  columns (`bool`/`boolean`) to `BOOLEAN` when all non-null cells parse cleanly,
  and falls back to `BYTE_ARRAY` (`ConvertedType::UTF8` + `LogicalType::STRING`)
  for text, numeric/decimal, temporal, and untyped columns.
- **FR-5509**: `write_parquet` refuses `ResultRetention::Unavailable` with
  `io::ErrorKind::InvalidInput` and writes a valid 0-row Parquet container when
  `set.rows.is_empty()` and `set.is_available()`.

## 4. Non-Functional Requirements

- Pure Rust with `#![deny(unsafe_code)]`, zero external crate additions, and zero
  network, database, filesystem, or terminal dependencies.
- `render_table()` and `render_plain()` emit strictly plain ASCII output;
  `sparkline_ascii` contains only ASCII characters.
- `src/analytics/tests.rs` holds all unit tests in a sidecar file (CodeQL rule).
- Zero clippy warnings (`#[warn(clippy::all)]`) and `rustfmt --edition 2024`
  cleanliness.

## 5. Out of Scope

- Sending follow-up `SELECT` / `ANALYZE` queries to PostgreSQL for unretained
  rows.
- Dictionary encoding, Snappy/Zstd/LZ4 page compression, or nested Parquet
  group/list columns in the built-in Parquet writer.
- CLI flag wiring (`--format html|parquet`, `ignatius profile`) and TUI overlay
  integration (owned by the coordinator in Phase 2).
