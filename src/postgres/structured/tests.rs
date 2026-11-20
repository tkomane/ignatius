use super::*;

#[test]
fn structure_uses_driver_type_and_extension_ownership() {
    let same_name = Type::new("geometry".into(), 51234, PgKind::Simple, "demo".into());
    assert_eq!(describe_type(&Type::TEXT, &BTreeMap::new()), None);
    assert_eq!(
        describe_type(&Type::JSONB, &BTreeMap::new()),
        Some(Kind::Jsonb)
    );
    assert_eq!(describe_type(&same_name, &BTreeMap::new()), None);
    let mut types = BTreeMap::from([(
        same_name.oid(),
        CatalogType {
            delimiter: Some(b','),
            extension: None,
        },
    )]);
    assert_eq!(describe_type(&same_name, &types), None);
    types.get_mut(&same_name.oid()).unwrap().extension = Some("postgis".into());
    assert_eq!(describe_type(&same_name, &types), Some(Kind::Geometry));
}

#[test]
fn custom_arrays_need_catalog_delimiter_for_their_element() {
    let element = Type::new("custom".into(), 51234, PgKind::Simple, "demo".into());
    let array = Type::new(
        "_custom".into(),
        51235,
        PgKind::Array(element.clone()),
        "demo".into(),
    );
    assert_eq!(describe_type(&array, &BTreeMap::new()), None);
    let types = BTreeMap::from([(
        element.oid(),
        CatalogType {
            delimiter: Some(b';'),
            extension: None,
        },
    )]);
    assert_eq!(
        describe_type(&array, &types),
        Some(Kind::Array { delimiter: b';' })
    );
    assert_eq!(
        describe_type(&Type::BOX_ARRAY, &BTreeMap::new()),
        Some(Kind::Array { delimiter: b';' })
    );
}

#[test]
fn type_enrichment_does_not_supply_missing_origin() {
    let catalog = BTreeMap::new();
    let tree = crate::structured::Tree::parse(Kind::Jsonb, r#"{"key":1}"#).unwrap();
    let derived = describe_column(&Type::JSONB, &catalog, None).unwrap();
    assert_eq!(derived.kind, Kind::Jsonb);
    assert_eq!(derived.origin, None);
    assert_eq!(derived.edit_input(&tree, 0), None);

    let original = Origin {
        table: 42,
        attribute: 2,
        type_oid: Type::JSONB.oid(),
        typmod: -1,
    };
    let direct = describe_column(&Type::JSONB, &catalog, Some(original)).unwrap();
    assert_eq!(direct.origin, Some(original));
    assert!(direct.edit_input(&tree, 0).is_some());
}
