use super::*;

#[test]
fn an_existing_temporary_is_never_reopened_or_truncated() {
    let root = tempfile::tempdir().expect("temporary root");
    let temporary = root.path().join("reserved");
    fs::write(&temporary, b"earlier operation").expect("existing temporary");
    assert_eq!(
        create_private_temporary(&temporary)
            .expect_err("exclusive creation")
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&temporary).unwrap(), b"earlier operation");
}

#[cfg(unix)]
#[test]
fn even_a_dangling_temporary_symlink_is_not_followed() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().expect("temporary root");
    let outside = root.path().join("not-created");
    let temporary = root.path().join("reserved");
    symlink(&outside, &temporary).expect("dangling temporary");
    assert_eq!(
        create_private_temporary(&temporary)
            .expect_err("exclusive creation")
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(!outside.exists());
    assert!(fs::symlink_metadata(temporary).unwrap().is_symlink());
}

#[cfg(unix)]
#[test]
fn a_temporary_is_private_before_any_content_is_written() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("temporary root");
    let temporary = root.path().join("private");
    let file = create_private_temporary(&temporary).expect("exclusive temporary");
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn replacement_keeps_old_bytes_until_the_new_file_is_ready() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("state");
    fs::write(&path, b"old").expect("old state");
    write_atomic(&path, b"new").expect("replace");
    assert_eq!(fs::read(&path).expect("new state"), b"new");
    assert_eq!(fs::read_dir(root.path()).expect("list").count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn failed_rename_preserves_the_destination_and_removes_only_its_own_temporary() {
    let root = tempfile::tempdir().expect("temporary root");
    let destination = root.path().join("occupied");
    fs::create_dir(&destination).expect("destination directory");
    fs::write(destination.join("keep"), b"original").expect("existing contents");
    let unrelated = root.path().join(".occupied.tmp.old");
    fs::write(&unrelated, b"interrupted earlier operation").expect("unrelated temporary");
    assert!(write_atomic(&destination, b"replacement").is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"original");
    assert_eq!(
        fs::read(&unrelated).unwrap(),
        b"interrupted earlier operation"
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn missing_parent_is_a_failure_without_creating_a_destination() {
    let root = tempfile::tempdir().expect("temporary root");
    let path = root.path().join("missing/state");
    assert!(write_atomic(&path, b"state").is_err());
    assert!(!path.exists());
}
