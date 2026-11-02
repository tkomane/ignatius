use super::*;

#[test]
fn identifiers_are_quoted_so_hostile_names_cannot_escape() {
    assert_eq!(quote_identifier("orders"), "\"orders\"");
    // Quoting unconditionally is what makes reserved words and mixed case work.
    assert_eq!(quote_identifier("select"), "\"select\"");
    assert_eq!(quote_identifier("Mixed Case"), "\"Mixed Case\"");
    // An embedded quote is doubled, which is PostgreSQL's own rule.
    assert_eq!(quote_identifier("we\"ird"), "\"we\"\"ird\"");
    // The shape someone would use to break out of a quoted identifier.
    assert_eq!(
        quote_identifier("x\"; DROP TABLE orders; --"),
        "\"x\"\"; DROP TABLE orders; --\""
    );
}

#[test]
fn a_qualified_name_is_safe_to_paste_into_sql() {
    let object = ObjectSummary {
        kind: ObjectKind::Table,
        schema: "public".into(),
        name: "we\"ird name".into(),
        readable: true,
        detail: None,
    };
    assert_eq!(object.qualified_sql(), "\"public\".\"we\"\"ird name\"");
}

#[test]
fn every_kind_has_a_singular_and_a_plural_noun() {
    for kind in [
        ObjectKind::Table,
        ObjectKind::View,
        ObjectKind::MaterializedView,
        ObjectKind::Sequence,
        ObjectKind::ForeignTable,
        ObjectKind::PartitionedTable,
        ObjectKind::Function,
        ObjectKind::Index,
        ObjectKind::Extension,
        ObjectKind::Column,
    ] {
        assert!(!kind.singular().is_empty());
        assert!(!kind.plural().is_empty());
        assert_ne!(kind.singular(), kind.plural());
    }
}

#[test]
fn relkinds_round_trip() {
    for kind in ObjectKind::IN_SCHEMA {
        if let Some(relkind) = kind.relkind() {
            assert_eq!(ObjectKind::from_relkind(relkind), Some(*kind));
        }
    }
    assert_eq!(
        ObjectKind::from_relkind("t"),
        None,
        "toast tables are not shown"
    );
    assert_eq!(ObjectKind::from_relkind(""), None);
}

#[test]
fn schema_counts_default_to_zero_rather_than_being_absent() {
    let mut counts = BTreeMap::new();
    counts.insert(ObjectKind::Table, 18);
    let summary = SchemaSummary {
        name: "public".into(),
        usable: true,
        counts,
    };
    assert_eq!(summary.count(ObjectKind::Table), 18);
    assert_eq!(summary.count(ObjectKind::View), 0);
    assert_eq!(summary.total(), 18);
}
