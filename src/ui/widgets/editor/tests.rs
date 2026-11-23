//! Focused tests for the editor completion popup and guidance placeholders.

use super::{render_completion, render_editor, render_recovery_capsule};
use crate::app::completion::CompletionMenu;
use crate::app::model::{ConnectionState, Model};
use crate::config::ThemeChoice;
use crate::connection::Environment;
use crate::postgres::{SessionInfo, TlsState};
use crate::query::completion::{
    Candidate, CandidateKind, CatalogObjectKind, CompletionResult, CompletionScope,
};
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

fn session() -> Box<SessionInfo> {
    Box::new(SessionInfo {
        target: "app@localhost:5432/orders".into(),
        database: "orders".into(),
        user: "app".into(),
        server_version: "18.6".into(),
        backend_pid: "42".into(),
        search_path: "public".into(),
        read_only: false,
        tls: TlsState::Disabled,
        environment: Environment::Local,
    })
}

fn presentation(depth: ColorDepth) -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
        Glyphs::new(GlyphTier::Unicode),
        false,
    )
}

fn test_completion_result() -> CompletionResult {
    CompletionResult {
        replacement: 0..3,
        prefix: "sel".into(),
        scope: CompletionScope::Keywords,
        candidates: vec![
            Candidate {
                label: "SELECT".into(),
                insert_text: "SELECT".into(),
                kind: CandidateKind::Keyword,
                schema: None,
                source: None,
                detail: None,
            },
            Candidate {
                label: "users".into(),
                insert_text: "users".into(),
                kind: CandidateKind::Object(CatalogObjectKind::Table),
                schema: Some("public".into()),
                source: None,
                detail: None,
            },
            Candidate {
                label: "now".into(),
                insert_text: "now()".into(),
                kind: CandidateKind::Object(CatalogObjectKind::Function),
                schema: Some("pg_catalog".into()),
                source: None,
                detail: Some("() -> timestamp".into()),
            },
            Candidate {
                label: "id".into(),
                insert_text: "id".into(),
                kind: CandidateKind::Column,
                schema: None,
                source: Some("users".into()),
                detail: Some("bigint".into()),
            },
            Candidate {
                label: "cte_data".into(),
                insert_text: "cte_data".into(),
                kind: CandidateKind::Cte,
                schema: None,
                source: None,
                detail: None,
            },
        ],
        matching_count: 5,
        total_count: 5,
    }
}

#[test]
fn completion_menu_renders_full_width_selection_and_kind_colored_candidates() {
    let mut model = Model::new(100);
    model.editor.set_text("SELECT 1;");
    model.editor.set_position(1, 4);
    model.completion.menu = Some(CompletionMenu::new(test_completion_result(), true));

    // 1. TrueColor: selection highlight spans full interior width; candidates use semantic colors.
    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 25);
    let mut buf = Buffer::empty(area);
    render_editor(&model, &Keymap::new(), &pres, area, &mut buf);
    render_completion(&model, &pres, area, &mut buf);

    // Locate the completion popup box by searching for the "Complete" title in the top border
    let complete_pos = (0..area.height)
        .find_map(|y| {
            (0..area.width)
                .find(|&x| {
                    buf[(x, y)].symbol() == "C"
                        && (x + 7 < area.width)
                        && (0..8).map(|i| buf[(x + i, y)].symbol()).collect::<String>()
                            == "Complete"
                })
                .map(|x| (x, y))
        })
        .expect("Complete title found in buffer");

    let popup_y = complete_pos.1;
    let top_left_sym = pres.glyphs.borders(true).top_left;
    let top_right_sym = pres.glyphs.borders(true).top_right;
    let popup_left = (0..complete_pos.0)
        .rev()
        .find(|&x| buf[(x, popup_y)].symbol() == top_left_sym)
        .expect("popup top-left corner");
    let popup_right = (complete_pos.0..area.width)
        .find(|&x| buf[(x, popup_y)].symbol() == top_right_sym)
        .expect("popup top-right corner");

    let sel_bg = pres.theme.style(Token::Selection).bg.unwrap();
    let border_fg = pres.theme.color(Token::Border).unwrap();

    // Top hairline rule separates scope header on popup_y + 1 from candidate rows.
    assert_eq!(buf[(popup_left + 1, popup_y + 2)].symbol(), "\u{2500}");
    assert_eq!(buf[(popup_left + 1, popup_y + 2)].fg, border_fg);

    // Candidate 0 (SELECT, selected) is on row popup_y + 3
    let sel_y = popup_y + 3;
    for x in (popup_left + 1)..popup_right {
        assert_eq!(
            buf[(x, sel_y)].bg,
            sel_bg,
            "x={x} on selected candidate row must carry selection background"
        );
    }

    // Candidate 1 (users, Table, unselected) is on row popup_y + 4
    let table_y = popup_y + 4;
    let info_fg = pres.theme.color(Token::Info).unwrap();
    let muted_fg = pres.theme.color(Token::Muted).unwrap();

    // Find where "users" starts on this row
    let users_x = ((popup_left + 1)..popup_right)
        .find(|&x| {
            x + 5 <= popup_right
                && (0..5)
                    .map(|i| buf[(x + i, table_y)].symbol())
                    .collect::<String>()
                    == "users"
        })
        .expect("users label on candidate 1");

    assert_eq!(
        buf[(users_x, table_y)].fg,
        info_fg,
        "Table candidate prefix must use Token::Info foreground"
    );

    // Detail suffix for users ("table") must use Token::Muted
    let table_suffix_x = (users_x..popup_right)
        .find(|&x| {
            x + 5 <= popup_right
                && (0..5)
                    .map(|i| buf[(x + i, table_y)].symbol())
                    .collect::<String>()
                    == "table"
        })
        .expect("table suffix on candidate 1");
    assert_eq!(
        buf[(table_suffix_x, table_y)].fg,
        muted_fg,
        "detail suffix must use Token::Muted foreground"
    );

    // Candidate 2 (now, Function, unselected) is on row popup_y + 5 -> Token::SyntaxNumber
    let fn_y = popup_y + 5;
    let number_fg = pres.theme.color(Token::SyntaxNumber).unwrap();
    let now_x = ((popup_left + 1)..popup_right)
        .find(|&x| {
            x + 3 <= popup_right
                && (0..3)
                    .map(|i| buf[(x + i, fn_y)].symbol())
                    .collect::<String>()
                    == "now"
        })
        .expect("now label on candidate 2");
    assert_eq!(
        buf[(now_x, fn_y)].fg,
        number_fg,
        "Function candidate prefix must use Token::SyntaxNumber foreground"
    );

    // Candidate 3 (id, Column, unselected) is on row popup_y + 6 -> Token::Text
    let col_y = popup_y + 6;
    let text_fg = pres.theme.color(Token::Text).unwrap();
    let id_x = ((popup_left + 1)..popup_right)
        .find(|&x| {
            x + 2 <= popup_right
                && (0..2)
                    .map(|i| buf[(x + i, col_y)].symbol())
                    .collect::<String>()
                    == "id"
        })
        .expect("id label on candidate 3");
    assert_eq!(
        buf[(id_x, col_y)].fg,
        text_fg,
        "Column candidate prefix must use Token::Text foreground"
    );

    // Candidate 4 (cte_data, Cte, unselected) is on row popup_y + 7 -> Token::Focus
    let cte_y = popup_y + 7;
    let focus_fg = pres.theme.color(Token::Focus).unwrap();
    let cte_x = ((popup_left + 1)..popup_right)
        .find(|&x| {
            x + 8 <= popup_right
                && (0..8)
                    .map(|i| buf[(x + i, cte_y)].symbol())
                    .collect::<String>()
                    == "cte_data"
        })
        .expect("cte_data label on candidate 4");
    assert_eq!(
        buf[(cte_x, cte_y)].fg,
        focus_fg,
        "CTE candidate prefix must use Token::Focus foreground"
    );

    // Bottom hairline rule separates candidates from footer hints/count.
    assert_eq!(buf[(popup_left + 1, popup_y + 8)].symbol(), "\u{2500}");
    assert_eq!(buf[(popup_left + 1, popup_y + 8)].fg, border_fg);

    // 2. ColorDepth::Basic16 and ColorDepth::None leave selected background as Reset
    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let pres_uncolored = presentation(depth);
        let mut buf_uncolored = Buffer::empty(area);
        render_editor(
            &model,
            &Keymap::new(),
            &pres_uncolored,
            area,
            &mut buf_uncolored,
        );
        render_completion(&model, &pres_uncolored, area, &mut buf_uncolored);

        let cell_bg = buf_uncolored[(popup_right - 2, sel_y)].bg;
        assert_eq!(
            cell_bg,
            Color::Reset,
            "depth {depth:?} must not paint a selection background fill"
        );
    }
}

#[test]
fn empty_editor_placeholder_highlights_keycap_prefixes() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("");

    let keymap = Keymap::new();
    let run_key = crate::ui::widgets::action_key_label(&keymap, &crate::app::Action::RunBuffer);
    let palette_key =
        crate::ui::widgets::action_key_label(&keymap, &crate::app::Action::OpenPalette);

    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 20);
    let mut buf = Buffer::empty(area);
    render_editor(&model, &keymap, &pres, area, &mut buf);

    let focus_color = pres.theme.color(Token::Focus).unwrap();
    let info_color = pres.theme.color(Token::Info).unwrap();
    let muted_color = pres.theme.color(Token::Muted).unwrap();

    // Check RunBuffer hint: format!("{run_key} Run the buffer once SQL is ready.")
    // "{run_key} " must have Token::Focus + BOLD
    let (run_x, run_y) = (0..area.height)
        .find_map(|y| {
            (0..area.width)
                .find(|&x| {
                    x as usize + run_key.len() <= area.width as usize
                        && (0..run_key.len())
                            .map(|i| buf[(x + i as u16, y)].symbol())
                            .collect::<String>()
                            == run_key
                })
                .map(|x| (x, y))
        })
        .expect("run_key text in buffer");

    assert_eq!(buf[(run_x, run_y)].fg, focus_color);
    assert!(buf[(run_x, run_y)].modifier.contains(Modifier::BOLD));

    // Description text "Run the buffer once SQL is ready." must have Token::Info
    let run_desc_x = run_x + run_key.len() as u16 + 1; // after "{run_key} "
    assert_eq!(buf[(run_desc_x, run_y)].fg, info_color);

    // Check OpenPalette hint: format!("{palette_key} Command palette answers what else is possible.")
    // "{palette_key} " must have Token::Focus + BOLD
    let (palette_x, palette_y) = (0..area.height)
        .find_map(|y| {
            (0..area.width)
                .find(|&x| {
                    x as usize + palette_key.len() <= area.width as usize
                        && (0..palette_key.len())
                            .map(|i| buf[(x + i as u16, y)].symbol())
                            .collect::<String>()
                            == palette_key
                })
                .map(|x| (x, y))
        })
        .expect("palette_key text in buffer");

    assert_eq!(buf[(palette_x, palette_y)].fg, focus_color);
    assert!(
        buf[(palette_x, palette_y)]
            .modifier
            .contains(Modifier::BOLD)
    );

    // Description text "Command palette answers what else is possible." must have Token::Muted
    let palette_desc_x = palette_x + palette_key.len() as u16 + 1; // after "{palette_key} "
    assert_eq!(buf[(palette_desc_x, palette_y)].fg, muted_color);
}

#[test]
fn multi_buffer_tab_bar_renders_status_capsule_and_tabs() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("SELECT 1;");
    model.sync_active_buffer_from_editor();

    // Create a second buffer
    model
        .buffers
        .new_buffer(Some("query-2.sql".into()), Some("SELECT 2;".into()));
    model.sync_editor_from_active_buffer();

    let keymap = Keymap::new();
    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 20);
    let mut buf = Buffer::empty(area);

    render_editor(&model, &keymap, &pres, area, &mut buf);

    let rendered_text = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered_text.contains("[BUFFERS: 2/16]"),
        "Must render [BUFFERS: 2/16] status capsule in tab bar:\n{rendered_text}"
    );
    assert!(
        rendered_text.contains("[2: query-2.sql"),
        "Must render active tab [2: query-2.sql] highlighted:\n{rendered_text}"
    );
    assert!(
        rendered_text.contains("1: buffer-1.sql"),
        "Must render inactive tab 1: buffer-1.sql:\n{rendered_text}"
    );
}

#[test]
fn split_mode_renders_separator_and_panes() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("SELECT 'left';");
    model.sync_active_buffer_from_editor();

    model
        .buffers
        .new_buffer(Some("right.sql".into()), Some("SELECT 'right';".into()));
    model.sync_editor_from_active_buffer();

    // Turn on vertical split mode
    model.buffers.split_mode = crate::buffer::EditorSplitMode::Vertical;
    model.buffers.secondary_idx = Some(0);

    let keymap = Keymap::new();
    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 80, 20);
    let mut buf = Buffer::empty(area);

    render_editor(&model, &keymap, &pres, area, &mut buf);

    let rendered_text = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered_text.contains("[SPLIT: VERTICAL]"),
        "Must render split mode capsule in tab bar:\n{rendered_text}"
    );
    assert!(
        rendered_text.contains("│") || rendered_text.contains("|"),
        "Must render split vertical separator:\n{rendered_text}"
    );
}

fn recovery_model(buffer_count: usize) -> Model {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("SELECT 1;");
    model.sync_active_buffer_from_editor();
    for index in 2..=buffer_count {
        model
            .buffers
            .new_buffer(Some(format!("query-{index}.sql")), Some("SELECT 2;".into()));
    }
    for buffer in &mut model.buffers.buffers {
        buffer.file_path = None;
        buffer.editor.set_cursor(1);
        buffer.editor.insert(' ');
    }
    model.buffers.active_idx = 0;
    model.sync_editor_from_active_buffer();
    let journal =
        crate::recovery::RecoveryJournal::from_buffers(&model.buffers, "render-session", false);
    model.recovery_banner = Some(crate::app::model::SessionRecoveryBanner::new(
        journal,
        std::path::PathBuf::from("/tmp/ignatius-recovery"),
    ));
    model
}

fn rendered_rows(buf: &Buffer, area: Rect) -> Vec<String> {
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn recovery_capsule_sits_above_the_tab_bar_with_ascii_wording_and_keys() {
    let model = recovery_model(2);
    let area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(area);
    render_editor(
        &model,
        &Keymap::new(),
        &presentation(ColorDepth::TrueColor),
        area,
        &mut buf,
    );

    let rows = rendered_rows(&buf, area);
    let capsule = rows
        .iter()
        .position(|row| row.contains("CRASH RECOVERY"))
        .expect("recovery capsule is rendered");
    assert!(
        rows[capsule].contains("[CRASH RECOVERY: 2 UNSAVED BUFFERS]"),
        "capsule names the buffer count: {}",
        rows[capsule]
    );
    assert!(
        rows[capsule].contains("Ctrl+K r") && rows[capsule].contains("Ctrl+K d"),
        "capsule advertises both contextual chords: {}",
        rows[capsule]
    );
    assert!(
        rows[capsule].contains("Restore") && rows[capsule].contains("Discard"),
        "capsule spells out both choices: {}",
        rows[capsule]
    );
    let capsule_row = &rows[capsule];
    let start = capsule_row
        .find("[CRASH RECOVERY")
        .expect("capsule opens with its label");
    let end = capsule_row
        .find("Discard")
        .expect("capsule ends with the discard word")
        + "Discard".len();
    assert!(
        capsule_row[start..end].is_ascii(),
        "the capsule is ASCII only: {capsule_row}"
    );
    let tab_bar = rows
        .iter()
        .position(|row| row.contains("[+]"))
        .expect("the tab bar is still rendered");
    assert!(
        capsule < tab_bar,
        "the capsule sits above the buffer tab bar ({capsule} vs {tab_bar})"
    );
}

#[test]
fn recovery_capsule_reports_truncated_buffers_and_singular_counts() {
    let mut model = recovery_model(1);
    if let Some(banner) = model.recovery_banner.as_mut() {
        banner.journal.buffers[0].truncated = true;
    }
    let area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(area);
    render_editor(
        &model,
        &Keymap::new(),
        &presentation(ColorDepth::TrueColor),
        area,
        &mut buf,
    );

    let text = rendered_rows(&buf, area).join("\n");
    assert!(
        text.contains("[CRASH RECOVERY: 1 UNSAVED BUFFER, 1 TRUNCATED]"),
        "single unsaved buffer and the truncation suffix: {text}"
    );
}

#[test]
fn recovery_capsule_uses_the_compact_form_in_a_narrow_pane() {
    let model = recovery_model(2);
    let area = Rect::new(0, 0, 24, 20);
    let mut buf = Buffer::empty(area);
    render_editor(
        &model,
        &Keymap::new(),
        &presentation(ColorDepth::TrueColor),
        area,
        &mut buf,
    );

    let text = rendered_rows(&buf, area).join("\n");
    assert!(
        text.contains("[RECOVERY: 2]"),
        "narrow panes get the compact capsule: {text}"
    );
    assert!(
        text.contains("^K r/d"),
        "the narrowest capsule keeps both letters without clipping a word: {text}"
    );
    assert!(
        !text.contains("CRASH RECOVERY"),
        "the wide capsule is not used below 24 columns: {text}"
    );
}

#[test]
fn recovery_capsule_keeps_both_words_at_a_typical_pane_width() {
    let model = recovery_model(2);
    let area = Rect::new(0, 0, 50, 1);
    let mut buf = Buffer::empty(area);
    render_recovery_capsule(&model, &presentation(ColorDepth::TrueColor), area, &mut buf);

    let text = rendered_rows(&buf, area).join("\n");
    assert!(
        text.contains("Restore") && text.contains("Discard"),
        "a 50-column pane keeps both choices readable: {text}"
    );
    assert!(
        !text.contains("[Ctrl+K r]"),
        "the full key hint is reserved for panes that can hold it: {text}"
    );
}

#[test]
fn recovery_capsule_calls_untouched_tabs_open_rather_than_unsaved() {
    use crate::buffer::BufferManager;

    let mut loaded = BufferManager::new();
    loaded.active_buffer_mut().editor.set_text("SELECT 1;");
    loaded.new_buffer(Some("query-2.sql".into()), Some("SELECT 2;".into()));
    let journal = crate::recovery::RecoveryJournal::from_buffers(&loaded, "loaded", false);

    let mut model = Model::new(100);
    model.recovery_banner = Some(crate::app::model::SessionRecoveryBanner::new(
        journal,
        std::path::PathBuf::from("/tmp/ignatius-recovery"),
    ));

    let area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(area);
    render_editor(
        &model,
        &Keymap::new(),
        &presentation(ColorDepth::TrueColor),
        area,
        &mut buf,
    );

    let text = rendered_rows(&buf, area).join("\n");
    assert!(
        text.contains("[CRASH RECOVERY: 2 OPEN BUFFERS]"),
        "loaded tabs that were never edited are open, not unsaved: {text}"
    );
}

#[test]
fn no_recovery_capsule_is_rendered_without_a_banner() {
    let mut model = recovery_model(2);
    model.recovery_banner = None;
    let area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(area);
    render_editor(
        &model,
        &Keymap::new(),
        &presentation(ColorDepth::TrueColor),
        area,
        &mut buf,
    );

    let text = rendered_rows(&buf, area).join("\n");
    assert!(
        !text.contains("CRASH RECOVERY"),
        "no banner means no capsule: {text}"
    );
}

#[test]
fn recovery_capsule_uses_warning_capsule_for_unsaved_and_focus_pill_for_open() {
    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 100, 1);

    // Unsaved buffers use theme.capsule(Token::Warning) without trailing space bleed.
    let unsaved_model = recovery_model(2);
    let mut unsaved_buf = Buffer::empty(area);
    render_recovery_capsule(&unsaved_model, &pres, area, &mut unsaved_buf);
    let row = rendered_rows(&unsaved_buf, area).pop().unwrap();
    let label = "[CRASH RECOVERY: 2 UNSAVED BUFFERS]";
    let start = row.find(label).unwrap();
    let after = start + label.len();
    let warn_style = pres.theme.capsule(Token::Warning);
    assert_eq!(unsaved_buf[(start as u16, 0)].fg, warn_style.fg.unwrap());
    assert_eq!(unsaved_buf[(start as u16, 0)].bg, warn_style.bg.unwrap());
    assert!(
        unsaved_buf[(start as u16, 0)]
            .modifier
            .contains(Modifier::BOLD)
    );
    assert_eq!(
        unsaved_buf[(after as u16, 0)].bg,
        Color::Reset,
        "trailing space after capsule must not carry capsule background"
    );

    // Untouched open buffers use theme.pill(Token::Focus).
    let mut loaded = crate::buffer::BufferManager::new();
    loaded.active_buffer_mut().editor.set_text("SELECT 1;");
    let journal = crate::recovery::RecoveryJournal::from_buffers(&loaded, "loaded", false);
    let mut open_model = Model::new(100);
    open_model.recovery_banner = Some(crate::app::model::SessionRecoveryBanner::new(
        journal,
        std::path::PathBuf::from("/tmp/ignatius-recovery"),
    ));
    let mut open_buf = Buffer::empty(area);
    render_recovery_capsule(&open_model, &pres, area, &mut open_buf);
    let open_row = rendered_rows(&open_buf, area).pop().unwrap();
    let open_start = open_row.find("[CRASH RECOVERY: 1 OPEN BUFFER]").unwrap();
    let focus_pill = pres.theme.pill(Token::Focus);
    assert_eq!(open_buf[(open_start as u16, 0)].fg, focus_pill.fg.unwrap());
    assert_eq!(open_buf[(open_start as u16, 0)].bg, focus_pill.bg.unwrap());
    assert!(
        open_buf[(open_start as u16, 0)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn sixteen_tab_narrow_bar_keeps_active_tab_and_new_tab_button_visible() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("SELECT 1;");
    model.sync_active_buffer_from_editor();
    for i in 2..=crate::buffer::MAX_EDITOR_BUFFERS {
        model
            .buffers
            .new_buffer(Some(format!("q-{i}.sql")), Some(format!("SELECT {i};")));
    }
    model.buffers.active_idx = crate::buffer::MAX_EDITOR_BUFFERS - 1;
    model.sync_editor_from_active_buffer();

    let pres = presentation(ColorDepth::TrueColor);
    let area = Rect::new(0, 0, 56, 20);
    let mut buf = Buffer::empty(area);
    render_editor(&model, &Keymap::new(), &pres, area, &mut buf);

    let rows = rendered_rows(&buf, area);
    let border_row = &rows[0];
    assert!(
        !border_row.contains("[BUFFERS: 16/16]"),
        "border title must not duplicate tab bar [BUFFERS: 16/16]: {border_row}"
    );
    let tab_row = rows
        .iter()
        .find(|r| r.contains("[BUFFERS: 16/16]"))
        .expect("tab bar row present");
    assert!(
        tab_row.contains("< "),
        "left overflow marker must appear when active_idx is near the end: {tab_row}"
    );
    assert!(
        tab_row.contains("[16: q-16.sql]"),
        "active tab near end must remain visible: {tab_row}"
    );
    assert!(
        tab_row.contains("[+]"),
        "new-tab [+] indicator must remain visible without clipping: {tab_row}"
    );
}

#[test]
fn split_secondary_pane_renders_badge_when_buffer_has_sql_text() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("SELECT 'alpha';");
    model.sync_active_buffer_from_editor();

    model
        .buffers
        .new_buffer(Some("right.sql".into()), Some("SELECT 'beta';".into()));
    model.sync_editor_from_active_buffer();
    model.buffers.secondary_idx = Some(0);

    let pres = presentation(ColorDepth::TrueColor);
    for split_mode in [
        crate::buffer::EditorSplitMode::Vertical,
        crate::buffer::EditorSplitMode::Horizontal,
    ] {
        model.buffers.split_mode = split_mode;
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        render_editor(&model, &Keymap::new(), &pres, area, &mut buf);

        let text = rendered_rows(&buf, area).join("\n");
        assert!(
            text.contains("[SPLIT 1: buffer-1.sql]"),
            "non-empty secondary pane in {split_mode:?} split must show split badge:\n{text}"
        );
        assert!(
            text.contains("SELECT 'alpha';"),
            "non-empty secondary SQL text must remain visible:\n{text}"
        );
    }
}

#[test]
fn editor_split_header_vertical_gutter_rule_and_completion_ascii_parity() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.focus = crate::app::model::Focus::Editor;
    model.editor.set_text("SELECT 1;\nSELECT 2;");
    model.editor.set_position(1, 1);
    model.completion.menu = Some(CompletionMenu::new(test_completion_result(), true));

    let area = Rect::new(0, 0, 80, 20);
    let unicode = presentation(ColorDepth::TrueColor);
    let mut ubuf = Buffer::empty(area);
    render_editor(&model, &Keymap::new(), &unicode, area, &mut ubuf);
    render_completion(&model, &unicode, area, &mut ubuf);

    let rows = rendered_rows(&ubuf, area);
    assert!(
        rows[0].contains('\u{258d}'),
        "focus accent leader missing: {}",
        rows[0]
    );
    assert!(rows[0].contains("Editor"), "{}", rows[0]);
    assert!(rows[0].contains("line 1, column 1"), "{}", rows[0]);
    assert_eq!(ubuf[(2, 1)].fg, unicode.theme.color(Token::Focus).unwrap());
    assert_eq!(
        ubuf[(2, 2)].fg,
        unicode.theme.color(Token::Muted).unwrap(),
        "inactive line number must use Token::Muted"
    );
    assert_eq!(ubuf[(4, 2)].symbol(), "\u{2502}");
    assert_eq!(ubuf[(4, 2)].fg, unicode.theme.color(Token::Border).unwrap());

    for depth in [ColorDepth::Basic16, ColorDepth::None] {
        let ascii = Presentation::new(
            Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
            Glyphs::new(GlyphTier::Ascii),
            false,
        );
        let mut abuf = Buffer::empty(area);
        render_editor(&model, &Keymap::new(), &ascii, area, &mut abuf);
        render_completion(&model, &ascii, area, &mut abuf);
        let arows = rendered_rows(&abuf, area);
        let joined = arows.join("\n");
        assert!(joined.is_ascii(), "{depth:?}: {joined:?}");
        assert!(arows[0].contains("> Editor"), "{}", arows[0]);
        assert_eq!(abuf[(4, 2)].symbol(), "|");
        assert_eq!(abuf[(4, 2)].bg, Color::Reset);
        assert!(
            joined.contains("----"),
            "ASCII completion rules must use dashes: {joined}"
        );
    }
}

#[test]
fn completion_displays_kind_icons_and_empty_editor_renders_editorial_keycaps() {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connected(session());
    model.editor.set_text("SELECT ");
    model.completion.menu = Some(CompletionMenu::new(test_completion_result(), true));

    let nerd = Presentation::new(
        Theme::new(ThemeChoice::Atelier, true).with_depth(ColorDepth::TrueColor),
        Glyphs::new(GlyphTier::Nerd),
        false,
    );
    let area = Rect::new(0, 0, 88, 22);
    let mut cbuf = Buffer::empty(area);
    render_editor(&model, &Keymap::new(), &nerd, area, &mut cbuf);
    render_completion(&model, &nerd, area, &mut cbuf);
    let ctext = buffer_to_string(&cbuf);

    let table_icon = nerd.icon(crate::ui::glyphs::Icon::Table);
    let fn_icon = nerd.icon(crate::ui::glyphs::Icon::Function);
    let col_icon = nerd.icon(crate::ui::glyphs::Icon::Column);
    assert!(
        ctext.contains(&format!("{table_icon}users")),
        "missing table kind icon in completion menu:\n{ctext}"
    );
    assert!(
        ctext.contains(&format!("{fn_icon}now")),
        "missing function kind icon in completion menu:\n{ctext}"
    );
    assert!(
        ctext.contains(&format!("{col_icon}id")),
        "missing column kind icon in completion menu:\n{ctext}"
    );

    let mut empty_model = Model::new(100);
    empty_model.connection = ConnectionState::Connected(session());
    let mut ebuf = Buffer::empty(area);
    render_editor(&empty_model, &Keymap::new(), &nerd, area, &mut ebuf);
    let etext = buffer_to_string(&ebuf);
    assert!(
        etext.contains("\u{2500}\u{2500}\u{2500}\u{2500}"),
        "{etext}"
    );

    let run_pos = (0..area.height)
        .find_map(|y| {
            let line: String = (0..area.width).map(|x| ebuf[(x, y)].symbol()).collect();
            line.find("Ctrl+R").map(|x| (x as u16, y))
        })
        .expect("Ctrl+R starter keycap");
    assert_eq!(
        ebuf[run_pos].bg,
        nerd.theme.surface(Token::SurfaceOverlay).bg.unwrap(),
        "empty editor starter keycap must elevate onto SurfaceOverlay"
    );

    let ascii = Presentation::new(
        Theme::new(ThemeChoice::Dark, false).with_depth(ColorDepth::None),
        Glyphs::new(GlyphTier::Ascii),
        false,
    );
    let mut abuf = Buffer::empty(area);
    render_editor(&empty_model, &Keymap::new(), &ascii, area, &mut abuf);
    let atext = buffer_to_string(&abuf);
    assert!(atext.is_ascii(), "{atext:?}");
    assert!(atext.contains("Start here"), "{atext}");
    assert!(
        atext.contains("Ctrl+R Run the buffer once SQL is ready."),
        "{atext}"
    );
}
