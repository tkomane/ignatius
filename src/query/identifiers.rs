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
mod tests {
    use super::*;

    #[test]
    fn every_identifier_is_quoted_and_embedded_quotes_are_doubled() {
        assert_eq!(quote_identifier("orders"), "\"orders\"");
        assert_eq!(quote_identifier("select"), "\"select\"");
        assert_eq!(quote_identifier("Mixed Case"), "\"Mixed Case\"");
        assert_eq!(quote_identifier("we\"ird"), "\"we\"\"ird\"");
        assert_eq!(
            quote_identifier("x\"; DROP TABLE orders; --"),
            "\"x\"\"; DROP TABLE orders; --\""
        );
    }
}
