use super::*;
use crate::app::PlanView;
use crate::app::model::{NamePrompt, UpdateCandidate, UpdatePrompt};
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

    // Metrics after " | " are styled with Token::Muted
    let metric_idx = line2[..line2.find("est rows").unwrap()].chars().count() as u16;
    assert_eq!(buf_tc[(metric_idx, 2)].fg, muted_fg);

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
