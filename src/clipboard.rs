//! The opt-in, write-only terminal clipboard boundary.
//!
//! A result value is treated as data, not terminal syntax. The value is encoded
//! before it reaches the terminal, and the payload type deliberately redacts its
//! contents from `Debug` output so an effect cannot leak it in a test, panic, or
//! diagnostic. Nothing in this module reads or clears a clipboard.

use std::fmt;
use std::io::{self, Write};

/// Maximum raw UTF-8 payload accepted by the client before terminal transport.
pub const MAX_OSC52_BYTES: usize = 1024 * 1024;

/// A value that has passed the client-side clipboard size bound.
///
/// The text is private and the type is consumed by the terminal boundary. Its
/// `Debug` implementation never displays the text.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardPayload {
    value: String,
}

impl ClipboardPayload {
    /// Wraps an exact result text value when it fits the client bound.
    pub fn try_new(value: String) -> Result<Self, PayloadTooLarge> {
        let bytes = value.len();
        if bytes > MAX_OSC52_BYTES {
            return Err(PayloadTooLarge { bytes });
        }
        Ok(Self { value })
    }

    /// Returns the raw UTF-8 byte count without exposing the value.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns whether the validated payload contains no bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Returns the Unicode scalar-value count without exposing the value.
    #[must_use]
    pub fn characters(&self) -> usize {
        self.value.chars().count()
    }

    /// Consumes the payload at the terminal write boundary.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.value
    }
}

impl fmt::Debug for ClipboardPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClipboardPayload(<hidden>)")
    }
}

/// The safe, value-free error from the client-side size bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadTooLarge {
    /// Raw UTF-8 byte count, safe to report because it is not the value.
    pub bytes: usize,
}

/// Builds the OSC 52 set-clipboard sequence for a validated payload.
///
/// The framing is `ESC ] 52 ; c ; <base64> BEL`. The base64 body contains only
/// ASCII alphabet characters, so raw result controls cannot become terminal
/// syntax.
#[must_use]
pub fn osc52_sequence(payload: &ClipboardPayload) -> String {
    let encoded = base64_encode(payload.value.as_bytes());
    let mut sequence = String::with_capacity(7 + encoded.len() + 1);
    sequence.push('\x1b');
    sequence.push(']');
    sequence.push_str("52;c;");
    sequence.push_str(&encoded);
    sequence.push('\x07');
    sequence
}

/// Writes and flushes one OSC 52 sequence.
pub fn write_osc52<W: Write>(writer: &mut W, payload: &ClipboardPayload) -> io::Result<()> {
    let sequence = osc52_sequence(payload);
    writer.write_all(sequence.as_bytes())?;
    writer.flush()
}

/// Encodes bytes using the standard padded base64 alphabet.
#[must_use]
pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let combined = (first << 16) | (second << 8) | third;
        output.push(ALPHABET[((combined >> 18) & 0x3f) as usize] as char);
        output.push(ALPHABET[((combined >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((combined >> 6) & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(combined & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn base64_matches_the_standard_examples() {
        for (input, expected) in [
            (b"".as_slice(), ""),
            (b"f".as_slice(), "Zg=="),
            (b"fo".as_slice(), "Zm8="),
            (b"foo".as_slice(), "Zm9v"),
            (b"foobar".as_slice(), "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(input), expected);
        }
    }

    #[test]
    fn osc52_has_fixed_framing_and_round_trips_the_exact_utf8_bytes() {
        let value = "line\n\t\u{1b}]52;c;fake\u{7}\u{1f600}";
        let payload = ClipboardPayload::try_new(value.to_owned()).expect("fits");
        let sequence = osc52_sequence(&payload);
        assert!(sequence.starts_with("\u{1b}]52;c;"));
        assert!(sequence.ends_with('\u{7}'));

        let encoded = &sequence["\u{1b}]52;c;".len()..sequence.len() - 1];
        assert_eq!(encoded, base64_encode(value.as_bytes()));
        assert!(!encoded.as_bytes().contains(&0x1b));
        assert!(!encoded.as_bytes().contains(&0x07));
        assert!(
            !encoded
                .as_bytes()
                .windows(value.len())
                .any(|window| window == value.as_bytes())
        );
        assert_eq!(payload.into_inner().as_bytes(), value.as_bytes());
    }

    #[test]
    fn empty_text_is_a_valid_zero_byte_payload() {
        let payload = ClipboardPayload::try_new(String::new()).expect("empty text fits");
        assert_eq!(payload.len(), 0);
        assert_eq!(payload.characters(), 0);
        assert_eq!(osc52_sequence(&payload), "\u{1b}]52;c;\u{7}");
    }

    #[test]
    fn the_limit_is_inclusive_and_rejection_does_not_expose_text() {
        let accepted =
            ClipboardPayload::try_new("x".repeat(MAX_OSC52_BYTES)).expect("the exact limit fits");
        assert_eq!(accepted.len(), MAX_OSC52_BYTES);

        let rejected = ClipboardPayload::try_new("secret".repeat(MAX_OSC52_BYTES / 6 + 1))
            .expect_err("one byte over the limit must refuse");
        assert!(rejected.bytes > MAX_OSC52_BYTES);
        assert!(!format!("{rejected:?}").contains("secret"));
    }

    #[test]
    fn debug_never_contains_the_payload() {
        let payload = ClipboardPayload::try_new("sensitive-value".to_owned()).expect("fits");
        assert_eq!(format!("{payload:?}"), "ClipboardPayload(<hidden>)");
    }

    #[test]
    fn writing_flushes_and_reports_io_failures() {
        let payload = ClipboardPayload::try_new("value".to_owned()).expect("fits");
        let mut output = Vec::new();
        write_osc52(&mut output, &payload).expect("vec accepts the write");
        assert_eq!(output, osc52_sequence(&payload).as_bytes());

        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("synthetic write failure"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("synthetic flush failure"))
            }
        }
        assert!(write_osc52(&mut FailingWriter, &payload).is_err());
    }
}
