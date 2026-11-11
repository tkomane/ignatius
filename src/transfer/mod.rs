//! Pure domain engine for bulk data transfer, streaming COPY, and backup/restore.
//!
//! Provides protocol-level SQL builders for PostgreSQL `COPY ... FROM STDIN`
//! and `COPY ... TO STDOUT`, CSV/TSV delimiter detection and sample parsing,
//! column type inference, and command-line generators for `pg_dump` and `pg_restore`.

use std::fmt;

#[cfg(test)]
mod tests;

/// Supported transfer formats for bulk import and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferFormat {
    /// Comma-separated values (RFC 4180).
    #[default]
    Csv,
    /// Tab-separated values.
    Tsv,
    /// Newline-delimited JSON (one JSON object per row).
    Jsonl,
    /// Standard SQL INSERT statements.
    SqlInsert,
    /// PostgreSQL binary COPY protocol format.
    Binary,
}

impl TransferFormat {
    /// Parses a format name string into a `TransferFormat`.
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
            "jsonl" | "ndjson" => Some(Self::Jsonl),
            "sql" | "insert" | "sql-insert" => Some(Self::SqlInsert),
            "bin" | "binary" => Some(Self::Binary),
            _ => None,
        }
    }

    /// Returns the canonical format name.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Jsonl => "jsonl",
            Self::SqlInsert => "sql",
            Self::Binary => "binary",
        }
    }
}

impl fmt::Display for TransferFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Options configuring a bulk data import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    /// Schema of the destination table.
    pub schema: String,
    /// Destination table name.
    pub table: String,
    /// Source file path or identifier.
    pub file_path: String,
    /// Transfer format.
    pub format: TransferFormat,
    /// Field delimiter byte (e.g. `b','` or `b'\t'`).
    pub delimiter: u8,
    /// Whether the first line is a column header row.
    pub has_header: bool,
    /// String representing NULL values.
    pub null_string: String,
    /// Quote character (e.g. `b'"'`).
    pub quote: u8,
    /// Escape character (e.g. `b'"'`).
    pub escape: u8,
    /// Optional specific column names to import into.
    pub columns: Vec<String>,
    /// Whether this is a dry-run test (executed in transaction then rolled back).
    pub dry_run: bool,
    /// Whether to wrap the import in a transaction (`BEGIN; ... COMMIT;`).
    pub transactional: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            schema: "public".to_owned(),
            table: String::new(),
            file_path: String::new(),
            format: TransferFormat::Csv,
            delimiter: b',',
            has_header: true,
            null_string: String::new(),
            quote: b'"',
            escape: b'"',
            columns: Vec::new(),
            dry_run: false,
            transactional: true,
        }
    }
}

/// A mapped column in the import wizard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportColumnMapping {
    /// Column name from the source file.
    pub source_column: String,
    /// Target column name in the database table.
    pub target_column: String,
    /// Inferred or target PostgreSQL data type.
    pub target_type: String,
    /// Whether this column allows null values.
    pub is_nullable: bool,
    /// Whether to skip importing this column.
    pub skip: bool,
}

/// Preview of source file data and inferred structure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportPreview {
    /// Detected header names.
    pub headers: Vec<String>,
    /// Sample parsed rows (up to preview bound).
    pub sample_rows: Vec<Vec<String>>,
    /// Column mapping recommendations.
    pub column_mappings: Vec<ImportColumnMapping>,
    /// Estimated total rows in the input.
    pub estimated_rows: usize,
}

/// Options configuring a bulk data export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    /// SQL query or table expression to export.
    pub query: String,
    /// Destination file path, or None for standard output.
    pub output_path: Option<String>,
    /// Transfer format.
    pub format: TransferFormat,
    /// Delimiter byte.
    pub delimiter: u8,
    /// Whether to include a header row in the export.
    pub has_header: bool,
    /// String representing NULL values.
    pub null_string: String,
    /// Quote character.
    pub quote: u8,
    /// Escape character.
    pub escape: u8,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            output_path: None,
            format: TransferFormat::Csv,
            delimiter: b',',
            has_header: true,
            null_string: String::new(),
            quote: b'"',
            escape: b'"',
        }
    }
}

/// Supported backup formats for `pg_dump`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackupFormat {
    /// Custom pg_dump archive format (`-F c`).
    #[default]
    Custom,
    /// Plain SQL script (`-F p`).
    Plain,
    /// Directory archive (`-F d`).
    Directory,
    /// Tar archive (`-F t`).
    Tar,
}

impl BackupFormat {
    /// Returns the pg_dump `-F` flag argument.
    #[must_use]
    pub const fn flag_arg(&self) -> &'static str {
        match self {
            Self::Custom => "c",
            Self::Plain => "p",
            Self::Directory => "d",
            Self::Tar => "t",
        }
    }

    /// Parses from string representation.
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "custom" | "c" => Some(Self::Custom),
            "plain" | "p" | "sql" => Some(Self::Plain),
            "directory" | "dir" | "d" => Some(Self::Directory),
            "tar" | "t" => Some(Self::Tar),
            _ => None,
        }
    }
}

/// Options configuring a `pg_dump` backup execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOptions {
    /// Target database name.
    pub database: String,
    /// Database host.
    pub host: Option<String>,
    /// Database port.
    pub port: Option<u16>,
    /// Database user.
    pub user: Option<String>,
    /// Output file or directory path.
    pub output_path: String,
    /// Backup format.
    pub format: BackupFormat,
    /// Optional specific schema to back up.
    pub schema: Option<String>,
    /// Optional specific table to back up.
    pub table: Option<String>,
    /// Clean (drop) database objects prior to output.
    pub clean: bool,
    /// Include commands to create the database.
    pub create: bool,
    /// Dump only data, not schema.
    pub data_only: bool,
    /// Dump only schema, no data.
    pub schema_only: bool,
    /// Verbose output mode.
    pub verbose: bool,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            database: String::new(),
            host: None,
            port: None,
            user: None,
            output_path: String::new(),
            format: BackupFormat::Custom,
            schema: None,
            table: None,
            clean: false,
            create: false,
            data_only: false,
            schema_only: false,
            verbose: false,
        }
    }
}

/// Options configuring a `pg_restore` execution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreOptions {
    /// Target database name.
    pub database: String,
    /// Database host.
    pub host: Option<String>,
    /// Database port.
    pub port: Option<u16>,
    /// Database user.
    pub user: Option<String>,
    /// Input file or directory path.
    pub input_path: String,
    /// Clean (drop) database objects prior to restore.
    pub clean: bool,
    /// Create the database before restoring into it.
    pub create: bool,
    /// Restore only data.
    pub data_only: bool,
    /// Restore only schema.
    pub schema_only: bool,
    /// Verbose output mode.
    pub verbose: bool,
}

/// Result summary of an import execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    /// Number of rows successfully imported.
    pub rows_imported: u64,
    /// Total bytes processed.
    pub bytes_processed: u64,
    /// Elapsed duration in milliseconds.
    pub elapsed_ms: u64,
    /// Status description.
    pub status: String,
}

/// Builds the PostgreSQL `COPY ... FROM STDIN` SQL statement.
#[must_use]
pub fn build_copy_from_sql(options: &ImportOptions) -> String {
    let target = if options.schema.is_empty() {
        format!("\"{}\"", options.table)
    } else {
        format!("\"{}\".\"{}\"", options.schema, options.table)
    };

    let cols = if options.columns.is_empty() {
        String::new()
    } else {
        let quoted: Vec<String> = options.columns.iter().map(|c| format!("\"{c}\"")).collect();
        format!(" ({})", quoted.join(", "))
    };

    match options.format {
        TransferFormat::Binary => {
            format!("COPY {target}{cols} FROM STDIN WITH (FORMAT binary)")
        }
        TransferFormat::Csv => {
            let delim_char = options.delimiter as char;
            let quote_char = options.quote as char;
            let escape_char = options.escape as char;
            format!(
                "COPY {target}{cols} FROM STDIN WITH (FORMAT csv, DELIMITER '{delim_char}', HEADER {}, NULL '{}', QUOTE '{quote_char}', ESCAPE '{escape_char}')",
                options.has_header,
                escape_sql_literal(&options.null_string)
            )
        }
        TransferFormat::Tsv => {
            format!(
                "COPY {target}{cols} FROM STDIN WITH (FORMAT text, DELIMITER E'\\t', NULL '{}')",
                escape_sql_literal(&options.null_string)
            )
        }
        TransferFormat::Jsonl | TransferFormat::SqlInsert => {
            // For JSONL or SQL insert, intermediate row streaming or COPY text staging is used
            let delim_char = options.delimiter as char;
            format!(
                "COPY {target}{cols} FROM STDIN WITH (FORMAT csv, DELIMITER '{delim_char}', HEADER {}, NULL '{}')",
                options.has_header,
                escape_sql_literal(&options.null_string)
            )
        }
    }
}

/// Builds the PostgreSQL `COPY ... TO STDOUT` SQL statement.
#[must_use]
pub fn build_copy_to_sql(options: &ExportOptions) -> String {
    let trimmed = options.query.trim();
    let is_select = trimmed.starts_with("select")
        || trimmed.starts_with("SELECT")
        || trimmed.starts_with("with")
        || trimmed.starts_with("WITH");

    let source = if is_select {
        // Strip trailing semicolon if present
        let clean = trimmed.strip_suffix(';').unwrap_or(trimmed);
        format!("({clean})")
    } else if trimmed.contains('.') {
        trimmed.to_owned()
    } else {
        format!("\"{}\"", trimmed)
    };

    match options.format {
        TransferFormat::Binary => {
            format!("COPY {source} TO STDOUT WITH (FORMAT binary)")
        }
        TransferFormat::Csv => {
            let delim_char = options.delimiter as char;
            let quote_char = options.quote as char;
            let escape_char = options.escape as char;
            format!(
                "COPY {source} TO STDOUT WITH (FORMAT csv, DELIMITER '{delim_char}', HEADER {}, NULL '{}', QUOTE '{quote_char}', ESCAPE '{escape_char}')",
                options.has_header,
                escape_sql_literal(&options.null_string)
            )
        }
        TransferFormat::Tsv => {
            format!(
                "COPY {source} TO STDOUT WITH (FORMAT text, DELIMITER E'\\t', NULL '{}')",
                escape_sql_literal(&options.null_string)
            )
        }
        TransferFormat::Jsonl | TransferFormat::SqlInsert => {
            let delim_char = options.delimiter as char;
            format!(
                "COPY {source} TO STDOUT WITH (FORMAT csv, DELIMITER '{delim_char}', HEADER {}, NULL '{}')",
                options.has_header,
                escape_sql_literal(&options.null_string)
            )
        }
    }
}

/// Automatically detects the CSV/TSV delimiter by analyzing candidate characters outside quotes.
#[must_use]
pub fn detect_csv_delimiter(sample: &str) -> u8 {
    let candidates = *b",\t;|";
    let mut scores = [0usize; 4];

    for line in sample.lines().take(20) {
        let mut in_quotes = false;
        let mut line_counts = [0usize; 4];

        for &b in line.as_bytes() {
            if b == b'"' {
                in_quotes = !in_quotes;
            } else if !in_quotes {
                for (idx, &candidate) in candidates.iter().enumerate() {
                    if b == candidate {
                        line_counts[idx] += 1;
                    }
                }
            }
        }

        for (idx, count) in line_counts.iter().enumerate() {
            if *count > 0 {
                scores[idx] += *count;
            }
        }
    }

    // Default to comma if no candidate is detected
    let mut best_idx = 0;
    let mut best_score = 0;
    for (idx, &score) in scores.iter().enumerate() {
        if score > best_score {
            best_score = score;
            best_idx = idx;
        }
    }

    candidates[best_idx]
}

/// Parses a CSV or TSV sample into header columns and sample rows.
#[must_use]
pub fn parse_csv_header_and_sample(
    content: &str,
    delimiter: u8,
    max_rows: usize,
) -> (Vec<String>, Vec<Vec<String>>) {
    let mut rows = Vec::new();
    let mut current_row = Vec::new();
    let mut current_field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                chars.next();
                current_field.push('"');
            } else {
                in_quotes = !in_quotes;
            }
        } else if !in_quotes && ch == delimiter as char {
            current_row.push(current_field);
            current_field = String::new();
        } else if !in_quotes && (ch == '\n' || ch == '\r') {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            current_row.push(current_field);
            current_field = String::new();

            if !current_row.iter().all(|s| s.trim().is_empty()) {
                rows.push(current_row);
            }
            current_row = Vec::new();

            if rows.len() > max_rows {
                break;
            }
        } else {
            current_field.push(ch);
        }
    }

    if !current_field.is_empty() || !current_row.is_empty() {
        current_row.push(current_field);
        if !current_row.iter().all(|s| s.trim().is_empty()) {
            rows.push(current_row);
        }
    }

    if rows.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let headers = rows.remove(0);
    let sample = rows.into_iter().take(max_rows).collect();
    (headers, sample)
}

/// Infers the PostgreSQL data type from a slice of sample string values.
#[must_use]
pub fn infer_column_type(values: &[&str]) -> &'static str {
    let non_empty: Vec<&str> = values
        .iter()
        .copied()
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("null") && *v != "\\N")
        .collect();

    if non_empty.is_empty() {
        return "text";
    }

    // Check boolean
    let all_bool = non_empty.iter().all(|v| {
        matches!(
            v.to_ascii_lowercase().as_str(),
            "true" | "false" | "t" | "f" | "1" | "0" | "yes" | "no"
        )
    });
    if all_bool {
        return "boolean";
    }

    // Check integer
    let all_int = non_empty.iter().all(|v| v.parse::<i64>().is_ok());
    if all_int {
        return "bigint";
    }

    // Check numeric / float
    let all_numeric = non_empty.iter().all(|v| v.parse::<f64>().is_ok());
    if all_numeric {
        return "numeric";
    }

    // Check timestamp / date (e.g. YYYY-MM-DD)
    let all_date = non_empty.iter().all(|v| {
        v.len() >= 10
            && (v.chars().nth(4) == Some('-') && v.chars().nth(7) == Some('-')
                || v.chars().nth(2) == Some('/') && v.chars().nth(5) == Some('/'))
    });
    if all_date {
        return "timestamptz";
    }

    // Check JSON
    let all_json = non_empty.iter().all(|v| {
        (v.starts_with('{') && v.ends_with('}')) || (v.starts_with('[') && v.ends_with(']'))
    });
    if all_json {
        return "jsonb";
    }

    "text"
}

/// Builds the `pg_dump` command invocation and environment variables.
#[must_use]
pub fn build_pg_dump_command(
    options: &BackupOptions,
    password: Option<&str>,
    redact: bool,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let mut args = Vec::new();

    args.push("-F".to_owned());
    args.push(options.format.flag_arg().to_owned());

    if let Some(host) = &options.host {
        args.push("-h".to_owned());
        args.push(host.clone());
    }

    if let Some(port) = options.port {
        args.push("-p".to_owned());
        args.push(port.to_string());
    }

    if let Some(user) = &options.user {
        args.push("-U".to_owned());
        args.push(user.clone());
    }

    if options.clean {
        args.push("--clean".to_owned());
    }

    if options.create {
        args.push("--create".to_owned());
    }

    if options.data_only {
        args.push("--data-only".to_owned());
    }

    if options.schema_only {
        args.push("--schema-only".to_owned());
    }

    if options.verbose {
        args.push("--verbose".to_owned());
    }

    if let Some(schema) = &options.schema {
        args.push("-n".to_owned());
        args.push(schema.clone());
    }

    if let Some(table) = &options.table {
        args.push("-t".to_owned());
        args.push(table.clone());
    }

    args.push("-f".to_owned());
    args.push(options.output_path.clone());

    args.push(options.database.clone());

    let mut env = Vec::new();
    if let Some(pass) = password {
        let val = if redact {
            crate::diagnostics::redaction::REDACTED.to_owned()
        } else {
            pass.to_owned()
        };
        env.push(("PGPASSWORD".to_owned(), val));
    }

    ("pg_dump".to_owned(), args, env)
}

/// Builds the `pg_restore` command invocation and environment variables.
#[must_use]
pub fn build_pg_restore_command(
    options: &RestoreOptions,
    password: Option<&str>,
    redact: bool,
) -> (String, Vec<String>, Vec<(String, String)>) {
    let mut args = Vec::new();

    if let Some(host) = &options.host {
        args.push("-h".to_owned());
        args.push(host.clone());
    }

    if let Some(port) = options.port {
        args.push("-p".to_owned());
        args.push(port.to_string());
    }

    if let Some(user) = &options.user {
        args.push("-U".to_owned());
        args.push(user.clone());
    }

    args.push("-d".to_owned());
    args.push(options.database.clone());

    if options.clean {
        args.push("--clean".to_owned());
    }

    if options.create {
        args.push("--create".to_owned());
    }

    if options.data_only {
        args.push("--data-only".to_owned());
    }

    if options.schema_only {
        args.push("--schema-only".to_owned());
    }

    if options.verbose {
        args.push("--verbose".to_owned());
    }

    args.push(options.input_path.clone());

    let mut env = Vec::new();
    if let Some(pass) = password {
        let val = if redact {
            crate::diagnostics::redaction::REDACTED.to_owned()
        } else {
            pass.to_owned()
        };
        env.push(("PGPASSWORD".to_owned(), val));
    }

    ("pg_restore".to_owned(), args, env)
}

fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}

/// Renders a transfer result summary as JSON.
#[must_use]
pub fn render_transfer_result_json(result: &ImportResult) -> String {
    format!(
        "{{\n  \"rows\": {},\n  \"bytes\": {},\n  \"elapsed_ms\": {},\n  \"status\": \"{}\"\n}}",
        result.rows_imported,
        result.bytes_processed,
        result.elapsed_ms,
        json_escape(&result.status)
    )
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}
