use super::*;

#[test]
fn empty_and_identical_schemas_produce_no_diff() {
    let s1 = SchemaSnapshot::empty("public");
    let s2 = SchemaSnapshot::empty("public");
    let diff = compute_schema_diff(&s1, &s2);
    assert!(diff.is_empty());
    assert_eq!(diff.total_changes(), 0);

    let sql = generate_migration_sql(&diff, &MigrationOptions::default());
    assert!(sql.contains("no migration necessary"));
}

#[test]
fn detects_added_table_and_generates_transactional_create_table() {
    let s1 = SchemaSnapshot::empty("public");
    let mut s2 = SchemaSnapshot::empty("public");
    s2.tables.push(TableDef {
        schema: "public".to_owned(),
        name: "orders".to_owned(),
        columns: vec![
            ColumnDef {
                name: "id".to_owned(),
                data_type: "bigint".to_owned(),
                is_nullable: false,
                default_expr: None,
                is_identity: true,
                is_pk: true,
                is_unique: true,
            },
            ColumnDef {
                name: "status".to_owned(),
                data_type: "text".to_owned(),
                is_nullable: false,
                default_expr: Some("'pending'::text".to_owned()),
                is_identity: false,
                is_pk: false,
                is_unique: false,
            },
        ],
        constraints: vec![ConstraintDef {
            name: "chk_orders_status".to_owned(),
            kind: ConstraintKind::Check,
            definition: "CHECK (status IN ('pending', 'completed'))".to_owned(),
        }],
        indexes: vec![IndexDef {
            name: "idx_orders_status".to_owned(),
            definition: "CREATE INDEX idx_orders_status ON public.orders (status)".to_owned(),
            is_unique: false,
        }],
        comment: None,
    });

    let diff = compute_schema_diff(&s1, &s2);
    assert!(!diff.is_empty());
    assert_eq!(diff.added_tables.len(), 1);
    assert_eq!(diff.total_changes(), 1);

    let sql = generate_migration_sql(&diff, &MigrationOptions::default());
    assert!(sql.starts_with("-- Ignatius Migration: public -> public"));
    assert!(sql.contains("BEGIN;"));
    assert!(sql.contains("CREATE TABLE \"public\".\"orders\""));
    assert!(sql.contains("\"id\" bigint PRIMARY KEY"));
    assert!(sql.contains("\"status\" text NOT NULL DEFAULT 'pending'::text"));
    assert!(sql.contains("ALTER TABLE \"public\".\"orders\" ADD CONSTRAINT \"chk_orders_status\" CHECK (status IN ('pending', 'completed'));"));
    assert!(sql.contains("CREATE INDEX idx_orders_status ON public.orders (status);"));
    assert!(sql.ends_with("COMMIT;\n"));
}

#[test]
fn detects_dropped_table_and_generates_drop_table_cascade() {
    let mut s1 = SchemaSnapshot::empty("public");
    s1.tables.push(TableDef {
        schema: "public".to_owned(),
        name: "deprecated_logs".to_owned(),
        columns: vec![ColumnDef {
            name: "id".to_owned(),
            data_type: "integer".to_owned(),
            is_nullable: false,
            default_expr: None,
            is_identity: false,
            is_pk: true,
            is_unique: true,
        }],
        constraints: Vec::new(),
        indexes: Vec::new(),
        comment: None,
    });
    let s2 = SchemaSnapshot::empty("public");

    let diff = compute_schema_diff(&s1, &s2);
    assert_eq!(diff.dropped_tables, vec!["deprecated_logs"]);
    assert_eq!(diff.total_changes(), 1);

    let sql = generate_migration_sql(&diff, &MigrationOptions::default());
    assert!(sql.contains("DROP TABLE IF EXISTS \"public\".\"deprecated_logs\" CASCADE;"));
}

#[test]
fn detects_column_additions_modifications_and_deletions() {
    let mut s1 = SchemaSnapshot::empty("public");
    s1.tables.push(TableDef {
        schema: "public".to_owned(),
        name: "users".to_owned(),
        columns: vec![
            ColumnDef {
                name: "id".to_owned(),
                data_type: "integer".to_owned(),
                is_nullable: false,
                default_expr: None,
                is_identity: false,
                is_pk: true,
                is_unique: true,
            },
            ColumnDef {
                name: "email".to_owned(),
                data_type: "character varying(100)".to_owned(),
                is_nullable: true,
                default_expr: None,
                is_identity: false,
                is_pk: false,
                is_unique: false,
            },
            ColumnDef {
                name: "legacy_fax".to_owned(),
                data_type: "text".to_owned(),
                is_nullable: true,
                default_expr: None,
                is_identity: false,
                is_pk: false,
                is_unique: false,
            },
        ],
        constraints: Vec::new(),
        indexes: Vec::new(),
        comment: None,
    });

    let mut s2 = SchemaSnapshot::empty("public");
    s2.tables.push(TableDef {
        schema: "public".to_owned(),
        name: "users".to_owned(),
        columns: vec![
            ColumnDef {
                name: "id".to_owned(),
                data_type: "integer".to_owned(),
                is_nullable: false,
                default_expr: None,
                is_identity: false,
                is_pk: true,
                is_unique: true,
            },
            ColumnDef {
                name: "email".to_owned(),
                data_type: "text".to_owned(),
                is_nullable: false,
                default_expr: Some("''::text".to_owned()),
                is_identity: false,
                is_pk: false,
                is_unique: true,
            },
            ColumnDef {
                name: "phone".to_owned(),
                data_type: "text".to_owned(),
                is_nullable: true,
                default_expr: None,
                is_identity: false,
                is_pk: false,
                is_unique: false,
            },
        ],
        constraints: Vec::new(),
        indexes: Vec::new(),
        comment: None,
    });

    let diff = compute_schema_diff(&s1, &s2);
    assert_eq!(diff.modified_tables.len(), 1);
    let tbl_diff = &diff.modified_tables[0];
    assert_eq!(tbl_diff.table_name, "users");
    assert_eq!(tbl_diff.added_columns.len(), 1);
    assert_eq!(tbl_diff.added_columns[0].name, "phone");
    assert_eq!(tbl_diff.dropped_columns, vec!["legacy_fax"]);
    assert_eq!(tbl_diff.modified_columns.len(), 1);
    let col_diff = &tbl_diff.modified_columns[0];
    assert_eq!(col_diff.column_name, "email");
    assert!(col_diff.type_changed());
    assert!(col_diff.nullability_changed());
    assert!(col_diff.default_changed());

    let sql = generate_migration_sql(&diff, &MigrationOptions::default());
    assert!(sql.contains("ALTER TABLE \"public\".\"users\" DROP COLUMN IF EXISTS \"legacy_fax\";"));
    assert!(sql.contains("ALTER TABLE \"public\".\"users\" ADD COLUMN \"phone\" text;"));
    assert!(sql.contains(
        "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"email\" TYPE text USING \"email\"::text;"
    ));
    assert!(sql.contains("ALTER TABLE \"public\".\"users\" ALTER COLUMN \"email\" SET NOT NULL;"));
    assert!(
        sql.contains(
            "ALTER TABLE \"public\".\"users\" ALTER COLUMN \"email\" SET DEFAULT ''::text;"
        )
    );

    let tree = render_diff_tree(&diff, false);
    assert!(tree.contains("Schema Diff: public -> public"));
    assert!(tree.contains("~ Table users"));
    assert!(tree.contains("+ Column phone text"));
    assert!(tree.contains("- Column legacy_fax"));

    let json = render_diff_json(&diff);
    assert!(json.contains("\"schema_version\": 1"));
    assert!(json.contains("\"name\": \"users\""));
}

#[test]
fn table_blueprint_generates_accurate_create_table_ddl() {
    let blueprint = TableBlueprint::default();
    let ddl = blueprint.to_create_table_sql();
    assert!(ddl.contains("CREATE TABLE \"public\".\"new_table\" ("));
    assert!(ddl.contains("\"id\" bigint PRIMARY KEY GENERATED ALWAYS AS IDENTITY"));
    assert!(ddl.contains("\"name\" text NOT NULL"));
    assert!(ddl.contains("\"created_at\" timestamp with time zone NOT NULL DEFAULT now()"));
    assert!(ddl.ends_with(");\n"));

    let table_def = blueprint.to_table_def();
    assert_eq!(table_def.name, "new_table");
    assert_eq!(table_def.columns.len(), 3);
    assert!(table_def.columns[0].is_pk);
    assert!(table_def.columns[0].is_identity);
}
