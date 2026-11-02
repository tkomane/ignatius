//! SQL identifier helpers shared by metadata and completion.

/// Quotes an identifier for safe inclusion in SQL.
///
/// PostgreSQL doubles an embedded quote inside a quoted identifier. Quoting
/// unconditionally is deliberate: it is correct for reserved words, mixed
/// case, and names containing anything at all, and it never needs a judgement
/// call about which names are "safe".
#[must_use]
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests;
