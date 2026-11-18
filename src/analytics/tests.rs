//! Sidecar unit tests for zero-query result analytics, Field Industrial HTML
//! reports, and streaming pure-Rust Apache Parquet export.

use super::*;
use crate::query::result::RetentionReason;

fn make_set(
    columns: Vec<&str>,
    types: Vec<Option<&str>>,
    rows: Vec<Vec<Cell>>,
    cap: usize,
) -> ResultSet {
    let mut set = ResultSet::with_column_types(
        columns.into_iter().map(str::to_owned).collect(),
        types.into_iter().map(|t| t.map(str::to_owned)).collect(),
        cap,
    );
    for row in rows {
        set.push(row);
    }
    set
}

#[test]
fn empty_result_set_profiles_cleanly() {
    let set = make_set(
        vec!["id", "label"],
        vec![Some("int4"), Some("text")],
        vec![],
        100,
    );
    let analytics = profile_result_set(&set);
    assert_eq!(analytics.row_count, 0);
    assert_eq!(analytics.rows_seen, 0);
    assert!(!analytics.truncated);
    assert_eq!(analytics.column_count, 2);
    assert_eq!(analytics.total_nulls(), 0);

    let col = &analytics.columns[0];
    assert_eq!(col.null_count, 0);
    assert_eq!(col.empty_count, 0);
    assert_eq!(col.non_null_count, 0);
    assert_eq!(col.null_pct, 0.0);
    assert_eq!(col.distinct_count, 0);
    assert!(!col.distinct_capped);
    assert_eq!(col.min_value, None);
    assert_eq!(col.max_value, None);
    assert_eq!(col.mean, None);
    assert_eq!(col.stddev, None);
    assert_eq!(col.p50, None);
    assert_eq!(col.p95, None);
    assert_eq!(col.histogram_counts, vec![0; HISTOGRAM_BUCKETS]);
    assert_eq!(col.sparkline_ascii, "        ");
    assert_eq!(col.sparkline_unicode, "        ");
}

#[test]
fn unavailable_result_set_reports_truncation_without_inventing_columns() {
    let mut set = ResultSet::unavailable(3, 100);
    set.push(vec![Cell::Null]);
    let analytics = profile_result_set(&set);
    assert_eq!(analytics.row_count, 0);
    assert_eq!(analytics.rows_seen, 1);
    assert!(analytics.truncated);
    assert_eq!(analytics.column_count, 3);
    assert!(analytics.columns.is_empty());

    let table = analytics.render_table();
    assert!(table.contains("truncated=true"));
    assert!(table.contains("(no columns available to profile)"));
    assert!(table.is_ascii());
}

#[test]
fn distinguishes_sql_null_empty_string_and_literal_null_text() {
    let set = make_set(
        vec!["val"],
        vec![Some("text")],
        vec![
            vec![Cell::Null],
            vec![Cell::Text(String::new())],
            vec![Cell::Text("NULL".to_owned())],
            vec![Cell::Text("alpha".to_owned())],
            vec![Cell::Text("alpha".to_owned())],
        ],
        100,
    );
    let analytics = profile_result_set(&set);
    assert_eq!(analytics.total_nulls(), 1);

    let col = &analytics.columns[0];
    assert_eq!(col.total_rows, 5);
    assert_eq!(col.null_count, 1);
    assert_eq!(col.empty_count, 1);
    assert_eq!(col.non_null_count, 4);
    assert!((col.null_pct - 20.0).abs() < 1e-9);
    assert_eq!(col.distinct_count, 3); // "", "NULL", "alpha"
    assert_eq!(col.numeric_count, 0);
    assert_eq!(col.min_value.as_deref(), Some(""));
    assert_eq!(col.max_value.as_deref(), Some("alpha"));
    assert_eq!(col.top_values[0], ("alpha".to_owned(), 2));
}

#[test]
fn computes_numeric_statistics_percentiles_and_numeric_minmax() {
    let rows = vec![
        vec![Cell::Text("2".to_owned())],
        vec![Cell::Text("10".to_owned())],
        vec![Cell::Text("20".to_owned())],
        vec![Cell::Text("30".to_owned())],
        vec![Cell::Text("40".to_owned())],
        vec![Cell::Null],
    ];
    let set = make_set(vec!["score"], vec![Some("int4")], rows, 100);
    let analytics = profile_result_set(&set);
    let col = &analytics.columns[0];

    assert_eq!(col.numeric_count, 5);
    assert_eq!(col.min_value.as_deref(), Some("2"));
    assert_eq!(col.max_value.as_deref(), Some("40"));

    // Mean of [2, 10, 20, 30, 40] = 20.4
    let mean = col.mean.expect("mean");
    assert!((mean - 20.4).abs() < 1e-9, "mean={mean}");

    // Population variance = ((18.4^2 + 10.4^2 + 0.4^2 + 9.6^2 + 19.6^2) / 5) = 184.64
    let expected_stddev = 184.64_f64.sqrt();
    let stddev = col.stddev.expect("stddev");
    assert!(
        (stddev - expected_stddev).abs() < 1e-9,
        "stddev={stddev} expected={expected_stddev}"
    );

    // p50 = index 2 = 20.0
    assert!((col.p50.expect("p50") - 20.0).abs() < 1e-9);
    // p95 = 0.95 * 4 = 3.8 => 30 + 0.8 * (40 - 30) = 38.0
    assert!((col.p95.expect("p95") - 38.0).abs() < 1e-9);

    assert_eq!(col.histogram_counts.len(), HISTOGRAM_BUCKETS);
    assert_eq!(col.histogram_counts.iter().sum::<usize>(), 5);
    assert_eq!(col.sparkline_ascii.len(), HISTOGRAM_BUCKETS);
    assert!(col.sparkline_ascii.is_ascii());
    assert!(col.sparkline_ascii.chars().all(|c| " .:-=+*#%".contains(c)));
    assert_eq!(col.sparkline_unicode.chars().count(), HISTOGRAM_BUCKETS);
    assert!(
        col.sparkline_unicode
            .chars()
            .all(|c| " ▁▂▃▄▅▆▇█".contains(c))
    );
}

#[test]
fn explicit_text_column_with_digit_strings_uses_string_length_histogram() {
    let set = make_set(
        vec!["zip"],
        vec![Some("text")],
        vec![
            vec![Cell::Text("1".to_owned())],
            vec![Cell::Text("12".to_owned())],
            vec![Cell::Text("12345678".to_owned())],
        ],
        100,
    );
    let analytics = profile_result_set(&set);
    let col = &analytics.columns[0];
    assert_eq!(col.numeric_count, 0);
    assert_eq!(col.mean, None);
    assert_eq!(col.histogram_counts.iter().sum::<usize>(), 3);
    assert!(col.histogram_counts[0] >= 1);
    assert!(col.histogram_counts[HISTOGRAM_BUCKETS - 1] >= 1);
}

#[test]
fn caps_distinct_tracking_at_max_distinct_tracked_and_limits_top_values() {
    let mut rows = Vec::with_capacity(MAX_DISTINCT_TRACKED + 15);
    for i in 0..(MAX_DISTINCT_TRACKED + 10) {
        rows.push(vec![Cell::Text(format!("val_{i:05}"))]);
    }
    // Repeat val_00001 twice more so it leads top_values.
    rows.push(vec![Cell::Text("val_00001".to_owned())]);
    rows.push(vec![Cell::Text("val_00001".to_owned())]);

    let set = make_set(vec!["k"], vec![None], rows, MAX_DISTINCT_TRACKED + 100);
    let analytics = profile_result_set(&set);
    let col = &analytics.columns[0];

    assert_eq!(col.distinct_count, MAX_DISTINCT_TRACKED);
    assert!(col.distinct_capped);
    assert_eq!(col.top_values.len(), MAX_TOP_VALUES);
    assert_eq!(col.top_values[0], ("val_00001".to_owned(), 3));
}

#[test]
fn renderers_produce_ascii_table_plain_and_valid_json() {
    let mut set = make_set(
        vec!["id", "name\x1b[31m"],
        vec![Some("int4"), Some("text")],
        vec![
            vec![Cell::Text("1".to_owned()), Cell::Text("alice".to_owned())],
            vec![Cell::Text("2".to_owned()), Cell::Text("bob".to_owned())],
        ],
        2,
    );
    // Simulate a 3rd row discarded at cap.
    assert!(!set.push(vec![
        Cell::Text("3".to_owned()),
        Cell::Text("carol".to_owned())
    ]));

    let analytics = profile_result_set(&set);
    assert!(analytics.truncated);
    assert_eq!(analytics.row_count, 2);
    assert_eq!(analytics.rows_seen, 3);

    let ascii_table = analytics.render_table();
    assert!(ascii_table.is_ascii(), "render_table must be plain ASCII");
    assert!(ascii_table.contains("truncated=true"));
    assert!(ascii_table.contains("2 retained row(s) of 3 seen"));
    assert!(!ascii_table.contains('\x1b'));

    let unicode_table = analytics.render_table_with_glyphs(true);
    assert!(unicode_table.contains("alice") || unicode_table.contains("id"));

    let plain = analytics.render_plain();
    assert!(plain.is_ascii());
    assert!(plain.contains("rows=2 rows_seen=3"));

    let json_text = analytics.render_json();
    let parsed: serde_json::Value = serde_json::from_str(&json_text).expect("valid json");
    assert_eq!(parsed["row_count"], 2);
    assert_eq!(parsed["rows_seen"], 3);
    assert_eq!(parsed["truncated"], true);
}

#[test]
fn html_report_uses_field_industrial_palette_and_escapes_xss() {
    let mut set = make_set(
        vec!["<b>col</b>", "notes"],
        vec![Some("text\"<svg>"), Some("text")],
        vec![
            vec![Cell::Null, Cell::Text(String::new())],
            vec![
                Cell::Text("<script>alert('xss')</script>".to_owned()),
                Cell::Text("NULL".to_owned()),
            ],
        ],
        2,
    );
    set.discard_row(RetentionReason::RowCap);

    let analytics = profile_result_set(&set);
    let html = render_html_report(&set, "Report & <script>alert(1)</script>", &analytics);

    assert!(html.starts_with("<!DOCTYPE html>"));
    for color in [
        "#16181E", "#1C1F27", "#232732", "#F4F3EF", "#FF6A00", "#FBBF24", "#24C992",
    ] {
        assert!(html.contains(color), "missing palette color {color}");
    }
    assert!(
        !html.contains("<script"),
        "raw <script tag leaked into HTML"
    );
    assert!(!html.contains("http://") && !html.contains("https://"));

    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(html.contains("&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"));
    assert!(html.contains("&lt;b&gt;col&lt;/b&gt;"));
    assert!(html.contains("text&quot;&lt;svg&gt;"));
    assert!(html.contains("TRUNCATED RESULT:"));
    assert!(html.contains("badge-null\">[null]"));
    assert!(html.contains("badge-empty\">[empty]"));
    assert!(html.contains("badge-literal\">NULL"));
}

#[test]
fn parquet_refuses_unavailable_and_writes_valid_empty_container() {
    let unavailable = ResultSet::unavailable(2, 100);
    let err = export_parquet_bytes(&unavailable).expect_err("unavailable must fail");
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

    let empty = make_set(
        vec!["id", "title"],
        vec![Some("int8"), Some("text")],
        vec![],
        100,
    );
    let bytes = export_parquet_bytes(&empty).expect("empty parquet");
    assert_eq!(&bytes[..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");

    let footer_len =
        u32::from_le_bytes(bytes[bytes.len() - 8..bytes.len() - 4].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 4 + footer_len + 4 + 4);

    let meta = decode_file_metadata(&bytes[4..4 + footer_len]);
    assert_eq!(meta.version, 1);
    assert_eq!(meta.num_rows, 0);
    assert!(meta.row_groups.is_empty());
    assert_eq!(meta.schema.len(), 3);
    assert_eq!(meta.schema[0].name, "ignatius_result");
    assert_eq!(meta.schema[0].num_children, Some(2));
    assert_eq!(meta.schema[1].name, "id");
    assert_eq!(meta.schema[1].phys_type, Some(2)); // INT64
    assert_eq!(meta.schema[2].name, "title");
    assert_eq!(meta.schema[2].phys_type, Some(6)); // BYTE_ARRAY
    assert_eq!(meta.schema[2].converted_type, Some(0)); // UTF8
    assert!(meta.schema[2].has_string_logical_type);
    assert!(
        meta.created_by
            .as_deref()
            .unwrap_or("")
            .contains("Ignatius")
    );
}

#[test]
fn parquet_round_trips_typed_columns_null_bitmaps_and_streaming_parity() {
    let mut rows = Vec::new();
    for i in 0..10 {
        let id = if i == 3 || i == 7 {
            Cell::Null
        } else {
            Cell::Text((i as i64 * 100 - 50).to_string())
        };
        let flag = if i == 4 {
            Cell::Null
        } else {
            Cell::Text(if i % 2 == 0 { "t" } else { "f" }.to_owned())
        };
        let ratio = if i == 1 {
            Cell::Null
        } else {
            Cell::Text(format!("{}.5", i))
        };
        let label = match i {
            0 => Cell::Text(String::new()),
            2 => Cell::Null,
            5 => Cell::Text("NULL".to_owned()),
            8 => Cell::Text("café".to_owned()),
            _ => Cell::Text(format!("row-{i}")),
        };
        rows.push(vec![id, flag, ratio, label]);
    }

    let set = make_set(
        vec!["id", "flag", "ratio", "label"],
        vec![Some("int4"), Some("bool"), Some("float8"), Some("text")],
        rows,
        100,
    );

    let bytes = export_parquet_bytes(&set).expect("parquet bytes");
    let mut streamed = Vec::new();
    write_parquet(&set, &mut streamed).expect("streaming parquet");
    assert_eq!(bytes, streamed);

    assert_eq!(&bytes[..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");

    let footer_len =
        u32::from_le_bytes(bytes[bytes.len() - 8..bytes.len() - 4].try_into().unwrap()) as usize;
    let footer_start = bytes.len() - 8 - footer_len;
    let meta = decode_file_metadata(&bytes[footer_start..footer_start + footer_len]);

    assert_eq!(meta.version, 1);
    assert_eq!(meta.num_rows, 10);
    assert_eq!(meta.row_groups.len(), 1);
    let rg = &meta.row_groups[0];
    assert_eq!(rg.num_rows, 10);
    assert_eq!(rg.file_offset, 4);
    assert_eq!(rg.columns.len(), 4);

    // Decode Column 0: INT64 with NULLs at indices 3 and 7.
    let c0 = &rg.columns[0];
    assert_eq!(c0.phys_type, 2);
    assert_eq!(c0.data_page_offset, c0.file_offset);
    let (defs0, vals0) = decode_data_page(&bytes[c0.file_offset as usize..], 10);
    assert_eq!(defs0, vec![1, 1, 1, 0, 1, 1, 1, 0, 1, 1]);
    let ints: Vec<i64> = vals0
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| i64::from_le_bytes(*b))
        .collect();
    assert_eq!(ints, vec![-50, 50, 150, 350, 450, 550, 750, 850]);

    // Decode Column 1: BOOLEAN (9 non-null values packed across 2 bytes) with NULL at index 4.
    let c1 = &rg.columns[1];
    assert_eq!(c1.phys_type, 0);
    let (defs1, vals1) = decode_data_page(&bytes[c1.file_offset as usize..], 10);
    assert_eq!(defs1, vec![1, 1, 1, 1, 0, 1, 1, 1, 1, 1]);
    assert_eq!(vals1.len(), 2); // 9 bits -> 2 bytes
    let mut bools = Vec::new();
    for bit_idx in 0..9 {
        bools.push(((vals1[bit_idx / 8] >> (bit_idx % 8)) & 1) == 1);
    }
    assert_eq!(
        bools,
        vec![true, false, true, false, false, true, false, true, false]
    );

    // Decode Column 2: DOUBLE with NULL at index 1.
    let c2 = &rg.columns[2];
    assert_eq!(c2.phys_type, 5);
    let (defs2, vals2) = decode_data_page(&bytes[c2.file_offset as usize..], 10);
    assert_eq!(defs2, vec![1, 0, 1, 1, 1, 1, 1, 1, 1, 1]);
    let floats: Vec<f64> = vals2
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| f64::from_le_bytes(*b))
        .collect();
    assert_eq!(floats, vec![0.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 8.5, 9.5]);

    // Decode Column 3: BYTE_ARRAY with "", NULL, "NULL", and UTF-8 "café".
    let c3 = &rg.columns[3];
    assert_eq!(c3.phys_type, 6);
    let (defs3, vals3) = decode_data_page(&bytes[c3.file_offset as usize..], 10);
    assert_eq!(defs3, vec![1, 1, 0, 1, 1, 1, 1, 1, 1, 1]);
    let strings = decode_plain_byte_array(vals3);
    assert_eq!(
        strings,
        vec![
            "", "row-1", "row-3", "row-4", "NULL", "row-6", "row-7", "café", "row-9"
        ]
    );
}

#[test]
fn parquet_uniform_all_null_column_uses_rle_definition_levels() {
    let set = make_set(
        vec!["only_null"],
        vec![Some("int8")],
        vec![vec![Cell::Null], vec![Cell::Null], vec![Cell::Null]],
        100,
    );
    let bytes = export_parquet_bytes(&set).expect("all-null parquet");
    let footer_len =
        u32::from_le_bytes(bytes[bytes.len() - 8..bytes.len() - 4].try_into().unwrap()) as usize;
    let footer_start = bytes.len() - 8 - footer_len;
    let meta = decode_file_metadata(&bytes[footer_start..footer_start + footer_len]);
    let c0 = &meta.row_groups[0].columns[0];
    let (defs, vals) = decode_data_page(&bytes[c0.file_offset as usize..], 3);
    assert_eq!(defs, vec![0, 0, 0]);
    assert!(vals.is_empty());
}

// ---------------------------------------------------------------------------
// Independent Parquet / Thrift TCompactProtocol Test Decoder
// ---------------------------------------------------------------------------

struct CompactReader<'a> {
    data: &'a [u8],
    pos: usize,
    last_field: Vec<i16>,
}

impl<'a> CompactReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            last_field: vec![0],
        }
    }

    fn read_u8(&mut self) -> u8 {
        let b = self.data[self.pos];
        self.pos += 1;
        b
    }

    fn read_uvarint(&mut self) -> u64 {
        let mut val = 0u64;
        let mut shift = 0u32;
        loop {
            let b = self.read_u8();
            val |= u64::from(b & 0x7f) << shift;
            if (b & 0x80) == 0 {
                return val;
            }
            shift += 7;
        }
    }

    fn read_i32(&mut self) -> i32 {
        let z = self.read_uvarint() as u32;
        ((z >> 1) as i32) ^ -((z & 1) as i32)
    }

    fn read_i64(&mut self) -> i64 {
        let z = self.read_uvarint();
        ((z >> 1) as i64) ^ -((z & 1) as i64)
    }

    fn read_bytes(&mut self) -> &'a [u8] {
        let len = self.read_uvarint() as usize;
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        slice
    }

    fn read_string(&mut self) -> String {
        String::from_utf8(self.read_bytes().to_vec()).expect("utf8")
    }

    fn push_struct(&mut self) {
        self.last_field.push(0);
    }

    fn pop_struct(&mut self) {
        self.last_field.pop();
    }

    fn read_field_begin(&mut self) -> Option<(i16, u8)> {
        let header = self.read_u8();
        if header == 0 {
            return None;
        }
        let ftype = header & 0x0f;
        let delta = i16::from(header >> 4);
        let fid = if delta == 0 {
            self.read_i32() as i16
        } else {
            *self.last_field.last().unwrap() + delta
        };
        *self.last_field.last_mut().unwrap() = fid;
        Some((fid, ftype))
    }

    fn read_list_begin(&mut self) -> (u8, usize) {
        let header = self.read_u8();
        let elem_type = header & 0x0f;
        let len_nibble = header >> 4;
        let len = if len_nibble == 0x0f {
            self.read_uvarint() as usize
        } else {
            usize::from(len_nibble)
        };
        (elem_type, len)
    }
}

struct DecodedSchema {
    name: String,
    phys_type: Option<i32>,
    num_children: Option<i32>,
    converted_type: Option<i32>,
    has_string_logical_type: bool,
}

struct DecodedColumnChunk {
    file_offset: i64,
    phys_type: i32,
    data_page_offset: i64,
}

struct DecodedRowGroup {
    columns: Vec<DecodedColumnChunk>,
    num_rows: i64,
    file_offset: i64,
}

struct DecodedFileMeta {
    version: i32,
    schema: Vec<DecodedSchema>,
    num_rows: i64,
    row_groups: Vec<DecodedRowGroup>,
    created_by: Option<String>,
}

fn decode_file_metadata(bytes: &[u8]) -> DecodedFileMeta {
    let mut r = CompactReader::new(bytes);
    let mut version = 0;
    let mut schema = Vec::new();
    let mut num_rows = 0;
    let mut row_groups = Vec::new();
    let mut created_by = None;

    while let Some((fid, ftype)) = r.read_field_begin() {
        match (fid, ftype) {
            (1, 5) => version = r.read_i32(),
            (2, 9) => {
                let (et, len) = r.read_list_begin();
                assert_eq!(et, 12);
                for _ in 0..len {
                    schema.push(decode_schema_element(&mut r));
                }
            }
            (3, 6) => num_rows = r.read_i64(),
            (4, 9) => {
                let (et, len) = r.read_list_begin();
                assert_eq!(et, 12);
                for _ in 0..len {
                    row_groups.push(decode_row_group(&mut r));
                }
            }
            (6, 8) => created_by = Some(r.read_string()),
            other => panic!("unexpected FileMetaData field {other:?}"),
        }
    }

    DecodedFileMeta {
        version,
        schema,
        num_rows,
        row_groups,
        created_by,
    }
}

fn decode_schema_element(r: &mut CompactReader<'_>) -> DecodedSchema {
    r.push_struct();
    let mut elem = DecodedSchema {
        name: String::new(),
        phys_type: None,
        num_children: None,
        converted_type: None,
        has_string_logical_type: false,
    };
    while let Some((fid, ftype)) = r.read_field_begin() {
        match (fid, ftype) {
            (1, 5) => elem.phys_type = Some(r.read_i32()),
            (3, 5) => {
                let rep = r.read_i32();
                assert_eq!(rep, 1);
            }
            (4, 8) => elem.name = r.read_string(),
            (5, 5) => elem.num_children = Some(r.read_i32()),
            (6, 5) => elem.converted_type = Some(r.read_i32()),
            (10, 12) => {
                r.push_struct();
                while let Some((lfid, lftype)) = r.read_field_begin() {
                    if (lfid, lftype) == (1, 12) {
                        r.push_struct();
                        assert!(r.read_field_begin().is_none());
                        r.pop_struct();
                        elem.has_string_logical_type = true;
                    } else {
                        panic!("unexpected LogicalType field ({lfid}, {lftype})");
                    }
                }
                r.pop_struct();
            }
            other => panic!("unexpected SchemaElement field {other:?}"),
        }
    }
    r.pop_struct();
    elem
}

fn decode_row_group(r: &mut CompactReader<'_>) -> DecodedRowGroup {
    r.push_struct();
    let mut columns = Vec::new();
    let mut num_rows = 0;
    let mut file_offset = 0;

    while let Some((fid, ftype)) = r.read_field_begin() {
        match (fid, ftype) {
            (1, 9) => {
                let (et, len) = r.read_list_begin();
                assert_eq!(et, 12);
                for _ in 0..len {
                    columns.push(decode_column_chunk(r));
                }
            }
            (2, 6) | (6, 6) => {
                let _ = r.read_i64();
            }
            (3, 6) => num_rows = r.read_i64(),
            (5, 6) => file_offset = r.read_i64(),
            other => panic!("unexpected RowGroup field {other:?}"),
        }
    }
    r.pop_struct();
    DecodedRowGroup {
        columns,
        num_rows,
        file_offset,
    }
}

fn decode_column_chunk(r: &mut CompactReader<'_>) -> DecodedColumnChunk {
    r.push_struct();
    let mut file_offset = 0;
    let mut phys_type = 0;
    let mut data_page_offset = 0;

    while let Some((fid, ftype)) = r.read_field_begin() {
        match (fid, ftype) {
            (2, 6) => file_offset = r.read_i64(),
            (3, 12) => {
                r.push_struct();
                while let Some((mfid, mftype)) = r.read_field_begin() {
                    match (mfid, mftype) {
                        (1, 5) => phys_type = r.read_i32(),
                        (2, 9) => {
                            let (et, len) = r.read_list_begin();
                            assert_eq!(et, 5);
                            for _ in 0..len {
                                let _ = r.read_i32();
                            }
                        }
                        (3, 9) => {
                            let (et, len) = r.read_list_begin();
                            assert_eq!(et, 8);
                            for _ in 0..len {
                                let _ = r.read_string();
                            }
                        }
                        (4, 5) => assert_eq!(r.read_i32(), 0),
                        (5, 6) | (6, 6) | (7, 6) => {
                            let _ = r.read_i64();
                        }
                        (9, 6) => data_page_offset = r.read_i64(),
                        other => panic!("unexpected ColumnMetaData field {other:?}"),
                    }
                }
                r.pop_struct();
            }
            other => panic!("unexpected ColumnChunk field {other:?}"),
        }
    }
    r.pop_struct();
    DecodedColumnChunk {
        file_offset,
        phys_type,
        data_page_offset,
    }
}

fn decode_data_page(chunk_slice: &[u8], expected_rows: usize) -> (Vec<u8>, &[u8]) {
    let mut r = CompactReader::new(chunk_slice);
    let mut page_type = -1;
    let mut uncompressed_size = 0i32;
    let mut compressed_size = 0i32;
    let mut num_values = 0i32;

    while let Some((fid, ftype)) = r.read_field_begin() {
        match (fid, ftype) {
            (1, 5) => page_type = r.read_i32(),
            (2, 5) => uncompressed_size = r.read_i32(),
            (3, 5) => compressed_size = r.read_i32(),
            (5, 12) => {
                r.push_struct();
                while let Some((dfid, dftype)) = r.read_field_begin() {
                    match (dfid, dftype) {
                        (1, 5) => num_values = r.read_i32(),
                        (2, 5) => assert_eq!(r.read_i32(), 0), // PLAIN
                        (3, 5) | (4, 5) => assert_eq!(r.read_i32(), 3), // RLE
                        other => panic!("unexpected DataPageHeader field {other:?}"),
                    }
                }
                r.pop_struct();
            }
            other => panic!("unexpected PageHeader field {other:?}"),
        }
    }

    assert_eq!(page_type, 0);
    assert_eq!(uncompressed_size, compressed_size);
    assert_eq!(num_values as usize, expected_rows);

    let payload = &chunk_slice[r.pos..r.pos + uncompressed_size as usize];
    let defs_len = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let defs_stream = &payload[4..4 + defs_len];
    let values_slice = &payload[4 + defs_len..];

    let defs = decode_rle_bit_packed_width1(defs_stream, expected_rows);
    (defs, values_slice)
}

fn decode_rle_bit_packed_width1(stream: &[u8], count: usize) -> Vec<u8> {
    let mut pos = 0usize;
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let mut header = 0u64;
        let mut shift = 0u32;
        loop {
            let b = stream[pos];
            pos += 1;
            header |= u64::from(b & 0x7f) << shift;
            if (b & 0x80) == 0 {
                break;
            }
            shift += 7;
        }
        if (header & 1) == 0 {
            let run_len = (header >> 1) as usize;
            let val = stream[pos];
            pos += 1;
            for _ in 0..run_len {
                if out.len() < count {
                    out.push(val);
                }
            }
        } else {
            let groups = (header >> 1) as usize;
            for _ in 0..groups {
                let packed = stream[pos];
                pos += 1;
                for bit in 0..8 {
                    if out.len() < count {
                        out.push((packed >> bit) & 1);
                    }
                }
            }
        }
    }
    assert_eq!(pos, stream.len());
    out
}

fn decode_plain_byte_array(bytes: &[u8]) -> Vec<String> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < bytes.len() {
        let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let s = std::str::from_utf8(&bytes[pos..pos + len]).expect("utf8");
        pos += len;
        out.push(s.to_owned());
    }
    out
}
