use super::*;

#[test]
fn fleet_nested_arrays_report_and_remove_the_overflow_witness() {
    let mut values = vec!["a".to_owned(), "b".to_owned()];
    assert!(trim_nested(&mut values, 1));
    assert_eq!(values, ["a"]);
    assert!(!trim_nested(&mut values, 1));
}

#[tokio::test]
#[allow(clippy::print_stderr)]
async fn fleet_wire_bound_marks_over_limit_without_returning_definition() {
    let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
        eprintln!("skipping fleet wire-bound proof: IGNATIUS_TEST_PG_URI is not set");
        return;
    };
    let (client, connection) = tokio_postgres::connect(&uri, tokio_postgres::NoTls)
        .await
        .expect("connect to synthetic plain fixture");
    let driver = tokio::spawn(connection);
    client
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await
        .unwrap();
    let ordinary = capture_schema_snapshot(&client, Some("public"))
        .await
        .unwrap();
    let limited = capture(&client, Some("public"), 1, 128).await.unwrap();
    assert!(!ordinary.truncated);
    assert!(limited.truncated);
    let default_expr = |snapshot: SchemaSnapshot| {
        snapshot
            .tables
            .into_iter()
            .find(|table| table.name == "orders")
            .unwrap()
            .columns
            .into_iter()
            .find(|column| column.name == "order_id")
            .unwrap()
            .default_expr
    };
    assert!(default_expr(ordinary).unwrap().len() > 1);
    assert!(default_expr(limited).is_none());
    client.batch_execute("ROLLBACK").await.unwrap();
    drop(client);
    driver.await.unwrap().unwrap();
}
