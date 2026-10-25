use super::*;

#[test]
fn focus_cycles_through_the_panes_that_are_on_screen() {
    assert_eq!(Focus::Editor.next(true), Focus::Results);
    assert_eq!(Focus::Results.next(true), Focus::Objects);
    assert_eq!(Focus::Objects.next(true), Focus::Editor);

    // With the tree hidden, Tab never lands on it.
    assert_eq!(Focus::Results.next(false), Focus::Editor);

    for focus in [Focus::Editor, Focus::Results, Focus::Objects] {
        assert!(!focus.label().is_empty());
    }
}

#[test]
fn cancellation_requested_is_a_state_of_its_own() {
    let requested = QueryPhase::CancellationRequested { job: JobId(1) };
    assert_eq!(requested.label(), "Cancellation requested");
    assert_ne!(requested.label(), QueryPhase::Idle.label());
    assert!(requested.is_busy(), "the statement may still be running");
    assert_eq!(requested.job(), Some(JobId(1)));
}

#[test]
fn a_lost_connection_still_names_its_target() {
    let info = Box::new(SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "1".into(),
        search_path: "public".into(),
        read_only: false,
        tls: crate::postgres::TlsState::Disabled,
        environment: Environment::Local,
    });
    let lost = ConnectionState::Lost { info };
    assert!(lost.label().contains("connection lost"));
    assert!(
        !lost.is_usable(),
        "statements must not be sent on a lost connection"
    );
    assert!(lost.info().is_some());
}

#[test]
fn job_identities_are_unique_and_increasing() {
    let mut model = Model::new(100);
    let first = model.allocate_job();
    let second = model.allocate_job();
    assert_ne!(first, second);
    assert!(second > first);
}

#[test]
fn an_unclassified_connection_is_not_production() {
    let model = Model::new(100);
    assert_eq!(model.environment(), Environment::Unclassified);
    assert!(!model.environment().is_production());
}
