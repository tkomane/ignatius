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

/// Largest server-rendered value retained in an interactive result cell.
///
/// Streaming export does not use this limit: it writes each value as it arrives
/// and therefore remains the route for larger values.
pub const MAX_RETAINED_VALUE_BYTES: usize = 1024 * 1024;

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

    /// Builds a retained cell, refusing a value that would exceed the result
    /// model's per-cell memory bound.
    pub fn from_option_bounded(value: Option<&str>) -> Result<Self, usize> {
        let Some(value) = value else {
            return Ok(Self::Null);
        };
        if value.len() > MAX_RETAINED_VALUE_BYTES {
            return Err(value.len());
        }
        Ok(Self::Text(value.to_owned()))
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
mod tests;
