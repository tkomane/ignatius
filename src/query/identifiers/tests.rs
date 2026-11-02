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
