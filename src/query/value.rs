//! Rendering database values safely.
//!
//! Values come from the server, and the server's contents come from whoever could
//! write to it. A value is therefore treated as hostile input: control characters
//! are escaped before they can reach the terminal, where they would otherwise be
//! able to move the cursor, repaint the screen, change the title, or forge parts
//! of the interface.
//!
//! Bidirectional formatting characters are escaped for the same reason: they can
//! make a value display in an order that does not match its bytes, which is a
//! spoofing route in a tool where people read values to make decisions.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// How a SQL NULL is shown in the interface.
///
/// A NULL must be distinguishable from an empty string and from the four-letter
/// text `NULL`. The marker plus the muted style does that; the cell inspector
/// states the distinction in words for the remaining ambiguous case.
pub const NULL_MARKER: &str = "[null]";

/// Marker appended when a value is shortened to fit a column.
pub const TRUNCATION_MARKER: char = '\u{2026}';

/// ASCII equivalent of [`TRUNCATION_MARKER`].
pub const TRUNCATION_MARKER_ASCII: &str = "...";

/// One value in a result row.
#[derive(Clone, PartialEq, Eq)]
pub enum Cell {
    /// SQL NULL.
    Null,
    /// A value, exactly as the server rendered it in text format.
    Text(String),
}

impl std::fmt::Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => f.write_str("Null"),
            Self::Text(_) => f.write_str("Text(<hidden>)"),
        }
    }
}

impl Cell {
    /// Builds a cell from an optional server value.
    #[must_use]
    pub fn from_option(value: Option<&str>) -> Self {
        value.map_or(Self::Null, |v| Self::Text(v.to_owned()))
    }

    /// Whether this is a SQL NULL.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Text safe to draw in the terminal, with NULL shown as the marker.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::Null => NULL_MARKER.to_owned(),
            Self::Text(value) => sanitize_for_display(value),
        }
    }

    /// Text for a machine-readable export, with NULL encoded as `null_encoding`.
    ///
    /// Export keeps the value byte-for-byte as the server sent it: escaping is a
    /// display concern, and silently altering exported data would be worse than
    /// the risk it guards against. The caller decides where the file goes.
    #[must_use]
    pub fn export(&self, null_encoding: &str) -> String {
        match self {
            Self::Null => null_encoding.to_owned(),
            Self::Text(value) => value.clone(),
        }
    }

    /// The raw value, for the cell inspector.
    #[must_use]
    pub fn raw(&self) -> Option<&str> {
        match self {
            Self::Null => None,
            Self::Text(value) => Some(value),
        }
    }

    /// A description of the value used by the inspector, which resolves the
    /// remaining ambiguity between NULL and text that looks like the marker.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Null => "SQL NULL".to_owned(),
            Self::Text(value) if value.is_empty() => "empty string, 0 characters".to_owned(),
            Self::Text(value) => format!("text, {} characters", value.chars().count()),
        }
    }
}

/// Escapes anything that could control the terminal, leaving printable text alone.
#[must_use]
pub fn sanitize_for_display(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // C0 controls, including ESC, and DEL.
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02X}", c as u32));
            }
            // C1 controls, which some terminals accept as escape introducers.
            c if (0x80..=0x9f).contains(&(c as u32)) => {
                out.push_str(&format!("\\u{{{:04X}}}", c as u32));
            }
            // Bidirectional overrides and isolates, which can reorder what is read.
            c if matches!(c as u32, 0x200e | 0x200f | 0x202a..=0x202e | 0x2066..=0x2069) => {
                out.push_str(&format!("\\u{{{:04X}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Display width in terminal cells, counting wide characters as two.
#[must_use]
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Shortens text to a maximum display width, appending a truncation marker.
///
/// Never splits a character, and accounts for wide characters so the result
/// occupies at most `max_width` cells.
#[must_use]
pub fn truncate_to_width(text: &str, max_width: usize, unicode: bool) -> String {
    if display_width(text) <= max_width {
        return text.to_owned();
    }
    let marker = if unicode {
        TRUNCATION_MARKER.to_string()
    } else {
        TRUNCATION_MARKER_ASCII.to_owned()
    };
    let marker_width = display_width(&marker);
    if max_width <= marker_width {
        return marker.chars().take(max_width).collect();
    }
    let budget = max_width - marker_width;
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > budget {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str(&marker);
    out
}

/// Breaks text into lines that each fit a display width.
///
/// Breaking happens at the width, not at a word boundary. A SQL value is not
/// prose: word wrapping would move characters onto lines they are not on, and
/// someone counting the characters of an identifier would be misled. Existing
/// line breaks in the text are kept, so a value's own shape survives.
///
/// The text is expected to have been through [`sanitize_for_display`] already,
/// which is what makes the widths honest.
#[must_use]
pub fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for source in text.split('\n') {
        let mut current = String::new();
        let mut used = 0usize;
        for ch in source.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            // The emptiness check matters in a pane narrower than one character:
            // without it a wide character in a one-column pane would push an
            // empty line forever and never be shown at all.
            if used + ch_width > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(ch);
            used += ch_width;
        }
        lines.push(current);
    }
    lines
}

/// Pads text to a fixed display width, for aligned table output.
#[must_use]
pub fn pad_to_width(text: &str, width: usize) -> String {
    let current = display_width(text);
    if current >= width {
        return text.to_owned();
    }
    format!("{text}{}", " ".repeat(width - current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_sequences_from_the_database_cannot_reach_the_terminal() {
        // A value crafted to clear the screen and repaint a fake status line.
        let hostile = "\x1b[2J\x1b[H\x1b[31mDROP TABLE orders\x1b[0m";
        let safe = sanitize_for_display(hostile);
        assert!(!safe.contains('\x1b'), "ESC survived: {safe:?}");
        assert!(safe.starts_with("\\x1B[2J"), "{safe}");
        // The readable text is preserved so the value is still inspectable.
        assert!(safe.contains("DROP TABLE orders"));
    }

    #[test]
    fn every_control_character_is_escaped() {
        for code in (0u32..0x20).chain(std::iter::once(0x7f)) {
            let ch = char::from_u32(code).expect("valid char");
            let safe = sanitize_for_display(&ch.to_string());
            assert!(
                !safe.chars().any(|c| (c as u32) < 0x20 || c as u32 == 0x7f),
                "U+{code:04X} survived as {safe:?}"
            );
        }
    }

    #[test]
    fn c1_controls_and_bidi_overrides_are_escaped() {
        let safe = sanitize_for_display("\u{9b}[31m");
        assert!(safe.starts_with("\\u{009B}"), "{safe}");

        let spoof = "user\u{202e}txt.exe";
        let safe = sanitize_for_display(spoof);
        assert!(safe.contains("\\u{202E}"), "{safe}");
        assert!(!safe.contains('\u{202e}'));
    }

    #[test]
    fn ordinary_text_including_unicode_is_left_alone() {
        for value in ["plain", "héllo wörld", "日本語", "emoji 🐘", "R 1,245.00"] {
            assert_eq!(sanitize_for_display(value), value);
        }
    }

    #[test]
    fn whitespace_is_made_visible_rather_than_breaking_alignment() {
        assert_eq!(sanitize_for_display("a\tb\nc\rd"), "a\\tb\\nc\\rd");
    }

    #[test]
    fn null_is_distinguishable_from_empty_string_and_from_the_text_null() {
        let null = Cell::Null;
        let empty = Cell::Text(String::new());
        let literal = Cell::Text("NULL".into());

        assert_eq!(null.display(), NULL_MARKER);
        assert_eq!(empty.display(), "");
        assert_eq!(literal.display(), "NULL");
        assert_ne!(null.display(), empty.display());
        assert_ne!(null.display(), literal.display());

        // Where the display could still be ambiguous, the description is not.
        assert_eq!(null.describe(), "SQL NULL");
        assert_eq!(empty.describe(), "empty string, 0 characters");
        assert_eq!(literal.describe(), "text, 4 characters");
        assert!(null.raw().is_none());
    }

    #[test]
    fn export_preserves_the_server_value_exactly() {
        let cell = Cell::Text("1.100".into());
        assert_eq!(
            cell.export(""),
            "1.100",
            "numeric text must not be reformatted"
        );
        assert_eq!(Cell::Null.export(""), "");
        assert_eq!(Cell::Null.export("\\N"), "\\N");
        // Export is not display: escaping here would corrupt the exported data.
        assert_eq!(Cell::Text("a\tb".into()).export(""), "a\tb");
    }

    #[test]
    fn width_counts_terminal_cells_not_characters() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("日本語"), 6, "wide characters take two cells");
        assert_eq!(
            display_width("e\u{301}"),
            1,
            "combining accents add no width"
        );
    }

    #[test]
    fn truncation_respects_wide_characters_and_never_splits_one() {
        assert_eq!(truncate_to_width("abcdef", 10, true), "abcdef");
        assert_eq!(truncate_to_width("abcdef", 4, true), "abc\u{2026}");
        assert_eq!(truncate_to_width("abcdef", 4, false), "a...");

        let wide = truncate_to_width("日本語です", 5, true);
        assert!(
            display_width(&wide) <= 5,
            "{wide} was {} cells",
            display_width(&wide)
        );
        assert!(wide.ends_with('\u{2026}'));

        // Degenerate widths must not panic.
        assert_eq!(display_width(&truncate_to_width("abcdef", 1, true)), 1);
        assert_eq!(truncate_to_width("abcdef", 0, true), "");
    }

    #[test]
    fn wrapping_fills_each_line_to_the_width_and_never_splits_a_character() {
        assert_eq!(wrap_to_width("abcdef", 3), vec!["abc", "def"]);
        assert_eq!(wrap_to_width("abcdefg", 3), vec!["abc", "def", "g"]);
        assert_eq!(wrap_to_width("ab", 10), vec!["ab"]);

        // A wide character that does not fit in the remainder moves down whole.
        let lines = wrap_to_width("a日本", 3);
        assert_eq!(lines, vec!["a日", "本"]);
        for line in &lines {
            assert!(display_width(line) <= 3, "{line} overflows");
        }
    }

    #[test]
    fn wrapping_keeps_the_line_breaks_the_value_already_had() {
        // The value's own shape is a fact about the data. An empty line in the
        // middle of a JSON document is part of it.
        assert_eq!(wrap_to_width("one\ntwo", 10), vec!["one", "two"]);
        assert_eq!(wrap_to_width("one\n\ntwo", 10), vec!["one", "", "two"]);
        assert_eq!(
            wrap_to_width("", 10),
            vec![""],
            "an empty value is one line"
        );
        assert_eq!(
            wrap_to_width("abcdef\ngh", 3),
            vec!["abc", "def", "gh"],
            "wrapping applies within each line"
        );
    }

    #[test]
    fn wrapping_to_no_width_yields_nothing_rather_than_looping() {
        assert!(wrap_to_width("anything", 0).is_empty());
        assert_eq!(wrap_to_width("abc", 1), vec!["a", "b", "c"]);
        // A wide character in a one-column pane cannot fit. It goes on a line of
        // its own and overflows by one cell rather than disappearing: a value
        // that is not shown is worse than a value that is clipped by the frame.
        assert_eq!(wrap_to_width("日", 1), vec!["日"]);
    }

    #[test]
    fn padding_aligns_by_display_width() {
        assert_eq!(pad_to_width("ab", 5), "ab   ");
        assert_eq!(display_width(&pad_to_width("日本", 6)), 6);
        assert_eq!(
            pad_to_width("toolong", 3),
            "toolong",
            "padding never truncates"
        );
    }

    #[test]
    fn sanitised_output_is_stable_under_repetition() {
        let value = "\x1b[31mred\x1b[0m";
        let once = sanitize_for_display(value);
        assert_eq!(sanitize_for_display(&once), once);
    }
}
