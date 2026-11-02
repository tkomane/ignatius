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
