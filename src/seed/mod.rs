//! Pure domain models and deterministic PRNG fixture generation for synthetic database seeding.

use crate::sync::TableKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Fast, deterministic, reproducible 64-bit pseudo-random number generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeterministicPrng {
    state: u64,
}

impl DeterministicPrng {
    /// Creates a new PRNG initialized with the given 64-bit seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x517c_c1b7_2722_0a95
            } else {
                seed
            },
        }
    }

    /// Generates next pseudo-random 64-bit unsigned integer.
    pub fn next_u64(&mut self) -> u64 {
        // Linear congruential generator with Knuth's 64-bit constants
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    /// Generates next integer in range `[min, max]`.
    pub fn next_range(&mut self, min: i64, max: i64) -> i64 {
        if min >= max {
            return min;
        }
        let diff = (max - min + 1) as u64;
        let r = self.next_u64() % diff;
        min + r as i64
    }

    /// Chooses a random element from a non-empty slice.
    pub fn choose<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            let idx = (self.next_u64() as usize) % slice.len();
            Some(&slice[idx])
        }
    }
}

/// Generation strategy for synthesizing column values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedStrategy {
    /// Monotonically increasing sequence (for primary keys and serials).
    IntegerSequence { start: i64, step: i64 },
    /// Bounded integer range.
    IntegerRange { min: i64, max: i64 },
    /// Fixed-point decimal number.
    NumericDecimal {
        min_cents: i64,
        max_cents: i64,
        scale: u32,
    },
    /// RFC 4122 v4 UUID string.
    UuidV4,
    /// Human person name.
    PersonName,
    /// Email address.
    Email,
    /// Company or organization name.
    CompanyName,
    /// Phone number.
    PhoneNumber,
    /// Street address.
    Address,
    /// City name.
    City,
    /// Country name.
    Country,
    /// Boolean flag with specified percentage probability of `true`.
    BooleanRatio { true_percent: u8 },
    /// Calendar date.
    DateRange { start_year: i32, end_year: i32 },
    /// Timestamp with timezone.
    TimestampRange { start_year: i32, end_year: i32 },
    /// Choice from an enumerated list of valid strings.
    EnumChoice { choices: Vec<String> },
    /// Sampled reference from an existing parent table primary key pool.
    ForeignKeySample {
        parent_table: TableKey,
        parent_column: String,
    },
    /// Constant generic text.
    ConstantText { text: String },
}

impl SeedStrategy {
    /// Human-readable strategy name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::IntegerSequence { .. } => "IntegerSequence",
            Self::IntegerRange { .. } => "IntegerRange",
            Self::NumericDecimal { .. } => "NumericDecimal",
            Self::UuidV4 => "UuidV4",
            Self::PersonName => "PersonName",
            Self::Email => "Email",
            Self::CompanyName => "CompanyName",
            Self::PhoneNumber => "PhoneNumber",
            Self::Address => "Address",
            Self::City => "City",
            Self::Country => "Country",
            Self::BooleanRatio { .. } => "BooleanRatio",
            Self::DateRange { .. } => "DateRange",
            Self::TimestampRange { .. } => "TimestampRange",
            Self::EnumChoice { .. } => "EnumChoice",
            Self::ForeignKeySample { .. } => "ForeignKeySample",
            Self::ConstantText { .. } => "ConstantText",
        }
    }
}

/// Column metadata and assigned synthetic generation strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnGenerator {
    /// Column name in table.
    pub name: String,
    /// PostgreSQL data type as reported by catalog.
    pub data_type: String,
    /// Whether column allows NULL values.
    pub is_nullable: bool,
    /// Whether column is part of primary key.
    pub is_primary_key: bool,
    /// Assigned generation strategy.
    pub strategy: SeedStrategy,
}

impl ColumnGenerator {
    /// Infers default strategy based on column name, data type, constraints, and foreign key references.
    #[must_use]
    pub fn infer(
        column_name: &str,
        data_type: &str,
        is_pk: bool,
        is_nullable: bool,
        fk_ref: Option<(&TableKey, &str)>,
        enum_values: Option<&[String]>,
    ) -> Self {
        if let Some((parent_tbl, parent_col)) = fk_ref {
            return Self {
                name: column_name.to_string(),
                data_type: data_type.to_string(),
                is_nullable,
                is_primary_key: is_pk,
                strategy: SeedStrategy::ForeignKeySample {
                    parent_table: parent_tbl.clone(),
                    parent_column: parent_col.to_string(),
                },
            };
        }

        if let Some(choices) = enum_values
            && !choices.is_empty()
        {
            return Self {
                name: column_name.to_string(),
                data_type: data_type.to_string(),
                is_nullable,
                is_primary_key: is_pk,
                strategy: SeedStrategy::EnumChoice {
                    choices: choices.to_vec(),
                },
            };
        }

        let lower_name = column_name.to_ascii_lowercase();
        let lower_type = data_type.to_ascii_lowercase();

        let strategy = if is_pk && (lower_type.contains("int") || lower_type.contains("serial")) {
            SeedStrategy::IntegerSequence { start: 1, step: 1 }
        } else if lower_type.contains("uuid")
            || lower_name.ends_with("_uuid")
            || lower_name == "uuid"
        {
            SeedStrategy::UuidV4
        } else if lower_type.contains("bool") {
            SeedStrategy::BooleanRatio { true_percent: 50 }
        } else if lower_type.contains("timestamp") {
            SeedStrategy::TimestampRange {
                start_year: 2024,
                end_year: 2026,
            }
        } else if lower_type.contains("date") {
            SeedStrategy::DateRange {
                start_year: 2024,
                end_year: 2026,
            }
        } else if lower_name.contains("email") {
            SeedStrategy::Email
        } else if lower_name.contains("phone")
            || lower_name.contains("mobile")
            || lower_name.contains("tel")
        {
            SeedStrategy::PhoneNumber
        } else if lower_name.contains("company") || lower_name.contains("organization") {
            SeedStrategy::CompanyName
        } else if lower_name.contains("city") {
            SeedStrategy::City
        } else if lower_name.contains("country") {
            SeedStrategy::Country
        } else if lower_name.contains("address") || lower_name.contains("street") {
            SeedStrategy::Address
        } else if lower_name.contains("name") {
            SeedStrategy::PersonName
        } else if lower_name.contains("price")
            || lower_name.contains("amount")
            || lower_name.contains("total")
            || lower_name.contains("cost")
            || lower_name.contains("salary")
            || lower_name.contains("balance")
            || lower_type.contains("numeric")
            || lower_type.contains("decimal")
            || lower_type.contains("money")
        {
            SeedStrategy::NumericDecimal {
                min_cents: 500,
                max_cents: 99_900,
                scale: 2,
            }
        } else if lower_type.contains("int") || lower_type.contains("serial") {
            SeedStrategy::IntegerRange {
                min: 1,
                max: 10_000,
            }
        } else {
            SeedStrategy::ConstantText {
                text: format!("sample_{column_name}"),
            }
        };

        Self {
            name: column_name.to_string(),
            data_type: data_type.to_string(),
            is_nullable,
            is_primary_key: is_pk,
            strategy,
        }
    }

    /// Generates a single SQL-escaped literal string representation for a row.
    pub fn generate_sql_literal(
        &self,
        prng: &mut DeterministicPrng,
        row_idx: usize,
        pk_pools: &HashMap<TableKey, Vec<String>>,
    ) -> String {
        match &self.strategy {
            SeedStrategy::IntegerSequence { start, step } => {
                let val = start + (row_idx as i64 * step);
                val.to_string()
            }
            SeedStrategy::IntegerRange { min, max } => {
                let val = prng.next_range(*min, *max);
                val.to_string()
            }
            SeedStrategy::NumericDecimal {
                min_cents,
                max_cents,
                scale,
            } => {
                let cents = prng.next_range(*min_cents, *max_cents);
                let divisor = 10_i64.pow(*scale);
                let integer = cents / divisor;
                let fraction = (cents % divisor).abs();
                format!("{integer}.{fraction:02}")
            }
            SeedStrategy::UuidV4 => {
                let p1 = prng.next_u64();
                let p2 = prng.next_u64();
                // Format into RFC 4122 v4 UUID shape
                let d1 = (p1 >> 32) as u32;
                let d2 = (p1 >> 16) as u16;
                let d3 = (p1 & 0x0fff) as u16 | 0x4000; // version 4
                let d4 = ((p2 >> 48) & 0x3fff) as u16 | 0x8000; // variant 1
                let d5 = p2 & 0x0000_ffff_ffff_ffff;
                format!("'{d1:08x}-{d2:04x}-{d3:04x}-{d4:04x}-{d5:012x}'")
            }
            SeedStrategy::PersonName => {
                const FIRSTS: &[&str] = &[
                    "Alex", "Jordan", "Taylor", "Morgan", "Sam", "Chris", "Pat", "Riley", "Robin",
                    "Casey", "Logan", "Jesse", "Avery", "Cameron", "Quinn", "Skyler", "Rowan",
                ];
                const LASTS: &[&str] = &[
                    "Smith",
                    "Johnson",
                    "Williams",
                    "Brown",
                    "Jones",
                    "Garcia",
                    "Miller",
                    "Davis",
                    "Rodriguez",
                    "Martinez",
                    "Hernandez",
                    "Lopez",
                    "Gonzalez",
                    "Wilson",
                    "Anderson",
                ];
                let first = prng.choose(FIRSTS).copied().unwrap_or("John");
                let last = prng.choose(LASTS).copied().unwrap_or("Doe");
                format!("'{first} {last}'")
            }
            SeedStrategy::Email => {
                const DOMAINS: &[&str] = &[
                    "example.com",
                    "test.org",
                    "fixtures.net",
                    "sample.io",
                    "ignatius.dev",
                ];
                let user_id = prng.next_range(100, 9999);
                let domain = prng.choose(DOMAINS).copied().unwrap_or("example.com");
                format!("'user{user_id}@{domain}'")
            }
            SeedStrategy::CompanyName => {
                const ADJS: &[&str] = &[
                    "Apex", "Summit", "Quantum", "Nexus", "Vertex", "Pinnacle", "Vanguard",
                    "Horizon",
                ];
                const NOUNS: &[&str] = &[
                    "Technologies",
                    "Solutions",
                    "Industries",
                    "Enterprises",
                    "Logistics",
                    "Systems",
                ];
                let adj = prng.choose(ADJS).copied().unwrap_or("Global");
                let noun = prng.choose(NOUNS).copied().unwrap_or("Services");
                format!("'{adj} {noun}'")
            }
            SeedStrategy::PhoneNumber => {
                let area = prng.next_range(200, 999);
                let mid = prng.next_range(100, 999);
                let last = prng.next_range(1000, 9999);
                format!("'+1-{area}-{mid}-{last}'")
            }
            SeedStrategy::Address => {
                let num = prng.next_range(10, 9999);
                const STREETS: &[&str] = &[
                    "Main St",
                    "Market St",
                    "Oak Ave",
                    "Pine Rd",
                    "Cedar Blvd",
                    "Elm St",
                    "Broadway",
                ];
                let street = prng.choose(STREETS).copied().unwrap_or("Main St");
                format!("'{num} {street}'")
            }
            SeedStrategy::City => {
                const CITIES: &[&str] = &[
                    "Springfield",
                    "Riverdale",
                    "Centerville",
                    "Franklin",
                    "Greenville",
                    "Bristol",
                    "Fairview",
                    "Salem",
                ];
                let city = prng.choose(CITIES).copied().unwrap_or("Metropolis");
                format!("'{city}'")
            }
            SeedStrategy::Country => {
                const COUNTRIES: &[&str] = &[
                    "United States",
                    "United Kingdom",
                    "Germany",
                    "France",
                    "Canada",
                    "Australia",
                    "Japan",
                    "Netherlands",
                ];
                let country = prng.choose(COUNTRIES).copied().unwrap_or("United States");
                format!("'{country}'")
            }
            SeedStrategy::BooleanRatio { true_percent } => {
                let roll = prng.next_range(1, 100);
                if roll <= *true_percent as i64 {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            SeedStrategy::DateRange {
                start_year,
                end_year,
            } => {
                let year = prng.next_range(*start_year as i64, *end_year as i64);
                let month = prng.next_range(1, 12);
                let day = prng.next_range(1, 28);
                format!("'{year:04}-{month:02}-{day:02}'")
            }
            SeedStrategy::TimestampRange {
                start_year,
                end_year,
            } => {
                let year = prng.next_range(*start_year as i64, *end_year as i64);
                let month = prng.next_range(1, 12);
                let day = prng.next_range(1, 28);
                let hour = prng.next_range(0, 23);
                let minute = prng.next_range(0, 59);
                let second = prng.next_range(0, 59);
                format!("'{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}+00'")
            }
            SeedStrategy::EnumChoice { choices } => {
                let choice = prng
                    .choose(choices)
                    .map(String::as_str)
                    .unwrap_or("DEFAULT");
                format!("'{}'", choice.replace('\'', "''"))
            }
            SeedStrategy::ForeignKeySample { parent_table, .. } => {
                if let Some(pool) = pk_pools.get(parent_table)
                    && !pool.is_empty()
                {
                    let sampled = prng.choose(pool).unwrap();
                    sampled.clone()
                } else {
                    // Fallback to sequential ID 1 if pool is empty
                    "1".to_string()
                }
            }
            SeedStrategy::ConstantText { text } => {
                format!("'{}'", text.replace('\'', "''"))
            }
        }
    }
}

/// Table synthetic generator configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableGenerator {
    /// Table identifier.
    pub key: TableKey,
    /// Column generators.
    pub columns: Vec<ColumnGenerator>,
}

/// Generated synthetic data rows for a single table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedTableData {
    /// Table identifier.
    pub key: TableKey,
    /// Column names.
    pub columns: Vec<String>,
    /// Rows of string values (unquoted for display/CSV/JSON, SQL literals for SQL).
    pub rows: Vec<Vec<String>>,
}

/// Comprehensive report and container of synthesized test fixtures.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FixtureReport {
    /// 64-bit PRNG seed used.
    pub seed: u64,
    /// Target schema name, if scoped.
    pub schema: Option<String>,
    /// Per-table generated fixture datasets.
    pub tables: Vec<GeneratedTableData>,
    /// Total row count generated across all tables.
    pub total_rows: usize,
    /// Generation duration in milliseconds.
    pub duration_ms: u64,
}

impl FixtureReport {
    /// Generates executable transactional SQL script (`BEGIN; ... INSERT ...; COMMIT;`).
    #[must_use]
    pub fn render_sql(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "-- Ignatius Synthetic Test Fixtures");
        let _ = writeln!(
            out,
            "-- Generated with deterministic seed: {} | Total Rows: {}",
            self.seed, self.total_rows
        );
        let _ = writeln!(out, "BEGIN;");
        let _ = writeln!(out);

        for table in &self.tables {
            if table.rows.is_empty() {
                continue;
            }
            let cols = table
                .columns
                .iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(", ");

            let _ = writeln!(
                out,
                "-- Fixtures for table: {} ({} rows)",
                table.key.display(),
                table.rows.len()
            );

            for chunk in table.rows.chunks(100) {
                let _ = writeln!(
                    out,
                    "INSERT INTO {} ({}) VALUES",
                    table.key.qualified_sql(),
                    cols
                );
                for (row_idx, row) in chunk.iter().enumerate() {
                    let vals = row.join(", ");
                    let terminator = if row_idx == chunk.len() - 1 { ";" } else { "," };
                    let _ = writeln!(out, "  ({vals}){terminator}");
                }
            }
            let _ = writeln!(out);
        }

        let _ = writeln!(out, "COMMIT;");
        out
    }

    /// Generates structured newline-delimited JSON (NDJSON) per table.
    #[must_use]
    pub fn render_jsonl(&self) -> String {
        let mut out = String::new();
        for table in &self.tables {
            for row in &table.rows {
                let mut map = serde_json::Map::new();
                for (col, val) in table.columns.iter().zip(row.iter()) {
                    let clean_val = val.trim_matches('\'').to_string();
                    map.insert(col.clone(), serde_json::Value::String(clean_val));
                }
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "_table".to_string(),
                    serde_json::Value::String(table.key.display()),
                );
                obj.insert("data".to_string(), serde_json::Value::Object(map));
                if let Ok(line) = serde_json::to_string(&obj) {
                    let _ = writeln!(out, "{line}");
                }
            }
        }
        out
    }

    /// Generates standard CSV format per table.
    #[must_use]
    pub fn render_csv(&self) -> String {
        let mut out = String::new();
        for table in &self.tables {
            let _ = writeln!(out, "# Table: {}", table.key.display());
            let _ = writeln!(out, "{}", table.columns.join(","));
            for row in &table.rows {
                let clean_row: Vec<String> = row
                    .iter()
                    .map(|v| {
                        let inner = v.trim_matches('\'');
                        if inner.contains(',') || inner.contains('"') {
                            format!("\"{}\"", inner.replace('"', "\"\""))
                        } else {
                            inner.to_string()
                        }
                    })
                    .collect();
                let _ = writeln!(out, "{}", clean_row.join(","));
            }
            let _ = writeln!(out);
        }
        out
    }

    /// Generates an aligned tabular summary of synthesized fixtures.
    #[must_use]
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "=== Ignatius Synthetic Fixture Generator ===");
        let _ = writeln!(
            out,
            "Seed: {} | Total Tables: {} | Total Rows: {} | Generation Time: {}ms",
            self.seed,
            self.tables.len(),
            self.total_rows,
            self.duration_ms
        );
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<16} {:<24} {:>10} {:>10} {:<30}",
            "SCHEMA", "TABLE", "COLUMNS", "ROWS", "SAMPLE PRIMARY KEY"
        );
        let _ = writeln!(out, "{}", "-".repeat(94));

        for t in &self.tables {
            let sample_pk = t
                .rows
                .first()
                .and_then(|r| r.first())
                .cloned()
                .unwrap_or_else(|| "-".to_string());
            let _ = writeln!(
                out,
                "{:<16} {:<24} {:>10} {:>10} {:<30}",
                crate::sync::truncate_str(&t.key.schema, 16),
                crate::sync::truncate_str(&t.key.table, 24),
                t.columns.len(),
                t.rows.len(),
                crate::sync::truncate_str(&sample_pk, 30)
            );
        }
        let _ = writeln!(out);
        out
    }
}

/// Executes fixture generation across ordered tables using a deterministic PRNG.
#[must_use]
pub fn generate_fixtures(
    tables: &[TableGenerator],
    rows_per_table: usize,
    seed: u64,
) -> FixtureReport {
    let start = std::time::Instant::now();
    let mut prng = DeterministicPrng::new(seed);
    let mut generated_tables = Vec::with_capacity(tables.len());
    let mut pk_pools: HashMap<TableKey, Vec<String>> = HashMap::new();
    let mut total_rows = 0;

    for table in tables {
        let mut table_rows = Vec::with_capacity(rows_per_table);
        let mut table_pks = Vec::with_capacity(rows_per_table);
        let column_names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();

        for row_idx in 0..rows_per_table {
            let mut row_vals = Vec::with_capacity(table.columns.len());
            for col in &table.columns {
                let val = col.generate_sql_literal(&mut prng, row_idx, &pk_pools);
                if col.is_primary_key {
                    table_pks.push(val.clone());
                }
                row_vals.push(val);
            }
            table_rows.push(row_vals);
            total_rows += 1;
        }

        pk_pools.insert(table.key.clone(), table_pks);
        generated_tables.push(GeneratedTableData {
            key: table.key.clone(),
            columns: column_names,
            rows: table_rows,
        });
    }

    FixtureReport {
        seed,
        schema: None,
        tables: generated_tables,
        total_rows,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests;
