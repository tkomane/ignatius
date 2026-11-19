use super::Store;

#[test]
fn lock_releases_with_a_retained_file_description() {
    let root = tempfile::tempdir().expect("temporary data directory");
    let store = Store::new(root.path());
    let guard = store.lock().expect("lock the catalog");
    let inherited = guard
        .0
        .try_clone()
        .expect("retain the open file description");
    let Err(error) = store.lock() else {
        panic!("an independent writer must be refused");
    };
    assert_eq!(error.headline, "another process is updating plan history");

    drop(guard);
    let reacquired = store
        .lock()
        .expect("release despite the retained reference");
    drop(inherited);
    drop(reacquired);
}
