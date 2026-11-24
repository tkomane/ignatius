use super::has_meaningful_stdout;

#[test]
fn empty_compose_output_is_not_a_running_database() {
    assert!(!has_meaningful_stdout(true, b"\n"));
}

#[test]
fn a_compose_container_id_is_running_database_evidence() {
    assert!(has_meaningful_stdout(true, b"container-id\n"));
}

#[test]
fn a_failed_compose_query_is_not_running_database_evidence() {
    assert!(!has_meaningful_stdout(false, b"container-id\n"));
}
