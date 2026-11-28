//! Pure, bounded structured filtering over retained [`ResultSet`] rows.
//!
//! Nothing here executes SQL, touches disk, or allocates unboundedly. Plain
//! substring searches (`alice`, `12:30:00`, `https://...`) keep working
//! identically while column predicates (`status:=failed`, `amount:>100`,
//! `deleted_at:null`, `name:"foo bar"`) and exclusions (`!debug`) narrow
//! retained rows in memory.

use crate::query::result::ResultSet;
use crate::query::value::Cell;

/// Maximum number of whitespace-separated terms evaluated per filter expression.
pub const MAX_FILTER_TERMS: usize = 16;

/// Maximum UTF-8 byte length admitted for a filter expression.
pub const MAX_FILTER_LEN: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumericCmp {
    Gt,
    Ge,
    Lt,
    Le,
}

impl NumericCmp {
    const fn matches(self, cell_value: f64, target: f64) -> bool {
        match self {
            Self::Gt => cell_value > target,
            Self::Ge => cell_value >= target,
            Self::Lt => cell_value < target,
            Self::Le => cell_value <= target,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ColumnFilterOp {
    Contains(String),
    Equals(String),
    NotEquals(String),
    IsNull,
    IsNotNull,
    Numeric { cmp: NumericCmp, value: f64 },
}

impl ColumnFilterOp {
    fn matches_cell(&self, cell: &Cell) -> bool {
        match self {
            Self::Contains(needle) => cell.display().to_lowercase().contains(needle),
            Self::Equals(needle) => match cell {
                Cell::Text(raw) => raw.to_lowercase() == *needle,
                Cell::Null => false,
            },
            Self::NotEquals(needle) => match cell {
                Cell::Text(raw) => raw.to_lowercase() != *needle,
                Cell::Null => true,
            },
            Self::IsNull => matches!(cell, Cell::Null),
            Self::IsNotNull => !matches!(cell, Cell::Null),
            Self::Numeric { cmp, value } => match cell {
                Cell::Text(raw) => raw
                    .trim()
                    .parse::<f64>()
                    .is_ok_and(|parsed| parsed.is_finite() && cmp.matches(parsed, *value)),
                Cell::Null => false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum FilterTerm {
    AnyContains(String),
    AnyExcludes(String),
    Column {
        columns: Vec<usize>,
        op: ColumnFilterOp,
    },
    Unsatisfied,
}

/// Parsed structured filter compiled against a specific [`ResultSet`] header.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedResultFilter {
    terms: Vec<FilterTerm>,
    /// Column names explicitly referenced with structured operators that do not
    /// exist in the active [`ResultSet`].
    pub unknown_columns: Vec<String>,
    /// Whether the input exceeded [`MAX_FILTER_TERMS`] or [`MAX_FILTER_LEN`].
    pub truncated: bool,
}

impl ParsedResultFilter {
    /// Parses `raw` against `columns` (matching column names case-insensitively).
    #[must_use]
    pub fn parse(raw: &str, columns: &[String]) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Self {
                terms: Vec::new(),
                unknown_columns: Vec::new(),
                truncated: false,
            };
        }

        let mut truncated = false;
        let bounded = if trimmed.len() > MAX_FILTER_LEN {
            truncated = true;
            let mut end = MAX_FILTER_LEN;
            while end > 0 && !trimmed.is_char_boundary(end) {
                end -= 1;
            }
            &trimmed[..end]
        } else {
            trimmed
        };

        let tokens = tokenize_filter(bounded);
        let mut terms = Vec::new();
        let mut unknown_columns = Vec::new();

        for token in tokens {
            if terms.len() >= MAX_FILTER_TERMS {
                truncated = true;
                break;
            }
            if token.raw.is_empty() {
                continue;
            }
            if token.fully_quoted {
                terms.push(FilterTerm::AnyContains(token.unquoted.to_lowercase()));
                continue;
            }
            if let Some(negated) = token.raw.strip_prefix('!')
                && !negated.is_empty()
            {
                terms.push(FilterTerm::AnyExcludes(
                    unquote_value(negated).to_lowercase(),
                ));
                continue;
            }

            if let Some((col_name, rhs, col_was_quoted)) = split_column_token(&token.raw) {
                let matching_cols: Vec<usize> = columns
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| name.eq_ignore_ascii_case(&col_name))
                    .map(|(index, _)| index)
                    .collect();

                if !matching_cols.is_empty() {
                    let op = parse_column_op(rhs);
                    terms.push(FilterTerm::Column {
                        columns: matching_cols,
                        op,
                    });
                    continue;
                }

                if col_was_quoted || has_explicit_column_operator(rhs) {
                    if !unknown_columns.iter().any(|known| known == &col_name) {
                        unknown_columns.push(col_name);
                    }
                    terms.push(FilterTerm::Unsatisfied);
                    continue;
                }
            }

            terms.push(FilterTerm::AnyContains(token.unquoted.to_lowercase()));
        }

        Self {
            terms,
            unknown_columns,
            truncated,
        }
    }

    /// Returns whether `row` satisfies every parsed term.
    #[must_use]
    pub fn matches_row(&self, row: &[Cell]) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        self.terms.iter().all(|term| match term {
            FilterTerm::AnyContains(needle) => row
                .iter()
                .any(|cell| cell.display().to_lowercase().contains(needle)),
            FilterTerm::AnyExcludes(needle) => !row
                .iter()
                .any(|cell| cell.display().to_lowercase().contains(needle)),
            FilterTerm::Column { columns, op } => columns
                .iter()
                .any(|&index| row.get(index).is_some_and(|cell| op.matches_cell(cell))),
            FilterTerm::Unsatisfied => false,
        })
    }
}

/// Returns retained row indices of `set` matching `filter`.
#[must_use]
pub fn filter_rows(set: &ResultSet, filter: &str) -> Vec<usize> {
    if !set.is_available() {
        return Vec::new();
    }
    if filter.trim().is_empty() {
        return (0..set.rows.len()).collect();
    }
    let parsed = ParsedResultFilter::parse(filter, &set.columns);
    set.rows
        .iter()
        .enumerate()
        .filter(|(_, row)| parsed.matches_row(row))
        .map(|(index, _)| index)
        .collect()
}

/// Builds a structured filter expression matching the selected cell (`col:=val`
/// or `col:null`).
#[must_use]
pub fn build_cell_filter_expression(column: &str, cell: &Cell) -> String {
    let col_part = format_filter_atom(column, true);
    match cell {
        Cell::Null => format!("{col_part}:null"),
        Cell::Text(raw) => {
            let val_part = format_filter_atom(raw, false);
            format!("{col_part}:={val_part}")
        }
    }
}

fn format_filter_atom(value: &str, is_column: bool) -> String {
    let needs_quotes = value.is_empty()
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\' || (is_column && ch == ':'))
        || (!is_column && value.starts_with('!'));
    if !needs_quotes {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

#[derive(Debug, Clone)]
struct Token {
    raw: String,
    unquoted: String,
    fully_quoted: bool,
}

fn tokenize_filter(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut raw = String::new();
    let mut unquoted = String::new();
    let mut in_quotes = false;
    let mut saw_quote = false;
    let mut saw_unquoted = false;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            raw.push(ch);
            unquoted.push(ch);
            escaped = false;
            continue;
        }
        if in_quotes && ch == '\\' {
            raw.push(ch);
            escaped = true;
            continue;
        }
        if ch == '"' {
            raw.push(ch);
            in_quotes = !in_quotes;
            saw_quote = true;
            continue;
        }
        if ch.is_whitespace() && !in_quotes {
            if !raw.is_empty() {
                tokens.push(Token {
                    raw: std::mem::take(&mut raw),
                    unquoted: std::mem::take(&mut unquoted),
                    fully_quoted: saw_quote && !saw_unquoted,
                });
                saw_quote = false;
                saw_unquoted = false;
            }
            continue;
        }
        if !in_quotes {
            saw_unquoted = true;
        }
        raw.push(ch);
        unquoted.push(ch);
    }
    if !raw.is_empty() {
        tokens.push(Token {
            raw,
            unquoted,
            fully_quoted: saw_quote && !saw_unquoted,
        });
    }
    tokens
}

fn split_column_token(raw: &str) -> Option<(String, &str, bool)> {
    if let Some(after_open) = raw.strip_prefix('"') {
        let mut col = String::new();
        let mut escaped = false;
        for (idx, ch) in after_open.char_indices() {
            if escaped {
                col.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                let rest = &after_open[idx + 1..];
                let rhs = rest.strip_prefix(':')?;
                if col.is_empty() {
                    return None;
                }
                return Some((col, rhs, true));
            }
            col.push(ch);
        }
        return None;
    }

    let (lhs, rhs) = raw.split_once(':')?;
    if lhs.is_empty() {
        return None;
    }
    Some((lhs.to_owned(), rhs, false))
}

fn has_explicit_column_operator(rhs: &str) -> bool {
    if rhs.starts_with('=')
        || rhs.starts_with("!=")
        || rhs.eq_ignore_ascii_case("is:null")
        || rhs.eq_ignore_ascii_case("!null")
        || rhs.eq_ignore_ascii_case("notnull")
        || rhs.eq_ignore_ascii_case("is:notnull")
    {
        return true;
    }
    for prefix in [">=", "<=", ">", "<"] {
        if let Some(rest) = rhs.strip_prefix(prefix)
            && rest.trim().parse::<f64>().is_ok_and(f64::is_finite)
        {
            return true;
        }
    }
    false
}

fn parse_column_op(rhs: &str) -> ColumnFilterOp {
    if rhs.eq_ignore_ascii_case("null") || rhs.eq_ignore_ascii_case("is:null") {
        return ColumnFilterOp::IsNull;
    }
    if rhs.eq_ignore_ascii_case("!null")
        || rhs.eq_ignore_ascii_case("notnull")
        || rhs.eq_ignore_ascii_case("is:notnull")
    {
        return ColumnFilterOp::IsNotNull;
    }
    if let Some(val) = rhs.strip_prefix("!=") {
        return ColumnFilterOp::NotEquals(unquote_value(val).to_lowercase());
    }
    if let Some(val) = rhs.strip_prefix('=') {
        return ColumnFilterOp::Equals(unquote_value(val).to_lowercase());
    }
    for (prefix, cmp) in [
        (">=", NumericCmp::Ge),
        ("<=", NumericCmp::Le),
        (">", NumericCmp::Gt),
        ("<", NumericCmp::Lt),
    ] {
        if let Some(rest) = rhs.strip_prefix(prefix)
            && let Ok(value) = rest.trim().parse::<f64>()
            && value.is_finite()
        {
            return ColumnFilterOp::Numeric { cmp, value };
        }
    }
    ColumnFilterOp::Contains(unquote_value(rhs).to_lowercase())
}

fn unquote_value(raw: &str) -> String {
    let trimmed = if let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        inner
    } else if let Some(inner) = raw.strip_prefix('"') {
        inner
    } else {
        raw
    };
    let mut out = String::with_capacity(trimmed.len());
    let mut escaped = false;
    for ch in trimmed.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests;
