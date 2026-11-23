use super::*;

#[test]
fn identity_constants_are_populated() {
    assert!(!BINARY_NAME.is_empty());
    assert!(!VERSION.is_empty());
    assert!(!TARGET_TRIPLE.is_empty());
    // A rebuild must not invent a product version: the version is the one in
    // Cargo.toml and nothing else may synthesise it.
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn the_identity_line_is_plain_ascii_so_every_glyph_tier_can_set_it() {
    assert!(IDENTITY_LINE.is_ascii());
    assert!(!IDENTITY_LINE.is_empty());
    assert_eq!(IDENTITY_LINE.trim(), IDENTITY_LINE);
}
