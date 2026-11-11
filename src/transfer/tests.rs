use super::*;

#[test]
fn copy_from_sql_builder_formats_clean_csv_and_text_statements() {
    let mut options = ImportOptions {
        schema: "public".to_owned(),
        table: "orders".to_owned(),
        file_path: "orders.csv".to_owned(),
        format: TransferFormat::Csv,
        delimiter: b',',
        has_header: true,
        null_string: "".to_owned(),
        quote: b'"',
        escape: b'"',
        columns: vec![
            "id".to_owned(),
            "customer_id".to_owned(),
            "total".to_owned(),
        ],
        dry_run: false,
        transactional: true,
    };

    let sql = build_copy_from_sql(&options);
    assert_eq!(
        sql,
        "COPY \"public\".\"orders\" (\"id\", \"customer_id\", \"total\") FROM STDIN WITH (FORMAT csv, DELIMITER ',', HEADER true, NULL '', QUOTE '\"', ESCAPE '\"')"
    );

    options.format = TransferFormat::Tsv;
    options.delimiter = b'\t';
    options.null_string = "\\N".to_owned();
    let sql_tsv = build_copy_from_sql(&options);
    assert_eq!(
        sql_tsv,
        "COPY \"public\".\"orders\" (\"id\", \"customer_id\", \"total\") FROM STDIN WITH (FORMAT text, DELIMITER E'\\t', NULL '\\N')"
    );
}

#[test]
fn copy_to_sql_builder_formats_select_query_and_table_sources() {
    let mut options = ExportOptions {
        query: "SELECT id, title, price FROM products WHERE active = true;".to_owned(),
        output_path: Some("products.csv".to_owned()),
        format: TransferFormat::Csv,
        delimiter: b',',
        has_header: true,
        null_string: "".to_owned(),
        quote: b'"',
        escape: b'"',
    };

    let sql = build_copy_to_sql(&options);
    assert_eq!(
        sql,
        "COPY (SELECT id, title, price FROM products WHERE active = true) TO STDOUT WITH (FORMAT csv, DELIMITER ',', HEADER true, NULL '', QUOTE '\"', ESCAPE '\"')"
    );

    options.query = "reporting.quarterly_summary".to_owned();
    let sql_table = build_copy_to_sql(&options);
    assert_eq!(
        sql_table,
        "COPY reporting.quarterly_summary TO STDOUT WITH (FORMAT csv, DELIMITER ',', HEADER true, NULL '', QUOTE '\"', ESCAPE '\"')"
    );
}

#[test]
fn csv_delimiter_detection_identifies_commas_tabs_semicolons_and_pipes() {
    let csv_sample = "id,name,value\n1,\"item, with comma\",42.50\n2,normal,10.00\n";
    assert_eq!(detect_csv_delimiter(csv_sample), b',');

    let tsv_sample = "id\tname\tvalue\n1\titem\t42.50\n2\tnormal\t10.00\n";
    assert_eq!(detect_csv_delimiter(tsv_sample), b'\t');

    let semi_sample = "id;name;value\n1;item;42.50\n2;normal;10.00\n";
    assert_eq!(detect_csv_delimiter(semi_sample), b';');

    let pipe_sample = "id|name|value\n1|item|42.50\n2|normal|10.00\n";
    assert_eq!(detect_csv_delimiter(pipe_sample), b'|');
}

#[test]
fn csv_parser_handles_quoted_newlines_escapes_and_headers() {
    let data = "id,description,total\n1,\"line 1\nline 2 with \"\"quotes\"\" and , comma\",100.50\n2,simple,200\n";
    let (headers, sample) = parse_csv_header_and_sample(data, b',', 10);

    assert_eq!(headers, vec!["id", "description", "total"]);
    assert_eq!(sample.len(), 2);
    assert_eq!(sample[0][0], "1");
    assert_eq!(sample[0][1], "line 1\nline 2 with \"quotes\" and , comma");
    assert_eq!(sample[0][2], "100.50");
    assert_eq!(sample[1], vec!["2", "simple", "200"]);
}

#[test]
fn column_type_inference_detects_int_float_bool_date_json_and_text() {
    assert_eq!(infer_column_type(&["1", "42", "-100", ""]), "bigint");
    assert_eq!(infer_column_type(&["1.5", "42.0", "-100.25"]), "numeric");
    assert_eq!(infer_column_type(&["true", "false", "t", "f"]), "boolean");
    assert_eq!(
        infer_column_type(&["2026-11-11", "2026-12-01 10:00:00"]),
        "timestamptz"
    );
    assert_eq!(
        infer_column_type(&["{\"key\": 1}", "{\"valid\": true}"]),
        "jsonb"
    );
    assert_eq!(
        infer_column_type(&["[1, 2, 3]", "[\"alpha\", \"beta\"]"]),
        "jsonb"
    );
    assert_eq!(infer_column_type(&["mixed", "123", "text"]), "text");
    assert_eq!(infer_column_type(&["", "null", "\\N"]), "text");
}

#[test]
fn pg_dump_and_restore_command_builders_generate_redacted_env_and_flags() {
    let backup_options = BackupOptions {
        database: "production_db".to_owned(),
        host: Some("db.internal".to_owned()),
        port: Some(5432),
        user: Some("dba".to_owned()),
        output_path: "/tmp/backup.dump".to_owned(),
        format: BackupFormat::Custom,
        schema: Some("public".to_owned()),
        table: None,
        clean: true,
        create: false,
        data_only: false,
        schema_only: false,
        verbose: true,
    };

    let (cmd, args, env) = build_pg_dump_command(&backup_options, Some("supersecret"), true);
    assert_eq!(cmd, "pg_dump");
    assert!(args.contains(&"-F".to_owned()));
    assert!(args.contains(&"c".to_owned()));
    assert!(args.contains(&"--clean".to_owned()));
    assert!(args.contains(&"--verbose".to_owned()));
    assert!(args.contains(&"-n".to_owned()));
    assert!(args.contains(&"public".to_owned()));
    assert_eq!(
        env,
        vec![("PGPASSWORD".to_owned(), "[redacted]".to_owned())]
    );

    let restore_options = RestoreOptions {
        database: "staging_db".to_owned(),
        host: Some("127.0.0.1".to_owned()),
        port: Some(5432),
        user: Some("postgres".to_owned()),
        input_path: "/tmp/backup.dump".to_owned(),
        clean: true,
        create: false,
        data_only: false,
        schema_only: false,
        verbose: false,
    };

    let (restore_cmd, restore_args, restore_env) =
        build_pg_restore_command(&restore_options, Some("secretpass"), false);
    assert_eq!(restore_cmd, "pg_restore");
    assert!(restore_args.contains(&"-d".to_owned()));
    assert!(restore_args.contains(&"staging_db".to_owned()));
    assert!(restore_args.contains(&"--clean".to_owned()));
    assert_eq!(
        restore_env,
        vec![("PGPASSWORD".to_owned(), "secretpass".to_owned())]
    );
}

#[test]
fn transfer_result_json_rendering_is_deterministic() {
    let result = ImportResult {
        rows_imported: 4200,
        bytes_processed: 1048576,
        elapsed_ms: 125,
        status: "Committed 4200 rows".to_owned(),
    };

    let json = render_transfer_result_json(&result);
    assert!(json.contains("\"rows\": 4200"));
    assert!(json.contains("\"bytes\": 1048576"));
    assert!(json.contains("\"elapsed_ms\": 125"));
    assert!(json.contains("\"status\": \"Committed 4200 rows\""));
}
