use super::*;

#[test]
fn restricting_a_file_is_idempotent_and_reported_honestly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("config.toml");
    std::fs::write(&file, b"schema_version = 1\n").expect("write");

    restrict_to_owner(&file).expect("first call");
    restrict_to_owner(&file).expect("repeat call is safe");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "owner-only file permissions");
        assert!(enforces_file_permissions());
    }
    #[cfg(not(unix))]
    assert!(!enforces_file_permissions());
}

#[test]
fn platform_name_is_never_empty() {
    assert!(!name().is_empty());
}
