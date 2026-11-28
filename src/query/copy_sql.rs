//! Pure query-ready SQL predicate and JSON row builders over retained result cells.
//!
//! Nothing here connects to a database, mutates an editor, or writes to a
//! clipboard. Values are escaped conservatively as PostgreSQL text literals or
//! serialized as compact JSON with explicit bounds and NUL-byte checks.

use crate::query::identifiers::quote_identifier;
use crate::query::value::Cell;
use std::collections::HashSet;

/// Maximum UTF-8 byte length permitted for generated SQL or JSON copy payloads.
pub const MAX_COPY_SQL_BYTES: usize = 1024 * 1024;

/// Default cap on distinct non-NULL values admitted into a generated `IN (...)` list.
pub const DEFAULT_IN_LIST_MAX_VALUES: usize = 200;

/// Escapes a raw string into a PostgreSQL single-quoted literal (`'...'`).
///
/// Refuses NUL bytes (which PostgreSQL text queries cannot represent) and
/// payloads exceeding [`MAX_COPY_SQL_BYTES`].
fn escape_sql_literal(raw: &str) -> Result<String, &'static str> {
    if raw.contains('\0') {
        return Err("cell value contains a NUL byte");
    }
    if raw.len() > MAX_COPY_SQL_BYTES {
        return Err("cell value exceeds the 1 MiB limit");
    }
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('\'');
    for ch in raw.chars() {
        if ch == '\'' {
            out.push_str("''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    if out.len() > MAX_COPY_SQL_BYTES {
        return Err("escaped SQL literal exceeds the 1 MiB limit");
    }
    Ok(out)
}

/// Builds a query-ready `WHERE` clause for one retained result cell.
///
/// Emits `WHERE "col" = 'escaped'` for non-NULL values (including empty
/// strings as `''`) and `WHERE "col" IS NULL` for SQL NULL. Refuses NUL bytes
/// and payloads exceeding 1 MiB.
pub fn build_where_clause(column: &str, cell: &Cell) -> Result<String, &'static str> {
    if column.contains('\0') {
        return Err("column name contains a NUL byte");
    }
    let quoted_col = quote_identifier(column);
    let sql = match cell {
        Cell::Null => format!("WHERE {quoted_col} IS NULL"),
        Cell::Text(raw) => {
            let literal = escape_sql_literal(raw)?;
            format!("WHERE {quoted_col} = {literal}")
        }
    };
    if sql.len() > MAX_COPY_SQL_BYTES {
        return Err("generated WHERE clause exceeds the 1 MiB limit");
    }
    Ok(sql)
}

/// Builds a query-ready `"col" IN ('a', 'b', ...)` list over distinct non-NULL
/// values in encounter order, bounded by `max_values`.
///
/// Returns `(sql, count, truncated)`. Refuses cleanly when `max_values == 0`,
/// when 0 non-NULL values exist, when NUL bytes are present, or when the
/// resulting SQL exceeds 1 MiB.
pub fn build_in_list(
    column: &str,
    cells: &[&Cell],
    max_values: usize,
) -> Result<(String, usize, bool), &'static str> {
    if column.contains('\0') {
        return Err("column name contains a NUL byte");
    }
    if max_values == 0 {
        return Err("IN list limit must be at least 1");
    }

    let mut seen: HashSet<&str> = HashSet::new();
    let mut literals: Vec<String> = Vec::new();
    let mut truncated = false;

    for cell in cells {
        let Some(raw) = cell.raw() else {
            continue;
        };
        if raw.contains('\0') {
            return Err("cell value contains a NUL byte");
        }
        if seen.contains(raw) {
            continue;
        }
        if literals.len() >= max_values {
            truncated = true;
            continue;
        }
        seen.insert(raw);
        literals.push(escape_sql_literal(raw)?);
    }

    if literals.is_empty() {
        return Err("no non-NULL values are available for an IN list");
    }

    let quoted_col = quote_identifier(column);
    let sql = format!("{quoted_col} IN ({})", literals.join(", "));
    if sql.len() > MAX_COPY_SQL_BYTES {
        return Err("generated IN list exceeds the 1 MiB limit");
    }
    Ok((sql, literals.len(), truncated))
}

/// Builds a compact deterministic JSON object for one retained row.
///
/// Distinguishes SQL `NULL` (`null`) from an empty string (`""`) and refuses
/// payloads exceeding 1 MiB.
pub fn build_row_json(columns: &[String], row: &[Cell]) -> Result<String, &'static str> {
    let mut map = serde_json::Map::new();
    for (index, column) in columns.iter().enumerate() {
        let value = match row.get(index) {
            Some(Cell::Text(text)) => {
                if text.len() > MAX_COPY_SQL_BYTES {
                    return Err("row JSON exceeds the 1 MiB limit");
                }
                serde_json::Value::String(text.clone())
            }
            Some(Cell::Null) | None => serde_json::Value::Null,
        };
        map.insert(column.clone(), value);
    }
    let json = serde_json::to_string(&serde_json::Value::Object(map))
        .map_err(|_| "failed to serialize row as JSON")?;
    if json.len() > MAX_COPY_SQL_BYTES {
        return Err("row JSON exceeds the 1 MiB limit");
    }
    Ok(json)
}

#[cfg(test)]
mod tests;
