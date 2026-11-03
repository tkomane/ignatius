//! Focused tests for the editor completion popup and guidance placeholders.

use super::{render_completion, render_editor};
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
use crate::ui::layout::Presentation;
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

    // Candidate 0 (SELECT, selected) is on row popup_y + 2
    let sel_y = popup_y + 2;
    for x in (popup_left + 1)..popup_right {
        assert_eq!(
            buf[(x, sel_y)].bg,
            sel_bg,
            "x={x} on selected candidate row must carry selection background"
        );
    }

    // Candidate 1 (users, Table, unselected) is on row popup_y + 3
    let table_y = popup_y + 3;
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

    // Candidate 2 (now, Function, unselected) is on row popup_y + 4 -> Token::SyntaxNumber
    let fn_y = popup_y + 4;
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

    // Candidate 3 (id, Column, unselected) is on row popup_y + 5 -> Token::Text
    let col_y = popup_y + 5;
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

    // Candidate 4 (cte_data, Cte, unselected) is on row popup_y + 6 -> Token::Focus
    let cte_y = popup_y + 6;
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
