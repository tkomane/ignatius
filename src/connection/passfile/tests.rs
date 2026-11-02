use super::*;
use secrecy::ExposeSecret;
use std::io::Write;

/// Writes a password file with owner-only permissions.
fn write_passfile(contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("pgpass");
    let mut file = std::fs::File::create(&path).expect("create");
    file.write_all(contents.as_bytes()).expect("write");
    drop(file);
    crate::platform::restrict_to_owner(&path).expect("restrict");
    (dir, path)
}

#[test]
fn an_exact_line_matches_and_yields_its_password() {
    let (_dir, path) = write_passfile(
        "# a comment\n\
         db.example.net:5432:orders:app:not-a-real-password\n",
    );
    let lookup = lookup(&path, "db.example.net", 5432, "orders", "app");
    assert_eq!(
        lookup.password().expect("matched").expose_secret(),
        "not-a-real-password"
    );
    assert!(lookup.note().is_none(), "a working file says nothing");
    let Lookup::Matched { line, .. } = lookup else {
        panic!("expected a match");
    };
    assert_eq!(line, 2, "the line number helps the user find it");
}

#[test]
fn wildcards_match_any_value_in_their_column() {
    let (_dir, path) = write_passfile("*:*:*:app:wildcard-password\n");
    assert_eq!(
        lookup(&path, "anything", 1234, "whatever", "app")
            .password()
            .expect("matched")
            .expose_secret(),
        "wildcard-password"
    );
    assert!(
        lookup(&path, "anything", 1234, "whatever", "someone-else")
            .password()
            .is_none(),
        "the user column still has to match"
    );
}

#[test]
fn the_first_matching_line_wins() {
    let (_dir, path) = write_passfile(
        "db:5432:orders:app:first\n\
         *:*:*:*:second\n",
    );
    assert_eq!(
        lookup(&path, "db", 5432, "orders", "app")
            .password()
            .expect("matched")
            .expose_secret(),
        "first"
    );
}

#[test]
fn colons_and_backslashes_in_a_password_are_escaped() {
    let (_dir, path) = write_passfile("db:5432:orders:app:pa\\:ss\\\\word\n");
    assert_eq!(
        lookup(&path, "db", 5432, "orders", "app")
            .password()
            .expect("matched")
            .expose_secret(),
        "pa:ss\\word",
        "an escaped colon is part of the password, not a field separator"
    );
}

#[test]
fn a_socket_path_is_matched_as_localhost_the_way_libpq_does_it() {
    let (_dir, path) = write_passfile("localhost:5432:orders:app:socket-password\n");
    assert_eq!(
        lookup(&path, "/var/run/postgresql", 5432, "orders", "app")
            .password()
            .expect("matched")
            .expose_secret(),
        "socket-password"
    );
}

#[test]
fn a_file_others_can_read_is_refused_and_says_how_to_fix_it() {
    let (_dir, path) = write_passfile("*:*:*:*:secret\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("loosen");

        let outcome = lookup(&path, "db", 5432, "orders", "app");
        assert!(
            outcome.password().is_none(),
            "a world-readable password file must not be used"
        );
        let note = outcome.note().expect("it must say why");
        assert!(note.contains("readable by others"), "{note}");
        assert!(note.contains("chmod 600"), "and how to fix it: {note}");
    }
    #[cfg(not(unix))]
    {
        // No permission bits to check; the file is used and the platform's
        // ACL is what protects it. Documented in data-handling.md.
        assert!(
            lookup(&path, "db", 5432, "orders", "app")
                .password()
                .is_some()
        );
    }
}

#[test]
fn a_file_that_matches_nothing_says_so_rather_than_staying_silent() {
    let (_dir, path) = write_passfile("other:5432:other:other:nope\n");
    let outcome = lookup(&path, "db", 5432, "orders", "app");
    assert!(outcome.password().is_none());
    let note = outcome.note().expect("a note");
    assert!(note.contains("no line"), "{note}");
    assert!(
        note.contains("1 line(s) read"),
        "the count distinguishes an empty file: {note}"
    );
}

#[test]
fn a_missing_file_is_not_an_error_and_says_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let outcome = lookup(&dir.path().join("absent"), "db", 5432, "orders", "app");
    assert!(matches!(outcome, Lookup::NotFound { .. }));
    assert!(outcome.note().is_none(), "not having one is normal");
}

#[test]
fn malformed_lines_are_skipped_rather_than_guessed_at() {
    let (_dir, path) = write_passfile(
        "this line has too few fields\n\
         \n\
         db:5432:orders:app:good\n",
    );
    assert_eq!(
        lookup(&path, "db", 5432, "orders", "app")
            .password()
            .expect("matched")
            .expose_secret(),
        "good"
    );
}

#[test]
fn a_password_containing_a_hash_is_not_treated_as_a_comment() {
    let (_dir, path) = write_passfile("db:5432:orders:app:pa#ss\n");
    assert_eq!(
        lookup(&path, "db", 5432, "orders", "app")
            .password()
            .expect("matched")
            .expose_secret(),
        "pa#ss"
    );
}

#[test]
fn no_note_or_debug_output_ever_contains_the_password() {
    let (_dir, path) = write_passfile("db:5432:orders:app:hunter2-not-a-real-password\n");
    let outcome = lookup(&path, "db", 5432, "orders", "app");
    assert!(
        !format!("{outcome:?}").contains("hunter2"),
        "Debug leaked it"
    );
    assert!(outcome.note().unwrap_or_default().is_empty());
}
