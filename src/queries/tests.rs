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

#[cfg(unix)]
#[test]
fn collection_directory_symlinks_and_collection_file_symlinks_are_refused() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().expect("temporary root");
    let library = library(&dir);
    std::fs::create_dir_all(library.directory()).expect("library");

    let outside_dir = dir.path().join("outside_dir");
    std::fs::create_dir_all(&outside_dir).expect("outside dir");
    std::fs::write(outside_dir.join("secret.sql"), "SELECT 'secret';\n").expect("outside sql");

    // 1. Symlinked collection directory must be refused by save, load, and skipped by list.
    let linked_col = library.directory().join("linked_col");
    symlink(&outside_dir, &linked_col).expect("collection symlink");
    assert!(library.save("linked_col/secret", "SELECT 2;").is_err());
    assert!(library.load("linked_col/secret").is_err());
    assert!(library.list().expect("list").is_empty());
    assert_eq!(
        std::fs::read_to_string(outside_dir.join("secret.sql")).unwrap(),
        "SELECT 'secret';\n"
    );

    // 2. Real collection directory containing a symlinked .sql file must also be refused.
    let real_col = library.directory().join("billing");
    std::fs::create_dir_all(&real_col).expect("real collection");
    let outside_file = dir.path().join("outside_query.sql");
    std::fs::write(&outside_file, "SELECT 'private';\n").expect("outside file");
    symlink(&outside_file, real_col.join("linked_leaf.sql")).expect("leaf symlink");

    assert!(library.save("billing/linked_leaf", "SELECT 9;").is_err());
    assert!(library.load("billing/linked_leaf").is_err());
    assert!(library.list().expect("list").is_empty());
    assert_eq!(
        std::fs::read_to_string(&outside_file).unwrap(),
        "SELECT 'private';\n"
    );
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

#[test]
fn a_regular_file_cannot_act_as_a_collection_directory() {
    let dir = tempfile::tempdir().expect("temporary root");
    let library = library(&dir);
    std::fs::create_dir_all(library.directory()).expect("library");
    std::fs::write(library.directory().join("billing"), "not a dir").expect("file");
    assert!(library.save("billing/revenue", "SELECT 1;").is_err());
    assert!(library.load("billing/revenue").is_err());
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
fn single_level_collection_queries_save_load_and_list_cleanly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);

    let path = library
        .save(
            "billing/monthly-revenue",
            "-- @tags: finance, monthly\n-- @description: Monthly gross revenue\nSELECT sum(total)\nFROM orders;",
        )
        .expect("save collection query");
    assert_eq!(
        path,
        library
            .directory()
            .join("billing")
            .join("monthly-revenue.sql")
    );
    assert_eq!(
        library.load("billing/monthly-revenue").expect("load"),
        "-- @tags: finance, monthly\n-- @description: Monthly gross revenue\nSELECT sum(total)\nFROM orders;\n"
    );
    assert_eq!(
        library
            .load("billing/monthly-revenue.sql")
            .expect("load with extension"),
        "-- @tags: finance, monthly\n-- @description: Monthly gross revenue\nSELECT sum(total)\nFROM orders;\n"
    );

    library
        .save("root-health", "SELECT 1;")
        .expect("save flat query");

    // Nested depth > 1 on disk is ignored by list().
    let nested = library.directory().join("billing").join("nested");
    std::fs::create_dir_all(&nested).expect("nested dir");
    std::fs::write(nested.join("ignored.sql"), "SELECT 999;\n").expect("nested file");

    let found = library.list().expect("list");
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].name, "billing/monthly-revenue");
    assert_eq!(found[0].collection.as_deref(), Some("billing"));
    assert_eq!(found[0].tags, vec!["finance", "monthly"]);
    assert_eq!(
        found[0].description.as_deref(),
        Some("Monthly gross revenue")
    );
    assert_eq!(found[0].summary, "SELECT sum(total)");
    assert_eq!(found[0].preview, "SELECT sum(total)\nFROM orders;");

    assert_eq!(found[1].name, "root-health");
    assert_eq!(found[1].collection, None);
    assert!(found[1].tags.is_empty());
    assert_eq!(found[1].description, None);
}

#[test]
fn a_name_can_never_reach_outside_the_directory() {
    // This is the security boundary of the feature: a name is a name or
    // single-level collection/name.
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);

    for name in [
        "../escape",
        "..",
        ".",
        ".sql",
        "a/b/c",
        "/billing/monthly",
        "billing/",
        "billing/.",
        "billing/..",
        "billing/.sql",
        "./monthly",
        "billing/../escape",
        "billing /monthly",
        "billing/ monthly",
        "sub\\dir",
        "/etc/passwd",
        "C:\\windows\\system32",
        "billing:stream",
        "",
        "   ",
        "with\nnewline",
        "billing/with\u{1b}escape",
        // Reserved by Windows whatever follows them, in either segment.
        "CON",
        "nul",
        "COM1.sql",
        "aux",
        "CON/revenue",
        "lpt1/revenue",
        "billing/CON",
        "billing/nul.sql",
        "billing/COM9",
    ] {
        let error = library.path_for(name).expect_err(name);
        assert_eq!(error.exit_code(), crate::ExitCode::Usage, "{name}");
        assert!(error.next_action.is_some(), "{name}");
    }

    // And saving refuses for the same reason, before touching anything.
    assert!(library.save("../escape", "SELECT 1").is_err());
    assert!(library.save("a/b/c", "SELECT 1").is_err());
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
fn metadata_header_comments_and_filtered_listing_behave_cleanly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);

    library
        .save(
            "billing/ar-aging",
            "-- @tag: #Finance, aging, finance\n-- @description: Open AR invoices\nSELECT id FROM invoices;",
        )
        .expect("save ar");
    library
        .save(
            "billing/refunds",
            "-- @TAG: refunds\n-- @Description: Recent refunds\nSELECT id FROM refunds;",
        )
        .expect("save refunds");
    library
        .save(
            "ops/locks",
            "-- @tags: finance locks\nSELECT pid FROM pg_stat_activity;",
        )
        .expect("save locks");

    let by_col = library
        .list_filtered(Some("billing"), None)
        .expect("filter collection");
    assert_eq!(
        by_col.iter().map(|q| q.name.as_str()).collect::<Vec<_>>(),
        vec!["billing/ar-aging", "billing/refunds"]
    );

    let by_tag = library
        .list_filtered(None, Some("#FINANCE"))
        .expect("filter tag");
    assert_eq!(
        by_tag.iter().map(|q| q.name.as_str()).collect::<Vec<_>>(),
        vec!["billing/ar-aging", "ops/locks"]
    );
    assert_eq!(by_tag[0].tags, vec!["finance", "aging"]);

    let both = library
        .list_filtered(Some("billing"), Some("finance"))
        .expect("filter both");
    assert_eq!(both.len(), 1);
    assert_eq!(both[0].name, "billing/ar-aging");

    assert!(library.list_filtered(Some("../bad"), None).is_err());
    assert!(library.list_filtered(None, Some("   ")).is_err());
}

#[test]
fn oversized_query_files_are_bounded_in_load_and_skipped_in_list() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);
    std::fs::create_dir_all(library.directory()).expect("library");

    library.save("small", "SELECT 1;").expect("save small");
    let oversized_path = library.path_for("huge").expect("path");
    let oversized_payload = vec![b' '; usize::try_from(MAX_QUERY_FILE_BYTES).unwrap() + 16];
    std::fs::write(&oversized_path, oversized_payload).expect("write oversized");

    let load_err = library.load("huge").expect_err("oversized must refuse");
    assert_eq!(load_err.kind, DiagnosticKind::Usage);

    let listed = library.list().expect("list succeeds without oversized");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "small");
}

#[test]
fn preview_bounds_lines_and_strips_control_sequences() {
    let dir = tempfile::tempdir().expect("temp dir");
    let library = library(&dir);

    let mut sql = String::from("-- @tag: \u{1b}[31mops\n-- @description: \u{1b}[2JHealth\n");
    for idx in 0..40 {
        sql.push_str(&format!("SELECT {idx};\n"));
    }
    library.save("many-lines", &sql).expect("save");

    let found = library.list().expect("list");
    assert_eq!(found.len(), 1);
    let q = &found[0];
    assert!(!q.tags[0].contains('\u{1b}'));
    assert!(!q.description.as_deref().unwrap().contains('\u{1b}'));
    assert!(q.preview.lines().count() <= MAX_PREVIEW_LINES + 1);
    assert!(q.preview.ends_with("..."));
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
    assert!(!found[0].preview.contains('\u{1b}'), "{}", found[0].preview);
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
