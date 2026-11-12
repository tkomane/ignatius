use super::*;

#[test]
fn prng_is_deterministic_and_reproducible() {
    let mut prng1 = DeterministicPrng::new(42);
    let mut prng2 = DeterministicPrng::new(42);

    let seq1: Vec<u64> = (0..20).map(|_| prng1.next_u64()).collect();
    let seq2: Vec<u64> = (0..20).map(|_| prng2.next_u64()).collect();

    assert_eq!(seq1, seq2);

    let mut prng3 = DeterministicPrng::new(999);
    let seq3: Vec<u64> = (0..20).map(|_| prng3.next_u64()).collect();
    assert_ne!(seq1, seq3);
}

#[test]
fn prng_next_range_bounds() {
    let mut prng = DeterministicPrng::new(12345);
    for _ in 0..100 {
        let v = prng.next_range(10, 25);
        assert!((10..=25).contains(&v));
    }

    assert_eq!(prng.next_range(5, 5), 5);
    assert_eq!(prng.next_range(10, 5), 10);
}

#[test]
fn prng_choose_slice() {
    let mut prng = DeterministicPrng::new(777);
    let empty: [i32; 0] = [];
    assert_eq!(prng.choose(&empty), None);

    let items = ["apple", "banana", "cherry"];
    let chosen = prng.choose(&items);
    assert!(chosen.is_some());
    assert!(items.contains(chosen.unwrap()));
}

#[test]
fn column_generator_inference_heuristics() {
    let col_id = ColumnGenerator::infer("id", "integer", true, false, None, None);
    assert!(col_id.is_primary_key);
    assert_eq!(
        col_id.strategy,
        SeedStrategy::IntegerSequence { start: 1, step: 1 }
    );

    let col_uuid = ColumnGenerator::infer("account_id", "uuid", false, false, None, None);
    assert_eq!(col_uuid.strategy, SeedStrategy::UuidV4);

    let col_email = ColumnGenerator::infer("user_email", "varchar", false, true, None, None);
    assert_eq!(col_email.strategy, SeedStrategy::Email);

    let col_phone = ColumnGenerator::infer("phone_number", "text", false, true, None, None);
    assert_eq!(col_phone.strategy, SeedStrategy::PhoneNumber);

    let col_city = ColumnGenerator::infer("shipping_city", "text", false, true, None, None);
    assert_eq!(col_city.strategy, SeedStrategy::City);

    let col_country = ColumnGenerator::infer("billing_country", "text", false, true, None, None);
    assert_eq!(col_country.strategy, SeedStrategy::Country);

    let col_bool = ColumnGenerator::infer("is_active", "boolean", false, true, None, None);
    assert_eq!(
        col_bool.strategy,
        SeedStrategy::BooleanRatio { true_percent: 50 }
    );

    let col_amount = ColumnGenerator::infer("total_amount", "numeric", false, true, None, None);
    assert_eq!(
        col_amount.strategy,
        SeedStrategy::NumericDecimal {
            min_cents: 500,
            max_cents: 99_900,
            scale: 2
        }
    );

    let parent_key = TableKey::new("public", "authors");
    let col_fk = ColumnGenerator::infer(
        "author_id",
        "integer",
        false,
        true,
        Some((&parent_key, "id")),
        None,
    );
    assert_eq!(
        col_fk.strategy,
        SeedStrategy::ForeignKeySample {
            parent_table: TableKey::new("public", "authors"),
            parent_column: "id".to_string(),
        }
    );
}

#[test]
fn generate_fixtures_parent_child_referential_integrity() {
    let authors_table = TableGenerator {
        key: TableKey::new("public", "authors"),
        columns: vec![
            ColumnGenerator {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                is_primary_key: true,
                is_nullable: false,
                strategy: SeedStrategy::IntegerSequence {
                    start: 100,
                    step: 1,
                },
            },
            ColumnGenerator {
                name: "name".to_string(),
                data_type: "text".to_string(),
                is_primary_key: false,
                is_nullable: false,
                strategy: SeedStrategy::PersonName,
            },
            ColumnGenerator {
                name: "email".to_string(),
                data_type: "text".to_string(),
                is_primary_key: false,
                is_nullable: false,
                strategy: SeedStrategy::Email,
            },
        ],
    };

    let books_table = TableGenerator {
        key: TableKey::new("public", "books"),
        columns: vec![
            ColumnGenerator {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                is_primary_key: true,
                is_nullable: false,
                strategy: SeedStrategy::IntegerSequence { start: 1, step: 1 },
            },
            ColumnGenerator {
                name: "author_id".to_string(),
                data_type: "integer".to_string(),
                is_primary_key: false,
                is_nullable: false,
                strategy: SeedStrategy::ForeignKeySample {
                    parent_table: TableKey::new("public", "authors"),
                    parent_column: "id".to_string(),
                },
            },
            ColumnGenerator {
                name: "title".to_string(),
                data_type: "text".to_string(),
                is_primary_key: false,
                is_nullable: false,
                strategy: SeedStrategy::CompanyName,
            },
            ColumnGenerator {
                name: "price".to_string(),
                data_type: "numeric".to_string(),
                is_primary_key: false,
                is_nullable: false,
                strategy: SeedStrategy::NumericDecimal {
                    min_cents: 1000,
                    max_cents: 5000,
                    scale: 2,
                },
            },
        ],
    };

    let report = generate_fixtures(&[authors_table, books_table], 5, 42);

    assert_eq!(report.total_rows, 10);
    assert_eq!(report.tables.len(), 2);

    let authors_data = &report.tables[0];
    assert_eq!(authors_data.rows.len(), 5);
    let author_ids: Vec<String> = authors_data.rows.iter().map(|r| r[0].clone()).collect();
    assert_eq!(author_ids, vec!["100", "101", "102", "103", "104"]);

    let books_data = &report.tables[1];
    assert_eq!(books_data.rows.len(), 5);
    for row in &books_data.rows {
        let fk_val = &row[1];
        assert!(
            author_ids.contains(fk_val),
            "Child FK {fk_val} must be in parent PKs {author_ids:?}"
        );
    }
}

#[test]
fn fixture_report_formatters() {
    let users_table = TableGenerator {
        key: TableKey::new("public", "users"),
        columns: vec![
            ColumnGenerator {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                is_primary_key: true,
                is_nullable: false,
                strategy: SeedStrategy::IntegerSequence { start: 1, step: 1 },
            },
            ColumnGenerator {
                name: "name".to_string(),
                data_type: "text".to_string(),
                is_primary_key: false,
                is_nullable: false,
                strategy: SeedStrategy::ConstantText {
                    text: "Alice".to_string(),
                },
            },
        ],
    };

    let report = generate_fixtures(&[users_table], 2, 99);

    let sql = report.render_sql();
    assert!(sql.contains("BEGIN;"));
    assert!(sql.contains("INSERT INTO \"public\".\"users\" (\"id\", \"name\") VALUES"));
    assert!(sql.contains("(1, 'Alice')"));
    assert!(sql.contains("(2, 'Alice')"));
    assert!(sql.contains("COMMIT;"));

    let jsonl = report.render_jsonl();
    assert!(jsonl.contains("\"_table\":\"public.users\""));
    assert!(jsonl.contains("\"id\":\"1\""));
    assert!(jsonl.contains("\"name\":\"Alice\""));
    assert!(jsonl.contains("\"id\":\"2\""));

    let csv = report.render_csv();
    assert!(csv.contains("# Table: public.users"));
    assert!(csv.contains("id,name"));
    assert!(csv.contains("1,Alice"));
    assert!(csv.contains("2,Alice"));

    let table = report.render_table();
    assert!(table.contains("Seed: 99"));
    assert!(table.contains("public"));
    assert!(table.contains("users"));
    assert!(table.contains("COLUMNS"));
    assert!(table.contains("ROWS"));
}

#[test]
fn strategy_serde_roundtrip() {
    let strat = SeedStrategy::NumericDecimal {
        min_cents: 10,
        max_cents: 200,
        scale: 2,
    };
    let serialized = serde_json::to_string(&strat).expect("serialize");
    let deserialized: SeedStrategy = serde_json::from_str(&serialized).expect("deserialize");
    assert_eq!(strat, deserialized);
}
