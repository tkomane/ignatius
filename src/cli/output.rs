//! Output formats for the non-interactive command line.
//!
//! These are a compatibility contract. Scripts depend on them, so the rules are
//! fixed and tested:
//!
//! - Data goes to stdout. Diagnostics go to stderr. Machine formats never carry
//!   decoration, progress, or colour.
//! - Values are written exactly as PostgreSQL rendered them. Display escaping is
//!   for the screen, not for a file.
//! - NULL is encoded distinctly in every format, and how is documented per format.
//! - Multiple result sets are always explicit. NDJSON, which cannot express them
//!   unambiguously, refuses rather than guessing.

use crate::query::result::{Execution, ResultSet};
use crate::query::value::{Cell, display_width, pad_to_width, sanitize_for_display};
use std::io::{self, Write};

/// A supported output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum Format {
    /// Aligned columns for reading. Values are escaped for terminal safety.
    #[default]
    Table,
    /// RFC 4180 comma-separated values.
    Csv,
    /// Tab-separated values, with tabs and newlines backslash-escaped.
    Tsv,
    /// A JSON array of result-set objects.
    Json,
    /// One JSON object per row. Single result set only.
    Ndjson,
    /// A Markdown pipe table.
    Markdown,
}

impl Format {
    /// Whether the format is meant for machines, which forbids decoration.
    #[must_use]
    pub const fn is_machine_readable(self) -> bool {
        matches!(self, Self::Csv | Self::Tsv | Self::Json | Self::Ndjson)
    }
}

/// How to write output.
#[derive(Debug, Clone)]
pub struct OutputOptions {
    /// The format.
    pub format: Format,
    /// Whether to write a header row, where the format has one.
    pub header: bool,
    /// Text used for SQL NULL in CSV and TSV. Empty by default, like `psql`.
    pub null_encoding: String,
    /// Whether Unicode may be used for table borders.
    pub unicode: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            format: Format::Table,
            header: true,
            null_encoding: String::new(),
            unicode: true,
        }
    }
}

/// Writes an execution's results.
///
/// Returns an error only for I/O failures and for the NDJSON multi-result case,
/// which is a usage error rather than a data problem.
pub fn write_execution(
    out: &mut impl Write,
    execution: &Execution,
    options: &OutputOptions,
) -> io::Result<()> {
    let sets: Vec<&ResultSet> = execution
        .statements
        .iter()
        .filter_map(|s| s.result_set.as_ref())
        .collect();

    if options.format == Format::Ndjson && sets.len() > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "ndjson cannot represent {} result sets without ambiguity; run one statement, \
                 or use --format json",
                sets.len()
            ),
        ));
    }

    match options.format {
        Format::Json => write_json(out, &sets),
        Format::Ndjson => {
            if let Some(set) = sets.first() {
                write_ndjson(out, set)?;
            }
            Ok(())
        }
        Format::Csv => write_separated(out, &sets, ',', options),
        Format::Tsv => write_separated(out, &sets, '\t', options),
        Format::Markdown => write_markdown(out, &sets, options),
        Format::Table => write_table(out, execution, options),
    }
}

fn write_json(out: &mut impl Write, sets: &[&ResultSet]) -> io::Result<()> {
    let payload: Vec<serde_json::Value> = sets
        .iter()
        .map(|set| {
            let rows: Vec<serde_json::Value> = set
                .rows
                .iter()
                .map(|row| {
                    let object: serde_json::Map<String, serde_json::Value> = set
                        .columns
                        .iter()
                        .zip(row)
                        .map(|(name, cell)| {
                            let value = match cell {
                                // JSON null is the only unambiguous encoding of
                                // SQL NULL, and every other value stays the text
                                // the server produced.
                                Cell::Null => serde_json::Value::Null,
                                Cell::Text(text) => serde_json::Value::String(text.clone()),
                            };
                            (name.clone(), value)
                        })
                        .collect();
                    serde_json::Value::Object(object)
                })
                .collect();
            serde_json::json!({
                "columns": set.columns,
                "rows": rows,
                "rows_seen": set.rows_seen,
                "truncated": set.is_truncated(),
            })
        })
        .collect();
    writeln!(out, "{}", serde_json::Value::Array(payload))
}

fn write_ndjson(out: &mut impl Write, set: &ResultSet) -> io::Result<()> {
    for row in &set.rows {
        let object: serde_json::Map<String, serde_json::Value> = set
            .columns
            .iter()
            .zip(row)
            .map(|(name, cell)| {
                let value = match cell {
                    Cell::Null => serde_json::Value::Null,
                    Cell::Text(text) => serde_json::Value::String(text.clone()),
                };
                (name.clone(), value)
            })
            .collect();
        writeln!(out, "{}", serde_json::Value::Object(object))?;
    }
    Ok(())
}

fn write_separated(
    out: &mut impl Write,
    sets: &[&ResultSet],
    delimiter: char,
    options: &OutputOptions,
) -> io::Result<()> {
    for (index, set) in sets.iter().enumerate() {
        if index > 0 {
            // A blank line is the documented separator between result sets.
            writeln!(out)?;
        }
        if options.header {
            let header: Vec<String> = set
                .columns
                .iter()
                .map(|name| encode_field(name, delimiter))
                .collect();
            writeln!(out, "{}", header.join(&delimiter.to_string()))?;
        }
        for row in &set.rows {
            let values: Vec<String> = row
                .iter()
                .map(|cell| encode_field(&cell.export(&options.null_encoding), delimiter))
                .collect();
            writeln!(out, "{}", values.join(&delimiter.to_string()))?;
        }
    }
    Ok(())
}

/// Quotes or escapes one field for the delimiter in use.
pub(crate) fn encode_field(value: &str, delimiter: char) -> String {
    if delimiter == '\t' {
        // Tab-separated data cannot quote, so control characters are escaped the
        // way PostgreSQL's own COPY TEXT format escapes them.
        return value
            .replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
    }
    if value.contains(delimiter)
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r')
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn write_markdown(
    out: &mut impl Write,
    sets: &[&ResultSet],
    options: &OutputOptions,
) -> io::Result<()> {
    for (index, set) in sets.iter().enumerate() {
        if index > 0 {
            writeln!(out)?;
        }
        let escape = |text: &str| sanitize_for_display(text).replace('|', "\\|");
        writeln!(
            out,
            "| {} |",
            set.columns
                .iter()
                .map(|c| escape(c))
                .collect::<Vec<_>>()
                .join(" | ")
        )?;
        writeln!(
            out,
            "|{}|",
            set.columns
                .iter()
                .map(|_| " --- ")
                .collect::<Vec<_>>()
                .join("|")
        )?;
        for row in &set.rows {
            let values: Vec<String> = row
                .iter()
                .map(|cell| match cell {
                    Cell::Null => format!("_{}_", options.null_encoding_or_marker()),
                    Cell::Text(text) => escape(text),
                })
                .collect();
            writeln!(out, "| {} |", values.join(" | "))?;
        }
    }
    Ok(())
}

impl OutputOptions {
    /// The NULL marker used where an empty cell would be ambiguous.
    fn null_encoding_or_marker(&self) -> String {
        if self.null_encoding.is_empty() {
            "null".to_owned()
        } else {
            self.null_encoding.clone()
        }
    }
}

fn write_table(
    out: &mut impl Write,
    execution: &Execution,
    options: &OutputOptions,
) -> io::Result<()> {
    for (index, statement) in execution.statements.iter().enumerate() {
        if index > 0 {
            writeln!(out)?;
        }
        match &statement.result_set {
            Some(set) => {
                let widths = table_widths(set);
                let separator = if options.unicode { "│" } else { "|" };
                let rule = if options.unicode { "─" } else { "-" };

                if options.header {
                    let header: Vec<String> = set
                        .columns
                        .iter()
                        .zip(&widths)
                        .map(|(name, width)| pad_to_width(&sanitize_for_display(name), *width))
                        .collect();
                    writeln!(out, " {} ", header.join(&format!(" {separator} ")))?;
                    let rules: Vec<String> =
                        widths.iter().map(|width| rule.repeat(*width)).collect();
                    writeln!(out, " {} ", rules.join(&format!("{rule}{separator}{rule}")))?;
                }

                for row in &set.rows {
                    let values: Vec<String> = row
                        .iter()
                        .zip(&widths)
                        .map(|(cell, width)| pad_to_width(&cell.display(), *width))
                        .collect();
                    writeln!(out, " {} ", values.join(&format!(" {separator} ")))?;
                }
                writeln!(out, "({})", set.window_label())?;
            }
            None => writeln!(out, "{}", statement.summary())?,
        }
    }
    if execution.statements.is_empty() && execution.error.is_none() {
        // A failure is a diagnostic and belongs on stderr. Writing it here too
        // would put non-data into the stream a script is reading.
        writeln!(out, "{}", execution.status.label())?;
    }
    Ok(())
}

fn table_widths(set: &ResultSet) -> Vec<usize> {
    set.columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let widest = set
                .rows
                .iter()
                .filter_map(|row| row.get(i))
                .map(|cell| display_width(&cell.display()))
                .max()
                .unwrap_or(0);
            display_width(&sanitize_for_display(name)).max(widest)
        })
        .collect()
}

/// Writes rows one at a time, for export.
///
/// Only the formats that need no wrapper can stream: CSV, TSV and NDJSON. JSON,
/// Markdown and the aligned table all need the whole result before the first
/// byte is correct, and buffering it would quietly give up the memory guarantee
/// an export exists to keep. They are refused rather than silently downgraded.
#[derive(Debug)]
pub struct StreamWriter {
    format: Format,
    options: OutputOptions,
    columns: Vec<String>,
    rows: u64,
}

impl StreamWriter {
    /// Creates a writer, or explains why this format cannot stream.
    pub fn new(options: OutputOptions) -> Result<Self, crate::diagnostics::Diagnostic> {
        if !matches!(options.format, Format::Csv | Format::Tsv | Format::Ndjson) {
            return Err(crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Usage,
                format!("{:?} cannot be written as a stream", options.format).to_lowercase(),
                "preparing an export",
            )
            .likely_cause(
                "json, markdown and table need the whole result before the first byte is \
                 correct, and holding a large result in memory is exactly what an export \
                 avoids",
            )
            .next_action("export as csv, tsv or ndjson"));
        }
        Ok(Self {
            format: options.format,
            options,
            columns: Vec::new(),
            rows: 0,
        })
    }

    /// Records the column names and writes a header where the format has one.
    pub fn columns(&mut self, out: &mut impl Write, columns: Vec<String>) -> io::Result<()> {
        self.columns = columns;
        if !self.options.header || self.format == Format::Ndjson {
            return Ok(());
        }
        let delimiter = self.delimiter();
        let header: Vec<String> = self
            .columns
            .iter()
            .map(|name| encode_field(name, delimiter))
            .collect();
        writeln!(out, "{}", header.join(&delimiter.to_string()))
    }

    /// Writes one row.
    pub fn row(&mut self, out: &mut impl Write, row: &[Cell]) -> io::Result<()> {
        self.rows += 1;
        if self.format == Format::Ndjson {
            let object: serde_json::Map<String, serde_json::Value> = self
                .columns
                .iter()
                .zip(row)
                .map(|(name, cell)| {
                    let value = match cell {
                        Cell::Null => serde_json::Value::Null,
                        Cell::Text(text) => serde_json::Value::String(text.clone()),
                    };
                    (name.clone(), value)
                })
                .collect();
            return writeln!(out, "{}", serde_json::Value::Object(object));
        }
        let delimiter = self.delimiter();
        let values: Vec<String> = row
            .iter()
            .map(|cell| encode_field(&cell.export(&self.options.null_encoding), delimiter))
            .collect();
        writeln!(out, "{}", values.join(&delimiter.to_string()))
    }

    /// How many rows have been written.
    #[must_use]
    pub const fn rows(&self) -> u64 {
        self.rows
    }

    const fn delimiter(&self) -> char {
        match self.format {
            Format::Tsv => '\t',
            _ => ',',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::result::{ExecutionStatus, JobId, StatementResult};
    use std::time::Duration;

    fn execution(sets: Vec<ResultSet>) -> Execution {
        Execution {
            job: JobId(1),
            statements: sets
                .into_iter()
                .map(|set| StatementResult {
                    rows_affected: Some(set.rows_seen),
                    result_set: Some(set),
                    elapsed: Duration::from_millis(1),
                    notices: Vec::new(),
                })
                .collect(),
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(2),
            error: None,
            transaction: crate::query::result::TransactionState::Autocommit,
        }
    }

    fn sample() -> ResultSet {
        let mut set = ResultSet::new(vec!["id".into(), "name".into(), "note".into()], 100);
        set.push(vec![
            Cell::Text("1".into()),
            Cell::Text("Ada".into()),
            Cell::Null,
        ]);
        set.push(vec![
            Cell::Text("2".into()),
            Cell::Text("with, comma".into()),
            Cell::Text("say \"hi\"".into()),
        ]);
        set
    }

    fn render(format: Format, execution: &Execution) -> String {
        let mut out = Vec::new();
        write_execution(
            &mut out,
            execution,
            &OutputOptions {
                format,
                ..OutputOptions::default()
            },
        )
        .expect("write");
        String::from_utf8(out).expect("utf8")
    }

    #[test]
    fn csv_quotes_only_what_needs_quoting_and_encodes_null_as_empty() {
        let text = render(Format::Csv, &execution(vec![sample()]));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "id,name,note");
        assert_eq!(
            lines[1], "1,Ada,",
            "SQL NULL is an empty CSV field by default"
        );
        assert_eq!(lines[2], "2,\"with, comma\",\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_null_encoding_is_configurable() {
        let mut out = Vec::new();
        write_execution(
            &mut out,
            &execution(vec![sample()]),
            &OutputOptions {
                format: Format::Csv,
                null_encoding: "\\N".into(),
                ..OutputOptions::default()
            },
        )
        .expect("write");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("1,Ada,\\N"), "{text}");
    }

    #[test]
    fn tsv_escapes_control_characters_rather_than_quoting() {
        let mut set = ResultSet::new(vec!["v".into()], 10);
        set.push(vec![Cell::Text("a\tb\nc".into())]);
        let text = render(Format::Tsv, &execution(vec![set]));
        assert!(text.contains("a\\tb\\nc"), "{text:?}");
        assert_eq!(
            text.lines().count(),
            2,
            "the escaped value stays on one line"
        );
    }

    #[test]
    fn json_distinguishes_null_from_empty_string() {
        let mut set = ResultSet::new(vec!["a".into(), "b".into()], 10);
        set.push(vec![Cell::Null, Cell::Text(String::new())]);
        let text = render(Format::Json, &execution(vec![set]));
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert!(parsed[0]["rows"][0]["a"].is_null());
        assert_eq!(parsed[0]["rows"][0]["b"], "");
    }

    #[test]
    fn json_reports_truncation_and_the_true_row_count() {
        let mut set = ResultSet::new(vec!["n".into()], 1);
        set.push(vec![Cell::Text("1".into())]);
        set.push(vec![Cell::Text("2".into())]);
        let text = render(Format::Json, &execution(vec![set]));
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(parsed[0]["rows"].as_array().expect("rows").len(), 1);
        assert_eq!(parsed[0]["rows_seen"], 2);
        assert_eq!(parsed[0]["truncated"], true);
    }

    #[test]
    fn ndjson_writes_one_object_per_line_and_refuses_multiple_result_sets() {
        let text = render(Format::Ndjson, &execution(vec![sample()]));
        assert_eq!(text.lines().count(), 2);
        for line in text.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("each line is valid json");
        }

        let mut out = Vec::new();
        let err = write_execution(
            &mut out,
            &execution(vec![sample(), sample()]),
            &OutputOptions {
                format: Format::Ndjson,
                ..OutputOptions::default()
            },
        )
        .expect_err("must refuse rather than guess");
        assert!(err.to_string().contains("--format json"), "{err}");
    }

    #[test]
    fn machine_formats_carry_no_decoration() {
        for format in [Format::Csv, Format::Tsv, Format::Json, Format::Ndjson] {
            assert!(format.is_machine_readable());
            let text = render(format, &execution(vec![sample()]));
            assert!(
                !text.contains('\x1b'),
                "{format:?} emitted an escape sequence"
            );
            assert!(
                !text.contains("rows)"),
                "{format:?} emitted a row-count banner"
            );
            assert!(!text.contains('│'), "{format:?} emitted table drawing");
        }
    }

    #[test]
    fn the_table_format_aligns_columns_and_states_the_row_count() {
        let text = render(Format::Table, &execution(vec![sample()]));
        assert!(text.contains("id"), "{text}");
        assert!(
            text.contains("[null]"),
            "NULL stays distinguishable on screen"
        );
        assert!(text.contains("(2 rows)"), "{text}");
    }

    #[test]
    fn the_table_format_escapes_hostile_values() {
        let mut set = ResultSet::new(vec!["v".into()], 10);
        set.push(vec![Cell::Text("\x1b[2Jboom".into())]);
        let text = render(Format::Table, &execution(vec![set]));
        assert!(!text.contains('\x1b'), "{text:?}");
    }

    #[test]
    fn markdown_escapes_pipes_so_the_table_stays_a_table() {
        let mut set = ResultSet::new(vec!["a|b".into()], 10);
        set.push(vec![Cell::Text("x|y".into())]);
        set.push(vec![Cell::Null]);
        let text = render(Format::Markdown, &execution(vec![set]));
        assert!(text.contains(r"| a\|b |"), "{text}");
        assert!(text.contains(r"| x\|y |"), "{text}");
        assert!(text.contains("_null_"), "NULL is marked, not blank: {text}");
    }

    #[test]
    fn multiple_result_sets_are_separated_in_every_format_that_allows_them() {
        for format in [Format::Csv, Format::Tsv, Format::Markdown, Format::Table] {
            let text = render(format, &execution(vec![sample(), sample()]));
            assert!(
                text.contains("\n\n"),
                "{format:?} ran two result sets together"
            );
        }
        let text = render(Format::Json, &execution(vec![sample(), sample()]));
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(parsed.as_array().expect("array").len(), 2);
    }

    #[test]
    fn headers_can_be_suppressed_for_piping() {
        let mut out = Vec::new();
        write_execution(
            &mut out,
            &execution(vec![sample()]),
            &OutputOptions {
                format: Format::Csv,
                header: false,
                ..OutputOptions::default()
            },
        )
        .expect("write");
        let text = String::from_utf8(out).expect("utf8");
        assert!(!text.starts_with("id,name"), "{text}");
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn a_streamed_export_matches_what_the_buffered_writer_produces() {
        // The two paths must not drift: a script cannot care whether the rows
        // went through a file or through a pipe.
        for format in [Format::Csv, Format::Tsv, Format::Ndjson] {
            let options = OutputOptions {
                format,
                ..OutputOptions::default()
            };
            let buffered = {
                let mut out = Vec::new();
                write_execution(&mut out, &execution(vec![sample()]), &options).expect("write");
                String::from_utf8(out).expect("utf8")
            };

            let streamed = {
                let mut out = Vec::new();
                let mut writer = StreamWriter::new(options.clone()).expect("streamable");
                let set = sample();
                writer
                    .columns(&mut out, set.columns.clone())
                    .expect("columns");
                for row in &set.rows {
                    writer.row(&mut out, row).expect("row");
                }
                assert_eq!(writer.rows(), 2);
                String::from_utf8(out).expect("utf8")
            };

            assert_eq!(
                buffered, streamed,
                "{format:?} differs between the two paths"
            );
        }
    }

    #[test]
    fn formats_that_cannot_stream_are_refused_rather_than_buffered() {
        for format in [Format::Json, Format::Markdown, Format::Table] {
            let error = StreamWriter::new(OutputOptions {
                format,
                ..OutputOptions::default()
            })
            .expect_err("must refuse");
            assert_eq!(error.exit_code(), crate::ExitCode::Usage);
            assert!(
                error.next_action.expect("action").contains("ndjson"),
                "it should name a format that works"
            );
        }
    }

    #[test]
    fn a_streamed_export_can_omit_its_header() {
        let mut out = Vec::new();
        let mut writer = StreamWriter::new(OutputOptions {
            format: Format::Csv,
            header: false,
            ..OutputOptions::default()
        })
        .expect("streamable");
        writer
            .columns(&mut out, vec!["id".into()])
            .expect("columns");
        writer
            .row(&mut out, &[Cell::Text("1".into())])
            .expect("row");
        assert_eq!(String::from_utf8(out).expect("utf8"), "1\n");
    }

    #[test]
    fn a_failed_execution_writes_nothing_to_the_data_stream() {
        // The diagnostic goes to stderr. Anything written here would land in the
        // middle of whatever a script is parsing.
        let execution = Execution {
            job: JobId(1),
            statements: Vec::new(),
            status: ExecutionStatus::Failed,
            elapsed: Duration::from_millis(1),
            error: Some(crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Query,
                "relation does not exist",
                "running statement 1",
            )),
            transaction: crate::query::result::TransactionState::Autocommit,
        };
        for format in [Format::Table, Format::Csv, Format::Json, Format::Ndjson] {
            let text = render(format, &execution);
            assert!(
                text.trim().is_empty() || format == Format::Json,
                "{format:?} wrote {text:?} to stdout for a failed query"
            );
        }
    }

    #[test]
    fn a_statement_with_no_rows_reports_what_it_did() {
        let execution = Execution {
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: None,
                rows_affected: Some(3),
                elapsed: Duration::from_millis(4),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(5),
            error: None,
            transaction: crate::query::result::TransactionState::Autocommit,
        };
        let text = render(Format::Table, &execution);
        assert!(text.contains("3 rows affected"), "{text}");
    }
}
