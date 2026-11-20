use super::*;

#[test]
fn editing_json_preserves_numeric_and_sibling_source_bytes() {
    let raw = r#"{"k / \"": {"nested": [null, 123456789012345678901.234567890123456789]}, "other":"unchanged"}"#;
    let tree = Tree::parse(Kind::Jsonb, raw).unwrap();
    assert_eq!(tree.path(3), r#"$ / "k / \"" / "nested" / [0]"#);
    assert_eq!(tree.preview(3), "null");
    let patched = tree.replace(3, r#"{"unicode":"\u732b"}"#).unwrap();
    assert_eq!(patched, raw.replacen("null", r#"{"unicode":"\u732b"}"#, 1));
}

#[test]
fn arrays_keep_postgres_subscripts_and_quote_distinctions() {
    let raw = r#"[0:1][2:3]={{NULL,"NULL"},{"a,b",""}}"#;
    let tree = Tree::parse(Kind::Array { delimiter: b',' }, raw).unwrap();
    assert_eq!(tree.path(2), "$ / [0] / [2]");
    assert_eq!(tree.preview(2), "NULL array element");
    assert_eq!(tree.preview(3), r#""NULL""#);
    assert_eq!(tree.preview(6), r#""""#);
    assert_eq!(
        tree.replace(2, r#""a\\\"b""#).unwrap(),
        raw.replacen("NULL", r#""a\\\"b""#, 1)
    );
    assert_eq!(tree.replace(3, "foo,bar"), Err(Error::Invalid));

    let boxes = Tree::parse(Kind::Array { delimiter: b';' }, "{(1,1),(0,0);(3,3),(2,2)}").unwrap();
    assert_eq!(boxes.nodes.len(), 3);
    assert_eq!(boxes.preview(2), "(3,3),(2,2)");
}

#[test]
fn array_input_preserves_literal_whitespace_and_replaces_only_the_element_span() {
    let kind = Kind::Array { delimiter: b',' };
    let compact = Tree::parse(kind, "{old}").unwrap();
    let raw = " \t[0:1] [3] = { { \" kept \" ,  NULL \t, two words  }, \
               { end\\  , \"NULL\" \t, \\NULL  } } \n";
    assert_eq!(compact.replace(0, raw).unwrap(), raw);

    let tree = Tree::parse(kind, raw).unwrap();
    assert_eq!(tree.path(2), "$ / [0] / [1]");
    assert_eq!(tree.preview(2), r#"" kept ""#);
    assert_eq!(tree.preview(3), "NULL array element");
    assert_eq!(tree.preview(4), "two words");
    assert_eq!(tree.preview(6), "end\\ ");
    assert_eq!(tree.preview(7), r#""NULL""#);
    assert_eq!(tree.preview(8), r"\NULL");
    assert_eq!(
        tree.replace(6, r#"  "new value"  "#).unwrap(),
        raw.replacen("end\\ ", r#"  "new value"  "#, 1)
    );
    assert_eq!(tree.replace(6, r#""quoted" trailing"#), Err(Error::Invalid));
    assert_eq!(tree.replace(6, " \t"), Err(Error::Invalid));

    let box_kind = Kind::Array { delimiter: b';' };
    let boxes = Tree::parse(box_kind, r#"{ "(1,1),(0,0)"  ; (3,3),(2,2) }"#).unwrap();
    assert_eq!(boxes.nodes.len(), 3);
    assert_eq!(boxes.preview(2), "(3,3),(2,2)");
}

#[test]
fn range_preserves_delimiters_and_distinguishes_unbounded_and_empty() {
    let tree = Tree::parse(Kind::Range, r#"(,"x,y")"#).unwrap();
    assert_eq!(tree.preview(1), "unbounded");
    assert_eq!(tree.path(2), "$ / Upper bound (exclusive)");
    assert_eq!(tree.replace(1, r#""a\"b""#).unwrap(), r#"("a\"b","x,y")"#);
    assert_eq!(tree.replace(2, "").unwrap(), "(,)");
    let empty = Tree::parse(Kind::Range, "empty").unwrap();
    assert_eq!(empty.preview(0), "empty range");
    assert_eq!(empty.replace(0, "[1,3)").unwrap(), "[1,3)");
}

#[test]
fn vectors_preserve_unedited_coordinate_text_and_reject_nonfinite_input() {
    let tree = Tree::parse(Kind::HalfVector, "[-0,1.25,2e-3]").unwrap();
    assert_eq!(tree.path(3), "$ / [3]");
    assert_eq!(tree.replace(2, "3.25").unwrap(), "[-0,3.25,2e-3]");
    assert_eq!(tree.replace(3, "NaN"), Err(Error::Invalid));

    let tree = Tree::parse(Kind::SparseVector, "{1:2e-3,5:-3.125}/25").unwrap();
    assert_eq!(tree.preview(0), "25 dimensions, 2 stored coordinates");
    assert_eq!(tree.path(2), "$ / [5]");
    assert_eq!(tree.replace(2, "0").unwrap(), "{1:2e-3,5:0}/25");
}

#[test]
fn spatial_inspection_decodes_coordinates_and_retains_full_value_edit() {
    // PostGIS EWKB, little endian POINT Z M with SRID 4326 and coordinates 1,2,3,4.
    let raw = "01010000E0E6100000000000000000F03F000000000000004000000000000008400000000000001040";
    let tree = Tree::parse(Kind::Geometry, raw).unwrap();
    assert_eq!(tree.preview(0), "Point XYZM, SRID 4326");
    assert_eq!(tree.preview(1), "X=1.0, Y=2.0, Z=3.0, M=4.0");
    assert_eq!(tree.input(1), None);
    assert_eq!(
        tree.replace(0, "SRID=4326;POINT (1 2)").unwrap(),
        "SRID=4326;POINT (1 2)"
    );

    let raw = "0020000001000010E640080000000000004010000000000000";
    let tree = Tree::parse(Kind::Geography, raw).unwrap();
    assert_eq!(tree.preview(0), "Point XY, SRID 4326");
    assert_eq!(tree.preview(1), "X=3.0, Y=4.0");
}

#[test]
fn limits_are_disclosed_and_make_the_tree_read_only() {
    let many = format!("[{}0]", "0,".repeat(MAX_NODES));
    let tree = Tree::parse(Kind::Jsonb, &many).unwrap();
    assert!(tree.truncated);
    assert_eq!(tree.nodes.len(), MAX_NODES);
    assert_eq!(tree.replace(1, "1"), Err(Error::NotEditable));
    let deep = format!("{}0{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
    let tree = Tree::parse(Kind::Jsonb, &deep).unwrap();
    assert!(tree.truncated);
    assert!(matches!(
        Tree::parse(Kind::Jsonb, &"x".repeat(MAX_BYTES + 1)),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Tree::parse(Kind::Jsonb, "[1,2,]"),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        Tree::parse(Kind::Range, "[1,2"),
        Err(Error::Invalid)
    ));
}
