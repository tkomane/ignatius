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
