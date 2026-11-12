use super::*;

#[test]
fn default_library_loads_all_curated_builtins() {
    let lib = SnippetLibrary::new();
    assert_eq!(lib.total_count(), 8);
    assert_eq!(lib.category_filter, SnippetCategory::All);
    assert_eq!(lib.filtered_snippets().len(), 8);

    let selected = lib.selected_snippet().expect("selected snippet");
    assert_eq!(selected.id, "cache-hit-ratio");
    assert!(selected.is_builtin);
}

#[test]
fn category_filtering_and_cycling() {
    let mut lib = SnippetLibrary::new();

    // Cycle to Admin
    lib.cycle_category();
    assert_eq!(lib.category_filter, SnippetCategory::Admin);
    let admin_snippets = lib.filtered_snippets();
    assert!(!admin_snippets.is_empty());
    assert!(
        admin_snippets
            .iter()
            .all(|s| s.category == SnippetCategory::Admin)
    );

    // Cycle to Performance
    lib.cycle_category();
    assert_eq!(lib.category_filter, SnippetCategory::Performance);
    let perf_snippets = lib.filtered_snippets();
    assert!(!perf_snippets.is_empty());
    assert!(
        perf_snippets
            .iter()
            .all(|s| s.category == SnippetCategory::Performance)
    );

    // Cycle to Patterns
    lib.cycle_category();
    assert_eq!(lib.category_filter, SnippetCategory::Patterns);
    let pattern_snippets = lib.filtered_snippets();
    assert!(!pattern_snippets.is_empty());
    assert!(
        pattern_snippets
            .iter()
            .all(|s| s.category == SnippetCategory::Patterns)
    );

    // Cycle to User
    lib.cycle_category();
    assert_eq!(lib.category_filter, SnippetCategory::User);
    assert!(lib.filtered_snippets().is_empty());

    // Cycle back to All
    lib.cycle_category();
    assert_eq!(lib.category_filter, SnippetCategory::All);
    assert_eq!(lib.filtered_snippets().len(), 8);
}

#[test]
fn text_search_filtering() {
    let mut lib = SnippetLibrary::new();

    lib.filter_query = "bloat".to_string();
    let matches = lib.filtered_snippets();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, "index-bloat");

    lib.filter_query = "nonexistent_query_xyz".to_string();
    assert!(lib.filtered_snippets().is_empty());
    assert_eq!(lib.selected_snippet(), None);
}

#[test]
fn selection_navigation_clamps_correctly() {
    let mut lib = SnippetLibrary::new();
    assert_eq!(lib.selected_idx, 0);

    lib.move_selection(1);
    assert_eq!(lib.selected_idx, 1);

    lib.move_selection(100);
    assert_eq!(lib.selected_idx, 7);

    lib.move_selection(-100);
    assert_eq!(lib.selected_idx, 0);
}

#[test]
fn name_validation_guards_boundaries() {
    assert!(SnippetLibrary::validate_name("").is_err());
    assert!(SnippetLibrary::validate_name("   ").is_err());
    assert!(SnippetLibrary::validate_name("foo/bar").is_err());
    assert!(SnippetLibrary::validate_name("foo\\bar").is_err());
    assert!(SnippetLibrary::validate_name("../secret").is_err());
    assert!(SnippetLibrary::validate_name("valid-name_123").is_ok());
}

#[test]
fn user_snippet_creation_and_disk_roundtrip() {
    let mut lib = SnippetLibrary::new();
    let temp_dir = tempfile::tempdir().expect("tempdir");

    let snippet = lib
        .add_user_snippet(
            "monthly-revenue",
            "Monthly Revenue Report",
            "Aggregates total customer revenue by calendar month.",
            SnippetCategory::User,
            "SELECT date_trunc('month', created_at) AS m, sum(total) FROM orders GROUP BY 1;",
            Some(temp_dir.path()),
        )
        .expect("add snippet");

    assert_eq!(snippet.id, "monthly-revenue");
    assert_eq!(snippet.category, SnippetCategory::User);
    assert!(!snippet.is_builtin);
    assert!(snippet.path.is_some());

    // Create a fresh library and load from directory
    let mut lib2 = SnippetLibrary::new();
    lib2.load_user_directory(temp_dir.path());

    assert_eq!(lib2.total_count(), 9);
    let loaded = lib2
        .snippets
        .iter()
        .find(|s| s.id == "monthly-revenue")
        .expect("found loaded snippet");
    assert_eq!(loaded.title, "Monthly Revenue Report");
    assert_eq!(
        loaded.description,
        "Aggregates total customer revenue by calendar month."
    );
    assert_eq!(loaded.category, SnippetCategory::User);
    assert!(loaded.sql.contains("sum(total)"));
}
