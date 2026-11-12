use super::*;

#[test]
fn test_pii_type_heuristics() {
    assert_eq!(PiiType::infer_from_name("email"), Some(PiiType::Email));
    assert_eq!(PiiType::infer_from_name("user_email"), Some(PiiType::Email));
    assert_eq!(PiiType::infer_from_name("phone"), Some(PiiType::Phone));
    assert_eq!(PiiType::infer_from_name("mobile_num"), Some(PiiType::Phone));
    assert_eq!(
        PiiType::infer_from_name("first_name"),
        Some(PiiType::PersonName)
    );
    assert_eq!(
        PiiType::infer_from_name("lastname"),
        Some(PiiType::PersonName)
    );
    assert_eq!(
        PiiType::infer_from_name("credit_card"),
        Some(PiiType::Financial)
    );
    assert_eq!(
        PiiType::infer_from_name("account_number"),
        Some(PiiType::Financial)
    );
    assert_eq!(PiiType::infer_from_name("ssn"), Some(PiiType::NationalId));
    assert_eq!(
        PiiType::infer_from_name("tax_id"),
        Some(PiiType::NationalId)
    );
    assert_eq!(
        PiiType::infer_from_name("billing_address"),
        Some(PiiType::Address)
    );
    assert_eq!(
        PiiType::infer_from_name("postal_code"),
        Some(PiiType::Address)
    );
    assert_eq!(
        PiiType::infer_from_name("password_hash"),
        Some(PiiType::Secret)
    );
    assert_eq!(
        PiiType::infer_from_name("auth_token"),
        Some(PiiType::Secret)
    );
    assert_eq!(PiiType::infer_from_name("created_at"), None);
    assert_eq!(PiiType::infer_from_name("order_id"), None);
}

#[test]
fn test_mask_rule_sql_synthesis() {
    let email_sql = MaskRule::Email.to_sql_expression("user_email");
    assert!(email_sql.contains("REGEXP_REPLACE(\"user_email\"::text"));

    let phone_sql = MaskRule::Phone.to_sql_expression("phone");
    assert!(phone_sql.contains("+1-555-"));

    let name_sql = MaskRule::FullName.to_sql_expression("full_name");
    assert!(name_sql.contains("'User ' || UPPER(SUBSTRING(MD5(\"full_name\"::text)"));

    let hash_sql = MaskRule::HashHex {
        salt: "secret_salt".to_string(),
    }
    .to_sql_expression("id");
    assert!(hash_sql.contains("MD5(\"id\"::text || 'secret_salt')"));

    let redact_sql = MaskRule::Redact {
        text: "[REDACTED]".to_string(),
    }
    .to_sql_expression("notes");
    assert_eq!(redact_sql, "'[REDACTED]'");

    let null_sql = MaskRule::Nullify.to_sql_expression("secret");
    assert_eq!(null_sql, "NULL");

    let rand_sql = MaskRule::RandomNumber { min: 10, max: 100 }.to_sql_expression("age");
    assert!(rand_sql.contains("FLOOR(10 + RANDOM() * (100 - 10 + 1))::bigint"));

    let custom_sql = MaskRule::CustomSql("MD5(col)".to_string()).to_sql_expression("col");
    assert_eq!(custom_sql, "MD5(col)");
}

#[test]
fn test_table_mask_policy_generate_view_sql() {
    let all_cols = vec![
        "id".to_string(),
        "email".to_string(),
        "password_hash".to_string(),
        "created_at".to_string(),
    ];

    let policy = TableMaskPolicy {
        schema: "public".to_string(),
        table: "users".to_string(),
        all_columns: all_cols.clone(),
        column_rules: vec![
            ("email".to_string(), MaskRule::Email),
            ("password_hash".to_string(), MaskRule::Nullify),
        ],
    };

    let view_sql = policy.generate_view_sql("masked_", &all_cols);
    assert!(view_sql.contains("CREATE OR REPLACE VIEW \"masked_users\" AS"));
    assert!(view_sql.contains("FROM \"public\".\"users\";"));
    assert!(view_sql.contains("\"id\","));
    assert!(view_sql.contains("REGEXP_REPLACE(\"email\"::text"));
    assert!(view_sql.contains("NULL AS \"password_hash\""));
    assert!(view_sql.contains("\"created_at\""));
}

#[test]
fn test_mask_report_rendering_and_json() {
    let report = MaskReport {
        candidates: vec![
            PiiCandidate {
                schema: "public".to_string(),
                table: "customers".to_string(),
                column: "email".to_string(),
                data_type: "text".to_string(),
                pii_type: PiiType::Email,
                rule: MaskRule::Email,
                confidence: 95,
            },
            PiiCandidate {
                schema: "public".to_string(),
                table: "customers".to_string(),
                column: "phone_number".to_string(),
                data_type: "varchar".to_string(),
                pii_type: PiiType::Phone,
                rule: MaskRule::Phone,
                confidence: 90,
            },
        ],
        policies: vec![TableMaskPolicy {
            schema: "public".to_string(),
            table: "customers".to_string(),
            all_columns: vec![
                "id".to_string(),
                "email".to_string(),
                "phone_number".to_string(),
            ],
            column_rules: vec![
                ("email".to_string(), MaskRule::Email),
                ("phone_number".to_string(), MaskRule::Phone),
            ],
        }],
    };

    assert_eq!(report.total_candidates(), 2);

    let table = report.render_table();
    assert!(table.contains("Ignatius Data Masking & PII Forensics"));
    assert!(table.contains("customers"));
    assert!(table.contains("email"));
    assert!(table.contains("EMAIL_MASK"));

    let json = report.render_json();
    assert!(json.contains("\"table\": \"customers\""));
    assert!(json.contains("\"rule\": \"Email\""));

    let parsed: MaskReport = serde_json::from_str(&json).expect("valid json");
    assert_eq!(parsed, report);

    let sql = report.render_sql("masked_");
    assert!(sql.contains("CREATE OR REPLACE VIEW \"masked_customers\" AS"));
}

#[test]
fn test_truncate_str() {
    assert_eq!(truncate_str("hello", 10), "hello");
    assert_eq!(truncate_str("superlongstringexceeding", 12), "superlong...");
}
