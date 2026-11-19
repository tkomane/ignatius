use super::store::Store;
use super::*;
use std::fs::{self, OpenOptions};

fn measured(p95_ms: f64) -> Observation {
    Observation {
        unix_millis: 1_800_000_000_000,
        server_version_num: 180007,
        schema_sha256: "a".repeat(64),
        plan_sha256: "b".repeat(64),
        settings_sha256: "c".repeat(64),
        total_cost: 23.0,
        measurement: Some(Measurement {
            p50_ms: 2.0,
            p95_ms,
            explain_ms: 4.0,
            shared_read_blocks: 5,
            temp_written_blocks: 6,
            wal_bytes: 7,
        }),
    }
}

#[test]
fn timeline_marks_regressions_only_for_comparable_measurements() {
    let before = measured(10.0);
    let mut after = measured(13.0);
    after.schema_sha256 = "d".repeat(64);
    assert!(Changes::between(Some(&before), &after).p95_regression == Some(true));
    assert_eq!(
        Changes::between(Some(&before), &after).schema_changed,
        Some(true)
    );
    assert_eq!(Changes::between(None, &before).p95_delta_pct, None);

    after.server_version_num += 1;
    let change = Changes::between(Some(&before), &after);
    assert_eq!(change.p95_regression, None);
    assert_eq!(change.cost_delta_pct, None);
    assert_eq!(change.schema_changed, Some(true));

    after.server_version_num = before.server_version_num;
    after.measurement = None;
    assert!(!Changes::between(Some(&before), &after).comparable);
    assert_eq!(delta_pct(0.0, 1.0), None);
}

#[test]
fn store_bounds_observations_and_clears_only_the_named_series() {
    let root = tempfile::tempdir().expect("temporary data directory");
    let store = Store::new(root.path());
    let binding = "f".repeat(64);
    for entry in 0..=MAX_OBSERVATIONS {
        let mut observation = measured(4.0);
        observation.unix_millis += i64::try_from(entry).expect("bounded offset");
        store
            .append("read", binding.clone(), observation)
            .expect("append");
    }
    store
        .append("other", binding, measured(4.0))
        .expect("append");
    let catalog = store.read().expect("read catalog");
    let series = catalog.get("read").expect("series");
    assert_eq!(series.observations.len(), MAX_OBSERVATIONS);
    assert_eq!(
        series.observations[0].unix_millis,
        measured(4.0).unix_millis + 1
    );
    assert_eq!(series.forgotten_observations, 1);
    assert!(store.forget("read").expect("forget"));
    let catalog = store.read().expect("read catalog");
    assert!(catalog.get("read").is_none());
    assert!(catalog.get("other").is_some());
}

#[test]
fn store_refuses_corruption_binding_change_and_a_concurrent_writer() {
    let root = tempfile::tempdir().expect("temporary data directory");
    let store = Store::new(root.path());
    let binding = "f".repeat(64);
    store
        .append("read", binding.clone(), measured(4.0))
        .expect("initial append");
    let dir = root.path().join("plan-history");
    let data = dir.join("timeline.json");
    let initial = fs::read(&data).expect("saved catalog");
    assert!(store.append("read", "a".repeat(64), measured(4.0)).is_err());
    assert_eq!(fs::read(&data).expect("unchanged"), initial);

    let lock = OpenOptions::new()
        .write(true)
        .open(dir.join("timeline.lock"))
        .expect("open lock");
    lock.try_lock().expect("hold another writer's lock");
    let error = store
        .append("read", binding.clone(), measured(4.0))
        .expect_err("busy");
    assert!(error.headline.contains("another process"));
    assert_eq!(fs::read(&data).expect("unchanged"), initial);
    drop(lock);

    for bytes in [b"invalid".to_vec(), vec![b' '; 2 * 1024 * 1024 + 1]] {
        fs::write(&data, &bytes).expect("simulate corruption");
        assert!(
            store
                .append("read", binding.clone(), measured(4.0))
                .is_err()
        );
        assert_eq!(fs::read(&data).expect("unchanged"), bytes);
    }
}

#[test]
fn store_keeps_capture_order_when_a_later_observation_saves_first() {
    let root = tempfile::tempdir().expect("temporary data directory");
    let store = Store::new(root.path());
    let binding = "f".repeat(64);
    let mut later = measured(13.0);
    later.unix_millis += 5;
    store
        .append("read", binding.clone(), later)
        .expect("save later capture");
    let catalog = store
        .append("read", binding, measured(10.0))
        .expect("save earlier capture");
    let timeline = Timeline::new("read", catalog.get("read"));
    assert_eq!(timeline.observations[0].changes.p95_regression, None);
    assert_eq!(timeline.observations[1].changes.p95_regression, Some(true));
}
