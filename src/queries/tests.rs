use super::*;

#[test]
fn replacing_a_long_valid_query_name_does_not_lengthen_its_temporary_name() {
    let dir = tempfile::tempdir().expect("temporary root");
    let library = library(&dir);
    std::fs::create_dir_all(library.directory()).expect("library");
    let name = "q".repeat(250);
    let path = library.path_for(&name).expect("valid plain name");
    std::fs::write(&path, "SELECT 1\n").expect("valid 254-byte file name");
    library
        .save(&name, "SELECT 2")
        .expect("replace valid named file");
    assert_eq!(library.load(&name).unwrap(), "SELECT 2\n");
}

#[test]
fn replacing_a_hard_link_does_not_modify_the_other_file() {
    let dir = tempfile::tempdir().expect("temporary root");
    let library = library(&dir);
    std::fs::create_dir_all(library.directory()).expect("library");
    let outside = dir.path().join("outside.sql");
    std::fs::write(&outside, "outside original").expect("outside file");
    std::fs::hard_link(&outside, library.path_for("shared").unwrap()).expect("query hard link");
    library
        .save("shared", "SELECT 2")
        .expect("replace named file");
    assert_eq!(library.load("shared").unwrap(), "SELECT 2\n");
    assert_eq!(
        std::fs::read_to_string(outside).unwrap(),
        "outside original"
    );
}

#[cfg(unix)]
#[test]
fn saved_query_links_cannot_read_or_overwrite_outside_the_library() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().expect("temporary root");
    let library = library(&dir);
    std::fs::create_dir_all(library.directory()).expect("library");
    let outside = dir.path().join("outside.sql");
    std::fs::write(&outside, "private outside text").expect("outside file");
    let link = library.path_for("linked").expect("name");
    symlink(&outside, &link).expect("query link");
    assert!(library.save("linked", "SELECT 2").is_err());
    assert!(library.load("linked").is_err());
    assert!(library.list().expect("list").is_empty());
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "private outside text"
    );
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
}

#[test]
fn a_non_regular_query_entry_is_refused_without_losing_its_contents() {
    let dir = tempfile::tempdir().expect("temporary root");
    let library = library(&dir);
    let path = library.path_for("directory").expect("name");
    std::fs::create_dir_all(&path).expect("non-regular entry");
    std::fs::write(path.join("keep"), "preserve").expect("original contents");
    assert!(library.save("directory", "SELECT 1").is_err());
    assert!(library.load("directory").is_err());
    assert!(library.list().expect("list").is_empty());
    assert_eq!(
        std::fs::read_to_string(path.join("keep")).unwrap(),
        "preserve"
    );
}

fn library(dir: &tempfile::TempDir) -> Library {
    Library::new(&dir.path().join("queries"))
}

#[test]
fn a_query_is_saved_as_an_ordinary_sql_file_and_read_back() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);

    let path = library
        .save("monthly-revenue", "SELECT sum(total) FROM orders;")
        .expect("save");
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("sql"));
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        "SELECT sum(total) FROM orders;\n",
        "it is the SQL, with a final newline, and nothing of ours around it"
    );
    assert_eq!(
        library.load("monthly-revenue").expect("load"),
        "SELECT sum(total) FROM orders;\n"
    );

    // The extension is not doubled when the name already has it.
    let path = library
        .save("with-extension.sql", "SELECT 1")
        .expect("save");
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        Some("with-extension.sql")
    );
}

#[test]
fn a_name_can_never_reach_outside_the_directory() {
    // This is the security boundary of the feature: a name is a name.
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);

    for name in [
        "../escape",
        "..",
        ".",
        "sub/dir",
        "sub\\dir",
        "/etc/passwd",
        "C:\\windows\\system32",
        "",
        "   ",
        "with\nnewline",
        "with\u{1b}escape",
        // Reserved by Windows whatever follows them, and refused everywhere
        // so a saved query can travel between platforms.
        "CON",
        "nul",
        "COM1.sql",
        "aux",
    ] {
        let error = library.path_for(name).expect_err(name);
        assert_eq!(error.exit_code(), crate::ExitCode::Usage, "{name}");
        assert!(error.next_action.is_some(), "{name}");
    }

    // And saving refuses for the same reason, before touching anything.
    assert!(library.save("../escape", "SELECT 1").is_err());
    assert!(
        !dir.path().join("escape.sql").exists(),
        "nothing was written outside the directory"
    );
}

#[test]
fn the_listing_is_by_name_and_says_what_each_one_does() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);
    library
        .save(
            "second",
            "-- a header comment\n-- another\nSELECT 2 FROM orders;",
        )
        .expect("save");
    library.save("first", "SELECT 1;").expect("save");
    // Something that is not a saved query at all.
    std::fs::write(library.directory().join("notes.txt"), "ignore me").expect("write");

    let found = library.list().expect("list");
    assert_eq!(found.len(), 2, "only .sql files: {found:?}");
    assert_eq!(found[0].name, "first");
    assert_eq!(found[1].name, "second");
    assert_eq!(
        found[1].summary, "SELECT 2 FROM orders;",
        "the header comment is skipped: a query is listed by what it does"
    );
}

#[test]
fn a_directory_that_does_not_exist_yet_lists_nothing_rather_than_failing() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(library(&dir).list().expect("list").is_empty());
}

#[test]
fn a_hostile_saved_query_cannot_drive_the_terminal_from_the_listing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);
    library.save("hostile", "\u{1b}[2JSELECT 1;").expect("save");
    let found = library.list().expect("list");
    assert!(!found[0].summary.contains('\u{1b}'), "{}", found[0].summary);
}

#[test]
fn saving_under_a_name_that_exists_replaces_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);
    library.save("q", "SELECT 1;").expect("save");
    library.save("q", "SELECT 2;").expect("save");
    assert_eq!(library.load("q").expect("load"), "SELECT 2;\n");
    assert_eq!(library.list().expect("list").len(), 1);
}
