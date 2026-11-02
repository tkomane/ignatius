use super::*;

fn write(contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("pg_service.conf");
    std::fs::write(&path, contents).expect("write");
    (dir, path)
}

#[test]
fn services_and_their_parameters_are_read() {
    let (_dir, path) = write(
        "# the team's databases\n\
         [orders-prod]\n\
         host=db.example.net\n\
         port=6432\n\
         dbname=orders\n\
         user=app\n\
         \n\
         [orders-dev]\n\
         host=localhost\n\
         dbname=orders_dev\n",
    );
    let file = load(&path).expect("parse");
    assert_eq!(file.names(), vec!["orders-dev", "orders-prod"]);

    let prod = file.service("orders-prod").expect("service");
    assert_eq!(prod["host"], "db.example.net");
    assert_eq!(prod["port"], "6432");
    assert_eq!(prod["dbname"], "orders");
    assert_eq!(prod["user"], "app");

    let dev = file.service("orders-dev").expect("service");
    assert_eq!(dev["host"], "localhost");
    assert!(
        !dev.contains_key("port"),
        "a service only carries what it states"
    );
}

#[test]
fn keys_are_case_insensitive_and_values_keep_their_case() {
    let (_dir, path) = write("[s]\nHost=Db.Example.Net\nDBNAME=Orders\n");
    let file = load(&path).expect("parse");
    let service = file.service("s").expect("service");
    assert_eq!(service["host"], "Db.Example.Net");
    assert_eq!(service["dbname"], "Orders");
}

#[test]
fn whitespace_and_comments_are_ignored() {
    let (_dir, path) =
        write("\n   # a comment\n\n  [spaced]  \n  host = db  \n\n# another\n  port=5432\n");
    let service = load(&path)
        .expect("parse")
        .service("spaced")
        .cloned()
        .expect("service");
    assert_eq!(service["host"], "db");
    assert_eq!(service["port"], "5432");
}

#[test]
fn a_malformed_line_is_an_error_rather_than_something_to_skip() {
    // Skipping half a service file would quietly send someone to the wrong
    // server, which is worse than refusing to read it.
    let (_dir, path) = write("[s]\nhost=db\nthis is not a parameter\n");
    let error = load(&path).expect_err("must refuse");
    assert!(
        error
            .technical
            .iter()
            .any(|f| f.label == "Line" && f.value == "3")
    );
    assert!(error.next_action.is_some());
}

#[test]
fn a_parameter_before_any_service_heading_is_an_error() {
    let (_dir, path) = write("host=db\n[s]\nport=5432\n");
    let error = load(&path).expect_err("must refuse");
    assert!(
        error
            .likely_cause
            .unwrap_or_default()
            .contains("before any"),
        "it should say what is wrong"
    );
}

#[test]
fn an_unknown_service_lists_the_ones_that_exist() {
    let (_dir, path) = write("[orders-prod]\nhost=db\n\n[orders-dev]\nhost=localhost\n");
    let error = resolve("orders-stage", Some(&path)).expect_err("must fail");
    let cause = error.likely_cause.expect("a cause");
    assert!(cause.contains("orders-prod"), "{cause}");
    assert!(cause.contains("orders-dev"), "{cause}");
    assert!(error.next_action.is_some());
}

#[test]
fn asking_for_a_service_with_no_file_says_where_to_put_one() {
    let error = resolve("anything", None).expect_err("must fail");
    assert!(
        error.headline.contains("no service file"),
        "{}",
        error.headline
    );
    assert!(
        error
            .next_action
            .expect("action")
            .contains("pg_service.conf"),
        "it should say where to create one"
    );
}

#[test]
fn an_empty_service_is_valid_and_carries_nothing() {
    let (_dir, path) = write("[empty]\n");
    let service = resolve("empty", Some(&path)).expect("resolve");
    assert!(service.is_empty());
}
