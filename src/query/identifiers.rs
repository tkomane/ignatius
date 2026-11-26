//! SQL identifier quoting and PostgreSQL byte rules for bare names.

/// Uses PostgreSQL's byte ranges, including the non-ASCII bytes of UTF-8.
pub(crate) const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// Byte range shared by a bare name and a dollar tag after the first byte.
pub(crate) const fn is_identifier_part_without_dollar(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

/// Scans a bare name from an identifier start, including dollar signs in the
/// name so they cannot start a separate dollar-quoted literal.
pub(crate) fn unquoted_identifier_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start + 1;
    while end < bytes.len() && (is_identifier_part_without_dollar(bytes[end]) || bytes[end] == b'$')
    {
        end += 1;
    }
    end
}

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
