use super::*;

#[test]
fn density_existing_constructors_and_old_configuration_remain_comfortable() {
    let old: crate::config::Config = toml::from_str("[ui]\ntheme=\"dark\"\n").unwrap();
    assert_eq!(old.ui.density, crate::config::Density::Comfortable);
    assert_eq!(
        Presentation::default().density,
        crate::config::Density::Comfortable
    );
    let p = Presentation::new(
        crate::ui::Theme::new(crate::config::ThemeChoice::Dark, false),
        crate::ui::Glyphs::new(crate::ui::GlyphTier::Ascii),
        false,
    );
    assert_eq!(p.density, crate::config::Density::Comfortable);
    assert_eq!(p.spacing().grid_cell_padding, 1);
    assert_eq!(
        p.with_density(crate::config::Density::Compact)
            .spacing()
            .grid_cell_padding,
        1
    );
}

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

#[test]
fn notice_identity_checkpoints_retirement_and_acknowledgement_are_bounded_facts() {
    use crate::query::result::{NoticeCheckpoint, NoticeCount, NoticeCounts};
    let counts = |n| NoticeCounts {
        omitted: NoticeCount::Exact(n),
        ..NoticeCounts::default()
    };
    let mut state = NoticeState::default();
    let id = NoticeSessionId::new();
    let stale = NoticeSessionId::new();
    assert_eq!(id, id.clone());
    assert_ne!(id, stale);
    assert_eq!(format!("{id:?}"), "NoticeSessionId(..)");
    state.install(NoticeSource::User, 1, id.clone());
    state.observe(
        NoticeSource::User,
        1,
        &stale,
        NoticeCheckpoint {
            revision: 1,
            totals: counts(99),
        },
        None,
    );
    assert!(!state.pending.needs_report());
    let checkpoint = NoticeCheckpoint {
        revision: 1,
        totals: counts(4),
    };
    state.observe(NoticeSource::User, 1, &id, checkpoint, None);
    state.observe(NoticeSource::User, 1, &id, checkpoint, None);
    assert_eq!(state.pending, counts(4));
    let snapshot = state.snapshot();
    state.observe(
        NoticeSource::User,
        1,
        &id,
        NoticeCheckpoint {
            revision: 2,
            totals: counts(7),
        },
        None,
    );
    state.acknowledge(&snapshot);
    assert_eq!(state.pending, counts(3));
    state.retire(NoticeSource::User, 1, &id, counts(9));
    assert_eq!(state.pending, counts(5));
    state.retire(NoticeSource::User, 1, &id, counts(99));
    assert_eq!(state.pending, counts(5));
    state.observe(
        NoticeSource::User,
        1,
        &id,
        NoticeCheckpoint {
            revision: 3,
            totals: counts(99),
        },
        None,
    );
    assert_eq!(state.pending, counts(5));
    let snapshot = state.snapshot();
    state.acknowledge(&snapshot);
    assert!(!state.pending.needs_report());
    assert_eq!(state.sources.iter().flatten().count(), 0);
}

#[test]
fn notice_acknowledgement_after_source_retirement_keeps_new_loss_and_unknown_precision() {
    use crate::query::result::{NoticeCheckpoint, NoticeCount, NoticeCounts};
    let counts = |n| NoticeCounts {
        omitted: NoticeCount::Exact(n),
        ..NoticeCounts::default()
    };
    let mut state = NoticeState::default();
    let id = NoticeSessionId::new();
    state.install(NoticeSource::User, 1, id.clone());
    state.observe(
        NoticeSource::User,
        1,
        &id,
        NoticeCheckpoint {
            revision: 1,
            totals: counts(4),
        },
        None,
    );
    let snapshot = state.snapshot();
    state.retire(NoticeSource::User, 1, &id, counts(6));
    state.acknowledge(&snapshot);
    assert_eq!(state.pending, counts(2));
    assert_eq!(state.previous, counts(2));
    state.revision = u64::MAX;
    let snapshot = state.snapshot();
    state.pending = state.pending.plus(counts(1));
    state.acknowledge(&snapshot);
    assert_eq!(
        state.pending,
        counts(1),
        "saturated revision equality cannot erase new loss"
    );
    assert_eq!(
        NoticeCounts::UNKNOWN.since(NoticeCounts::UNKNOWN),
        NoticeCounts::UNKNOWN
    );
}
