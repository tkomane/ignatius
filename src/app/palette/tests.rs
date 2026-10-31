use super::*;

fn entry(label: &str) -> PaletteEntry {
    PaletteEntry {
        label: label.to_owned(),
        detail: String::new(),
        group: "Test",
        command: PaletteCommand::Insert(label.to_owned()),
    }
}

#[test]
fn a_subsequence_matches_and_a_missing_character_does_not() {
    assert!(fuzzy_score("cusord", "customer_orders").is_some());
    assert!(fuzzy_score("orders", "orders").is_some());
    assert!(fuzzy_score("", "anything").is_some());
    assert!(fuzzy_score("zzz", "orders").is_none());
    assert!(
        fuzzy_score("ordersx", "orders").is_none(),
        "every character of the query must appear"
    );
}

#[test]
fn matching_ignores_case_in_both_directions() {
    assert!(fuzzy_score("ORD", "orders").is_some());
    assert!(fuzzy_score("ord", "ORDERS").is_some());
}

#[test]
fn a_contiguous_match_beats_a_scattered_one() {
    let contiguous = fuzzy_score("order", "orders").expect("match");
    let scattered = fuzzy_score("order", "o_r_d_e_r").expect("match");
    assert!(
        contiguous > scattered,
        "contiguous {contiguous} should beat scattered {scattered}"
    );
}

#[test]
fn initials_find_what_you_meant() {
    // Typing the initials of a snake_case name should rank it first.
    let mut palette = Palette::new(vec![
        entry("customer_order_totals"),
        entry("cost"),
        entry("accounts"),
    ]);
    palette.query = "cot".into();
    let matches = palette.matches();
    assert_eq!(matches[0].label, "customer_order_totals", "{:?}", matches);
}

#[test]
fn a_shorter_name_wins_a_tie() {
    let short = fuzzy_score("orders", "orders").expect("match");
    let long = fuzzy_score("orders", "orders_archive_2019").expect("match");
    assert!(short > long, "short {short} should beat long {long}");
}

#[test]
fn an_empty_query_lists_everything_in_its_original_order() {
    let palette = Palette::new(vec![entry("b"), entry("a"), entry("c")]);
    let labels: Vec<&str> = palette.matches().iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, vec!["b", "a", "c"], "browsing order is preserved");
}

#[test]
fn results_do_not_reshuffle_between_identical_queries() {
    let mut palette = Palette::new(vec![entry("alpha"), entry("alpen"), entry("alpaca")]);
    palette.query = "alp".into();
    let first: Vec<String> = palette.matches().iter().map(|e| e.label.clone()).collect();
    let second: Vec<String> = palette.matches().iter().map(|e| e.label.clone()).collect();
    assert_eq!(first, second);
}

#[test]
fn the_detail_field_is_searched_but_ranks_below_the_label() {
    let mut palette = Palette::new(vec![
        PaletteEntry {
            label: "unrelated".into(),
            detail: "public.orders".into(),
            group: "Objects",
            command: PaletteCommand::Insert("x".into()),
        },
        entry("orders"),
    ]);
    palette.query = "orders".into();
    let matches = palette.matches();
    assert_eq!(matches.len(), 2, "both are found");
    assert_eq!(matches[0].label, "orders", "the label match ranks first");
}

#[test]
fn typing_resets_the_selection_so_enter_never_picks_a_stale_row() {
    let mut palette = Palette::new(vec![entry("alpha"), entry("beta"), entry("gamma")]);
    palette.move_selection(2);
    assert_eq!(palette.selected, 2);
    palette.push('a');
    assert_eq!(palette.selected, 0, "a new query starts at the top");

    palette.move_selection(1);
    palette.backspace();
    assert_eq!(palette.selected, 0);
}

#[test]
fn selection_is_clamped_to_the_matches() {
    let mut palette = Palette::new(vec![entry("alpha"), entry("beta")]);
    for _ in 0..10 {
        palette.move_selection(1);
    }
    assert_eq!(palette.selected, 1);
    assert_eq!(palette.selected_entry().expect("entry").label, "beta");

    // Narrowing the matches must not leave the selection pointing past them.
    palette.query = "alpha".into();
    assert_eq!(
        palette.selected_entry().expect("entry").label,
        "alpha",
        "the selection follows the matches"
    );
}

#[test]
fn a_query_matching_nothing_selects_nothing() {
    let mut palette = Palette::new(vec![entry("alpha")]);
    palette.query = "zzzz".into();
    assert!(palette.matches().is_empty());
    assert!(palette.selected_entry().is_none());
}

#[test]
fn multibyte_queries_do_not_panic() {
    let mut palette = Palette::new(vec![entry("日本語のテーブル"), entry("orders")]);
    palette.query = "日本".into();
    assert_eq!(palette.matches().len(), 1);
    palette.push('語');
    palette.backspace();
    assert!(palette.selected_entry().is_some());
}

#[test]
fn result_grid_palettes_name_the_surface_and_search_their_entries() {
    let mut controls = Palette::over_result_controls(vec![PaletteEntry {
        label: "Sort selected column".into(),
        detail: "original server order".into(),
        group: "Result grid",
        command: PaletteCommand::Grid(GridCommand::SortSelected),
    }]);
    assert!(controls.purpose.title().contains("Result grid"));
    assert!(
        controls
            .purpose
            .standing_note()
            .expect("safety note")
            .contains("never rerun")
    );
    controls.query = "sort".into();
    assert_eq!(
        controls.selected_entry().expect("sort").label,
        "Sort selected column"
    );

    let mut columns = Palette::over_result_columns(vec![
        PaletteEntry {
            label: "Column 1: name".into(),
            detail: "shown  text".into(),
            group: "Result columns",
            command: PaletteCommand::ResultColumn(0),
        },
        PaletteEntry {
            label: "Column 2: name".into(),
            detail: "hidden  type unavailable".into(),
            group: "Result columns",
            command: PaletteCommand::ResultColumn(1),
        },
    ]);
    assert!(columns.purpose.title().contains("Result columns"));
    assert!(
        columns
            .purpose
            .standing_note()
            .expect("column note")
            .contains("one column")
    );
    columns.query = "Column 2".into();
    let selected = columns.selected_entry().expect("second duplicate label");
    assert_eq!(selected.label, "Column 2: name");
    assert_eq!(selected.command, PaletteCommand::ResultColumn(1));
    assert!(selected.detail.contains("hidden"));
    assert!(selected.detail.contains("unavailable"));
}

#[test]
fn command_palette_names_search_and_prerequisite_guidance() {
    let palette = Palette::over_commands(
        vec![entry("Run the whole buffer")],
        "Focus: Editor. Connect before running SQL. Type to search; Esc closes.",
    );
    assert_eq!(palette.purpose, Purpose::Commands);
    assert!(palette.purpose.title().contains("Command palette"));
    assert!(palette.purpose.title().contains("Type to search"));
    assert_eq!(
        palette.context_note.as_deref(),
        Some("Focus: Editor. Connect before running SQL. Type to search; Esc closes.")
    );
    assert!(palette.purpose.empty_message().contains("applicable"));
}

#[test]
fn supports_preview_is_true_only_for_the_four_preview_bearing_purposes() {
    let run_entry = PaletteEntry {
        label: "Command".into(),
        detail: "Run action".into(),
        group: "Session",
        command: PaletteCommand::Run(Action::SwitchTheme),
    };
    let insert_entry = PaletteEntry {
        label: "public.users".into(),
        detail: "table public.users".into(),
        group: "Objects",
        command: PaletteCommand::Insert("public.users".into()),
    };

    // GoTo with object/Insert entries supports preview
    let goto_objects = Palette::new(vec![insert_entry.clone()]);
    assert!(goto_objects.supports_preview());

    // GoTo with only Run command entries does NOT support preview (renders full width)
    let goto_commands = Palette::new(vec![run_entry.clone()]);
    assert!(!goto_commands.supports_preview());

    // Empty GoTo does not support preview
    let goto_empty = Palette::new(Vec::new());
    assert!(!goto_empty.supports_preview());

    // Saved queries, connections, dependencies support preview
    let saved = Palette::over_saved_queries(vec![PaletteEntry {
        label: "all_users".into(),
        detail: "SELECT * FROM users;".into(),
        group: "Saved",
        command: PaletteCommand::Open("all_users".into()),
    }]);
    assert!(saved.supports_preview());

    let connections = Palette::over_connections(vec![PaletteEntry {
        label: "production-replica".into(),
        detail: "postgres.internal:5432 / app_db".into(),
        group: "Production",
        command: PaletteCommand::ConnectProfile(Some("production-replica".into())),
    }]);
    assert!(connections.supports_preview());

    let dependencies = Palette::over_dependencies(vec![PaletteEntry {
        label: "public.orders".into(),
        detail: "view public.order_summary".into(),
        group: "Used by",
        command: PaletteCommand::Insert("public.orders".into()),
    }]);
    assert!(dependencies.supports_preview());

    // Full width elsewhere
    let commands = Palette::over_commands(vec![run_entry.clone()], "Context note");
    assert!(!commands.supports_preview());

    let history = Palette::over_history(vec![PaletteEntry {
        label: "SELECT 1".into(),
        detail: "just now".into(),
        group: "History",
        command: PaletteCommand::Insert("SELECT 1".into()),
    }]);
    assert!(!history.supports_preview());

    let controls = Palette::over_result_controls(vec![PaletteEntry {
        label: "Sort".into(),
        detail: "order".into(),
        group: "Grid",
        command: PaletteCommand::Grid(crate::app::grid::GridCommand::SortSelected),
    }]);
    assert!(!controls.supports_preview());

    let columns = Palette::over_result_columns(vec![PaletteEntry {
        label: "col1".into(),
        detail: "shown".into(),
        group: "Columns",
        command: PaletteCommand::ResultColumn(0),
    }]);
    assert!(!columns.supports_preview());

    let export = Palette::over_export_formats(
        vec![PaletteEntry {
            label: "CSV".into(),
            detail: "comma separated".into(),
            group: "Formats",
            command: PaletteCommand::ChooseExportFormat(ExportFormat::Csv),
        }],
        "Export format guidance",
    );
    assert!(!export.supports_preview());
}

#[test]
fn selected_preview_derives_pure_retained_text_for_all_four_preview_bearing_purposes() {
    // 1. Definition (GoTo with object entry)
    let def_palette = Palette::new(vec![PaletteEntry {
        label: "users".into(),
        detail: "table public.users".into(),
        group: "Objects",
        command: PaletteCommand::Insert("public.users".into()),
    }]);
    let preview = def_palette.selected_preview().expect("definition preview");
    let joined = preview.join("\n");
    assert!(joined.contains("Kind: Objects"));
    assert!(joined.contains("Identifier: public.users"));
    assert!(joined.contains("Summary: table public.users"));

    // 2. Saved queries
    let saved_palette = Palette::over_saved_queries(vec![PaletteEntry {
        label: "monthly_revenue".into(),
        detail: "SELECT sum(amount) FROM payments;".into(),
        group: "Saved",
        command: PaletteCommand::Open("monthly_revenue".into()),
    }]);
    let preview = saved_palette
        .selected_preview()
        .expect("saved query preview");
    let joined = preview.join("\n");
    assert!(joined.contains("Query: monthly_revenue"));
    assert!(joined.contains("Origin: Saved"));
    assert!(joined.contains("SQL: SELECT sum(amount) FROM payments;"));

    // 3. Profile (credential safety: safe summary only, never expose secret or credential)
    let conn_palette = Palette::over_connections(vec![PaletteEntry {
        label: "prod_db".into(),
        detail: "db.production.internal:5432 / prod (tls verify-full)".into(),
        group: "Production",
        command: PaletteCommand::ConnectProfile(Some("prod_db".into())),
    }]);
    let preview = conn_palette
        .selected_preview()
        .expect("connection profile preview");
    let joined = preview.join("\n");
    assert!(joined.contains("Profile: prod_db"));
    assert!(joined.contains("Posture: Production"));
    assert!(joined.contains("Target: db.production.internal:5432 / prod (tls verify-full)"));
    assert!(!joined.contains("password"));

    // 4. Dependency
    let dep_palette = Palette::over_dependencies(vec![PaletteEntry {
        label: "public.orders_view".into(),
        detail: "view on table public.orders".into(),
        group: "Depends on",
        command: PaletteCommand::Insert("public.orders_view".into()),
    }]);
    let preview = dep_palette.selected_preview().expect("dependency preview");
    let joined = preview.join("\n");
    assert!(joined.contains("Direction: Depends on"));
    assert!(joined.contains("Object: public.orders_view"));
    assert!(joined.contains("Details: view on table public.orders"));
}

#[test]
fn selected_preview_returns_fallback_message_when_matches_is_empty() {
    let mut palette = Palette::over_saved_queries(vec![PaletteEntry {
        label: "query_1".into(),
        detail: "SELECT 1".into(),
        group: "Saved",
        command: PaletteCommand::Open("query_1".into()),
    }]);
    palette.query = "nonexistent_needle".into();
    assert!(palette.matches().is_empty());
    let preview = palette
        .selected_preview()
        .expect("fallback preview when matches empty");
    assert_eq!(preview, vec!["No matching entry selected."]);
}

#[test]
fn selected_preview_returns_none_for_non_preview_purposes() {
    let palette = Palette::over_commands(
        vec![PaletteEntry {
            label: "Switch theme".into(),
            detail: "Cycle theme".into(),
            group: "Session",
            command: PaletteCommand::Run(Action::SwitchTheme),
        }],
        "Note",
    );
    assert!(palette.selected_preview().is_none());
}

#[test]
fn moving_selection_updates_selected_preview_synchronously_with_zero_side_effects() {
    let mut palette = Palette::over_saved_queries(vec![
        PaletteEntry {
            label: "query_alpha".into(),
            detail: "SELECT 'alpha';".into(),
            group: "Saved",
            command: PaletteCommand::Open("query_alpha".into()),
        },
        PaletteEntry {
            label: "query_beta".into(),
            detail: "SELECT 'beta';".into(),
            group: "Saved",
            command: PaletteCommand::Open("query_beta".into()),
        },
    ]);
    assert_eq!(palette.selected, 0);
    assert!(
        palette
            .selected_preview()
            .unwrap()
            .join("\n")
            .contains("query_alpha")
    );

    palette.move_selection(1);
    assert_eq!(palette.selected, 1);
    assert!(
        palette
            .selected_preview()
            .unwrap()
            .join("\n")
            .contains("query_beta")
    );

    palette.move_selection(-1);
    assert_eq!(palette.selected, 0);
    assert!(
        palette
            .selected_preview()
            .unwrap()
            .join("\n")
            .contains("query_alpha")
    );
}
