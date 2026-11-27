use super::*;
use crate::app::PlanView;
use crate::app::model::{
    NamePrompt, ParameterPrompt, PendingCopy, PendingPlan, PendingRun, UpdateCandidate,
    UpdatePrompt,
};
use crate::config::ThemeChoice;
use crate::postgres::metadata::{ColumnInfo, ObjectKind, UpdateRelation};
use crate::query::PlanDocument;
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const PLAIN_PLAN: &str = r#"[
  {
    "Plan": {
      "Node Type": "Nested Loop",
      "Startup Cost": 0.42,
      "Total Cost": 18.4,
      "Plan Rows": 4,
      "Plan Width": 32,
      "Plans": [
        {
          "Node Type": "Seq Scan",
          "Relation Name": "accounts",
          "Alias": "a",
          "Startup Cost": 0.0,
          "Total Cost": 24.0,
          "Plan Rows": 100,
          "Plan Width": 16,
          "Filter": "(owner = 'demo')"
        },
        {
          "Node Type": "Index Scan",
          "Relation Name": "orders",
          "Index Name": "orders_owner_idx",
          "Startup Cost": 0.2,
          "Total Cost": 5.1,
          "Plan Rows": 4,
          "Plan Width": 24,
          "Index Cond": "(owner_id = a.id)"
        }
      ]
    },
    "Planning Time": 0.31
  }
]"#;

fn presentation(depth: ColorDepth) -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

fn test_update_prompt() -> UpdatePrompt {
    let candidate = UpdateCandidate {
        result_job: crate::query::result::JobId(1),
        source_row: 0,
        result_column: 1,
        source_sql: "SELECT order_id, note FROM public.orders".into(),
    };
    let source = crate::query::parse_update_source(
        "SELECT order_id, note FROM public.orders",
        &["order_id".into(), "note".into()],
        1,
    )
    .expect("update source");
    let relation = UpdateRelation {
        schema: "public".into(),
        relation: "orders".into(),
        kind: ObjectKind::Table,
        readable: true,
        writable: true,
        columns: vec![
            ColumnInfo {
                name: "order_id".into(),
                data_type: "integer".into(),
                nullable: false,
                primary_key: true,
                default: None,
            },
            ColumnInfo {
                name: "note".into(),
                data_type: "text".into(),
                nullable: true,
                primary_key: false,
                default: None,
            },
        ],
    };
    UpdatePrompt::new(candidate, source, relation)
}

#[test]
fn plan_tree_renders_full_width_selection_bar_and_multi_span_unselected_nodes() {
    let document = PlanDocument::parse(PLAIN_PLAN, false).expect("valid plan");
    let mut view = PlanView::default();
    view.ready(document.clone());

    let area = Rect::new(0, 0, 80, 5);
    let pres_tc = presentation(ColorDepth::TrueColor);
    let mut buf_tc = Buffer::empty(area);
    render_plan_tree(&document, &view, &pres_tc, area, &mut buf_tc);

    let selection_bg = pres_tc
        .theme
        .style(Token::Selection)
        .bg
        .expect("selection bg");

    // 1. In TrueColor: selected plan row (row 0) must paint Token::Selection background across all cells of area.width
    for x in 0..80 {
        assert_eq!(
            buf_tc[(x, 0)].bg,
            selection_bg,
            "x={x} on selected row must have selection background"
        );
    }

    // 2. Unselected attention row (row 1: Seq Scan):
    let line1 = (0..80).map(|x| buf_tc[(x, 1)].symbol()).collect::<String>();
    assert!(line1.contains("Seq Scan accounts"));
    let warning_fg = pres_tc.theme.color(Token::Warning).unwrap();
    let scan_idx = line1[..line1.find("Seq Scan").unwrap()].chars().count() as u16;
    assert_eq!(buf_tc[(scan_idx, 1)].fg, warning_fg);

    // 3. Unselected non-attention row (row 2: Index Scan):
    let line2 = (0..80).map(|x| buf_tc[(x, 2)].symbol()).collect::<String>();
    assert!(line2.contains("Index Scan orders"));
    assert!(line2.contains(" | "));

    let header_fg = pres_tc.theme.color(Token::Header).unwrap();
    let border_fg = pres_tc.theme.color(Token::Border).unwrap();
    let muted_fg = pres_tc.theme.color(Token::Muted).unwrap();

    let pipe_idx = line2[..line2.find(" | ").unwrap()].chars().count() as u16;
    // Pipe delimiter " | " is styled with Token::Border
    assert_eq!(buf_tc[(pipe_idx + 1, 2)].fg, border_fg);

    // Subject before " | " (e.g. at start of "Index Scan") is styled with Token::Header
    let subject_idx = line2[..line2.find("Index Scan").unwrap()].chars().count() as u16;
    assert_eq!(buf_tc[(subject_idx, 2)].fg, header_fg);

    // Metrics after " | " are compact and styled with Token::Muted; the
    // width and cost units stay in the detail block.
    assert!(
        line2.contains("est 4 rows \u{00b7} cost 0.20..5.10"),
        "{line2}"
    );
    assert!(!line2.contains("width"), "{line2}");
    let metric_idx = line2[..line2.find("est 4").unwrap()].chars().count() as u16;
    assert_eq!(buf_tc[(metric_idx, 2)].fg, muted_fg);

    // Depth reads through Border-toned guides and an open chevron on the
    // node with children, the way the Objects tree draws it.
    let line0 = (0..80).map(|x| buf_tc[(x, 0)].symbol()).collect::<String>();
    assert!(line0.starts_with(">\u{25be} Nested Loop | "), "{line0}");
    assert!(line2.starts_with(" \u{2502}   Index Scan"), "{line2}");
    assert_eq!(buf_tc[(1, 2)].fg, border_fg);
    assert!(
        !line0.contains("\u{2514}\u{2500}") && !line0.contains("[-]"),
        "{line0}"
    );

    // 4. In Basic16 / None: backgrounds are Color::Reset and symbols remain identical
    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let pres_mono = presentation(depth);
        let mut buf_mono = Buffer::empty(area);
        render_plan_tree(&document, &view, &pres_mono, area, &mut buf_mono);

        for x in 0..80 {
            assert_eq!(
                buf_mono[(x, 0)].bg,
                Color::Reset,
                "mono background at {x} must be Reset"
            );
        }
        assert_eq!(buffer_to_string(&buf_tc), buffer_to_string(&buf_mono));
    }
}

#[test]
fn plan_summary_and_detail_render_multi_span_semantic_tokens_preserving_words() {
    let document = PlanDocument::parse(PLAIN_PLAN, false).expect("valid plan");
    let mut view = PlanView::default();
    view.ready(document.clone());

    let pres = presentation(ColorDepth::TrueColor);
    let summary_area = Rect::new(0, 0, 100, 2);
    let mut summary_buf = Buffer::empty(summary_area);
    let summary_text = plan_summary(&document);
    render_plan_summary(
        &summary_text,
        &document,
        &pres,
        summary_area,
        &mut summary_buf,
    );

    let summary_rendered = buffer_to_string(&summary_buf);
    assert!(summary_rendered.contains("EXPLAIN estimate only"));
    assert!(summary_rendered.contains("node(s)"));
    assert!(summary_rendered.contains("Attention: "));

    // Verify first line has mode in Token::Header and "; " in Token::Border
    let header_fg = pres.theme.color(Token::Header).unwrap();
    let border_fg = pres.theme.color(Token::Border).unwrap();
    let info_fg = pres.theme.color(Token::Info).unwrap();

    // "EXPLAIN" is in Token::Header
    assert_eq!(summary_buf[(0, 0)].fg, header_fg);

    // First "; " separator
    let semi_idx = summary_rendered[..summary_rendered.find("; ").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(summary_buf[(semi_idx, 0)].fg, border_fg);
    // After "; ", next segment is in Token::Info
    assert_eq!(summary_buf[(semi_idx + 2, 0)].fg, info_fg);

    // Detail pane
    let detail_area = Rect::new(0, 0, 100, 5);
    let mut detail_buf = Buffer::empty(detail_area);
    render_plan_detail(&document, &view, &pres, detail_area, &mut detail_buf);

    let detail_rendered = buffer_to_string(&detail_buf);
    assert!(detail_rendered.contains("Selected: Nested Loop  (1/3)"));

    // In detail: "Selected: " in Token::Info, rest in Token::Header
    assert_eq!(detail_buf[(0, 0)].fg, info_fg);
    let nested_idx = detail_rendered[..detail_rendered.find("Nested Loop").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(detail_buf[(nested_idx, 0)].fg, header_fg);

    // Test render_name_prompt styling
    let prompt = NamePrompt::for_query("saved_query_1");
    let prompt_area = Rect::new(0, 0, 80, 10);
    let mut prompt_buf = Buffer::empty(prompt_area);
    render_name_prompt(&prompt, &pres, prompt_area, &mut prompt_buf);
    let name_y = (0..10)
        .find(|&row| {
            (0..80)
                .map(|col| prompt_buf[(col, row)].symbol())
                .collect::<String>()
                .contains("Name:")
        })
        .unwrap();
    let name_line = (0..80)
        .map(|col| prompt_buf[(col, name_y)].symbol())
        .collect::<String>();
    let name_idx = name_line[..name_line.find("Name:").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(prompt_buf[(name_idx, name_y)].fg, header_fg);

    // Test render_update_value_prompt styling
    let update_prompt = test_update_prompt();
    let mut update_buf = Buffer::empty(prompt_area);
    render_update_value_prompt(&update_prompt, &pres, prompt_area, &mut update_buf);
    let target_y = (0..10)
        .find(|&row| {
            (0..80)
                .map(|col| update_buf[(col, row)].symbol())
                .collect::<String>()
                .contains("Target:")
        })
        .unwrap();
    let target_line = (0..80)
        .map(|col| update_buf[(col, target_y)].symbol())
        .collect::<String>();
    let target_idx = target_line[..target_line.find("Target:").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(update_buf[(target_idx, target_y)].fg, header_fg);
    let rel_idx = target_line[..target_line.find("public.orders.note").unwrap()]
        .chars()
        .count() as u16;
    let focus_fg = pres.theme.color(Token::Focus).unwrap();
    assert_eq!(update_buf[(rel_idx, target_y)].fg, focus_fg);

    let repl_y = (0..10)
        .find(|&row| {
            (0..80)
                .map(|col| update_buf[(col, row)].symbol())
                .collect::<String>()
                .contains("Replacement:")
        })
        .unwrap();
    let repl_line = (0..80)
        .map(|col| update_buf[(col, repl_y)].symbol())
        .collect::<String>();
    let repl_idx = repl_line[..repl_line.find("Replacement:").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(update_buf[(repl_idx, repl_y)].fg, header_fg);
}

fn col_after(line: &str, needle: &str) -> u16 {
    let byte = line.find(needle).unwrap() + needle.len();
    line[..byte].chars().count() as u16
}

#[test]
fn lint_findings_overlay_renders_counts_rules_and_keys() {
    let state = crate::app::model::LintFindingsState::new(
        crate::lint::lint_source(None, "DELETE FROM orders;\nSELECT id FROM a, b;").findings,
    );
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    let pres = presentation(ColorDepth::TrueColor);
    render_lint_findings(&state, &pres, area, &mut buf);
    let rendered = buffer_to_string(&buf);

    assert!(rendered.contains("SQL LINT - OFFLINE"), "{rendered}");
    assert!(rendered.contains(&pres.icon(Icon::Warning)), "{rendered}");
    assert!(rendered.contains("[FINDINGS: 2]"), "{rendered}");
    assert!(rendered.contains("[ERRORS: 1]"), "{rendered}");
    assert!(rendered.contains("[WARNINGS: 1]"), "{rendered}");
    assert!(rendered.contains("delete-without-where"), "{rendered}");
    assert!(
        rendered.contains("Enter Jump \u{00b7} r Re-lint \u{00b7} Up/Down Move \u{00b7} Esc Close"),
        "{rendered}"
    );

    // Capsule value colors: total = Focus, errors > 0 = Danger, warnings > 0 = Warning.
    let capsule_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("[FINDINGS: 2]")
        })
        .unwrap();
    let capsule_line: String = (0..area.width)
        .map(|x| buf[(x, capsule_y)].symbol())
        .collect();
    let find_val_x = col_after(&capsule_line, "[FINDINGS: ");
    let err_val_x = col_after(&capsule_line, "[ERRORS: ");
    let warn_val_x = col_after(&capsule_line, "[WARNINGS: ");
    assert_eq!(
        buf[(find_val_x, capsule_y)].fg,
        pres.theme.color(Token::Focus).unwrap()
    );
    assert_eq!(
        buf[(err_val_x, capsule_y)].fg,
        pres.theme.color(Token::Danger).unwrap()
    );
    assert_eq!(
        buf[(warn_val_x, capsule_y)].fg,
        pres.theme.color(Token::Warning).unwrap()
    );

    // Selected row carries full-width Token::Selection background across the modal body.
    let sel_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("> error")
        })
        .unwrap();
    let sel_bg = pres.theme.style(Token::Selection).bg.unwrap();
    let modal_width = (area.width.saturating_mul(84) / 100)
        .max(60)
        .min(area.width);
    let modal_left = (area.width - modal_width) / 2;
    let body_left = modal_left + 1;
    let body_right = modal_left + modal_width - 1;
    for x in body_left..body_right {
        assert_eq!(buf[(x, sel_y)].bg, sel_bg, "x={x} must carry selection bg");
    }

    // Basic16 uses REVERSED instead of a filled background.
    let basic_pres = presentation(ColorDepth::Basic16);
    let mut basic_buf = Buffer::empty(area);
    render_lint_findings(&state, &basic_pres, area, &mut basic_buf);
    for x in body_left..body_right {
        assert!(
            basic_buf[(x, sel_y)].modifier.contains(Modifier::REVERSED),
            "x={x} in Basic16 must use REVERSED selection"
        );
    }
}

#[test]
fn lint_findings_overlay_says_when_the_buffer_is_clean() {
    let state = crate::app::model::LintFindingsState::new(Vec::new());
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    let pres = presentation(ColorDepth::TrueColor);
    render_lint_findings(&state, &pres, area, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("[FINDINGS: 0]"), "{rendered}");
    assert!(rendered.contains("[ERRORS: 0]"), "{rendered}");
    assert!(rendered.contains("[WARNINGS: 0]"), "{rendered}");
    assert!(rendered.contains("The buffer is clean."), "{rendered}");

    let capsule_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("[ERRORS: 0]")
        })
        .unwrap();
    let capsule_line: String = (0..area.width)
        .map(|x| buf[(x, capsule_y)].symbol())
        .collect();
    let err_val_x = col_after(&capsule_line, "[ERRORS: ");
    let warn_val_x = col_after(&capsule_line, "[WARNINGS: ");
    let success_fg = pres.theme.color(Token::Success).unwrap();
    assert_eq!(buf[(err_val_x, capsule_y)].fg, success_fg);
    assert_eq!(buf[(warn_val_x, capsule_y)].fg, success_fg);
}

#[test]
fn lint_findings_overlay_truncates_long_messages_without_pushing_selected_row_out() {
    let findings = (1..=6)
        .map(|i| crate::lint::Finding {
            file: None,
            code: format!("rule-{i}"),
            severity: crate::lint::LintSeverity::Warning,
            statement_index: i - 1,
            line: i,
            column: 1,
            message: format!("Finding {i} with a very long diagnostic message that would wrap across multiple terminal lines if wrapping were enabled"),
            excerpt: "SELECT * FROM very_long_table_name_for_overflow_check".to_owned(),
        })
        .collect();
    let mut state = crate::app::model::LintFindingsState::new(findings);
    state.selected = 5;

    let area = Rect::new(0, 0, 72, 12);
    let mut buf = Buffer::empty(area);
    render_lint_findings(&state, &presentation(ColorDepth::TrueColor), area, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("> warning"), "{rendered}");
    assert!(rendered.contains("rule-6"), "{rendered}");
}

#[test]
fn lint_findings_overlay_uses_ascii_words() {
    let state = crate::app::model::LintFindingsState::new(
        crate::lint::lint_source(None, "DROP TABLE orders;").findings,
    );
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    render_lint_findings(&state, &presentation(ColorDepth::TrueColor), area, &mut buf);
    let rendered = buffer_to_string(&buf);
    assert!(rendered.contains("drop-object"));
    assert!(rendered.contains("DROP removes an object permanently"));
}

#[test]
fn snippet_library_renders_ascii_and_unicode_dividers_and_full_width_selection() {
    let state = crate::app::model::SnippetLibraryState::new();
    let area = Rect::new(0, 0, 100, 30);

    // Unicode mode: uses box-drawing divider sized to preview width, Unicode line gutter, and Selection bar.
    let unicode_pres = presentation(ColorDepth::TrueColor);
    let mut unicode_buf = Buffer::empty(area);
    render_snippet_library(&state, &unicode_pres, area, &mut unicode_buf);
    let unicode_text = buffer_to_string(&unicode_buf);
    assert!(unicode_text.contains("SQL SNIPPET & SCRATCHPAD LIBRARY"));
    assert!(unicode_text.contains("\u{2500}\u{2500}\u{2500}"));
    assert!(unicode_text.contains("  1 \u{2502} "));

    let sel_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| unicode_buf[(x, y)].symbol())
                .collect::<String>()
                .contains("> [")
        })
        .unwrap();
    let sel_row: String = (0..area.width)
        .map(|x| unicode_buf[(x, sel_y)].symbol())
        .collect();
    let list_left = sel_row[..sel_row.find("> [").unwrap()].chars().count() as u16;
    let sep_byte = sel_row.find('\u{2502}').unwrap();
    let sep_x = sel_row[..sep_byte].chars().count() as u16;
    let sel_bg = unicode_pres.theme.style(Token::Selection).bg.unwrap();
    for x in list_left..sep_x {
        assert_eq!(
            unicode_buf[(x, sel_y)].bg,
            sel_bg,
            "x={x} in snippet list must carry full-width selection background"
        );
    }

    // ASCII mode: uses '-' divider and '|' line gutter without box-drawing characters.
    let ascii_pres = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut ascii_buf = Buffer::empty(area);
    render_snippet_library(&state, &ascii_pres, area, &mut ascii_buf);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(ascii_text.contains("---"), "{ascii_text}");
    assert!(ascii_text.contains("  1 | "), "{ascii_text}");
    assert!(!ascii_text.contains('\u{2500}'), "{ascii_text}");
    assert!(!ascii_text.contains('\u{2502}'), "{ascii_text}");
}

#[test]
fn data_browser_overlay_renders_capsules_null_empty_and_ascii_cleanly() {
    let mut state = crate::app::model::DataBrowserState::new("public", "orders");
    state.loading = false;
    state.page = Some(crate::crud::BrowsePage {
        schema: "public".to_owned(),
        table: "orders".to_owned(),
        columns: vec!["id".to_owned(), "note".to_owned(), "tag".to_owned()],
        pk_columns: vec!["id".to_owned()],
        rows: vec![
            vec![Some("1".to_owned()), None, Some(String::new())],
            vec![
                Some("2".to_owned()),
                Some("paid".to_owned()),
                Some("vip".to_owned()),
            ],
        ],
        page_size: 50,
        has_more: true,
        has_prev: false,
        next_cursor: None,
        prev_cursor: None,
    });
    state.selected_col = 1;

    let area = Rect::new(0, 0, 120, 30);
    let ascii_pres = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut buf = Buffer::empty(area);
    render_data_browser(&state, &ascii_pres, area, &mut buf);
    let text = buffer_to_string(&buf);
    assert!(text.is_ascii(), "data browser emitted non-ASCII:\n{text}");
    for expected in [
        "TABLE: public.orders",
        "PK: id",
        "ROWS: 2",
        "SORT: id ASC",
        "[MORE]",
        "[note]",
        "[null]",
        "\"\"",
        "Next/Prev Keyset Page",
    ] {
        assert!(text.contains(expected), "{expected:?} missing:\n{text}");
    }
}

#[test]
fn plan_diff_overlay_renders_verdict_tabs_and_ascii_cleanly() {
    let baseline = crate::plandiff::parse_extended_explain_json(
        "Baseline (A): SELECT * FROM orders",
        PLAIN_PLAN,
    )
    .expect("baseline plan parses");
    let candidate = crate::plandiff::parse_extended_explain_json(
        "Candidate (B): SELECT * FROM orders WHERE id = 1",
        PLAIN_PLAN,
    )
    .expect("candidate plan parses");

    let mut state = crate::app::model::PlanDiffState {
        baseline: Some(baseline),
        candidate: Some(candidate),
        ..Default::default()
    };
    state.recompute_diff();

    let area = Rect::new(0, 0, 130, 32);
    let ascii_pres = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut buf = Buffer::empty(area);
    render_plan_diff(&state, &ascii_pres, area, &mut buf);
    let text = buffer_to_string(&buf);
    assert!(text.is_ascii(), "plan diff emitted non-ASCII:\n{text}");
    for expected in [
        "[UNCHANGED]",
        "[1] Summary & Diff",
        "[2] Nodes",
        "[3] Buffers / WAL / Settings",
        "[MODE: ESTIMATE]",
        "Toggle Analyze+Buffers+WAL",
    ] {
        assert!(text.contains(expected), "{expected:?} missing:\n{text}");
    }
}

#[test]
fn result_analytics_overlay_switches_sparkline_glyph_tier_and_tabs() {
    use crate::query::result::ResultSet;
    use crate::query::value::Cell;

    let mut set = ResultSet::with_column_types(
        vec!["id".to_owned(), "amount".to_owned()],
        vec![Some("int4".to_owned()), Some("numeric".to_owned())],
        100,
    );
    set.push(vec![
        Cell::Text("1".to_owned()),
        Cell::Text("10".to_owned()),
    ]);
    set.push(vec![Cell::Text("2".to_owned()), Cell::Null]);
    set.push(vec![
        Cell::Text("3".to_owned()),
        Cell::Text("30".to_owned()),
    ]);
    let mut state = crate::app::model::ResultAnalyticsState::from_result(&set);
    let area = Rect::new(0, 0, 130, 32);

    let ascii_pres = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut ascii_buf = Buffer::empty(area);
    render_result_analytics(&state, &ascii_pres, area, &mut ascii_buf);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(
        ascii_text.is_ascii(),
        "result analytics emitted non-ASCII:\n{ascii_text}"
    );
    assert!(ascii_text.contains("ROWS: 3/3"), "{ascii_text}");
    assert!(ascii_text.contains("COLUMNS: 2"), "{ascii_text}");
    assert!(ascii_text.contains("PROFILE: ZERO-QUERY"), "{ascii_text}");
    assert!(
        ascii_text.contains(&state.analytics.columns[0].sparkline_ascii),
        "{ascii_text}"
    );

    let unicode_pres = presentation(ColorDepth::TrueColor);
    let mut unicode_buf = Buffer::empty(area);
    render_result_analytics(&state, &unicode_pres, area, &mut unicode_buf);
    let unicode_text = buffer_to_string(&unicode_buf);
    assert!(
        unicode_text.contains(&state.analytics.columns[0].sparkline_unicode),
        "{unicode_text}"
    );

    state.set_tab(1);
    let mut detail_buf = Buffer::empty(area);
    render_result_analytics(&state, &ascii_pres, area, &mut detail_buf);
    let detail_text = buffer_to_string(&detail_buf);
    assert!(detail_text.is_ascii(), "{detail_text}");
    assert!(detail_text.contains("TOP FREQUENT VALUES"), "{detail_text}");
    assert!(detail_text.contains("HISTOGRAM BUCKETS"), "{detail_text}");
}

fn material_presentation(depth: ColorDepth) -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Material, depth != ColorDepth::None).with_depth(depth),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

fn ascii_presentation() -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    )
}

#[test]
fn query_bench_overlay_renders_ab_verdict_gate_sparklines_and_themes() {
    use crate::bench::{BenchConfig, BenchSample, compare_benchmarks, compute_bench_stats};

    let cfg = BenchConfig::new(2, 10, true);
    let base_samples = vec![
        BenchSample {
            iteration: 1,
            duration_ms: 4.0,
            rows_returned: 12,
        },
        BenchSample {
            iteration: 2,
            duration_ms: 6.0,
            rows_returned: 12,
        },
        BenchSample {
            iteration: 3,
            duration_ms: 8.0,
            rows_returned: 12,
        },
    ];
    let slower_samples = vec![
        BenchSample {
            iteration: 1,
            duration_ms: 10.0,
            rows_returned: 12,
        },
        BenchSample {
            iteration: 2,
            duration_ms: 14.0,
            rows_returned: 12,
        },
        BenchSample {
            iteration: 3,
            duration_ms: 18.0,
            rows_returned: 12,
        },
    ];
    let base = compute_bench_stats(
        "Baseline (A)",
        "SELECT * FROM orders WHERE id = 1",
        &cfg,
        &base_samples,
    );
    let slower = compute_bench_stats(
        "Candidate (B)",
        "SELECT * FROM orders ORDER BY id",
        &cfg,
        &slower_samples,
    );
    let slower_report = compare_benchmarks(base.clone(), Some(slower.clone()), Some(10.0));
    assert!(slower_report.failed_gate);

    let mut state = crate::app::model::QueryBenchState {
        config: cfg,
        baseline_sql: "SELECT * FROM orders WHERE id = 1".to_owned(),
        candidate_sql: "SELECT * FROM orders ORDER BY id".to_owned(),
        baseline: Some(base.clone()),
        candidate: Some(slower),
        report: Some(slower_report),
        active_tab: 0,
        selected_idx: 3,
        loading: false,
        cancelling: false,
        status_message: None,
        queued_candidate: None,
    };

    let area = Rect::new(0, 0, 126, 32);
    let ascii_pres = ascii_presentation();
    let mut ascii_buf = Buffer::empty(area);
    render_query_bench(&state, &ascii_pres, area, &mut ascii_buf);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(
        ascii_text.is_ascii(),
        "query bench ASCII emitted non-ASCII:\n{ascii_text}"
    );
    for expected in [
        "[BENCHMARK]",
        "[MODE: ROLLBACK]",
        "[WARMUP: 2]",
        "[RUNS: 10]",
        "[SLOWER]",
        "[GATE: FAILED]",
        "[1] Summary & A/B Compare",
        "[2] Latency Samples",
        "p50",
        "p95",
        "p99",
        "mean",
        "stddev",
        "qps",
        "sparkline",
        base.sparkline_ascii.as_str(),
        "1-2 Tabs | Up/Down Select | m Toggle rollback | r Re-run | Esc Close",
    ] {
        assert!(
            ascii_text.contains(expected),
            "{expected:?} missing:\n{ascii_text}"
        );
    }

    // Material TrueColor theme + LIVE mode + FASTER verdict + Unicode sparkline + Tab 2 samples.
    let faster_samples = vec![
        BenchSample {
            iteration: 1,
            duration_ms: 1.0,
            rows_returned: 12,
        },
        BenchSample {
            iteration: 2,
            duration_ms: 2.0,
            rows_returned: 12,
        },
        BenchSample {
            iteration: 3,
            duration_ms: 3.0,
            rows_returned: 12,
        },
    ];
    let live_cfg = cfg.with_rollback(false);
    let faster = compute_bench_stats("Candidate (B)", "SELECT 1", &live_cfg, &faster_samples);
    state.config = live_cfg;
    state.candidate = Some(faster.clone());
    state.report = Some(compare_benchmarks(base.clone(), Some(faster), None));
    state.set_tab(1);
    state.move_selection(1);

    for pres in [
        presentation(ColorDepth::TrueColor),
        material_presentation(ColorDepth::TrueColor),
    ] {
        let mut buf = Buffer::empty(area);
        render_query_bench(&state, &pres, area, &mut buf);
        let text = buffer_to_string(&buf);
        for expected in [
            "[BENCHMARK]",
            "[MODE: LIVE]",
            "[FASTER]",
            base.sparkline_unicode.as_str(),
            "#2",
        ] {
            assert!(text.contains(expected), "{expected:?} missing:\n{text}");
        }
        assert!(!text.contains("[GATE: FAILED]"), "{text}");

        let sel_bg = pres.theme.style(Token::Selection).bg.unwrap();
        let sel_y = (0..area.height)
            .find(|&y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .contains("> #2")
            })
            .expect("selected sample row present");
        let row_str: String = (0..area.width).map(|x| buf[(x, sel_y)].symbol()).collect();
        let left_x = row_str[..row_str.find("> #2").unwrap()].chars().count() as u16;
        let right_x = left_x + 40;
        for x in left_x..right_x {
            assert_eq!(
                buf[(x, sel_y)].bg,
                sel_bg,
                "x={x} must carry full-width Selection background"
            );
        }
    }
}

#[test]
fn schema_snapshot_overlay_renders_drift_pills_tables_and_themes() {
    use crate::snapshot::{
        ColumnSnapshot, SchemaSnapshot, TableSnapshot, ViewSnapshot, diff_snapshots,
    };

    let base_snap = SchemaSnapshot {
        tables: vec![TableSnapshot {
            schema: "public".to_owned(),
            name: "orders".to_owned(),
            columns: vec![
                ColumnSnapshot {
                    name: "id".to_owned(),
                    data_type: "bigint".to_owned(),
                    nullable: false,
                    default_expr: None,
                    ordinal: 1,
                },
                ColumnSnapshot {
                    name: "legacy_note".to_owned(),
                    data_type: "text".to_owned(),
                    nullable: true,
                    default_expr: None,
                    ordinal: 2,
                },
            ],
            constraints: Vec::new(),
            indexes: Vec::new(),
            rls_enabled: true,
            rls_forced: false,
            policies: Vec::new(),
        }],
        views: vec![ViewSnapshot {
            schema: "public".to_owned(),
            name: "active_orders".to_owned(),
            materialized: false,
            definition_sha256: "abc123".to_owned(),
        }],
        ..SchemaSnapshot::new(Some("public".to_owned()))
    }
    .finalize();

    let curr_snap = SchemaSnapshot {
        tables: vec![
            TableSnapshot {
                schema: "public".to_owned(),
                name: "orders".to_owned(),
                columns: vec![ColumnSnapshot {
                    name: "id".to_owned(),
                    data_type: "bigint".to_owned(),
                    nullable: false,
                    default_expr: None,
                    ordinal: 1,
                }],
                constraints: Vec::new(),
                indexes: Vec::new(),
                rls_enabled: true,
                rls_forced: true,
                policies: Vec::new(),
            },
            TableSnapshot {
                schema: "public".to_owned(),
                name: "audit_log".to_owned(),
                columns: vec![ColumnSnapshot {
                    name: "event_id".to_owned(),
                    data_type: "uuid".to_owned(),
                    nullable: false,
                    default_expr: None,
                    ordinal: 1,
                }],
                constraints: Vec::new(),
                indexes: Vec::new(),
                rls_enabled: false,
                rls_forced: false,
                policies: Vec::new(),
            },
        ],
        views: vec![ViewSnapshot {
            schema: "public".to_owned(),
            name: "active_orders".to_owned(),
            materialized: false,
            definition_sha256: "abc123".to_owned(),
        }],
        ..SchemaSnapshot::new(Some("public".to_owned()))
    }
    .finalize();

    let diff = diff_snapshots(&base_snap, &curr_snap);
    assert!(diff.breaking_count > 0);
    assert!(diff.info_count > 0);

    let sha12: String = curr_snap.fingerprint_sha256.chars().take(12).collect();
    let mut state = crate::app::model::SchemaSnapshotState {
        schema_filter: Some("public".to_owned()),
        baseline: Some(base_snap),
        current: Some(curr_snap),
        diff: Some(diff.clone()),
        active_tab: 0,
        selected_idx: 0,
        loading: false,
        status_message: None,
    };

    let area = Rect::new(0, 0, 126, 32);
    let ascii_pres = ascii_presentation();
    let mut ascii_buf = Buffer::empty(area);
    render_schema_snapshot(&state, &ascii_pres, area, &mut ascii_buf);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(
        ascii_text.is_ascii(),
        "schema snapshot ASCII emitted non-ASCII:\n{ascii_text}"
    );
    let expected_sha = format!("[SHA256: {sha12}]");
    let expected_breaking = format!("[BREAKING: {}]", diff.breaking_count);
    let expected_warn = format!("[WARN: {}]", diff.warning_count);
    let expected_info = format!("[INFO: {}]", diff.info_count);
    for expected in [
        "[SNAPSHOT v1]",
        expected_sha.as_str(),
        "[TABLES: 2]",
        "[VIEWS: 1]",
        expected_breaking.as_str(),
        expected_warn.as_str(),
        expected_info.as_str(),
        "[1] Overview & Drift",
        "[2] Catalog Tables",
        "public.orders",
        "1-2 Tabs | b Pin baseline | c Copy SHA-256 | Up/Down Select | r Refresh | Esc Close",
    ] {
        assert!(
            ascii_text.contains(expected),
            "{expected:?} missing:\n{ascii_text}"
        );
    }

    // At 112x32 showcase width, render_capsule_bar compacts outer padding so every capsule fits cleanly.
    let showcase_area = Rect::new(0, 0, 112, 32);
    let rich = Presentation::new(
        Theme::new(ThemeChoice::FieldIndustrial, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let mut showcase_buf = Buffer::empty(showcase_area);
    render_schema_snapshot(&state, &rich, showcase_area, &mut showcase_buf);
    let showcase_text = buffer_to_string(&showcase_buf);
    assert!(
        showcase_text.contains(expected_info.as_str()),
        "{expected_info:?} was clipped at 112x32:\n{showcase_text}"
    );

    // Switch to Tab 2 (Catalog Tables) and verify Material + Dark TrueColor full-width selection bar.
    state.set_tab(1);
    state.move_selection(1);
    for pres in [
        presentation(ColorDepth::TrueColor),
        material_presentation(ColorDepth::TrueColor),
    ] {
        let mut buf = Buffer::empty(area);
        render_schema_snapshot(&state, &pres, area, &mut buf);
        let text = buffer_to_string(&buf);
        assert!(text.contains("public.audit_log"), "{text}");
        assert!(text.contains("public.orders"), "{text}");

        let sel_bg = pres.theme.style(Token::Selection).bg.unwrap();
        let sel_y = (0..area.height)
            .find(|&y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .contains("> public.orders")
            })
            .expect("selected table row present");
        let row_str: String = (0..area.width).map(|x| buf[(x, sel_y)].symbol()).collect();
        let left_x = row_str[..row_str.find("> public.orders").unwrap()]
            .chars()
            .count() as u16;
        for x in left_x..(left_x + 50) {
            assert_eq!(
                buf[(x, sel_y)].bg,
                sel_bg,
                "x={x} must carry full-width Selection background"
            );
        }
    }
}

#[test]
fn sql_runbook_overlay_renders_badges_write_guard_and_dry_run_across_themes() {
    let md = r#"# Step 1: Health Check
```sql
-- @name: check-orders
-- @expect-rows: 5
SELECT count(*) FROM public.orders;
```

# Step 2: Update Note
```sql
-- @name: patch-note
UPDATE public.orders SET note = 'ok' WHERE id = 1;
```

# Step 3: Cleanup
```sql
-- @name: drop-staging
DROP TABLE IF EXISTS public.staging_tmp;
```
"#;
    let runbook = crate::runbook::parse_markdown_runbook(md, "ops.md").expect("valid markdown");
    let mut state = crate::app::model::SqlRunbookState {
        runbook,
        active_tab: 0,
        selected_idx: 1,
        allow_writes: false,
        status_message: None,
    };

    let area = Rect::new(0, 0, 126, 32);
    let ascii_pres = ascii_presentation();
    let mut ascii_buf = Buffer::empty(area);
    render_sql_runbook(&state, &ascii_pres, area, &mut ascii_buf);
    let ascii_text = buffer_to_string(&ascii_buf);
    assert!(
        ascii_text.is_ascii(),
        "sql runbook ASCII emitted non-ASCII:\n{ascii_text}"
    );
    for expected in [
        "[RUNBOOK]",
        "[CELLS: 3]",
        "[WRITES: BLOCKED]",
        "[WRITE-GUARD: 2]",
        "[1] Notebook Cells",
        "[2] Safety Dry-Run Plan",
        "check-orders",
        "Step 1: Health Check",
        "READ",
        "WRITE",
        "DESTRUCTIVE",
        "@expect-rows:5",
        "SELECT count(*) FROM public.orders;",
        "1-2 Tabs | w Allow writes | Up/Down Select cell | Enter Jump to cell | Esc Close",
    ] {
        assert!(
            ascii_text.contains(expected),
            "{expected:?} missing:\n{ascii_text}"
        );
    }

    // Tab 2 (Safety Dry-Run Plan) blocked vs allowed, Dark & Material themes.
    state.set_tab(1);
    for pres in [
        presentation(ColorDepth::TrueColor),
        material_presentation(ColorDepth::TrueColor),
    ] {
        let mut blocked_buf = Buffer::empty(area);
        render_sql_runbook(&state, &pres, area, &mut blocked_buf);
        let blocked_text = buffer_to_string(&blocked_buf);
        assert!(
            blocked_text.contains("blocked-by-write-guard"),
            "{blocked_text}"
        );
        assert!(blocked_text.contains("planned=1"), "{blocked_text}");
        assert!(blocked_text.contains("blocked=2"), "{blocked_text}");

        let mut allowed_state = state.clone();
        allowed_state.allow_writes = true;
        let mut allowed_buf = Buffer::empty(area);
        render_sql_runbook(&allowed_state, &pres, area, &mut allowed_buf);
        let allowed_text = buffer_to_string(&allowed_buf);
        assert!(allowed_text.contains("[WRITES: ALLOWED]"), "{allowed_text}");
        assert!(!allowed_text.contains("[WRITE-GUARD:"), "{allowed_text}");
        assert!(allowed_text.contains("planned=3"), "{allowed_text}");
    }
}

#[test]
fn new_overlays_block_panes_and_render_through_layout() {
    use crate::ui::layout::{render, render_to_string};
    use crate::ui::widgets::{overlay_blocks_panes, region_at};

    let area = Rect::new(0, 0, 120, 30);
    let mut model = crate::app::Model::default();
    model.sql_runbook = Some(crate::app::model::SqlRunbookState::from_buffer(
        "-- @name: first\nSELECT 1;",
    ));
    assert!(overlay_blocks_panes(&model));
    assert_eq!(region_at(&model, area, 10, 10), None);

    let keymap = crate::ui::keymap::Keymap::new();
    let pres = material_presentation(ColorDepth::TrueColor);
    let text = render_to_string(&model, &keymap, &pres, area.width, area.height);
    assert!(text.contains("[RUNBOOK]"), "{text}");

    let mut buf = Buffer::empty(area);
    render(&model, &keymap, area, &mut buf);
    assert!(buffer_to_string(&buf).contains("[RUNBOOK]"));

    model.sql_runbook = None;
    model.schema_snapshot = Some(crate::app::model::SchemaSnapshotState::default());
    assert!(overlay_blocks_panes(&model));
    assert_eq!(region_at(&model, area, 10, 10), None);
    let snap_text = render_to_string(&model, &keymap, &pres, area.width, area.height);
    assert!(snap_text.contains("[SNAPSHOT v1]"), "{snap_text}");

    model.schema_snapshot = None;
    model.query_bench = Some(crate::app::model::QueryBenchState::default());
    assert!(overlay_blocks_panes(&model));
    assert_eq!(region_at(&model, area, 10, 10), None);
    let bench_text = render_to_string(&model, &keymap, &pres, area.width, area.height);
    assert!(bench_text.contains("[BENCHMARK]"), "{bench_text}");
}

#[test]
fn overlay_chrome_recessed_pills_hairline_rule_and_footer_keycaps_across_depths() {
    use crate::ui::theme::Token;
    use ratatui::style::Modifier;

    let state = crate::app::model::LintFindingsState::new(
        crate::lint::lint_source(None, "DELETE FROM orders;\nSELECT id FROM a, b;").findings,
    );
    let area = Rect::new(0, 0, 100, 30);
    let pres = presentation(ColorDepth::TrueColor);
    let mut buf = Buffer::empty(area);
    render_lint_findings(&state, &pres, area, &mut buf);
    let text = buffer_to_string(&buf);

    let overlay_bg = pres.theme.surface(Token::SurfaceOverlay).bg.unwrap();
    let recessed_bg = pres.theme.surface(Token::SurfaceAlt).bg.unwrap();
    let focus_fg = pres.theme.color(Token::Focus).unwrap();

    // Recessed badge "[FINDINGS: 2]" uses SurfaceAlt bg, non-DIM label, and 1-cell overlay gap before "[ERRORS: 1]".
    let findings_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("[FINDINGS: 2]")
        })
        .expect("findings capsule row");
    let line: String = (0..area.width)
        .map(|x| buf[(x, findings_y)].symbol())
        .collect();
    let findings_col = line[..line.find("[FINDINGS: 2]").unwrap()].chars().count() as u16;
    assert_eq!(buf[(findings_col, findings_y)].bg, recessed_bg);
    assert!(
        !buf[(findings_col, findings_y)]
            .modifier
            .contains(Modifier::DIM),
        "recessed capsule label must never use DIM"
    );
    let after_findings = col_after(&line, "[FINDINGS: 2] ");
    assert_eq!(
        buf[(after_findings, findings_y)].bg,
        overlay_bg,
        "1-cell gap between adjacent pills must keep SurfaceOverlay bg"
    );

    // Hairline rule row sits cleanly in the modal interior using Unicode horizontal bar.
    assert!(text.contains("\u{2500}\u{2500}\u{2500}"), "{text}");

    // Footer hint "Enter" uses Theme::recessed_pill(Token::Focus), with no
    // brackets inside the keycap.
    let footer_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains("Enter Jump")
        })
        .expect("footer hints row");
    let footer_line: String = (0..area.width)
        .map(|x| buf[(x, footer_y)].symbol())
        .collect();
    assert!(!footer_line.contains('['), "{footer_line}");
    let enter_col = footer_line[..footer_line.find("Enter").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(buf[(enter_col, footer_y)].bg, recessed_bg);
    assert_eq!(buf[(enter_col, footer_y)].fg, focus_fg);
    assert!(buf[(enter_col, footer_y)].modifier.contains(Modifier::BOLD));

    // Basic16 & ColorDepth::None / ASCII stay unpainted and strictly ASCII.
    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut low_buf = Buffer::empty(area);
        render_lint_findings(&state, &low, area, &mut low_buf);
        let low_text = buffer_to_string(&low_buf);
        assert!(low_text.is_ascii(), "{depth:?}: {low_text:?}");
        assert!(low_text.contains("---"), "{low_text}");
        assert_eq!(low_buf[(findings_col, findings_y)].bg, Color::Reset);
        assert_eq!(low_buf[(enter_col, footer_y)].bg, Color::Reset);
        // Without a keycap fill the separator still pairs each key with its
        // action.
        assert!(
            low_text.contains("Enter Jump | r Re-lint | Up/Down Move | Esc Close"),
            "{depth:?}: {low_text}"
        );
    }
}

#[test]
fn plan_header_detail_strip_and_review_modal_craft_across_depths() {
    use crate::app::model::{Model, PendingUpdate};
    use crate::query::JobId;
    use crate::query::value::Cell;

    let document = PlanDocument::parse(PLAIN_PLAN, true).expect("valid plan");
    let mut model = Model::new(100);
    model.focus = crate::app::model::Focus::Results;
    model.plan.ready(document);

    let area = Rect::new(0, 0, 100, 16);
    let nerd = Presentation::new(
        Theme::new(ThemeChoice::Atelier, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    let mut pbuf = Buffer::empty(area);
    render_plan(
        &model,
        &crate::ui::keymap::Keymap::new(),
        &nerd,
        area,
        &mut pbuf,
    );
    let ptext = buffer_to_string(&pbuf);
    assert!(
        ptext.contains("\u{258d} "),
        "focused Plan needs accent leader: {ptext}"
    );
    assert!(ptext.contains("[focused]"), "{ptext}");
    let sel_y = (0..area.height)
        .find(|&y| {
            (0..area.width)
                .map(|x| pbuf[(x, y)].symbol())
                .collect::<String>()
                .contains("Selected: Nested Loop")
        })
        .expect("plan detail strip");
    assert_eq!(
        pbuf[(2, sel_y)].bg,
        nerd.theme.surface(Token::SurfaceAlt).bg.unwrap(),
        "plan Selected: strip must use SurfaceAlt"
    );

    let mut review_model = Model::new(100);
    let source_sql = "SELECT id, status FROM public.orders ORDER BY id ASC;";
    let candidate = UpdateCandidate {
        result_job: JobId(1),
        source_row: 0,
        result_column: 1,
        source_sql: source_sql.into(),
    };
    let source = crate::query::parse_update_source(source_sql, &["id".into(), "status".into()], 1)
        .expect("update source");
    let relation = UpdateRelation {
        schema: "public".into(),
        relation: "orders".into(),
        kind: ObjectKind::Table,
        readable: true,
        writable: true,
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                primary_key: true,
                default: None,
            },
            ColumnInfo {
                name: "status".into(),
                data_type: "text".into(),
                nullable: false,
                primary_key: false,
                default: None,
            },
        ],
    };
    let plan = crate::query::plan_update(
        &source,
        "public",
        "orders",
        &["id".into(), "status".into()],
        &[Cell::Text("1".into()), Cell::Text("paid".into())],
        &[
            crate::query::UpdateColumn {
                name: "id".into(),
                primary_key: true,
            },
            crate::query::UpdateColumn {
                name: "status".into(),
                primary_key: false,
            },
        ],
        "delivered",
    )
    .expect("update plan");
    review_model.pending_update = Some(PendingUpdate {
        relation,
        candidate,
        plan,
    });
    let r_area = Rect::new(0, 0, 110, 26);
    let mut rbuf = Buffer::empty(r_area);
    render_update_review(&review_model, &nerd, r_area, &mut rbuf);
    let rtext = buffer_to_string(&rbuf);
    let warn_icon = nerd.icon(crate::ui::glyphs::Icon::Warning);
    assert!(
        rtext.contains(&format!("{warn_icon}Confirm cell update")),
        "update review title should show warning icon: {rtext}"
    );
    let find_word = |needle: &str| -> (u16, u16) {
        for y in 0..r_area.height {
            let row: String = (0..r_area.width).map(|x| rbuf[(x, y)].symbol()).collect();
            if let Some(pos) = row.find(needle) {
                return (row[..pos].chars().count() as u16, y);
            }
        }
        panic!("missing {needle:?} in:\n{rtext}");
    };
    let update_pos = find_word("UPDATE \"public\"");
    assert_eq!(
        rbuf[update_pos].fg,
        nerd.theme.color(Token::SyntaxKeyword).unwrap(),
        "UPDATE keyword in review modal must be syntax-highlighted"
    );
    assert_eq!(
        rbuf[update_pos].bg,
        nerd.theme.surface(Token::SurfaceAlt).bg.unwrap(),
        "bound SQL statement row must sit on SurfaceAlt"
    );
    let rail_pos = find_word("Enter/Ctrl+R to run this UPDATE once");
    assert_eq!(
        rbuf[rail_pos].bg,
        nerd.theme.surface(Token::SurfaceAlt).bg.unwrap(),
        "review modal action rail must sit on SurfaceAlt"
    );
    for disclosure in [
        "Primary-key identity: id",
        "The replacement is literal text data",
        "The prior SELECT is a snapshot",
    ] {
        let start = find_word(disclosure);
        let expected_fg = nerd.theme.color(Token::Text).unwrap();
        for offset in 0..u16::try_from(disclosure.chars().count()).unwrap() {
            let cell = &rbuf[(start.0 + offset, start.1)];
            assert_eq!(cell.fg, expected_fg, "{disclosure} foreground");
            assert!(
                !cell.modifier.contains(ratatui::style::Modifier::DIM),
                "{disclosure} must use undimmed body text"
            );
        }
    }

    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let low = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut lbuf = Buffer::empty(r_area);
        render_update_review(&review_model, &low, r_area, &mut lbuf);
        let ltext = buffer_to_string(&lbuf);
        assert!(ltext.is_ascii(), "{depth:?}: {ltext:?}");
        assert_eq!(lbuf[update_pos].bg, Color::Reset);
        assert_eq!(lbuf[rail_pos].bg, Color::Reset);
    }
}

/// The first cell of `needle` in a rendered buffer.
fn cell_at<'a>(buf: &'a Buffer, needle: &str) -> &'a ratatui::buffer::Cell {
    let area = buf.area;
    for y in area.top()..area.bottom() {
        let line: String = (area.left()..area.right())
            .map(|x| buf[(x, y)].symbol())
            .collect();
        if let Some(byte) = line.find(needle) {
            let x = line[..byte].chars().count() as u16;
            return &buf[(area.x + x, y)];
        }
    }
    panic!("{needle:?} missing:\n{}", buffer_to_string(buf));
}

#[test]
fn required_review_disclosures_use_undimmed_body_text() {
    use crate::ui::review::Kind;

    let area = Rect::new(0, 0, 120, 30);
    let pres = presentation(ColorDepth::TrueColor);
    let expected_fg = pres.theme.color(Token::Text).unwrap();

    let mut copy = Model::new(100);
    copy.pending_copy = Some(PendingCopy {
        result_job: crate::query::result::JobId(1),
        source_row: 0,
        column: 0,
        bytes: 5,
        characters: 5,
    });
    let mut analyze = Model::new(100);
    analyze.pending_plan = Some(PendingPlan {
        sql: "SELECT 1".into(),
        impact: crate::query::Impact::Read,
        typed: String::new(),
        required: String::new(),
    });
    let mut production = Model::new(100);
    production.pending_run = Some(PendingRun {
        sql: "UPDATE orders SET status = 'paid'".into(),
        impact: crate::query::Impact::Write,
        typed: String::new(),
        required: "ignatius_demo".into(),
        source: None,
    });
    let mut parameter = Model::new(100);
    parameter.parameter_prompt = Some(ParameterPrompt::new(
        "SELECT :note".into(),
        None,
        vec!["note".into()],
    ));
    let reviews: [(Kind, Model, &[&str]); 4] = [
        (
            Kind::Copy,
            copy,
            &[
                "Path: this terminal",
                "Ignatius writes only",
                "Terminal acceptance is",
            ],
        ),
        (
            Kind::Analyze,
            analyze,
            &["Target: this database", "Classification is advisory"],
        ),
        (
            Kind::Production,
            production,
            &["This reads leading keywords"],
        ),
        (
            Kind::Parameter,
            parameter,
            &["The answer is literal text", "The SQL template stays"],
        ),
    ];
    let mut dimmed_disclosures = Vec::new();
    for (kind, model, disclosures) in reviews {
        let mut buf = Buffer::empty(area);
        render_sending_review(&model, kind, &pres, area, &mut buf);
        for disclosure in disclosures {
            let cell = cell_at(&buf, disclosure);
            if cell.fg != expected_fg || cell.modifier.contains(Modifier::DIM) {
                dimmed_disclosures.push(disclosure);
            }
        }
    }
    assert!(
        dimmed_disclosures.is_empty(),
        "required disclosures need undimmed body text: {dimmed_disclosures:?}"
    );
}

fn bench_state_with(rollback: bool, gate: Option<f64>) -> crate::app::model::QueryBenchState {
    use crate::bench::{BenchConfig, BenchSample, compare_benchmarks, compute_bench_stats};
    let config = BenchConfig::new(2, 10, rollback);
    let samples = |base: f64| {
        (1..=3)
            .map(|iteration| BenchSample {
                iteration,
                duration_ms: base * iteration as f64,
                rows_returned: 1,
            })
            .collect::<Vec<_>>()
    };
    let base = compute_bench_stats("Baseline (A)", "SELECT 1", &config, &samples(2.0));
    let slower = compute_bench_stats("Candidate (B)", "SELECT 2", &config, &samples(6.0));
    crate::app::model::QueryBenchState {
        config,
        baseline_sql: "SELECT 1".to_owned(),
        candidate_sql: "SELECT 2".to_owned(),
        report: Some(compare_benchmarks(base.clone(), Some(slower.clone()), gate)),
        baseline: Some(base),
        candidate: Some(slower),
        ..Default::default()
    }
}

#[test]
fn modal_metadata_rests_on_recessed_pills_and_only_safety_states_take_capsules() {
    use crate::ui::theme::Token;
    let area = Rect::new(0, 0, 126, 32);
    let pres = presentation(ColorDepth::TrueColor);
    let theme = &pres.theme;
    let recessed = theme.surface(Token::SurfaceAlt).bg.unwrap();

    // Rollback on, no gate: identity, mode and verdict are all metadata.
    let mut buf = Buffer::empty(area);
    render_query_bench(&bench_state_with(true, None), &pres, area, &mut buf);
    for badge in ["[BENCHMARK]", "[MODE: ", "[SLOWER]"] {
        assert_eq!(cell_at(&buf, badge).bg, recessed, "{badge} is recessed");
    }
    assert_eq!(
        cell_at(&buf, "[SLOWER]").fg,
        theme.color(Token::Danger).unwrap(),
        "the outcome keeps its meaning in the ink"
    );

    // Live mode and a failed gate are safety states.
    let mut buf = Buffer::empty(area);
    render_query_bench(&bench_state_with(false, Some(10.0)), &pres, area, &mut buf);
    let live = theme.capsule(Token::Warning);
    let gate = theme.capsule(Token::Danger);
    assert_eq!(Some(cell_at(&buf, "[MODE: LIVE]").bg), live.bg);
    assert_eq!(Some(cell_at(&buf, "[GATE: FAILED]").bg), gate.bg);
    assert_eq!(cell_at(&buf, "[BENCHMARK]").bg, recessed);

    // The active tab is a recessed Focus pill, never a saturated capsule.
    let tab = cell_at(&buf, "[1] Summary");
    assert_eq!(tab.bg, recessed);
    assert_eq!(tab.fg, theme.color(Token::Focus).unwrap());

    // Writes allowed in a runbook is a safety state; blocked is metadata.
    let runbook = crate::runbook::parse_markdown_runbook(
        "# One\n```sql\nUPDATE t SET a = 1;\n```\n",
        "ops.md",
    )
    .expect("valid runbook");
    let mut state = crate::app::model::SqlRunbookState {
        runbook,
        active_tab: 0,
        selected_idx: 0,
        allow_writes: false,
        status_message: None,
    };
    let mut buf = Buffer::empty(area);
    render_sql_runbook(&state, &pres, area, &mut buf);
    assert_eq!(cell_at(&buf, "[WRITES: ").bg, recessed);
    assert_eq!(cell_at(&buf, "[RUNBOOK]").bg, recessed);
    state.allow_writes = true;
    let mut buf = Buffer::empty(area);
    render_sql_runbook(&state, &pres, area, &mut buf);
    assert_eq!(
        Some(cell_at(&buf, "[WRITES: ALLOWED]").bg),
        theme.capsule(Token::Warning).bg
    );
}

#[test]
fn zero_drift_counts_are_muted_and_breaking_drift_is_a_capsule() {
    use crate::snapshot::{ColumnSnapshot, SchemaSnapshot, TableSnapshot, diff_snapshots};
    use crate::ui::theme::Token;
    let table = |column: &str| TableSnapshot {
        schema: "public".into(),
        name: "orders".into(),
        columns: vec![ColumnSnapshot {
            name: column.into(),
            data_type: "bigint".into(),
            nullable: false,
            default_expr: None,
            ordinal: 1,
        }],
        constraints: vec![],
        indexes: vec![],
        rls_enabled: false,
        rls_forced: false,
        policies: vec![],
    };
    let snapshot = |column: &str| {
        let mut snap = SchemaSnapshot::new(None);
        snap.tables.push(table(column));
        snap.finalize()
    };
    let area = Rect::new(0, 0, 126, 32);
    let pres = presentation(ColorDepth::TrueColor);
    let theme = &pres.theme;
    let muted = theme.color(Token::Muted).unwrap();

    let same = snapshot("id");
    let mut state = crate::app::model::SchemaSnapshotState {
        diff: Some(diff_snapshots(&same, &same)),
        baseline: Some(same.clone()),
        current: Some(same),
        ..Default::default()
    };
    let mut buf = Buffer::empty(area);
    render_schema_snapshot(&state, &pres, area, &mut buf);
    for badge in ["[BREAKING: 0]", "[WARN: 0]", "[INFO: 0]"] {
        let cell = cell_at(&buf, badge);
        assert_eq!(cell.fg, muted, "{badge}");
        assert_eq!(
            cell.bg,
            theme.surface(Token::SurfaceAlt).bg.unwrap(),
            "{badge}"
        );
    }

    let renamed = snapshot("order_id");
    let diff = diff_snapshots(state.baseline.as_ref().unwrap(), &renamed);
    assert!(diff.breaking_count > 0);
    let label = format!("[BREAKING: {}]", diff.breaking_count);
    state.current = Some(renamed);
    state.diff = Some(diff);
    let mut buf = Buffer::empty(area);
    render_schema_snapshot(&state, &pres, area, &mut buf);
    assert_eq!(
        Some(cell_at(&buf, &label).bg),
        theme.capsule(Token::Danger).bg
    );
}

#[test]
fn the_active_tab_stays_visible_without_colour() {
    use ratatui::style::Modifier;
    let area = Rect::new(0, 0, 126, 32);
    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let pres = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut buf = Buffer::empty(area);
        render_query_bench(&bench_state_with(true, None), &pres, area, &mut buf);
        let active = cell_at(&buf, "[1] Summary");
        assert!(active.modifier.contains(Modifier::REVERSED), "{depth:?}");
        assert_eq!(active.bg, Color::Reset, "{depth:?}");
        let inactive = cell_at(&buf, "[2] Latency");
        assert!(!inactive.modifier.contains(Modifier::REVERSED), "{depth:?}");
    }
}

/// The first row whose text contains `needle`.
fn row_of(buf: &Buffer, needle: &str) -> u16 {
    let area = buf.area;
    (area.top()..area.bottom())
        .find(|&y| {
            (area.left()..area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .contains(needle)
        })
        .unwrap_or_else(|| panic!("{needle:?} missing:\n{}", buffer_to_string(buf)))
}

#[test]
fn modal_shell_fits_its_content_with_status_and_keys_pinned_to_the_bottom() {
    let area = Rect::new(0, 0, 126, 40);
    let pres = ascii_presentation();
    let tallest = area.height * 88 / 100;
    let mut state = bench_state_with(true, None);

    let mut buf = Buffer::empty(area);
    render_query_bench(&state, &pres, area, &mut buf);
    let title = row_of(&buf, "Query benchmark & A/B regression suite");
    let rail = row_of(&buf, "Esc Close");
    let last_metric = row_of(&buf, "sparkline");
    assert_eq!(
        last_metric + 2,
        rail,
        "the body ends right above the status line"
    );
    assert!(
        rail + 2 - title < tallest,
        "the box is fitted, not the old fixed 88%: {}",
        buffer_to_string(&buf)
    );
    // The bottom border closes the box right under the key rail.
    assert_eq!(buf[(area.width / 2, rail + 1)].symbol(), "=");

    // Switching tabs or re-running never moves the title or tab strip, and
    // a re-run holds the box at the size its retained results need.
    state.set_tab(1);
    let mut tab_buf = Buffer::empty(area);
    render_query_bench(&state, &pres, area, &mut tab_buf);
    assert_eq!(
        row_of(&tab_buf, "Query benchmark & A/B regression suite"),
        title
    );
    state.set_tab(0);
    state.loading = true;
    let mut loading_buf = Buffer::empty(area);
    render_query_bench(&state, &pres, area, &mut loading_buf);
    assert_eq!(row_of(&loading_buf, "Running query benchmark"), title + 4);
    assert_eq!(row_of(&loading_buf, "Up/Down Select"), rail);
    // During a run Esc, m and r wait, while Ctrl+C Cancel is advertised.
    let running = buffer_to_string(&loading_buf);
    assert!(running.contains("Ctrl+C Cancel"), "{running}");
    assert!(!running.contains("Esc Close"), "{running}");
    assert!(!running.contains("r Re-run"), "{running}");

    state.cancelling = true;
    state.status_message = Some(
        "Cancellation requested for the benchmark. The server may still be running.".to_owned(),
    );
    let mut cancelling_buf = Buffer::empty(area);
    render_query_bench(&state, &pres, area, &mut cancelling_buf);
    let cancelling_text = buffer_to_string(&cancelling_buf);
    assert!(
        cancelling_text.contains("Cancelling query benchmark"),
        "{cancelling_text}"
    );
    assert!(
        cancelling_text.contains("Cancellation requested for the benchmark"),
        "{cancelling_text}"
    );
}

#[test]
fn a_modal_with_more_content_than_fits_is_capped_at_the_tallest_box() {
    let cells = (1..=60)
        .map(|index| format!("# Step {index}\n```sql\nSELECT {index};\n```\n"))
        .collect::<String>();
    let state = crate::app::model::SqlRunbookState {
        runbook: crate::runbook::parse_markdown_runbook(&cells, "long.md").expect("valid runbook"),
        active_tab: 0,
        selected_idx: 0,
        allow_writes: false,
        status_message: None,
    };
    let area = Rect::new(0, 0, 126, 30);
    let mut buf = Buffer::empty(area);
    render_sql_runbook(&state, &ascii_presentation(), area, &mut buf);
    let title = row_of(&buf, "SQL runbook & write-guard planner");
    let rail = row_of(&buf, "Esc Close");
    assert_eq!(rail + 2 - title, area.height * 88 / 100);
}

#[test]
fn plan_rows_use_tree_chevrons_and_compact_observed_metrics_in_ascii() {
    let analyzed = r#"[{"Plan":{"Node Type":"Nested Loop","Plan Rows":4,"Actual Rows":40,"Actual Total Time":12.0,"Actual Loops":1,"Plans":[{"Node Type":"Seq Scan","Relation Name":"orders","Plan Rows":10,"Actual Rows":420,"Actual Total Time":8.5,"Actual Loops":2}]}}]"#;
    let document = PlanDocument::parse(analyzed, true).expect("valid plan");
    let mut view = PlanView::default();
    view.ready(document.clone());
    let area = Rect::new(0, 0, 140, 4);
    let ascii = ascii_presentation();
    let mut buf = Buffer::empty(area);
    render_plan_tree(&document, &view, &ascii, area, &mut buf);
    let text = buffer_to_string(&buf);
    assert!(text.is_ascii(), "{text}");
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with(">v Nested Loop | est 4 -> actual 40 rows, 12.00 ms"),
        "{text}"
    );
    assert!(
        lines[1].starts_with(
            "     Seq Scan orders | est 10 -> actual 420 rows, 8.50 ms/loop, 2 loops [attention:"
        ),
        "{text}"
    );
    assert!(lines[1].contains("[estimate mismatch]"), "{text}");

    // Collapsed, the node with children shows a closed chevron.
    view.collapsed.insert(Vec::new());
    let mut buf = Buffer::empty(area);
    render_plan_tree(&document, &view, &ascii, area, &mut buf);
    assert!(
        buffer_to_string(&buf).starts_with(">> Nested Loop"),
        "{}",
        buffer_to_string(&buf)
    );
}

#[test]
fn modal_fitting_falls_back_to_the_tallest_box_when_content_depends_on_height() {
    let area = Rect::new(0, 0, 100, 30);
    let pres = ascii_presentation();
    // A body whose first row reports its own height would read differently
    // in a shorter box, so the fitted box must not be used.
    let render = |fit: &ModalFit, target: &mut Buffer| {
        if let Some((_, _, body, _, _)) =
            modal_inner_layout(" Probe ".to_owned(), &pres, area, target, fit)
        {
            Paragraph::new(format!("{} rows fit", body.height)).render(body, target);
        }
    };
    let mut fitted = Buffer::empty(area);
    render_fitted_modal(area, &mut fitted, 0, render);
    let mut tallest = Buffer::empty(area);
    render(&ModalFit::tallest(), &mut tallest);
    assert_eq!(fitted, tallest, "{}", buffer_to_string(&fitted));

    // A body that does not depend on its height is fitted.
    let steady = |fit: &ModalFit, target: &mut Buffer| {
        if let Some((_, _, body, _, _)) =
            modal_inner_layout(" Probe ".to_owned(), &pres, area, target, fit)
        {
            Paragraph::new("one line").render(body, target);
        }
    };
    let mut fitted = Buffer::empty(area);
    render_fitted_modal(area, &mut fitted, 0, steady);
    let mut full = Buffer::empty(area);
    steady(&ModalFit::tallest(), &mut full);
    assert_ne!(fitted, full);
    assert!(buffer_to_string(&fitted).contains("one line"));
}

#[test]
fn plan_tree_proportional_share_micro_bar_highlights_self_time_bottleneck() {
    use crate::app::model::{Focus, Model};

    // Nested Loop: inclusive 20.0 ms, child Seq Scan orders: 14.0 ms (self = 70%),
    // child Index Scan customers: 2.0 ms (self = 10%). Root self time = 4.0 ms (20%).
    let json = r#"[
      {
        "Plan": {
          "Node Type": "Nested Loop",
          "Startup Cost": 0.42,
          "Total Cost": 24.0,
          "Plan Rows": 10,
          "Actual Startup Time": 0.10,
          "Actual Total Time": 20.0,
          "Actual Rows": 10,
          "Actual Loops": 1,
          "Plans": [
            {
              "Node Type": "Seq Scan",
              "Relation Name": "orders",
              "Startup Cost": 0.0,
              "Total Cost": 16.0,
              "Plan Rows": 100,
              "Actual Startup Time": 0.05,
              "Actual Total Time": 14.0,
              "Actual Rows": 100,
              "Actual Loops": 1
            },
            {
              "Node Type": "Index Scan",
              "Relation Name": "customers",
              "Index Name": "customers_pkey",
              "Startup Cost": 0.42,
              "Total Cost": 4.0,
              "Plan Rows": 1,
              "Actual Startup Time": 0.02,
              "Actual Total Time": 2.0,
              "Actual Rows": 1,
              "Actual Loops": 1
            }
          ]
        }
      }
    ]"#;
    let doc = PlanDocument::parse(json, true).expect("valid analyzed plan");
    assert_eq!(doc.root.attention_path(true), Some(vec![0]));
    assert!((doc.root.exclusive_time_ms().unwrap() - 4.0).abs() < 1e-6);

    let mut model = Model::new(100);
    model.focus = Focus::Results;
    model.plan.ready(doc);
    // Move selection away from child 0 so unselected child bottleneck bar keeps Warning ink.
    let keymap = crate::ui::keymap::Keymap::new();

    let uni = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let wide = Rect::new(0, 0, 96, 18);
    let mut buf = Buffer::empty(wide);
    render_plan(&model, &keymap, &uni, wide, &mut buf);
    let rendered = buffer_to_string(&buf);

    assert!(rendered.contains("self time 4.00 ms"), "{rendered}");
    assert!(
        rendered.contains("\u{2501}\u{2501}\u{2501}\u{2501}\u{2500}"),
        "70% bottleneck child should display 4/5 filled bar (━━━━─): {rendered}"
    );
    assert!(
        rendered.contains("\u{2501}\u{2500}\u{2500}\u{2500}\u{2500}"),
        "10% sibling should display 1/5 bar (━────): {rendered}"
    );

    let find_tree_row = |needle: &str| -> (u16, String) {
        (0..wide.height)
            .find_map(|y| {
                let line: String = (0..wide.width).map(|x| buf[(x, y)].symbol()).collect();
                (line.contains(needle) && line.contains(" | ") && !line.contains("Selected:"))
                    .then_some((y, line))
            })
            .unwrap_or_else(|| panic!("tree row {needle:?} missing:\n{rendered}"))
    };
    let (warn_y, warn_line) = find_tree_row("Seq Scan orders");
    let warn_pos = (
        warn_line
            .find("\u{2501}\u{2501}\u{2501}\u{2501}\u{2500}")
            .map(|b| warn_line[..b].chars().count() as u16)
            .unwrap_or_else(|| {
                panic!("warning bar missing on row:\n{warn_line}\nfull:\n{rendered}")
            }),
        warn_y,
    );
    assert_eq!(buf[warn_pos].fg, uni.theme.color(Token::Warning).unwrap());
    assert_eq!(
        buf[(warn_pos.0 + 4, warn_pos.1)].fg,
        uni.theme.color(Token::Border).unwrap()
    );

    let (info_y, info_line) = find_tree_row("customers_pkey");
    let info_pos = (
        info_line
            .find("\u{2501}\u{2500}\u{2500}\u{2500}\u{2500}")
            .map(|b| info_line[..b].chars().count() as u16)
            .expect("info bar on Index Scan row"),
        info_y,
    );
    assert_eq!(buf[info_pos].fg, uni.theme.color(Token::Info).unwrap());
    assert_eq!(
        buf[(info_pos.0 + 1, info_pos.1)].fg,
        uni.theme.color(Token::Border).unwrap()
    );

    // Below width 72 or in ASCII, the micro-bar is omitted cleanly.
    let narrow = Rect::new(0, 0, 68, 18);
    let mut nbuf = Buffer::empty(narrow);
    render_plan(&model, &keymap, &uni, narrow, &mut nbuf);
    let ntext = buffer_to_string(&nbuf);
    let narrow_seq = (0..narrow.height)
        .map(|y| {
            (0..narrow.width)
                .map(|x| nbuf[(x, y)].symbol())
                .collect::<String>()
        })
        .find(|line| line.contains("Seq Scan orders") && line.contains(" | "))
        .expect("narrow Seq Scan row");
    assert!(
        !narrow_seq.contains('\u{2501}') && !narrow_seq.contains('\u{2500}'),
        "narrow plan omits micro-bar: {ntext}"
    );

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut abuf = Buffer::empty(wide);
    render_plan(&model, &keymap, &ascii, wide, &mut abuf);
    let atext = buffer_to_string(&abuf);
    assert!(atext.is_ascii(), "{atext}");
    assert!(atext.contains("self time 4.00 ms"), "{atext}");
}

#[test]
fn plan_loading_state_uses_inset_padding_comet_and_focus_keycap() {
    use crate::app::model::{Focus, Model};
    use crate::app::plan::PlanView;
    use crate::query::JobId;

    let mut model = Model::new(100);
    model.focus = Focus::Results;
    model.frame = 3;
    model.plan = PlanView::loading(JobId(7), true);
    let keymap = crate::ui::keymap::Keymap::new();

    let uni = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let area = Rect::new(0, 0, 80, 14);
    let mut buf = Buffer::empty(area);
    render_plan(&model, &keymap, &uni, area, &mut buf);
    let text = buffer_to_string(&buf);

    assert!(
        text.contains("Measuring the statement once with EXPLAIN ANALYZE."),
        "{text}"
    );
    assert!(text.contains("Ctrl+C cancels the request"), "{text}");
    // Spinner, heading, and heartbeat comet render with Warning ink for EXPLAIN ANALYZE.
    let warn_fg = uni.theme.color(Token::Warning).unwrap();
    let heading = cell_at(&buf, "Measuring the statement");
    assert_eq!(heading.fg, warn_fg);
    let cancel_key = cell_at(&buf, "Ctrl+C");
    assert_eq!(cancel_key.fg, uni.theme.color(Token::Focus).unwrap());
    assert!(cancel_key.modifier.contains(Modifier::BOLD));
    let has_comet = (0..area.height).any(|y| {
        (0..area.width).any(|x| buf[(x, y)].fg == warn_fg && buf[(x, y)].symbol() == "\u{2501}")
    });
    assert!(
        has_comet,
        "loading plan should paint Warning heartbeat comet"
    );

    // Non-analyzed EXPLAIN switches heartbeat tint to Info.
    model.plan = PlanView::loading(JobId(8), false);
    let mut ebuf = Buffer::empty(area);
    render_plan(&model, &keymap, &uni, area, &mut ebuf);
    let etext = buffer_to_string(&ebuf);
    assert!(etext.contains("Reading the planner estimate."), "{etext}");
    let info_fg = uni.theme.color(Token::Info).unwrap();
    let has_info_comet = (0..area.height).any(|y| {
        (0..area.width).any(|x| ebuf[(x, y)].fg == info_fg && ebuf[(x, y)].symbol() == "\u{2501}")
    });
    assert!(
        has_info_comet,
        "plain EXPLAIN loading should paint Info comet"
    );

    // Reduced motion keeps wording clean with no comet bar.
    let reduced = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        true,
    );
    let mut rbuf = Buffer::empty(area);
    render_plan(&model, &keymap, &reduced, area, &mut rbuf);
    let rtext = buffer_to_string(&rbuf);
    assert!(rtext.contains("Reading the planner estimate."));
    assert!(!rtext.contains('\u{2500}'));
}

#[test]
fn unified_modal_chrome_styles_titles_tables_chords_bench_deltas_and_review_rail() {
    use crate::app::model::{Model, PendingUpdate, QueryBenchState};
    use crate::bench::{BenchConfig, BenchSample, compare_benchmarks, compute_bench_stats};
    use crate::query::JobId;
    use crate::query::value::Cell;

    let pres = Presentation::new(
        Theme::new(ThemeChoice::Dark, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Unicode),
        false,
    );
    let stripe_bg = pres.theme.surface(Token::SurfaceAlt).bg.unwrap();
    let header_fg = pres.theme.color(Token::Header).unwrap();
    let focus_fg = pres.theme.color(Token::Focus).unwrap();
    let border_fg = pres.theme.color(Token::Border).unwrap();
    let success_fg = pres.theme.color(Token::Success).unwrap();
    let danger_fg = pres.theme.color(Token::Danger).unwrap();

    // 1. Chord overlay: recessed keycaps + Border column divider + Focus split title.
    let chord_area = Rect::new(0, 0, 120, 30);
    let mut chord_buf = Buffer::empty(chord_area);
    render_chords(
        &crate::ui::keymap::Keymap::new(),
        &pres,
        chord_area,
        &mut chord_buf,
    );
    let chord_text = buffer_to_string(&chord_buf);
    assert!(chord_text.contains("Ctrl+K then"), "{chord_text}");
    assert_eq!(cell_at(&chord_buf, "Ctrl+K then").fg, focus_fg);
    let div_y = row_of(&chord_buf, "estimated query plan");
    assert!(
        (1..chord_area.width - 1).any(|x| {
            chord_buf[(x, div_y)].symbol() == "\u{2502}" && chord_buf[(x, div_y)].fg == border_fg
        }),
        "two-column chord menu must draw a Border column divider"
    );

    // 2. Benchmark summary deltas (negative latency = Success, QPS gain = Success;
    //    positive latency = Danger, QPS drop = Danger) + stripe table header + loading comet.
    let cfg = BenchConfig::new(1, 3, true);
    let base_s = vec![
        BenchSample {
            iteration: 1,
            duration_ms: 10.0,
            rows_returned: 5,
        },
        BenchSample {
            iteration: 2,
            duration_ms: 10.0,
            rows_returned: 5,
        },
    ];
    let fast_s = vec![
        BenchSample {
            iteration: 1,
            duration_ms: 5.0,
            rows_returned: 5,
        },
        BenchSample {
            iteration: 2,
            duration_ms: 5.0,
            rows_returned: 5,
        },
    ];
    let slow_s = vec![
        BenchSample {
            iteration: 1,
            duration_ms: 20.0,
            rows_returned: 5,
        },
        BenchSample {
            iteration: 2,
            duration_ms: 20.0,
            rows_returned: 5,
        },
    ];
    let base = compute_bench_stats("Baseline", "SELECT 1", &cfg, &base_s);
    let faster = compute_bench_stats("Faster", "SELECT 2", &cfg, &fast_s);
    let slower = compute_bench_stats("Slower", "SELECT 3", &cfg, &slow_s);

    let fast_report = compare_benchmarks(base.clone(), Some(faster.clone()), None);
    let fast_state = QueryBenchState {
        config: cfg,
        baseline_sql: "SELECT 1".to_owned(),
        candidate_sql: "SELECT 2".to_owned(),
        baseline: Some(base.clone()),
        candidate: Some(faster),
        report: Some(fast_report),
        active_tab: 0,
        selected_idx: 0,
        loading: false,
        cancelling: false,
        status_message: None,
        queued_candidate: None,
    };
    let modal_area = Rect::new(0, 0, 120, 32);
    let mut fbuf = Buffer::empty(modal_area);
    render_query_bench_with_motion(&fast_state, 2, Some(0.4), &pres, modal_area, &mut fbuf);

    let hdr = cell_at(&fbuf, "METRIC");
    assert_eq!(hdr.bg, stripe_bg);
    assert_eq!(hdr.fg, header_fg);
    assert!(hdr.modifier.contains(Modifier::BOLD));

    let neg_lat = cell_at(&fbuf, "-50.0%");
    assert_eq!(neg_lat.fg, success_fg);
    let pos_qps = cell_at(&fbuf, "+100.0 /s");
    assert_eq!(pos_qps.fg, success_fg);

    let slow_report = compare_benchmarks(base.clone(), Some(slower.clone()), None);
    let mut slow_state = QueryBenchState {
        config: cfg,
        baseline_sql: "SELECT 1".to_owned(),
        candidate_sql: "SELECT 3".to_owned(),
        baseline: Some(base),
        candidate: Some(slower),
        report: Some(slow_report),
        active_tab: 0,
        selected_idx: 0,
        loading: false,
        cancelling: false,
        status_message: None,
        queued_candidate: None,
    };
    let mut sbuf = Buffer::empty(modal_area);
    render_query_bench_with_motion(&slow_state, 2, None, &pres, modal_area, &mut sbuf);
    assert_eq!(cell_at(&sbuf, "+100.0%").fg, danger_fg);
    assert_eq!(cell_at(&sbuf, "-50.0 /s").fg, danger_fg);

    slow_state.loading = true;
    let mut lbuf = Buffer::empty(modal_area);
    render_query_bench_with_motion(&slow_state, 4, None, &pres, modal_area, &mut lbuf);
    assert!(buffer_to_string(&lbuf).contains("Running query benchmark"));

    slow_state.cancelling = true;
    let mut cbuf = Buffer::empty(modal_area);
    render_query_bench_with_motion(&slow_state, 4, None, &pres, modal_area, &mut cbuf);
    assert!(buffer_to_string(&cbuf).contains("Cancelling query benchmark"));
    assert_eq!(
        cell_at(&cbuf, "Cancelling query benchmark").fg,
        pres.theme.color(Token::Warning).unwrap()
    );

    // 3. Review modal rail highlights key tokens in Focus + BOLD on SurfaceAlt while preserving words.
    let mut review_model = Model::new(100);
    let source_sql = "SELECT id, status FROM public.orders ORDER BY id ASC;";
    let candidate = UpdateCandidate {
        result_job: JobId(1),
        source_row: 0,
        result_column: 1,
        source_sql: source_sql.into(),
    };
    let source = crate::query::parse_update_source(source_sql, &["id".into(), "status".into()], 1)
        .expect("update source");
    let relation = UpdateRelation {
        schema: "public".into(),
        relation: "orders".into(),
        kind: ObjectKind::Table,
        readable: true,
        writable: true,
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                primary_key: true,
                default: None,
            },
            ColumnInfo {
                name: "status".into(),
                data_type: "text".into(),
                nullable: false,
                primary_key: false,
                default: None,
            },
        ],
    };
    let plan = crate::query::plan_update(
        &source,
        "public",
        "orders",
        &["id".into(), "status".into()],
        &[Cell::Text("1".into()), Cell::Text("paid".into())],
        &[
            crate::query::UpdateColumn {
                name: "id".into(),
                primary_key: true,
            },
            crate::query::UpdateColumn {
                name: "status".into(),
                primary_key: false,
            },
        ],
        "delivered",
    )
    .expect("update plan");
    review_model.pending_update = Some(PendingUpdate {
        relation,
        candidate,
        plan,
    });
    let r_area = Rect::new(0, 0, 110, 26);
    let mut rbuf = Buffer::empty(r_area);
    render_update_review(&review_model, &pres, r_area, &mut rbuf);
    let enter_cell = cell_at(&rbuf, "Enter");
    assert_eq!(enter_cell.bg, stripe_bg);
    assert_eq!(enter_cell.fg, focus_fg);
    assert!(enter_cell.modifier.contains(Modifier::BOLD));
    let esc_cell = cell_at(&rbuf, "Esc");
    assert_eq!(esc_cell.bg, stripe_bg);
    assert_eq!(esc_cell.fg, focus_fg);
    assert!(esc_cell.modifier.contains(Modifier::BOLD));
}

#[test]
fn plan_rows_reserve_attention_and_mismatch_badges_and_show_minimum_fill_for_nonzero_share() {
    let json = r#"[
      {
        "Plan": {
          "Node Type": "Nested Loop",
          "Startup Cost": 0.28,
          "Total Cost": 47.9,
          "Plan Rows": 10,
          "Actual Startup Time": 0.05,
          "Actual Total Time": 11.8,
          "Actual Rows": 420,
          "Actual Loops": 1,
          "Plans": [
            {
              "Node Type": "Seq Scan",
              "Relation Name": "orders",
              "Startup Cost": 0.0,
              "Total Cost": 25.5,
              "Plan Rows": 10,
              "Actual Startup Time": 0.01,
              "Actual Total Time": 2.9,
              "Actual Rows": 420,
              "Actual Loops": 1
            },
            {
              "Node Type": "Index Scan",
              "Relation Name": "customers",
              "Index Name": "customers_name_key",
              "Startup Cost": 0.28,
              "Total Cost": 2.2,
              "Plan Rows": 1,
              "Actual Startup Time": 0.015,
              "Actual Total Time": 0.018,
              "Actual Rows": 1,
              "Actual Loops": 420
            },
            {
              "Node Type": "Result",
              "Startup Cost": 0.0,
              "Total Cost": 0.01,
              "Plan Rows": 1,
              "Actual Startup Time": 0.0,
              "Actual Total Time": 0.0,
              "Actual Rows": 1,
              "Actual Loops": 1
            }
          ]
        }
      }
    ]"#;
    let document = PlanDocument::parse(json, true).expect("valid analyzed plan");
    let mut view = PlanView::default();
    view.ready(document.clone());
    view.selected_path = vec![0];

    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 88, 6);
    let mut buf = Buffer::empty(area);
    render_plan_tree(&document, &view, &pres, area, &mut buf);
    let rendered = buffer_to_string(&buf);
    let lines: Vec<&str> = rendered.lines().collect();

    // Row 0 (Nested Loop, 11.4% self time) fits micro-bar (>=1 filled cell) and mismatch badge.
    assert!(
        lines[0].contains("\u{2501}\u{2500}\u{2500}\u{2500}\u{2500}"),
        "{rendered}"
    );
    assert!(lines[0].contains("[estimate mismatch]"), "{rendered}");

    // Row 1 (Seq Scan orders) keeps mismatch badge unclipped.
    assert!(lines[1].contains("[estimate mismatch]"), "{rendered}");

    // Row 2 (Index Scan customers via customers_name_key) reserves [attention: measured time]
    // instead of clipping the badge off at 88 columns.
    assert!(
        lines[2].contains("[attention: measured time]"),
        "attention badge must not be clipped at 88 columns:\n{rendered}"
    );

    // Row 3 (Result, 0.0 ms = 0.0 share) renders all-empty track without artificial fill.
    assert!(
        lines[3].contains("\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"),
        "zero-share node keeps empty micro-bar track:\n{rendered}"
    );
    assert!(!lines[3].contains('\u{2501}'), "{rendered}");
}

#[test]
fn layout_wires_bench_settle_progress_into_benchmark_overlay_footer() {
    use crate::app::model::{Model, QueryBenchState};
    use crate::bench::{BenchConfig, BenchSample, compare_benchmarks, compute_bench_stats};

    let cfg = BenchConfig::new(1, 2, true);
    let base = compute_bench_stats(
        "A",
        "SELECT 1",
        &cfg,
        &[BenchSample {
            iteration: 1,
            duration_ms: 4.0,
            rows_returned: 1,
        }],
    );
    let report = compare_benchmarks(base.clone(), None, None);

    let mut settled_model = Model::new(100);
    settled_model.presentation = presentation(ColorDepth::TrueColor);
    settled_model.query_bench = Some(QueryBenchState {
        config: cfg,
        baseline_sql: "SELECT 1".to_owned(),
        candidate_sql: String::new(),
        baseline: Some(base),
        candidate: None,
        report: Some(report),
        active_tab: 0,
        selected_idx: 0,
        loading: false,
        cancelling: false,
        status_message: None,
        queued_candidate: None,
    });

    let mut animated_model = settled_model.clone();
    animated_model.motion.begin_bench_settle();

    let keymap = crate::ui::keymap::Keymap::new();
    let area = Rect::new(0, 0, 120, 32);
    let mut sbuf = Buffer::empty(area);
    crate::ui::layout::render(&settled_model, &keymap, area, &mut sbuf);
    let mut abuf = Buffer::empty(area);
    crate::ui::layout::render(&animated_model, &keymap, area, &mut abuf);

    let resting_fg = cell_at(&sbuf, "Single-query benchmark:").fg;
    let active_fg = cell_at(&abuf, "Single-query benchmark:").fg;
    assert_ne!(
        active_fg, resting_fg,
        "BenchSettle progress must tint the settled benchmark status line"
    );
    assert_eq!(
        active_fg,
        animated_model
            .presentation
            .theme
            .color(Token::Success)
            .unwrap()
    );
}

#[test]
fn plan_explorer_renders_surface_alt_kpi_strip_and_scales_micro_bars_with_sweep() {
    use crate::app::model::{Focus, Model};
    use crate::query::plan::PlanDocument;

    let json = r#"[
      {
        "Plan": {
          "Node Type": "Merge Join",
          "Startup Cost": 10.0,
          "Total Cost": 40.0,
          "Plan Rows": 100,
          "Actual Startup Time": 1.0,
          "Actual Total Time": 10.0,
          "Actual Rows": 100,
          "Actual Loops": 1,
          "Plans": [
            {
              "Node Type": "Seq Scan",
              "Relation Name": "orders",
              "Startup Cost": 0.0,
              "Total Cost": 20.0,
              "Plan Rows": 10,
              "Actual Startup Time": 0.1,
              "Actual Total Time": 8.0,
              "Actual Rows": 100,
              "Actual Loops": 1
            }
          ]
        },
        "Planning Time": 0.42,
        "Execution Time": 10.5
      }
    ]"#;
    let doc = PlanDocument::parse(json, true).expect("valid analyzed plan");
    let mut model = Model::new(100);
    model.focus = Focus::Results;
    model.plan.ready(doc.clone());
    let keymap = crate::ui::keymap::Keymap::new();
    let pres = presentation(ColorDepth::TrueColor);
    let stripe_bg = pres.theme.surface(Token::SurfaceAlt).bg.unwrap();

    // Full height (>= 10 inner): shows recessed SurfaceAlt KPI strip with Nodes, Planning, Execution, Hotspot.
    let wide = Rect::new(0, 0, 96, 18);
    let mut buf = Buffer::empty(wide);
    render_plan(&model, &keymap, &pres, wide, &mut buf);
    let kpi_cell = cell_at(&buf, "Nodes");
    assert_eq!(kpi_cell.bg, stripe_bg, "KPI strip must sit on SurfaceAlt");
    let text = buffer_to_string(&buf);
    for needle in [
        "Nodes 2",
        "Planning 0.42 ms",
        "Execution 10.50 ms",
        "Hotspot Seq Scan",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
    assert!(
        text.contains("Attention: Seq Scan (measured time)"),
        "{text}"
    );

    // Narrow height (< 10 inner): omits KPI strip without breaking Attention or tree rows.
    let narrow_h = Rect::new(0, 0, 96, 10);
    let mut nbuf = Buffer::empty(narrow_h);
    render_plan(&model, &keymap, &pres, narrow_h, &mut nbuf);
    let ntext = buffer_to_string(&nbuf);
    assert!(!ntext.contains("Hotspot"), "{ntext}");
    assert!(ntext.contains("Attention:"), "{ntext}");

    // Sweep hook scales micro-bar fill proportionally when < 1.0.
    let tree_area = Rect::new(0, 0, 120, 4);
    let mut full_tree = Buffer::empty(tree_area);
    let mut half_tree = Buffer::empty(tree_area);
    super::render_plan_tree_with_sweep(&doc, &model.plan, &pres, 1.0, tree_area, &mut full_tree);
    super::render_plan_tree_with_sweep(&doc, &model.plan, &pres, 0.25, tree_area, &mut half_tree);
    let full_fills: usize = (0..tree_area.width)
        .filter(|&x| full_tree[(x, 1)].symbol() == "\u{2501}")
        .count();
    let half_fills: usize = (0..tree_area.width)
        .filter(|&x| half_tree[(x, 1)].symbol() == "\u{2501}")
        .count();
    assert!(
        full_fills > half_fills,
        "full={full_fills} half={half_fills}"
    );

    // Live model.motion().plan_sweep() drives render_plan and settles to 1.0 under reduced_motion.
    let mut wide_model = Model::new(100);
    wide_model.focus = Focus::Results;
    wide_model.presentation = pres.clone();
    wide_model.plan.ready(doc);
    wide_model.motion.enable();
    wide_model.motion.begin_plan_sweep();
    wide_model
        .motion
        .advance(std::time::Duration::from_millis(80));
    let wide_120 = Rect::new(0, 0, 120, 18);
    let mut anim_buf = Buffer::empty(wide_120);
    render_plan(&wide_model, &keymap, &pres, wide_120, &mut anim_buf);
    let anim_fills: usize = (0..wide_120.height)
        .flat_map(|y| (0..wide_120.width).map(move |x| (x, y)))
        .filter(|&(x, y)| anim_buf[(x, y)].symbol() == "\u{2501}")
        .count();

    let mut reduced_pres = pres;
    reduced_pres.reduced_motion = true;
    let mut red_buf = Buffer::empty(wide_120);
    render_plan(&wide_model, &keymap, &reduced_pres, wide_120, &mut red_buf);
    let red_fills: usize = (0..wide_120.height)
        .flat_map(|y| (0..wide_120.width).map(move |x| (x, y)))
        .filter(|&(x, y)| red_buf[(x, y)].symbol() == "\u{2501}")
        .count();
    assert!(
        red_fills > anim_fills,
        "reduced_motion={red_fills} must render full bars while active plan_sweep={anim_fills}"
    );
}

#[test]
fn schema_diagram_renders_summary_pills_surface_alt_table_headers_and_key_indicators() {
    use crate::app::model::SchemaDiagramState;
    use crate::diagram::{Cardinality, DiagramColumn, DiagramTable, ForeignKeyEdge, SchemaGraph};

    let graph = SchemaGraph {
        tables: vec![
            DiagramTable {
                schema: "public".into(),
                name: "customers".into(),
                columns: vec![DiagramColumn {
                    name: "id".into(),
                    data_type: "integer".into(),
                    is_primary_key: true,
                    is_foreign_key: false,
                    is_unique: true,
                    nullable: false,
                }],
            },
            DiagramTable {
                schema: "public".into(),
                name: "orders".into(),
                columns: vec![
                    DiagramColumn {
                        name: "id".into(),
                        data_type: "integer".into(),
                        is_primary_key: true,
                        is_foreign_key: false,
                        is_unique: true,
                        nullable: false,
                    },
                    DiagramColumn {
                        name: "customer_id".into(),
                        data_type: "integer".into(),
                        is_primary_key: false,
                        is_foreign_key: true,
                        is_unique: false,
                        nullable: false,
                    },
                ],
            },
        ],
        edges: vec![ForeignKeyEdge {
            constraint_name: "orders_customer_fk".into(),
            source_schema: "public".into(),
            source_table: "orders".into(),
            source_columns: vec!["customer_id".into()],
            target_schema: "public".into(),
            target_table: "customers".into(),
            target_columns: vec!["id".into()],
            cardinality: Cardinality::OneToMany,
            nullable: false,
        }],
        truncated_from: None,
    };
    let state = SchemaDiagramState::new(graph, true);
    let area = Rect::new(0, 0, 120, 32);
    let pres = presentation(ColorDepth::TrueColor);
    let mut buf = Buffer::empty(area);
    super::render_schema_diagram(&state, &pres, area, &mut buf);

    let text = buffer_to_string(&buf);
    for needle in [
        "TABLES: 2",
        "EDGES: 1",
        "DEPTH: all tables",
        "FORMAT: Unicode",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }

    // Table header inside entity card sits on SurfaceAlt with Header ink.
    let stripe_bg = pres.theme.surface(Token::SurfaceAlt).bg.unwrap();
    let header_y = (0..area.height)
        .find(|&y| {
            let line: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
            line.contains("public.customers") && line.contains('\u{2502}') && !line.contains("> ")
        })
        .expect("entity card table header row");
    let header_line: String = (0..area.width)
        .map(|x| buf[(x, header_y)].symbol())
        .collect();
    let card_hdr_x = header_line[..header_line.find("public.customers").unwrap()]
        .chars()
        .count() as u16;
    assert_eq!(buf[(card_hdr_x, header_y)].bg, stripe_bg);
    assert_eq!(
        buf[(card_hdr_x, header_y)].fg,
        pres.theme.color(Token::Header).unwrap()
    );

    // PK uses Focus and FK uses Info.
    let pk_cell = cell_at(&buf, "PK ");
    assert_eq!(pk_cell.fg, pres.theme.color(Token::Focus).unwrap());
    let fk_cell = cell_at(&buf, "FK ");
    assert_eq!(fk_cell.fg, pres.theme.color(Token::Info).unwrap());

    // ASCII / None mode keeps output pure ASCII and backgrounds Reset.
    let ascii = ascii_presentation();
    let ascii_state = SchemaDiagramState::new(state.graph, false);
    let mut abuf = Buffer::empty(area);
    super::render_schema_diagram(&ascii_state, &ascii, area, &mut abuf);
    let atext = buffer_to_string(&abuf);
    assert!(atext.is_ascii(), "{atext}");
    assert!(abuf.content.iter().all(|c| c.bg == Color::Reset));
}

#[test]
fn server_activity_renders_kpi_summary_pills_and_proportional_session_state_bar() {
    use crate::activity::{BackendProcess, DatabaseHealthMetrics, ServerActivitySnapshot};
    use crate::app::model::ServerActivityState;

    let snapshot = ServerActivitySnapshot {
        metrics: DatabaseHealthMetrics {
            database: "ignatius_demo".into(),
            active_connections: 4,
            xact_commit: 420,
            xact_rollback: 1,
            cache_hit_bps: 9950,
            deadlocks: 0,
            temp_bytes: 0,
        },
        backends: vec![
            BackendProcess {
                pid: 101,
                database: "ignatius_demo".into(),
                username: "app".into(),
                application_name: "api".into(),
                client_addr: "127.0.0.1".into(),
                state: "active".into(),
                wait_event_type: None,
                wait_event: None,
                query_age_ms: 400,
                xact_age_ms: Some(400),
                query: "SELECT 1".into(),
                blocked_by: Vec::new(),
                lock_summary: None,
                is_self: false,
            },
            BackendProcess {
                pid: 102,
                database: "ignatius_demo".into(),
                username: "worker".into(),
                application_name: "jobs".into(),
                client_addr: "127.0.0.1".into(),
                state: "active".into(),
                wait_event_type: Some("Lock".into()),
                wait_event: Some("relation".into()),
                query_age_ms: 2500,
                xact_age_ms: Some(2500),
                query: "UPDATE orders SET status = 'paid'".into(),
                blocked_by: vec![101],
                lock_summary: Some("ExclusiveLock on orders".into()),
                is_self: false,
            },
            BackendProcess {
                pid: 103,
                database: "ignatius_demo".into(),
                username: "analyst".into(),
                application_name: "ignatius".into(),
                client_addr: "127.0.0.1".into(),
                state: "idle".into(),
                wait_event_type: Some("Client".into()),
                wait_event: Some("ClientRead".into()),
                query_age_ms: 100,
                xact_age_ms: None,
                query: "SELECT 2".into(),
                blocked_by: Vec::new(),
                lock_summary: None,
                is_self: true,
            },
        ],
        truncated_from: None,
    };

    let state = ServerActivityState::new(snapshot.clone(), true);
    let area = Rect::new(0, 0, 120, 32);
    let pres = presentation(ColorDepth::TrueColor);
    let mut buf = Buffer::empty(area);
    super::render_server_activity(&state, &pres, area, &mut buf);

    let text = buffer_to_string(&buf);
    for needle in [
        "Active: 2",
        "Idle: 1",
        "Blocked: 1",
        "Waiting: 1",
        "State mix:",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }

    let mix_y = row_of(&buf, "State mix:");
    let mix_row: String = (0..area.width).map(|x| buf[(x, mix_y)].symbol()).collect();
    assert!(
        mix_row.contains('\u{2588}'),
        "active bar segment: {mix_row}"
    );
    assert!(
        mix_row.contains('\u{2593}'),
        "waiting/blocked segment: {mix_row}"
    );
    assert!(mix_row.contains('\u{2591}'), "idle segment: {mix_row}");

    let active_x = mix_row[..mix_row.find('\u{2588}').unwrap()].chars().count() as u16;
    assert_eq!(
        buf[(active_x, mix_y)].fg,
        pres.theme.color(Token::Success).unwrap()
    );
    let blocked_x = mix_row[..mix_row.find('\u{2593}').unwrap()].chars().count() as u16;
    assert_eq!(
        buf[(blocked_x, mix_y)].fg,
        pres.theme.color(Token::Danger).unwrap()
    );

    let ascii = ascii_presentation();
    let ascii_state = ServerActivityState::new(snapshot, false);
    let mut abuf = Buffer::empty(area);
    super::render_server_activity(&ascii_state, &ascii, area, &mut abuf);
    let atext = buffer_to_string(&abuf);
    assert!(atext.is_ascii(), "{atext}");
    assert!(abuf.content.iter().all(|c| c.bg == Color::Reset));
}

#[test]
fn result_analytics_tab_two_renders_fractional_unicode_and_ascii_meters() {
    use crate::query::result::ResultSet;
    use crate::query::value::Cell;

    let mut set = ResultSet::with_column_types(
        vec!["bucket".to_owned()],
        vec![Some("int4".to_owned())],
        100,
    );
    // 10 occurrences of "1", 3 occurrences of "50", 1 occurrence of "100" -> exercises fractional eighth blocks.
    for _ in 0..10 {
        set.push(vec![Cell::Text("1".to_owned())]);
    }
    for _ in 0..3 {
        set.push(vec![Cell::Text("50".to_owned())]);
    }
    set.push(vec![Cell::Text("100".to_owned())]);

    let mut state = crate::app::model::ResultAnalyticsState::from_result(&set);
    state.set_tab(1);
    let area = Rect::new(0, 0, 130, 32);

    let pres = presentation(ColorDepth::TrueColor);
    let mut ubuf = Buffer::empty(area);
    render_result_analytics(&state, &pres, area, &mut ubuf);
    let utext = buffer_to_string(&ubuf);

    let top_row_y = row_of(&ubuf, "#2 ");
    let top_row: String = (0..area.width)
        .map(|x| ubuf[(x, top_row_y)].symbol())
        .collect();
    let has_unicode_meter = top_row
        .chars()
        .any(|ch| ('\u{2588}'..='\u{258f}').contains(&ch));
    assert!(
        has_unicode_meter,
        "expected fractional bar meter on top value row: {top_row}\n{utext}"
    );
    let meter_x = top_row
        .char_indices()
        .find(|(_, ch)| ('\u{2588}'..='\u{258f}').contains(ch))
        .map(|(b, _)| top_row[..b].chars().count() as u16)
        .unwrap();
    assert_eq!(
        ubuf[(meter_x, top_row_y)].fg,
        pres.theme.color(Token::Info).unwrap()
    );

    let hist_y = row_of(&ubuf, "B1:");
    let hist_row: String = (0..area.width)
        .map(|x| ubuf[(x, hist_y)].symbol())
        .collect();
    let hist_x = hist_row
        .char_indices()
        .find(|(_, ch)| ('\u{2588}'..='\u{258f}').contains(ch))
        .map(|(b, _)| hist_row[..b].chars().count() as u16)
        .unwrap();
    assert_eq!(
        ubuf[(hist_x, hist_y)].fg,
        pres.theme.color(Token::Focus).unwrap()
    );

    let ascii = ascii_presentation();
    let mut abuf = Buffer::empty(area);
    render_result_analytics(&state, &ascii, area, &mut abuf);
    let atext = buffer_to_string(&abuf);
    assert!(atext.is_ascii(), "{atext}");
    let ascii_top_y = row_of(&abuf, "#1 ");
    let ascii_top: String = (0..area.width)
        .map(|x| abuf[(x, ascii_top_y)].symbol())
        .collect();
    assert!(
        ascii_top.contains("###"),
        "ASCII top value meter: {ascii_top}"
    );
}

#[test]
fn result_diff_overlay_renders_kpi_strip_badges_and_ascii_fallback() {
    use crate::query::Cell;
    let mut base_set =
        crate::query::result::ResultSet::new(vec!["id".into(), "status".into()], 100);
    base_set.push(vec![Cell::Text("1".into()), Cell::Text("pending".into())]);
    base_set.push(vec![Cell::Text("2".into()), Cell::Text("old".into())]);
    let pinned = crate::result_diff::PinnedResult::capture(
        &base_set,
        Some("SELECT id, status FROM orders"),
        Some("id".into()),
        Some("local".into()),
    )
    .unwrap();

    let mut curr_set = crate::query::result::ResultSet::new(
        vec!["id".into(), "status".into(), "note".into()],
        100,
    );
    curr_set.push(vec![
        Cell::Text("1".into()),
        Cell::Text("paid".into()),
        Cell::Null,
    ]);
    curr_set.push(vec![
        Cell::Text("3".into()),
        Cell::Text("new".into()),
        Cell::Text("rush".into()),
    ]);

    let state = crate::app::model::ResultDiffState::new(&pinned, &curr_set);
    let area = Rect::new(0, 0, 130, 30);

    let ascii = ascii_presentation();
    let mut abuf = Buffer::empty(area);
    render_result_diff(&state, &ascii, area, &mut abuf);
    let atext = buffer_to_string(&abuf);
    assert!(atext.is_ascii(), "{atext}");
    assert!(atext.contains("+ADDED: 1"), "{atext}");
    assert!(atext.contains("-REMOVED: 1"), "{atext}");
    assert!(atext.contains("~CHANGED: 1"), "{atext}");
    assert!(atext.contains("[~]"), "{atext}");
    assert!(atext.contains("[-]"), "{atext}");
    assert!(atext.contains("[+]"), "{atext}");
    assert!(atext.contains("KEY:id"), "{atext}");
    assert!(atext.contains("pending->paid"), "{atext}");

    let pres = presentation(ColorDepth::TrueColor);
    let mut ubuf = Buffer::empty(area);
    render_result_diff(&state, &pres, area, &mut ubuf);
    let utext = buffer_to_string(&ubuf);
    assert!(utext.contains("RESULT SNAPSHOT DIFF"), "{utext}");
    assert!(utext.contains("SCHEMA: +1/-0"), "{utext}");
}

#[test]
fn find_replace_overlay_renders_counts_case_pill_replace_field_and_capped_count() {
    let mut model = Model::new(100);
    model.focus = Focus::Editor;
    model.editor.set_text("a a a a a a a");
    model.find_replace = Some(crate::app::model::FindReplaceState {
        query: "a".to_owned(),
        replacement: "b".to_owned(),
        active_field: crate::app::model::FindReplaceField::Replace,
        show_replace: true,
        case_sensitive: true,
        active_match: 1,
    });
    model.find_notice = Some("Replaced 7 occurrence(s).".to_owned());

    let area = Rect::new(0, 0, 100, 24);
    let pres = presentation(ColorDepth::TrueColor);
    let mut buf = Buffer::empty(area);
    render_find_replace(&model, &pres, area, &mut buf);
    let text = buffer_to_string(&buf);
    assert!(text.contains("Find & Replace"), "{text}");
    assert!(text.contains("Find: a"), "{text}");
    assert!(text.contains("Replace: b"), "{text}");
    assert!(text.contains("[Aa]"), "{text}");
    assert!(text.contains("Match 2 of 7"), "{text}");
    assert!(text.contains("Replaced 7 occurrence(s)."), "{text}");
    assert!(text.contains("Enter"), "{text}");
    assert!(text.contains("Alt+A"), "{text}");

    // Zero matches + case-insensitive pill
    model.find_replace = Some(crate::app::model::FindReplaceState {
        query: "zzz".to_owned(),
        replacement: String::new(),
        active_field: crate::app::model::FindReplaceField::Find,
        show_replace: false,
        case_sensitive: false,
        active_match: 0,
    });
    model.find_notice = None;
    let mut zero_buf = Buffer::empty(area);
    render_find_replace(&model, &pres, area, &mut zero_buf);
    let zero_text = buffer_to_string(&zero_buf);
    assert!(zero_text.contains("[aa]"), "{zero_text}");
    assert!(zero_text.contains("0 matches"), "{zero_text}");

    // Capped at 10,000+ matches
    model
        .editor
        .set_text("x".repeat(crate::app::editor::MAX_FIND_MATCHES + 3));
    model.find_replace.as_mut().unwrap().query = "x".to_owned();
    let mut cap_buf = Buffer::empty(area);
    render_find_replace(&model, &pres, area, &mut cap_buf);
    let cap_text = buffer_to_string(&cap_buf);
    assert!(cap_text.contains("10,000+ matches"), "{cap_text}");
}

#[test]
fn jump_to_line_prompt_renders_line_label_and_title() {
    let prompt = NamePrompt::for_jump_to_line(2, 12);
    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 10);
    let mut buf = Buffer::empty(area);
    render_name_prompt(&prompt, &pres, area, &mut buf);
    let text = buffer_to_string(&buf);
    assert!(text.contains("Jump to line"), "{text}");
    assert!(text.contains("Line:"), "{text}");
    assert!(text.contains("1-12, currently 2"), "{text}");
}
