//! Drawing the full-screen interface.
//!
//! Rendering is a pure function of the model: same model, same pixels. Nothing
//! here reads a clock, a file, or a connection. Even the animated indicators are
//! pure, because the frame counter and the elapsed time arrive in the model.
//!
//! Four rules shape every decision below:
//!
//! - Every state that colour or an icon expresses is also written in words, so
//!   the interface survives `NO_COLOR`, an ASCII terminal, a font without icons,
//!   and colour-blind vision. Decoration is added on top of a readable screen,
//!   never in place of one.
//! - A narrow terminal gets a deliberately different layout, not a clipped one.
//! - Nothing from the database is drawn without passing through
//!   [`crate::query::value::sanitize_for_display`].
//! - Motion is optional. With `ui.reduced-motion` every indicator becomes static
//!   text that says the same thing.

use crate::app::model::{Focus, Model};
use crate::query::value::display_width;
use crate::ui::keymap::Keymap;
use crate::ui::theme::Token;
use crate::ui::widgets::{
    render_chords, render_compact_header, render_completion, render_confirmation,
    render_connection_details, render_copy_confirmation, render_definition, render_editor,
    render_error, render_footer, render_header, render_help, render_inspector, render_name_prompt,
    render_objects, render_palette, render_parameter_prompt, render_password_prompt,
    render_plan_confirmation, render_results, render_update_review, render_update_value_prompt,
};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};

#[cfg(test)]
use crate::app::model::QueryPhase;
#[cfg(test)]
use crate::ui::widgets::overlays::plan_metrics;
#[cfg(test)]
use crate::ui::widgets::results::column_alignment;

pub use crate::app::model::Presentation;

/// Which layout the available space allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Editor and results together, with full status.
    Full,
    /// Only the focused pane, with a shortened status line.
    Compact,
    /// Too small to be useful; a message is shown instead.
    TooSmall,
}

/// Chooses a layout for the available space.
#[must_use]
pub const fn layout_mode(area: Rect) -> LayoutMode {
    if area.width < super::USABLE_COLUMNS || area.height < super::USABLE_ROWS {
        LayoutMode::TooSmall
    } else if area.width < super::MIN_COLUMNS || area.height < super::MIN_ROWS {
        LayoutMode::Compact
    } else {
        LayoutMode::Full
    }
}

/// Draws the whole interface using the presentation the model owns.
pub fn render(model: &Model, keymap: &Keymap, area: Rect, buf: &mut Buffer) {
    render_with(model, keymap, &model.presentation, area, buf);
}

/// Draws the whole interface with an explicit presentation.
///
/// The runtime goes through [`render`]; this is the seam that keeps the
/// pre-existing test helper able to pass a chosen presentation without the
/// model having to carry it.
fn render_with(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    // The frame owns its background: at depths that paint surfaces this cell
    // fill is the base elevation level, so no cell shows the terminal's own
    // background. At 16-colour depth and below it collapses to the existing
    // foreground-only style.
    Block::new()
        .style(
            presentation
                .theme
                .surface(Token::Surface)
                .patch(presentation.theme.style(Token::Text)),
        )
        .render(area, buf);

    match layout_mode(area) {
        LayoutMode::TooSmall => render_too_small(presentation, area, buf),
        LayoutMode::Compact => render_compact(model, keymap, presentation, area, buf),
        LayoutMode::Full => render_full(model, keymap, presentation, area, buf),
    }

    // Overlays are drawn in the order Esc peels them, so the topmost one is
    // always the one a keypress will act on.
    if model.completion.menu.is_some() {
        render_completion(model, presentation, area, buf);
    }
    if model.help_open {
        render_help(keymap, presentation, area, buf);
    }
    if let Some(inspector) = &model.inspector {
        render_inspector(model, *inspector, presentation, area, buf);
    }
    if let Some(definition) = &model.definition {
        render_definition(definition, presentation, area, buf);
    }
    if let Some(palette) = &model.palette {
        render_palette(palette, presentation, area, buf);
    }
    if model.connection_details {
        render_connection_details(model, presentation, area, buf);
    }
    if model.prefix_pending {
        render_chords(keymap, presentation, area, buf);
    }
    if let Some(pending) = &model.pending_run {
        render_confirmation(model, pending, presentation, area, buf);
    }
    if let Some(pending) = &model.pending_plan {
        render_plan_confirmation(model, pending, presentation, area, buf);
    }
    if let Some(pending) = &model.pending_copy {
        render_copy_confirmation(model, pending, presentation, area, buf);
    }
    if let Some(pending) = &model.pending_update {
        render_update_review(pending, presentation, area, buf);
    }
    if let Some(prompt) = &model.update_prompt {
        render_update_value_prompt(prompt, presentation, area, buf);
    }
    if let Some(prompt) = &model.parameter_prompt {
        render_parameter_prompt(prompt, presentation, area, buf);
    }
    if let Some(prompt) = &model.name_prompt {
        render_name_prompt(prompt, presentation, area, buf);
    }
    // Topmost, because nothing else can be done until the connection is open.
    if let Some(prompt) = &model.password_prompt {
        render_password_prompt(prompt, presentation, area, buf);
    }
}

/// How wide the object tree should be.
#[must_use]
pub const fn sidebar_width(available: u16) -> u16 {
    let proportional = available * 2 / 5;
    if proportional < 24 {
        24
    } else if proportional > 34 {
        34
    } else {
        proportional
    }
}

/// Where the editor and the results sit inside the body of the full layout.
///
/// One function, used by the renderer and by the reducer that needs to know how
/// big a screenful is, so a page key moves by what the user can actually see.
pub(crate) fn main_panes(body: Rect, sidebar_visible: bool) -> (Rect, Rect) {
    let main = if sidebar_visible {
        let width = sidebar_width(body.width);
        let [_, main] =
            Layout::horizontal([Constraint::Length(width), Constraint::Min(30)]).areas(body);
        main
    } else {
        body
    };
    let [editor, results] =
        Layout::vertical([Constraint::Percentage(40), Constraint::Min(3)]).areas(main);
    (editor, results)
}

/// The body of the layout: everything between the header and the footer.
pub(crate) fn body_area(area: Rect) -> Rect {
    let [_, body, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(area);
    body
}

/// How many lines of SQL are on screen at a terminal size.
#[must_use]
pub fn editor_page(size: (u16, u16)) -> usize {
    let area = Rect::new(0, 0, size.0, size.1);
    match layout_mode(area) {
        // Compact shows one pane at a time, so a page is the whole body.
        LayoutMode::Compact => (body_area(area).height as usize).saturating_sub(2).max(1),
        LayoutMode::TooSmall => 1,
        LayoutMode::Full => {
            let (editor, _) = main_panes(body_area(area), false);
            (editor.height as usize).saturating_sub(2).max(1)
        }
    }
}

/// How many result rows are on screen at a terminal size.
#[must_use]
pub fn results_page(size: (u16, u16)) -> usize {
    let area = Rect::new(0, 0, size.0, size.1);
    match layout_mode(area) {
        LayoutMode::Compact => (body_area(area).height as usize).saturating_sub(6).max(1),
        LayoutMode::TooSmall => 1,
        LayoutMode::Full => {
            let (_, results) = main_panes(body_area(area), false);
            // Two rows of the pane are the border. The grid reserves a compact
            // state toolbar, a breathing line, the header, its optional type
            // row, and its rule. Keeping one extra row reserved means paging
            // never advances past what a metadata-bearing grid can show.
            (results.height as usize).saturating_sub(6).max(1)
        }
    }
}

fn render_full(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(area);

    // The tree takes a fixed share of the width rather than a proportional one,
    // because object names are the same length whatever the window is.
    if model.sidebar_visible {
        let width = sidebar_width(body.width);
        let [sidebar, _] =
            Layout::horizontal([Constraint::Length(width), Constraint::Min(30)]).areas(body);
        render_objects(model, keymap, presentation, sidebar, buf);
    }

    let (editor, results) = main_panes(body, model.sidebar_visible);

    render_header(model, presentation, header, buf);
    render_editor(model, keymap, presentation, editor, buf);
    if model.transaction == crate::query::result::TransactionState::Failed {
        render_results(model, keymap, presentation, results, buf);
    } else if let Some(error) = &model.error {
        render_error(model, error, presentation, results, buf);
    } else {
        render_results(model, keymap, presentation, results, buf);
    }
    render_footer(model, keymap, presentation, footer, buf);
}

/// The narrow layout shows one pane at a time rather than clipping both.
fn render_compact(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_compact_header(model, presentation, header, buf);
    if model.transaction == crate::query::result::TransactionState::Failed {
        render_results(model, keymap, presentation, body, buf);
    } else if let Some(error) = &model.error {
        // Keep the source visible while the error is being read. This is the
        // compact equivalent of an editor and diagnostic split, so a valid
        // location feels like navigation rather than a modal interruption.
        let error_height = body.height.saturating_sub(4).clamp(2, 8);
        let [error_area, editor_area] =
            Layout::vertical([Constraint::Length(error_height), Constraint::Min(1)]).areas(body);
        render_error(model, error, presentation, error_area, buf);
        render_editor(model, keymap, presentation, editor_area, buf);
    } else {
        match model.focus {
            Focus::Editor => render_editor(model, keymap, presentation, body, buf),
            Focus::Results => render_results(model, keymap, presentation, body, buf),
            Focus::Objects => render_objects(model, keymap, presentation, body, buf),
        }
    }
    render_footer(model, keymap, presentation, footer, buf);
}

fn render_too_small(presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    // This message appears in the smallest windows the product ever sees, so it
    // is written to survive them: three short lines, no wrapping, and the way out
    // on a line of its own.
    let text = vec![
        Line::from("Terminal too small"),
        Line::from(format!(
            "Now {}x{}, need {}x{}",
            area.width,
            area.height,
            super::MIN_COLUMNS,
            super::MIN_ROWS
        )),
        Line::from(format!("Run: {} query", crate::branding::BINARY_NAME)),
    ];
    Paragraph::new(text)
        .style(presentation.theme.style(Token::Text))
        .render(area, buf);
}

// ---------------------------------------------------------------- inspector

/// Widest the inspector is ever drawn, whatever the terminal allows.
const INSPECTOR_MAX_WIDTH: u16 = 92;
/// Tallest the inspector is ever drawn.
const INSPECTOR_MAX_HEIGHT: u16 = 26;
/// Columns the frame and its padding take from the value.
const INSPECTOR_CHROME_COLUMNS: u16 = 4;
/// Rows the frame, the heading and the position line take from the value.
const INSPECTOR_CHROME_ROWS: u16 = 5;

/// The box the inspector occupies inside an area.
pub(crate) const fn inspector_box(area: Rect) -> Rect {
    let mut width = area.width.saturating_sub(6);
    if width > INSPECTOR_MAX_WIDTH {
        width = INSPECTOR_MAX_WIDTH;
    }
    let mut height = area.height.saturating_sub(4);
    if height > INSPECTOR_MAX_HEIGHT {
        height = INSPECTOR_MAX_HEIGHT;
    }
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// How much of a value the inspector can show at a terminal size: width in
/// cells, height in lines.
///
/// The reducer clamps scrolling with this and the renderer lays out with it, so
/// "how much fits" has one answer rather than two that drift apart.
#[must_use]
pub const fn inspector_viewport(size: (u16, u16)) -> (usize, usize) {
    let box_area = inspector_box(Rect {
        x: 0,
        y: 0,
        width: size.0,
        height: size.1,
    });
    (
        box_area.width.saturating_sub(INSPECTOR_CHROME_COLUMNS) as usize,
        box_area.height.saturating_sub(INSPECTOR_CHROME_ROWS) as usize,
    )
}

// --------------------------------------------------------------- definition

/// Rows the frame, the heading and the note take from a definition.
const DEFINITION_CHROME_ROWS: u16 = 5;

/// The box a definition occupies inside an area.
///
/// Wider and taller than the value inspector, because a table definition is
/// read as a whole rather than scanned for one value.
pub(crate) const fn definition_box(area: Rect) -> Rect {
    let mut width = area.width.saturating_sub(4);
    if width > 100 {
        width = 100;
    }
    let mut height = area.height.saturating_sub(2);
    if height > 34 {
        height = 34;
    }
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// How much of a definition is on screen: width in cells, height in lines.
///
/// One function for the renderer and for the reducer that clamps scrolling.
#[must_use]
pub const fn definition_viewport(size: (u16, u16)) -> (usize, usize) {
    let box_area = definition_box(Rect {
        x: 0,
        y: 0,
        width: size.0,
        height: size.1,
    });
    (
        box_area.width.saturating_sub(4) as usize,
        box_area.height.saturating_sub(DEFINITION_CHROME_ROWS) as usize,
    )
}

// ------------------------------------------------------------------- testing

/// Renders a buffer to plain text, for tests and for support bundles.
#[must_use]
pub fn buffer_to_string(buf: &Buffer) -> String {
    let area = buf.area();
    let mut out = String::new();
    for y in 0..area.height {
        let mut x = 0u16;
        while x < area.width {
            let symbol = buf[(area.x + x, area.y + y)].symbol();
            out.push_str(symbol);
            // A wide character occupies one cell and reserves the next one. The
            // reserved cell holds a placeholder, so stepping over it is what
            // keeps the reconstructed text the same width as the real screen.
            x += u16::try_from(display_width(symbol).max(1)).unwrap_or(1);
        }
        out.push('\n');
    }
    out
}

/// Convenience for tests: renders a model at a size and returns the text.
#[must_use]
pub fn render_to_string(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    width: u16,
    height: u16,
) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    render_with(model, keymap, presentation, area, &mut buf);
    buffer_to_string(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::{ConnectionState, Model};
    use crate::config::ThemeChoice;
    use crate::connection::Environment;
    use crate::postgres::{SessionInfo, TlsState};
    use crate::query::result::{Execution, ExecutionStatus, JobId, ResultSet, StatementResult};
    use crate::query::value::Cell;
    use crate::ui::glyphs::{GlyphTier, Glyphs};
    use crate::ui::theme::Theme;
    use std::time::Duration;

    /// Keeps the two pre-existing tests that render a buffer colour-exact on
    /// their original call shape: they choose a presentation explicitly rather
    /// than reading the model's.
    fn render(
        model: &Model,
        keymap: &Keymap,
        presentation: &Presentation,
        area: Rect,
        buf: &mut Buffer,
    ) {
        super::render_with(model, keymap, presentation, area, buf);
    }

    fn presentation(theme: ThemeChoice, color: bool, tier: GlyphTier) -> Presentation {
        Presentation::new(Theme::new(theme, color), Glyphs::new(tier), false)
    }

    fn presentation_at_depth(
        theme: ThemeChoice,
        color: bool,
        tier: GlyphTier,
        depth: crate::ui::theme::ColorDepth,
    ) -> Presentation {
        Presentation::new(
            Theme::new(theme, color).with_depth(depth),
            Glyphs::new(tier),
            false,
        )
    }

    /// Renders a frame to a buffer so tests can inspect cell styles, not just
    /// the words.
    fn rendered_buffer(
        model: &Model,
        presentation: &Presentation,
        width: u16,
        height: u16,
    ) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        render_with(model, &Keymap::new(), presentation, area, &mut buf);
        buf
    }

    /// The richest presentation: colour, icons, motion.
    fn rich() -> Presentation {
        presentation(ThemeChoice::Dark, true, GlyphTier::Nerd)
    }

    fn session(environment: Environment, tls: TlsState) -> Box<SessionInfo> {
        Box::new(SessionInfo {
            target: "app@localhost:5432/orders".into(),
            database: "orders".into(),
            user: "app".into(),
            server_version: "18.6".into(),
            backend_pid: "42".into(),
            search_path: "public".into(),
            read_only: false,
            tls,
            environment,
        })
    }

    fn connected_model(environment: Environment) -> Model {
        let mut model = Model::new(100);
        model.connection = ConnectionState::Connected(session(environment, TlsState::Disabled));
        model.editor.set_text("SELECT 1;");
        model
    }

    #[test]
    fn completion_menu_is_cursor_adjacent_and_readable_in_ascii_mode() {
        let mut model = connected_model(Environment::Local);
        model.completion.catalog = crate::app::completion::CatalogStatus::Ready {
            catalog: crate::query::completion::CompletionCatalog {
                objects: vec![crate::query::completion::CatalogObject {
                    kind: crate::query::completion::CatalogObjectKind::Table,
                    schema: "public".into(),
                    name: "orders".into(),
                    readable: true,
                    detail: None,
                }],
                relations: Vec::new(),
            },
            loaded_at: "2026-09-04 10:00:00 +02:00".into(),
        };
        model.editor.set_text("SELECT * FROM ord");
        crate::app::update::update(
            &mut model,
            crate::app::Message::Action(crate::app::Action::Complete),
        );

        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            100,
            30,
        );
        assert!(text.contains("Complete"), "{text}");
        assert!(text.contains("orders"), "{text}");
        assert!(text.contains("table"), "{text}");
        assert!(text.contains("Enter accepts"), "{text}");
        assert!(
            text.is_ascii(),
            "ASCII mode must not need Unicode: {text:?}"
        );
    }

    fn with_rows(model: &mut Model, columns: &[&str], rows: &[&[Cell]]) {
        let mut set = ResultSet::new(columns.iter().map(|c| (*c).to_owned()).collect(), 100);
        for row in rows {
            set.push((*row).to_vec());
        }
        model.last_execution = Some(Execution {
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(rows.len() as u64),
                elapsed: Duration::from_millis(87),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(90),
            error: None,

            transaction: crate::query::result::TransactionState::Autocommit,
        });
    }

    fn update_relation() -> crate::postgres::metadata::UpdateRelation {
        crate::postgres::metadata::UpdateRelation {
            schema: "public".into(),
            relation: "orders".into(),
            kind: crate::postgres::metadata::ObjectKind::Table,
            readable: true,
            writable: true,
            columns: vec![
                crate::postgres::metadata::ColumnInfo {
                    name: "order_id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    primary_key: true,
                    default: None,
                },
                crate::postgres::metadata::ColumnInfo {
                    name: "note".into(),
                    data_type: "text".into(),
                    nullable: true,
                    primary_key: false,
                    default: None,
                },
            ],
        }
    }

    fn update_candidate() -> crate::app::model::UpdateCandidate {
        crate::app::model::UpdateCandidate {
            result_job: JobId(1),
            source_row: 0,
            result_column: 1,
            source_sql: "SELECT order_id, note FROM public.orders".into(),
        }
    }

    fn update_source() -> crate::query::UpdateSource {
        crate::query::parse_update_source(
            "SELECT order_id, note FROM public.orders",
            &["order_id".into(), "note".into()],
            1,
        )
        .expect("direct update source")
    }

    fn update_plan() -> crate::query::UpdatePlan {
        crate::query::plan_update(
            &update_source(),
            "public",
            "orders",
            &["order_id".into(), "note".into()],
            &[Cell::Text("7".into()), Cell::Text("old".into())],
            &[
                crate::query::UpdateColumn {
                    name: "order_id".into(),
                    primary_key: true,
                },
                crate::query::UpdateColumn {
                    name: "note".into(),
                    primary_key: false,
                },
            ],
            "new note",
        )
        .expect("update plan")
    }

    #[test]
    fn cell_update_value_prompt_names_literal_semantics_and_review_boundary() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        model.last_sql = Some("SELECT order_id, note FROM public.orders".into());
        with_rows(
            &mut model,
            &["order_id", "note"],
            &[&[Cell::Text("7".into()), Cell::Text("old".into())]],
        );
        model.update_prompt = Some(crate::app::model::UpdatePrompt::new(
            update_candidate(),
            update_source(),
            update_relation(),
        ));
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("Edit result cell"), "{text}");
        assert!(text.contains("Replacement"), "{text}");
        assert!(text.contains("empty string"), "{text}");
        assert!(text.contains("literal text"), "{text}");
        assert!(text.contains("Nothing is sent"), "{text}");
    }

    #[test]
    fn cell_update_review_shows_the_exact_bound_statement_and_no_send_claim() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        model.last_sql = Some("SELECT order_id, note FROM public.orders".into());
        with_rows(
            &mut model,
            &["order_id", "note"],
            &[&[Cell::Text("7".into()), Cell::Text("old".into())]],
        );
        let plan = update_plan();
        model.pending_update = Some(crate::app::model::PendingUpdate {
            candidate: update_candidate(),
            relation: update_relation(),
            plan,
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("Review generated UPDATE"), "{text}");
        assert!(text.contains("Exact bound statement"), "{text}");
        assert!(text.contains("UPDATE \"public\".\"orders\""), "{text}");
        assert!(text.contains("new note"), "{text}");
        assert!(text.contains("Nothing has been sent yet"), "{text}");
        assert!(text.contains("will not be rerun automatically"), "{text}");
    }

    #[test]
    fn the_full_layout_shows_connection_environment_tls_and_hints() {
        let model = connected_model(Environment::Local);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);

        assert!(text.contains("Ignatius"), "{text}");
        assert!(
            text.contains("[LOCAL]"),
            "environment must be a visible word"
        );
        assert!(text.contains("[read-write]"), "posture must be visible");
        assert!(text.contains("app@localhost:5432/orders"));
        assert!(text.contains("TLS off"), "protection state must be stated");
        assert!(text.contains("Editor"));
        assert!(text.contains("Results"));
        assert!(text.contains("Ready"));
        assert!(text.contains("Ctrl+R"), "the run key must be discoverable");
        assert!(
            text.contains("Ctrl+P"),
            "the command palette must be discoverable"
        );
    }

    #[test]
    fn a_production_connection_is_unmistakable_without_colour_or_icons() {
        let model = connected_model(Environment::Production);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            // The bare tier: no colour, no icons, no Unicode.
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            100,
            30,
        );
        assert!(text.contains("[PROD]"), "production marker missing: {text}");
    }

    #[test]
    fn every_theme_colour_and_glyph_tier_renders_the_same_meaning() {
        let mut model = connected_model(Environment::Production);
        with_rows(&mut model, &["n"], &[&[Cell::Null]]);
        for theme in [
            ThemeChoice::Dark,
            ThemeChoice::Light,
            ThemeChoice::HighContrast,
        ] {
            for depth in [
                crate::ui::theme::ColorDepth::TrueColor,
                crate::ui::theme::ColorDepth::Indexed256,
                crate::ui::theme::ColorDepth::Basic16,
                crate::ui::theme::ColorDepth::None,
            ] {
                for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
                    let color = depth != crate::ui::theme::ColorDepth::None;
                    let text = render_to_string(
                        &model,
                        &Keymap::new(),
                        &presentation_at_depth(theme, color, tier, depth),
                        100,
                        30,
                    );
                    let context = format!("{theme:?}/{depth:?}/{tier:?}");
                    assert!(text.contains("[PROD]"), "{context}");
                    assert!(text.contains("[null]"), "{context}");
                    assert!(text.contains("Ready"), "{context}");
                    assert!(text.contains("Results"), "{context}");
                }
            }
        }
    }

    #[test]
    fn every_cell_is_painted_at_depths_that_paint_surfaces() {
        let mut model = connected_model(Environment::Local);
        with_rows(&mut model, &["id"], &[&[Cell::Text("1".into())]]);

        for theme in [
            ThemeChoice::Dark,
            ThemeChoice::Light,
            ThemeChoice::HighContrast,
        ] {
            for depth in [
                crate::ui::theme::ColorDepth::TrueColor,
                crate::ui::theme::ColorDepth::Indexed256,
            ] {
                let presentation = presentation_at_depth(theme, true, GlyphTier::Nerd, depth);
                let buf = rendered_buffer(&model, &presentation, 100, 30);
                for y in 0..buf.area.height {
                    for x in 0..buf.area.width {
                        let bg = buf[(x, y)].style().bg;
                        assert!(
                            matches!(bg, Some(colour) if colour != ratatui::style::Color::Reset),
                            "{theme:?}/{depth:?}: cell ({x},{y}) shows the terminal background: {bg:?}"
                        );
                    }
                }
            }

            for depth in [
                crate::ui::theme::ColorDepth::Basic16,
                crate::ui::theme::ColorDepth::None,
            ] {
                let presentation = presentation_at_depth(theme, true, GlyphTier::Unicode, depth);
                let buf = rendered_buffer(&model, &presentation, 100, 30);
                for y in 0..buf.area.height {
                    for x in 0..buf.area.width {
                        let bg = buf[(x, y)].style().bg;
                        assert!(
                            matches!(bg, None | Some(ratatui::style::Color::Reset)),
                            "{theme:?}/{depth:?}: cell ({x},{y}) paints a surface it must not: {bg:?}"
                        );
                    }
                }
            }
        }

        // An overlay owns most of the frame and must still leave no cell on the
        // terminal's own background.
        model.help_open = true;
        let presentation = presentation_at_depth(
            ThemeChoice::Dark,
            true,
            GlyphTier::Nerd,
            crate::ui::theme::ColorDepth::TrueColor,
        );
        let buf = rendered_buffer(&model, &presentation, 100, 30);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let bg = buf[(x, y)].style().bg;
                assert!(
                    matches!(bg, Some(colour) if colour != ratatui::style::Color::Reset),
                    "help overlay: cell ({x},{y}) shows the terminal background: {bg:?}"
                );
            }
        }
    }

    #[test]
    fn icons_decorate_words_rather_than_replacing_them() {
        let mut model = connected_model(Environment::Production);
        with_rows(&mut model, &["id"], &[&[Cell::Text("1".into())]]);
        let decorated = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        let bare = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, GlyphTier::Ascii),
            100,
            30,
        );

        for word in ["Ignatius", "[PROD]", "Editor", "Results", "Ready", "Ctrl+R"] {
            assert!(bare.contains(word), "{word} missing from the bare tier");
            assert!(
                decorated.contains(word),
                "{word} lost when icons were added"
            );
        }
    }

    #[test]
    fn ascii_mode_emits_no_characters_outside_ascii() {
        let mut model = connected_model(Environment::Local);
        with_rows(
            &mut model,
            &["id", "note"],
            &[
                &[Cell::Text("1".into()), Cell::Text("first".into())],
                &[Cell::Text("2".into()), Cell::Null],
            ],
        );
        model.focus = Focus::Results;
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, GlyphTier::Ascii),
            100,
            30,
        );
        assert!(
            text.is_ascii(),
            "non-ASCII reached ASCII mode: {:?}",
            text.chars().filter(|c| !c.is_ascii()).collect::<String>()
        );
    }

    #[test]
    fn results_show_row_numbers_headers_values_and_a_count() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(
            &mut model,
            &["customer_id", "total"],
            &[
                &[Cell::Text("10482".into()), Cell::Text("1245.00".into())],
                &[Cell::Text("10483".into()), Cell::Null],
            ],
        );
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);

        assert!(text.contains("customer_id"), "{text}");
        assert!(text.contains("10482"));
        assert!(text.contains("1245.00"));
        assert!(text.contains("2 rows"), "the row count must be visible");
        assert!(text.contains("87 ms"), "elapsed time must be visible");
        assert!(
            text.contains("[null]"),
            "NULL must be distinguishable: {text}"
        );

        // The row number is drawn in a gutter to the left of the first value.
        let grid_line = text
            .lines()
            .find(|l| l.contains("10482"))
            .expect("the first row is drawn");
        let number_at = grid_line.find(" 1 ").expect("row number drawn");
        let value_at = grid_line.find("10482").expect("value drawn");
        assert!(
            number_at < value_at,
            "the row number must precede the row: {grid_line:?}"
        );
    }

    #[test]
    fn copy_confirmation_names_the_cell_and_size_without_rendering_the_value() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        model.clipboard_osc52 = true;
        with_rows(
            &mut model,
            &["id", "secret_note"],
            &[&[
                Cell::Text("1".into()),
                Cell::Text("sensitive-result".into()),
            ]],
        );
        model.selected_column = 1;
        crate::app::update::update(
            &mut model,
            crate::app::Message::Action(crate::app::Action::CopyValue),
        );

        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("Confirm copy"), "{text}");
        assert!(text.contains("Cell: row 1"), "{text}");
        assert!(text.contains("column 2 (secret_note)"), "{text}");
        assert!(text.contains("16 UTF-8 bytes, 16 characters"), "{text}");
        assert!(text.contains("terminal clipboard sequence"), "{text}");
        assert!(
            text.contains("never reads or clears the clipboard"),
            "{text}"
        );
        assert!(text.contains("acceptance is unconfirmed"), "{text}");
        assert!(text.contains("Enter to send once"), "{text}");
        assert!(
            !text.contains("sensitive-result"),
            "the confirmation must not display the raw value: {text}"
        );

        let ascii = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            120,
            34,
        );
        assert!(ascii.is_ascii(), "{ascii:?}");
        assert!(ascii.contains("Confirm copy"), "{ascii}");
        assert!(ascii.contains("16 UTF-8 bytes"), "{ascii}");
    }

    #[test]
    fn clipboard_outcomes_are_visible_without_claiming_acceptance() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(
            &mut model,
            &["id", "value"],
            &[&[Cell::Text("1".into()), Cell::Text("ready".into())]],
        );
        for (notice, needle) in [
            (crate::app::model::ClipboardNotice::Disabled, "Copy is off"),
            (
                crate::app::model::ClipboardNotice::Null,
                "SQL NULL has no text value",
            ),
            (
                crate::app::model::ClipboardNotice::TooLarge { bytes: 2_000_000 },
                "over the 1 MiB copy limit",
            ),
            (
                crate::app::model::ClipboardNotice::Stale,
                "selected result changed",
            ),
            (
                crate::app::model::ClipboardNotice::Sent {
                    bytes: 5,
                    characters: 5,
                },
                "clipboard acceptance is unconfirm",
            ),
        ] {
            model.clipboard_notice = Some(notice.clone());
            let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
            assert!(text.contains(needle), "{notice:?}: {text}");
        }
    }

    #[test]
    fn format_outcomes_are_visible_in_the_editor_status_rail() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Editor;
        for (notice, needle) in [
            (
                crate::app::model::FormatNotice::Applied {
                    before_lines: 1,
                    after_lines: 4,
                },
                "Formatted SQL",
            ),
            (
                crate::app::model::FormatNotice::AlreadyFormatted,
                "already formatted",
            ),
            (crate::app::model::FormatNotice::Empty, "no SQL to format"),
            (
                crate::app::model::FormatNotice::Refused {
                    message: "Format skipped: close the string literal and try again.".into(),
                },
                "Format skipped",
            ),
        ] {
            model.format_notice = Some(notice);
            for (presentation, width, height) in [
                (rich(), 120, 30),
                (
                    presentation(ThemeChoice::Dark, true, GlyphTier::Unicode),
                    60,
                    20,
                ),
                (
                    presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
                    40,
                    12,
                ),
                (
                    Presentation::new(
                        Theme::new(ThemeChoice::HighContrast, false),
                        Glyphs::new(GlyphTier::Ascii),
                        true,
                    ),
                    40,
                    12,
                ),
            ] {
                let text = render_to_string(&model, &Keymap::new(), &presentation, width, height);
                assert!(text.contains(needle), "{needle}: {text}");
                if width == 40 {
                    assert!(
                        text.is_ascii(),
                        "ASCII format notice leaked glyphs: {text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn result_grid_shows_truthful_types_and_unavailable_metadata() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(
            &mut model,
            &["id", "label", "payload"],
            &[&[
                Cell::Text("7".into()),
                Cell::Text("ready".into()),
                Cell::Null,
            ]],
        );
        model.last_execution.as_mut().expect("execution").statements[0]
            .result_set
            .as_mut()
            .expect("result")
            .set_column_types(vec![Some("int4".into()), Some("text".into()), None]);

        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("int4"), "the available type is shown: {text}");
        assert!(text.contains("text"), "the second type is shown: {text}");
        assert!(
            text.contains("unavailable"),
            "missing metadata is named rather than inferred: {text}"
        );
        assert!(
            text.contains("types: partial"),
            "the state is explicit: {text}"
        );
    }

    #[test]
    fn result_grid_wide_views_keep_source_identity_and_degrade_to_ascii() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(
            &mut model,
            &["id", "hidden", "name", "unused", "amount", "status"],
            &[
                &[
                    Cell::Text("1".into()),
                    Cell::Text("secret".into()),
                    Cell::Text("Ada".into()),
                    Cell::Text("discard".into()),
                    Cell::Text("9007199254740993".into()),
                    Cell::Text("ready".into()),
                ],
                &[
                    Cell::Text("2".into()),
                    Cell::Text("secret-2".into()),
                    Cell::Text("Grace".into()),
                    Cell::Text("discard-2".into()),
                    Cell::Text("9007199254740992".into()),
                ],
            ],
        );
        model.last_execution.as_mut().expect("execution").statements[0]
            .result_set
            .as_mut()
            .expect("result")
            .set_column_types(vec![
                Some("int4".into()),
                None,
                Some("text".into()),
                None,
                Some("numeric".into()),
                Some("text".into()),
            ]);
        assert!(model.result_grid.toggle_column(1, 6));
        assert!(model.result_grid.toggle_column(3, 6));
        model.result_grid.freeze_first = true;
        model.selected_column = 5;

        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            180,
            24,
        );
        assert!(
            text.is_ascii(),
            "ASCII mode must be entirely ASCII: {text:?}"
        );
        assert!(
            text.contains("shown 4/6 columns"),
            "hidden count is visible: {text}"
        );
        assert!(
            text.contains("freeze first: on"),
            "freeze state is visible: {text}"
        );
        assert!(
            text.contains("Ctrl+K g"),
            "the control path is visible: {text}"
        );
        assert!(
            text.contains("status"),
            "the selected source column is reachable: {text}"
        );
        assert!(
            !text.contains("secret"),
            "a hidden source column was drawn: {text}"
        );
        assert!(
            !text.contains("discard"),
            "another hidden source column was drawn: {text}"
        );
        assert!(
            text.contains("[missing]"),
            "a short row is marked as missing rather than shifted: {text}"
        );

        let narrow = render_to_string(&model, &Keymap::new(), &rich(), 80, 24);
        assert!(
            narrow.contains("status"),
            "a narrow window still reaches the selected column: {narrow}"
        );
    }

    #[test]
    fn hostile_type_labels_cannot_emit_terminal_controls() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(&mut model, &["\x1b[31mname"], &[&[Cell::Text("ok".into())]]);
        model.last_execution.as_mut().expect("execution").statements[0]
            .result_set
            .as_mut()
            .expect("result")
            .set_column_types(vec![Some("text\x1b[2J".into())]);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            100,
            24,
        );
        assert!(!text.contains('\x1b'), "terminal control reached the grid");
    }

    #[test]
    fn result_outcomes_keep_failure_cancellation_and_connection_loss_distinct() {
        for (status, expected) in [
            (ExecutionStatus::Failed, "Failed"),
            (ExecutionStatus::Cancelled, "Query cancelled by server"),
            (
                ExecutionStatus::ConnectionLost,
                "Connection lost - query outcome unknown",
            ),
        ] {
            let mut model = connected_model(Environment::Local);
            model.focus = Focus::Results;
            model.last_execution = Some(Execution {
                job: JobId(1),
                statements: Vec::new(),
                status,
                elapsed: Duration::from_millis(1),
                error: None,
                transaction: crate::query::result::TransactionState::Autocommit,
            });
            let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 24);
            assert!(text.contains(expected), "{expected} missing from {text}");
        }
    }

    #[test]
    fn numeric_columns_are_right_aligned_and_text_columns_are_not() {
        let mut set = ResultSet::new(vec!["n".into(), "label".into()], 100);
        set.push(vec![Cell::Text("1".into()), Cell::Text("one".into())]);
        set.push(vec![Cell::Text("1000".into()), Cell::Text("a".into())]);
        assert_eq!(column_alignment(&set), vec![true, false]);

        let mut mixed = ResultSet::new(vec!["v".into()], 100);
        mixed.push(vec![Cell::Text("1".into())]);
        mixed.push(vec![Cell::Text("n/a".into())]);
        assert_eq!(column_alignment(&mixed), vec![false]);

        let mut nulls = ResultSet::new(vec!["v".into()], 100);
        nulls.push(vec![Cell::Null]);
        assert_eq!(
            column_alignment(&nulls),
            vec![false],
            "a column of NULLs has nothing to align"
        );
    }

    #[test]
    fn a_long_result_scrolls_to_keep_the_selection_visible() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        let mut set = ResultSet::new(vec!["v".into()], 500);
        for i in 0..200 {
            set.push(vec![Cell::Text(format!("row-{i}"))]);
        }
        model.last_execution = Some(Execution {
            job: JobId(1),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(200),
                elapsed: Duration::from_millis(5),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(6),
            error: None,

            transaction: crate::query::result::TransactionState::Autocommit,
        });

        let top = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(top.contains("row-0"), "the first row is visible at the top");

        model.selected_row = 150;
        let scrolled = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(
            scrolled.contains("row-150"),
            "the selected row must be visible"
        );
        assert!(!scrolled.contains("row-0 "), "the window moved with it");
    }

    #[test]
    fn hostile_values_cannot_emit_escape_sequences_into_the_interface() {
        let mut model = connected_model(Environment::Local);
        with_rows(
            &mut model,
            &["note"],
            &[&[Cell::Text("\x1b[2Jgotcha".into())]],
        );
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(!text.contains('\x1b'), "an escape reached the screen");

        let mut model = connected_model(Environment::Local);
        with_rows(&mut model, &["\x1b[31mname"], &[&[Cell::Text("x".into())]]);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(
            !text.contains('\x1b'),
            "an escape in a column name reached the screen"
        );
    }

    #[test]
    fn the_editor_window_follows_the_cursor_through_a_long_buffer() {
        let mut model = connected_model(Environment::Local);
        let text = (1..=200)
            .map(|n| format!("-- line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        model.editor.set_text(text);

        // The cursor is at the end after loading, so the end is what is shown.
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("-- line 200"), "{text}");
        assert!(!text.contains("-- line 1\n"), "the top scrolled away");

        model.editor.move_buffer_start();
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("-- line 1"), "{text}");
        assert!(!text.contains("-- line 200"), "the window came back");

        // Line numbers keep counting from the buffer, not from the window.
        model.editor.move_buffer_end();
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("200"), "{text}");
        assert!(text.contains("-- line 200"), "{text}");
        assert!(
            !text.contains("199 -- line 200"),
            "numbers match their lines"
        );
    }

    #[test]
    fn sql_is_coloured_by_what_it_is_and_the_text_is_unchanged() {
        let sql = "-- a note\nSELECT 42, 'x' FROM \"T\" WHERE id = $1;";
        let mut model = connected_model(Environment::Local);
        model.editor.set_text(sql);

        // Whatever the colouring does, the buffer reads exactly as typed.
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        for fragment in ["-- a note", "SELECT 42, 'x' FROM \"T\" WHERE id = $1;"] {
            assert!(text.contains(fragment), "{text}");
        }

        // And the colours are the ones the kinds ask for.
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        render(&model, &Keymap::new(), &rich(), area, &mut buf);
        let theme = rich().theme;
        let colour_of = |needle: &str| -> ratatui::style::Color {
            let text = buffer_to_string(&buf);
            let line_index = text
                .lines()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("{needle} is not on screen:\n{text}"));
            let line = text.lines().nth(line_index).expect("a line");
            // The line holds multi-byte glyphs, so the screen column is the
            // display width of what precedes the needle, not its byte offset.
            let byte = line.find(needle).expect("a column");
            let column = display_width(&line[..byte]);
            buf[(
                u16::try_from(column).unwrap_or(0),
                u16::try_from(line_index).unwrap_or(0),
            )]
                .fg
        };
        assert_eq!(
            colour_of("-- a note"),
            theme.rgb(Token::SyntaxComment).into()
        );
        assert_eq!(colour_of("42"), theme.rgb(Token::SyntaxNumber).into());
        assert_eq!(colour_of("\'x\'"), theme.rgb(Token::SyntaxLiteral).into());
        assert_eq!(colour_of("FROM"), theme.rgb(Token::SyntaxKeyword).into());
        assert_eq!(
            colour_of("$1"),
            theme.rgb(Token::SyntaxIdentifier).into(),
            "a placeholder is not a number"
        );
    }

    #[test]
    fn the_statement_the_run_key_would_send_is_marked_in_the_gutter() {
        let mut model = connected_model(Environment::Local);
        model.editor.set_text("SELECT 1;\nSELECT 2;");
        model.editor.move_buffer_start();

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        let marked: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("\u{258e}SELECT"))
            .collect();
        assert_eq!(marked.len(), 1, "one statement is marked: {text}");
        assert!(marked[0].contains("SELECT 1"), "{:?}", marked[0]);

        // Moving the cursor moves the mark, because the mark is the answer to
        // "what would Ctrl+T run".
        model.editor.move_buffer_end();
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        let marked: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("\u{258e}SELECT"))
            .collect();
        assert_eq!(marked.len(), 1);
        assert!(marked[0].contains("SELECT 2"), "{:?}", marked[0]);

        // In ASCII the mark is still there, in ASCII.
        let ascii = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            100,
            30,
        );
        assert!(ascii.contains("|SELECT 2"), "{ascii}");
        assert!(
            ascii.is_ascii(),
            "the ASCII tier emitted something that is not ASCII"
        );
    }

    #[test]
    fn a_definition_is_shown_as_sql_with_where_it_came_from() {
        let mut model = connected_model(Environment::Local);
        model.definition = Some(crate::app::model::Definition {
            pending: None,
            heading: "table \"public\".\"orders\"".to_owned(),
            definition: Some(crate::postgres::metadata::Definition {
                kind: crate::postgres::ObjectKind::Table,
                schema: "public".into(),
                name: "orders".into(),
                source: crate::postgres::metadata::DefinitionSource::Assembled,
                text: "CREATE TABLE \"public\".\"orders\" (\n    \"id\" bigint NOT NULL\n);"
                    .to_owned(),
            }),
            error: None,
            scroll: 0,
        });

        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("CREATE TABLE"), "{text}");
        assert!(text.contains("bigint NOT NULL"), "{text}");
        assert!(
            text.contains("not a script that recreates it")
                || text.contains("Assembled from the catalogue"),
            "where the text came from must be stated: {text}"
        );
        assert!(text.contains("Esc closes"), "{text}");

        // It is coloured the way the editor colours SQL, from the same lexer.
        let area = Rect::new(0, 0, 120, 34);
        let mut buf = Buffer::empty(area);
        render(&model, &Keymap::new(), &rich(), area, &mut buf);
        let screen = buffer_to_string(&buf);
        let row = screen
            .lines()
            .position(|line| line.contains("CREATE TABLE"))
            .expect("a line");
        let line = screen.lines().nth(row).expect("a line");
        let column = display_width(&line[..line.find("CREATE").expect("a column")]);
        assert_eq!(
            buf[(
                u16::try_from(column).unwrap_or(0),
                u16::try_from(row).unwrap_or(0)
            )]
                .fg,
            rich().theme.rgb(Token::SyntaxKeyword).into()
        );
    }

    #[test]
    fn a_definition_says_it_is_loading_and_says_when_it_failed() {
        let mut model = connected_model(Environment::Local);
        model.definition = Some(crate::app::model::Definition {
            pending: Some(crate::app::tree::RequestId(1)),
            heading: "view \"public\".\"recent_orders\"".to_owned(),
            definition: None,
            error: None,
            scroll: 0,
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("Reading the catalogue"), "{text}");
        assert!(text.contains("recent_orders"), "the title names it: {text}");

        model.definition = Some(crate::app::model::Definition {
            pending: None,
            heading: "table \"public\".\"orders\"".to_owned(),
            definition: None,
            error: Some("permission denied for table orders".to_owned()),
            scroll: 0,
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("permission denied"), "{text}");
    }

    #[test]
    fn a_long_definition_scrolls_and_says_where_it_is() {
        let mut model = connected_model(Environment::Local);
        let body = (1..=80)
            .map(|n| format!("    \"column_{n}\" text"))
            .collect::<Vec<_>>()
            .join("\n");
        model.definition = Some(crate::app::model::Definition {
            pending: None,
            heading: "table \"public\".\"wide\"".to_owned(),
            definition: Some(crate::postgres::metadata::Definition {
                kind: crate::postgres::ObjectKind::Table,
                schema: "public".into(),
                name: "wide".into(),
                source: crate::postgres::metadata::DefinitionSource::Assembled,
                text: body,
            }),
            error: None,
            scroll: 10,
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("column_11"), "the window moved: {text}");
        assert!(!text.contains("column_1 "), "{text}");
        assert!(text.contains("of 80 lines"), "{text}");
    }

    #[test]
    fn the_save_prompt_shows_the_name_and_says_what_the_file_will_be() {
        let mut model = connected_model(Environment::Local);
        model.name_prompt = Some(crate::app::model::NamePrompt::for_query("monthly-revenue"));
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("monthly-revenue"), "{text}");
        assert!(text.contains("Enter to save"), "{text}");
        assert!(
            text.contains(".sql file you can open in anything"),
            "what it becomes is stated: {text}"
        );
    }

    #[test]
    fn the_password_prompt_shows_a_count_and_never_a_character() {
        let mut model = connected_model(Environment::Local);
        let mut prompt = crate::app::model::PasswordPrompt::new(
            "app@db.example.net:5432/orders",
            "the server requires a password for role \"app\"",
        );
        for ch in "hunter2".chars() {
            prompt.push(ch);
        }
        model.password_prompt = Some(prompt);

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("requires a password"), "{text}");
        assert!(
            text.contains("db.example.net"),
            "the target is named: {text}"
        );
        assert!(text.contains("Enter to try again"), "{text}");
        assert!(text.contains("kept nowhere"), "{text}");
        assert!(
            !text.contains("hunter"),
            "the characters must never reach the screen: {text}"
        );
        assert!(
            text.contains("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
            "seven characters, seven dots: {text}"
        );

        // In ASCII the mask is still a mask.
        let ascii = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            100,
            30,
        );
        assert!(ascii.contains("*******"), "{ascii}");
        assert!(!ascii.contains("hunter"));
    }

    #[test]
    fn the_parameter_prompt_shows_progress_literal_semantics_and_only_a_mask() {
        let mut model = connected_model(Environment::Local);
        let mut prompt = crate::app::model::ParameterPrompt::new(
            "SELECT :customer_id;".into(),
            None,
            vec!["customer_id".into(), "status".into()],
        );
        for character in "secret-marker".chars() {
            prompt.push(character);
        }
        model.parameter_prompt = Some(prompt);

        let text = render_to_string(&model, &Keymap::new(), &rich(), 110, 30);
        assert!(text.contains("Parameter 1 of 2: :customer_id"), "{text}");
        assert!(text.contains("literal text data"), "{text}");
        assert!(text.contains("text is valid"), "{text}");
        assert!(text.contains("Enter to accept"), "{text}");
        assert!(
            !text.contains("secret-marker"),
            "the value was rendered: {text}"
        );
        assert!(
            text.contains("•••••••••••••") || text.contains("*************"),
            "the value is represented only by a mask: {text}"
        );
    }

    #[test]
    fn the_header_says_when_the_tree_could_not_get_its_own_connection() {
        let mut model = connected_model(Environment::Local);
        // The normal case needs no announcement.
        for link in [
            crate::app::model::MetadataLink::Dedicated,
            crate::app::model::MetadataLink::Opening,
        ] {
            model.metadata_link = link;
            let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 30);
            assert!(!text.contains("objects:"), "{text}");
        }

        // Falling back to a shared connection explains why the tree can wait
        // behind a long query, so it is said.
        model.metadata_link =
            crate::app::model::MetadataLink::Unavailable("too many connections".to_owned());
        let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 30);
        assert!(text.contains("[objects: shared connection"), "{text}");
    }

    #[test]
    fn a_session_that_keeps_no_record_says_so_in_the_header() {
        let mut model = connected_model(Environment::Local);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(
            !text.contains("history off"),
            "an ordinary session says nothing about it"
        );

        // A statement the history refused is said out loud too, in the same
        // place, and only until the next one is kept.
        model.history_note = Some("Not added to history: this statement mentions a credential.");
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("[not in history]"), "{text}");
        model.history_note = None;

        model.history_paused = true;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("[history off]"), "{text}");

        // And in the bare tier, where there is no icon to carry it.
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            120,
            30,
        );
        assert!(text.contains("[history off]"), "{text}");
    }

    #[test]
    fn the_history_search_says_what_it_is_and_how_to_leave_it() {
        let mut model = connected_model(Environment::Local);
        model.palette = Some(crate::app::palette::Palette::over_history(vec![
            crate::app::palette::PaletteEntry {
                label: "SELECT count(*) FROM orders".to_owned(),
                detail: "2026-08-16 09:12:03  orders  ok".to_owned(),
                group: "History",
                command: crate::app::palette::PaletteCommand::Insert("SELECT 1".to_owned()),
            },
        ]));
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("History"), "{text}");
        assert!(text.contains("Esc to cancel"), "{text}");
        assert!(text.contains("SELECT count(*) FROM orders"), "{text}");
        assert!(text.contains("orders  ok"), "the outcome is shown: {text}");
        assert!(
            text.contains("mention a credential are never recorded"),
            "the rule is stated where someone looks for a missing statement: {text}"
        );
    }

    #[test]
    fn the_command_palette_names_context_and_recovers_from_no_matches() {
        let mut model = connected_model(Environment::Local);
        model.palette = Some(crate::app::palette::Palette::over_commands(
            vec![crate::app::palette::PaletteEntry {
                label: "Run the whole buffer".to_owned(),
                detail: "Ctrl+R".to_owned(),
                group: "Editor",
                command: crate::app::palette::PaletteCommand::Run(crate::app::Action::RunBuffer),
            }],
            "Focus: Editor. Type to search; Esc closes.",
        ));
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Command palette"), "{text}");
        assert!(text.contains("Type to search"), "{text}");
        assert!(text.contains("Editor"), "{text}");
        assert!(text.contains("Run the whole buffer"), "{text}");

        model.palette.as_mut().expect("open").query = "does-not-exist".into();
        let ascii = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            80,
            24,
        );
        assert!(
            ascii.contains("No applicable command matches that"),
            "{ascii}"
        );
        assert!(ascii.contains("Esc closes"), "{ascii}");
        assert!(ascii.is_ascii(), "{ascii:?}");
    }

    #[test]
    fn a_screenful_is_what_the_pane_can_actually_show() {
        // The reducer moves by these numbers, so they have to match the panes
        // the renderer draws rather than being a guess.
        let (editor, results) = (editor_page((100, 30)), results_page((100, 30)));
        assert!(
            editor >= 8,
            "an 30-row terminal shows a real page: {editor}"
        );
        assert!(results >= 8, "{results}");
        assert!(
            editor_page((100, 40)) > editor,
            "a taller terminal pages further"
        );

        // Degenerate sizes must still yield a usable step rather than zero.
        for size in [(0, 0), (1, 1), (40, 8), (80, 24)] {
            assert!(editor_page(size) >= 1, "{size:?}");
            assert!(results_page(size) >= 1, "{size:?}");
        }
    }

    #[test]
    fn a_filtered_grid_shows_the_matches_and_says_what_it_searched() {
        let mut model = connected_model(Environment::Local);
        with_rows(
            &mut model,
            &["name"],
            &[
                &[Cell::Text("alpha".into())],
                &[Cell::Text("beta".into())],
                &[Cell::Text("gamma".into())],
            ],
        );
        model.focus = Focus::Results;
        model.result_filter = "ta".into();

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("beta"), "{text}");
        assert!(!text.contains("alpha"), "a row that does not match is gone");
        assert!(text.contains("matching 1 of 3 rows"), "{text}");
        assert!(
            text.contains(" 2 ") || text.contains("2 beta"),
            "the gutter keeps the row's own number: {text}"
        );

        // A filter matching nothing says so and says how to leave.
        model.result_filter = "nothing".into();
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("No retained row contains"), "{text}");
        assert!(text.contains("Esc clears the filter"), "{text}");
    }

    #[test]
    fn a_filter_over_a_truncated_result_never_implies_it_searched_everything() {
        let mut model = connected_model(Environment::Local);
        with_rows(
            &mut model,
            &["name"],
            &[&[Cell::Text("alpha".into())], &[Cell::Text("beta".into())]],
        );
        if let Some(execution) = model.last_execution.as_mut()
            && let Some(set) = execution.statements[0].result_set.as_mut()
        {
            set.rows_seen = 200_000;
            set.cap = 2;
        }
        model.result_filter = "a".into();

        let text = render_to_string(&model, &Keymap::new(), &rich(), 160, 30);
        assert!(text.contains("retained rows"), "{text}");
        assert!(text.contains("200000 returned"), "{text}");
    }

    #[test]
    fn the_expanded_view_lays_one_row_down_the_screen() {
        let mut model = connected_model(Environment::Local);
        let columns: Vec<String> = (0..12).map(|i| format!("column_{i}")).collect();
        let names: Vec<&str> = columns.iter().map(String::as_str).collect();
        let row: Vec<Cell> = (0..12).map(|i| Cell::Text(format!("value-{i}"))).collect();
        with_rows(&mut model, &names, &[&row]);
        model.expanded_row = true;

        // Eighty columns cannot hold twelve columns across. Down the screen it
        // fits, which is the entire point.
        let text = render_to_string(&model, &Keymap::new(), &rich(), 80, 30);
        for i in 0..12 {
            assert!(text.contains(&format!("column_{i}")), "{text}");
            assert!(text.contains(&format!("value-{i}")), "{text}");
        }
        assert!(text.contains("Row 1 of 1"), "{text}");

        // With no rows it says so rather than drawing an empty frame.
        let mut empty = connected_model(Environment::Local);
        with_rows(&mut empty, &["id"], &[]);
        empty.expanded_row = true;
        let text = render_to_string(&empty, &Keymap::new(), &rich(), 80, 30);
        assert!(text.contains("no row to expand"), "{text}");
    }

    #[test]
    fn the_inspector_shows_the_whole_value_and_says_where_it_is_in_it() {
        let mut model = connected_model(Environment::Local);
        let long = (0..40)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        with_rows(&mut model, &["document"], &[&[Cell::Text(long)]]);
        model.focus = Focus::Results;
        model.inspector = Some(crate::app::inspect::Inspector::new());

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("document"), "the column is named: {text}");
        assert!(text.contains("row 1 of 1, column 1 of 1"), "{text}");
        assert!(text.contains("line-0"), "{text}");
        assert!(
            text.contains("of 40 wrapped lines"),
            "a value that does not fit says how much is left: {text}"
        );
        assert!(text.contains("Esc closes"), "the way out is stated");
    }

    #[test]
    fn the_inspector_tells_null_an_empty_string_and_the_text_null_apart() {
        // The grid can only show a marker for two of these. This is where the
        // difference is stated in words.
        for (cell, expected) in [
            (Cell::Null, "SQL NULL"),
            (Cell::Text(String::new()), "empty string, 0 characters"),
            (Cell::Text("NULL".into()), "text, 4 characters"),
        ] {
            let mut model = connected_model(Environment::Local);
            with_rows(&mut model, &["value"], &[&[cell]]);
            model.focus = Focus::Results;
            model.inspector = Some(crate::app::inspect::Inspector::new());
            let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
            assert!(text.contains(expected), "expected {expected} in {text}");
        }
    }

    #[test]
    fn neither_view_lets_a_hostile_value_reach_the_terminal() {
        let hostile = Cell::Text("\x1b[2J\x1b[Hgotcha\u{202e}reversed".into());
        let wide = Cell::Text("日本語のとても長い値です".repeat(20));

        for cell in [hostile, wide] {
            for expanded in [true, false] {
                let mut model = connected_model(Environment::Local);
                with_rows(
                    &mut model,
                    &["\x1b[31mname"],
                    &[std::slice::from_ref(&cell)],
                );
                model.focus = Focus::Results;
                model.expanded_row = expanded;
                model.inspector = Some(crate::app::inspect::Inspector::new());
                let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
                assert!(!text.contains('\x1b'), "an escape reached the screen");
                assert!(!text.contains('\u{202e}'), "a bidi override survived");
            }
        }
    }

    #[test]
    fn a_running_query_shows_motion_a_word_and_an_honest_elapsed_time() {
        let mut model = connected_model(Environment::Local);
        model.phase = QueryPhase::Running {
            job: JobId(1),
            statements: 1,
        };
        model.running_for = Some(Duration::from_millis(2400));
        model.frame = 3;

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Running"), "the state is a word: {text}");
        assert!(text.contains("2.4s"), "the elapsed time is shown: {text}");
        assert!(
            text.contains("Ctrl+C asks the server to cancel."),
            "the way out is offered"
        );

        model.frame = 4;
        let next = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert_ne!(text, next, "the indicator did not advance between frames");
    }

    #[test]
    fn a_retained_result_refresh_is_visible_and_ascii_safe_while_in_flight() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(&mut model, &["value"], &[&[Cell::Text("old".into())]]);
        model.last_sql = Some("SELECT value FROM public.orders;".into());
        model.phase = QueryPhase::Running {
            job: JobId(2),
            statements: 1,
        };
        model.running_refresh = true;
        model.refresh_notice = Some(
            "Refreshing the retained result; the editor is unchanged and nothing is sent until any parameters are entered.".into(),
        );

        let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 30);
        assert!(text.contains("Refreshing retained result"), "{text}");
        assert!(text.contains("editor is unchanged"), "{text}");

        let mut idle = model.clone();
        idle.phase = QueryPhase::Idle;
        idle.running_refresh = false;
        let idle_text = render_to_string(&idle, &Keymap::new(), &rich(), 140, 30);
        assert!(idle_text.contains("F6 Refresh"), "{idle_text}");

        let ascii = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            140,
            30,
        );
        assert!(
            ascii.is_ascii(),
            "refresh state leaked a non-ASCII glyph: {ascii:?}"
        );
        assert!(ascii.contains("Refreshing retained result"), "{ascii}");
    }

    #[test]
    fn a_refused_retained_result_refresh_is_visible_in_a_narrow_ascii_layout() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(&mut model, &["value"], &[&[Cell::Text("old".into())]]);
        model.refresh_notice = Some(
            "Retained result refresh was refused: this source contains multiple statements.".into(),
        );

        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            60,
            24,
        );
        assert!(text.contains("refresh"), "refusal is visible: {text}");
        assert!(
            text.is_ascii(),
            "narrow refusal emitted non-ASCII: {text:?}"
        );
        assert!(!text.contains('\x1b'), "refusal emitted terminal controls");
    }

    #[test]
    fn reduced_motion_replaces_animation_with_the_same_words() {
        let mut model = connected_model(Environment::Local);
        model.phase = QueryPhase::Running {
            job: JobId(1),
            statements: 1,
        };
        model.running_for = Some(Duration::from_millis(1000));

        let still = Presentation::new(
            Theme::new(ThemeChoice::Dark, true),
            Glyphs::new(GlyphTier::Nerd),
            true,
        );
        let first = render_to_string(&model, &Keymap::new(), &still, 100, 30);
        let mut later = model.clone();
        later.frame = 99;
        let second = render_to_string(&later, &Keymap::new(), &still, 100, 30);

        assert_eq!(first, second, "reduced motion must not animate");
        assert!(first.contains("Running"), "the state is still stated");
        assert!(first.contains("1.0s"), "the elapsed time is still shown");
    }

    #[test]
    fn cancellation_is_worded_as_requested_until_the_server_answers() {
        let mut model = connected_model(Environment::Local);
        model.phase = QueryPhase::CancellationRequested { job: JobId(1) };
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Cancellation requested"), "{text}");
        assert!(!text.contains("Query cancelled by server"));
        assert!(
            text.contains("It may still be running"),
            "the uncertainty is stated: {text}"
        );
    }

    #[test]
    fn the_editor_shows_line_numbers_and_a_visible_cursor() {
        let mut model = connected_model(Environment::Local);
        model.editor.set_text("SELECT 1\nFROM orders");
        model.focus = Focus::Editor;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("SELECT 1"), "{text}");
        assert!(text.contains("FROM orders"));
        let numbered = text
            .lines()
            .find(|l| l.contains("SELECT 1"))
            .expect("first line drawn");
        assert!(
            numbered.contains('1'),
            "line numbers are drawn: {numbered:?}"
        );
    }

    #[test]
    fn an_unfocused_editor_still_shows_its_text() {
        // The pane loses its focus styling, not its contents. A differential
        // redraw can make this look otherwise on screen, so it is pinned here.
        let mut model = connected_model(Environment::Local);
        model
            .editor
            .set_text("-- a comment\nSELECT current_database(), current_user;");
        model.focus = Focus::Results;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(
            text.contains("SELECT current_database"),
            "an unfocused editor lost its text:\n{text}"
        );
        assert!(text.contains("-- a comment"));
    }

    #[test]
    fn an_empty_state_says_what_to_do_next() {
        let model = connected_model(Environment::Local);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("No query has run yet"), "{text}");
        assert!(
            text.contains("Ctrl+R"),
            "the empty state names the key to press"
        );
    }

    #[test]
    fn a_fresh_frame_names_connection_state_and_a_safe_next_action() {
        let disconnected = Model::new(100);
        let text = render_to_string(&disconnected, &Keymap::new(), &rich(), 80, 24);
        assert!(text.contains("Connection required"), "{text}");
        assert!(text.contains("Connect before running SQL"), "{text}");
        assert!(text.contains("Command palette"), "{text}");
        assert!(!text.contains("Run the buffer once SQL is ready"), "{text}");

        let mut connected = connected_model(Environment::Local);
        connected.editor.set_text("");
        let text = render_to_string(&connected, &Keymap::new(), &rich(), 80, 24);
        assert!(text.contains("Start here"), "{text}");
        assert!(text.contains("Type SQL in this editor"), "{text}");
        assert!(text.contains("Run the buffer once SQL is ready"), "{text}");
        assert!(text.contains("Command palette"), "{text}");
    }

    #[test]
    fn the_footer_changes_with_focus_and_uses_configured_keys() {
        let mut keys = std::collections::BTreeMap::new();
        keys.insert(
            "run-buffer".to_owned(),
            crate::config::schema::KeySpec::One("f2".to_owned()),
        );
        let keymap = Keymap::from_config(&keys).expect("valid configured keymap");
        let mut model = connected_model(Environment::Local);
        let editor = render_to_string(&model, &keymap, &rich(), 140, 30);
        assert!(editor.contains("F2 Run"), "{editor}");
        assert!(editor.contains("Palette"), "{editor}");

        model.focus = Focus::Results;
        with_rows(&mut model, &["id"], &[&[Cell::Text("1".into())]]);
        let results = render_to_string(&model, &keymap, &rich(), 140, 30);
        assert!(results.contains("Inspect"), "{results}");
        assert!(results.contains("Filter"), "{results}");
        assert!(results.contains("Grid controls"), "{results}");
        assert!(
            !results.contains("F2 Run"),
            "editor-only run hint leaked: {results}"
        );
        let hint_count = ["Inspect", "Filter", "Grid controls", "Focus"]
            .iter()
            .filter(|label| results.contains(**label))
            .count();
        assert!(hint_count <= crate::app::discovery::MAX_CONTEXTUAL_HINTS);
    }

    #[test]
    fn empty_and_filtered_results_keep_distinct_recovery_words() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        with_rows(&mut model, &["id"], &[]);
        let empty = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(empty.contains("No rows returned"), "{empty}");
        assert!(!empty.contains("No query has run yet"), "{empty}");

        with_rows(&mut model, &["id"], &[&[Cell::Text("one".into())]]);
        model.result_filter = "missing".into();
        let filtered = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(filtered.contains("No retained row contains"), "{filtered}");
        assert!(filtered.contains("Esc clears the filter"), "{filtered}");

        let mut keys = std::collections::BTreeMap::new();
        keys.insert(
            "dismiss".to_owned(),
            crate::config::schema::KeySpec::One("f8".to_owned()),
        );
        let configured = Keymap::from_config(&keys).expect("valid configured keymap");
        let filtered = render_to_string(&model, &configured, &rich(), 120, 30);
        assert!(filtered.contains("F8 clears the filter"), "{filtered}");
    }

    #[test]
    fn an_error_shows_headline_cause_and_next_action_before_technical_detail() {
        let mut model = connected_model(Environment::Local);
        model.error = Some(
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Query,
                "relation \"orders\" does not exist",
                "running statement 1",
            )
            .likely_cause("the relation is not visible under the current search_path")
            .next_action("qualify it with a schema")
            .technical("SQLSTATE", "42P01"),
        );
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);

        assert!(text.contains("Query error"), "{text}");
        assert!(text.contains("does not exist"));
        assert!(text.contains("Likely cause"));
        assert!(text.contains("Next:"));
        assert!(!text.contains("42P01"), "technical detail starts collapsed");
        assert!(text.contains("Ctrl+D to expand"));

        model.error_expanded = true;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("42P01"), "expanding reveals SQLSTATE");
    }

    #[test]
    fn a_mapped_error_marks_the_editor_line_and_keeps_the_source_visible() {
        let mut model = connected_model(Environment::Local);
        let sql = "SELECT 1;\nSELECT café FROM orders WHERE id = 0;";
        model.editor.set_text(sql);
        model.error = Some(
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Query,
                "syntax error at or near orders",
                "running statement 2",
            )
            .in_statement(2)
            .at_position(19),
        );
        model.error_location = crate::query::error_location::locate(
            sql,
            None,
            2,
            crate::diagnostics::SqlPosition { character: 19 },
        );
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 32);
        assert!(
            text.contains("Location: statement 2, line 2, column 19"),
            "{text}"
        );
        assert!(text.contains("SELECT café FROM orders"), "{text}");
        assert!(
            text.lines()
                .any(|line| line.contains("!") && line.contains("SELECT café")),
            "the error line has an explicit ASCII marker: {text}"
        );

        let compact = render_to_string(&model, &Keymap::new(), &rich(), 80, 20);
        assert!(compact.contains("syntax error"), "{compact}");
        assert!(
            compact.contains("SELECT café"),
            "compact mode hid the editor: {compact}"
        );
    }

    #[test]
    fn object_context_is_quoted_and_catalogue_claims_are_bounded_by_a_match() {
        let mut model = connected_model(Environment::Local);
        model.error = Some(
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Query,
                "duplicate key value violates unique constraint",
                "running statement 1",
            )
            .in_statement(1)
            .object_context(
                crate::diagnostics::ObjectContext::from_server_fields(
                    Some("public"),
                    Some("orders"),
                    Some("order_id"),
                    Some("orders_pkey"),
                )
                .expect("object context"),
            ),
        );
        model.completion.catalog = crate::app::completion::CatalogStatus::Ready {
            catalog: crate::query::completion::CompletionCatalog {
                objects: Vec::new(),
                relations: vec![crate::query::completion::CatalogRelation {
                    schema: "public".into(),
                    name: "orders".into(),
                    columns: vec![crate::query::completion::CatalogColumn {
                        name: "order_id".into(),
                        data_type: "bigint".into(),
                    }],
                }],
            },
            loaded_at: "2026-09-04 12:00:00 +02:00".into(),
        };
        let matched = render_to_string(&model, &Keymap::new(), &rich(), 140, 40);
        assert!(
            matched.contains(
                "Object context: relation \"public\".\"orders\", column \"order_id\", constraint \"orders_pkey\""
            ),
            "{matched}"
        );
        assert!(
            matched.contains("Catalogue: matched relation and column \"order_id\"; type bigint"),
            "{matched}"
        );

        model.completion.catalog = crate::app::completion::CatalogStatus::Unavailable {
            message: "permission denied".into(),
        };
        let unavailable = render_to_string(&model, &Keymap::new(), &rich(), 140, 40);
        assert!(
            unavailable.contains("Catalogue: Schema snapshot unavailable"),
            "{unavailable}"
        );
        assert!(
            unavailable.contains("catalogue-derived claim"),
            "{unavailable}"
        );
    }

    // -------------------------------------------------------- object tree

    fn with_tree(model: &mut Model) {
        use crate::postgres::{ObjectKind, SchemaSummary};
        let mut counts = std::collections::BTreeMap::new();
        counts.insert(ObjectKind::Table, 2);
        counts.insert(ObjectKind::View, 1);
        model.tree.set_schemas(vec![
            SchemaSummary {
                name: "public".into(),
                usable: true,
                counts,
            },
            SchemaSummary {
                name: "locked".into(),
                usable: false,
                counts: std::collections::BTreeMap::new(),
            },
        ]);
        model.tree.expand_schema(&[0]);
    }

    #[test]
    fn the_object_tree_shows_schemas_groups_and_their_counts() {
        let mut model = connected_model(Environment::Local);
        with_tree(&mut model);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);

        assert!(text.contains("public"), "{text}");
        assert!(
            text.contains("locked"),
            "a schema without permission is still listed"
        );
        assert!(
            text.contains("no permission"),
            "and says why it cannot be opened"
        );
        // The group label carries the kind and the count in words, so the tree
        // is readable with no icons at all.
        assert!(text.contains("tables (2)"), "{text}");
        assert!(text.contains("views (1)"), "{text}");
    }

    #[test]
    fn the_tree_reads_correctly_in_every_glyph_tier() {
        let mut model = connected_model(Environment::Local);
        with_tree(&mut model);
        for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
            for color in [true, false] {
                let text = render_to_string(
                    &model,
                    &Keymap::new(),
                    &presentation(ThemeChoice::Dark, color, tier),
                    120,
                    30,
                );
                let context = format!("{tier:?}/colour={color}");
                assert!(text.contains("public"), "{context}");
                assert!(text.contains("tables (2)"), "{context}");
                assert!(text.contains("no permission"), "{context}");
            }
        }
    }

    #[test]
    fn an_open_node_is_distinguishable_from_a_closed_one_without_colour() {
        let mut model = connected_model(Environment::Local);
        with_tree(&mut model);
        let plain = presentation(ThemeChoice::Dark, false, GlyphTier::Ascii);

        let opened = render_to_string(&model, &Keymap::new(), &plain, 120, 30);
        model.tree.roots[0].expanded = false;
        let closed = render_to_string(&model, &Keymap::new(), &plain, 120, 30);
        assert_ne!(opened, closed, "the chevron must change with the state");
        assert!(opened.contains('v'), "an open node is marked");
        assert!(closed.contains('>'), "a closed node is marked");
    }

    #[test]
    fn the_tree_title_is_a_breadcrumb_to_the_selected_object() {
        let mut model = connected_model(Environment::Local);
        with_tree(&mut model);
        model.tree.selected = 1; // tables (2)
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(
            text.contains("public") && text.contains("tables (2)"),
            "the path to the selection is in the title: {text}"
        );
    }

    #[test]
    fn hiding_the_tree_gives_its_width_back_to_the_editor() {
        let mut model = connected_model(Environment::Local);
        with_tree(&mut model);
        let shown = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(shown.contains("tables (2)"));

        model.sidebar_visible = false;
        let hidden = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(!hidden.contains("tables (2)"), "the tree is gone");
        assert!(hidden.contains("Editor"), "and the editor is still there");
    }

    #[test]
    fn a_filtered_tree_shows_the_filter_and_says_when_nothing_matches() {
        let mut model = connected_model(Environment::Local);
        with_tree(&mut model);
        model.tree.filtering = true;
        model.tree.filter = "zzz".into();
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("zzz"), "the filter text is visible: {text}");
        assert!(text.contains("No object matches this filter."), "{text}");
        assert!(text.contains("Searched \"zzz\"."), "{text}");
    }

    #[test]
    fn the_tree_says_it_is_reading_the_catalogue_while_it_loads() {
        let mut model = connected_model(Environment::Local);
        model.tree.begin_loading();
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("Reading the catalogue"), "{text}");
    }

    #[test]
    fn a_tree_that_cannot_load_explains_itself_and_offers_a_way_forward() {
        let mut model = connected_model(Environment::Local);
        model.tree.set_error(
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Connection,
                "the connection closed",
                "reading the catalogue",
            )
            .next_action("reconnect and try again"),
        );
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("the connection closed"), "{text}");
        assert!(text.contains("reconnect and try again"), "{text}");
    }

    #[test]
    fn hostile_object_names_cannot_emit_escape_sequences_from_the_tree() {
        use crate::postgres::{ObjectKind, ObjectSummary, SchemaSummary};
        let mut model = connected_model(Environment::Local);
        let mut counts = std::collections::BTreeMap::new();
        counts.insert(ObjectKind::Table, 1);
        model.tree.set_schemas(vec![SchemaSummary {
            name: "\x1b[2Jpublic".into(),
            usable: true,
            counts,
        }]);
        model.tree.expand_schema(&[0]);
        model.tree.selected = 1;
        let (request, path, _) = model.tree.expand_selected().expect("load");
        model.tree.apply(
            request,
            &path,
            crate::app::tree::MetadataPayload::Objects(vec![ObjectSummary {
                kind: ObjectKind::Table,
                schema: "public".into(),
                name: "\x1b[31mgotcha".into(),
                readable: true,
                detail: None,
            }]),
        );

        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(
            !text.contains('\x1b'),
            "an escape in an object name reached the screen"
        );
    }

    // ----------------------------------------------------------- palette

    #[test]
    fn the_palette_shows_the_query_the_matches_and_how_to_leave() {
        let mut model = connected_model(Environment::Local);
        model.palette = Some(crate::app::palette::Palette::new(vec![
            crate::app::palette::PaletteEntry {
                label: "Run the whole buffer".into(),
                detail: "Ctrl+R".into(),
                group: "Command",
                command: crate::app::palette::PaletteCommand::Run(
                    crate::app::message::Action::RunBuffer,
                ),
            },
            crate::app::palette::PaletteEntry {
                label: "orders".into(),
                detail: "table \"public\".\"orders\"".into(),
                group: "Object",
                command: crate::app::palette::PaletteCommand::Insert(
                    "\"public\".\"orders\"".into(),
                ),
            },
        ]));

        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("Go to"), "{text}");
        assert!(text.contains("Esc to cancel"), "{text}");
        assert!(text.contains("Run the whole buffer"), "{text}");
        assert!(text.contains("orders"), "{text}");
        assert!(
            text.contains("Command"),
            "entries say which group they are in"
        );
        assert!(text.contains("2 match(es)"), "the count is visible: {text}");
    }

    #[test]
    fn the_palette_says_when_nothing_matches_rather_than_going_blank() {
        let mut model = connected_model(Environment::Local);
        let mut palette =
            crate::app::palette::Palette::new(vec![crate::app::palette::PaletteEntry {
                label: "orders".into(),
                detail: String::new(),
                group: "Object",
                command: crate::app::palette::PaletteCommand::Insert("orders".into()),
            }]);
        palette.query = "zzzz".into();
        model.palette = Some(palette);

        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("Nothing matches that."), "{text}");
        assert!(text.contains("0 match(es)"), "{text}");
    }

    #[test]
    fn the_connection_picker_names_the_default_profile_and_secret_boundary() {
        let mut model = connected_model(Environment::Local);
        model.palette = Some(crate::app::palette::Palette::over_connections(vec![
            crate::app::palette::PaletteEntry {
                label: "Use default connection settings".into(),
                detail: "ordinary resolution; no profile selected".into(),
                group: "Default",
                command: crate::app::palette::PaletteCommand::ConnectProfile(None),
            },
            crate::app::palette::PaletteEntry {
                label: "orders-prod".into(),
                detail: "db.example.net:5432 | orders | role app | read-only".into(),
                group: "Profiles",
                command: crate::app::palette::PaletteCommand::ConnectProfile(Some(
                    "orders-prod".into(),
                )),
            },
        ]));

        let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 34);
        for expected in [
            "Connection picker",
            "Use default connection settings",
            "orders-prod",
            "Profiles",
            "read-only",
            "passwords",
            "provider tokens",
            "Enter to connect",
        ] {
            assert!(text.contains(expected), "{expected:?} missing:\n{text}");
        }

        let plain = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::HighContrast, false, GlyphTier::Ascii),
            140,
            34,
        );
        assert!(plain.contains("Connection picker"), "{plain}");
        assert!(plain.is_ascii(), "picker ASCII mode emitted non-ASCII");

        let compact = render_to_string(&model, &Keymap::new(), &rich(), 80, 24);
        assert!(compact.contains("Connection picker"), "{compact}");
        let no_colour = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Unicode),
            100,
            24,
        );
        assert!(no_colour.contains("read-only"), "{no_colour}");
        let reduced = Presentation::new(
            Theme::new(ThemeChoice::Dark, true),
            Glyphs::new(GlyphTier::Unicode),
            true,
        );
        let reduced_text = render_to_string(&model, &Keymap::new(), &reduced, 100, 24);
        assert!(reduced_text.contains("Connection picker"), "{reduced_text}");
    }

    #[test]
    fn the_export_palette_names_shapes_and_retained_row_scope() {
        let mut model = connected_model(Environment::Local);
        model.palette = Some(crate::app::palette::Palette::over_export_formats(
            [
                crate::app::model::ExportFormat::Csv,
                crate::app::model::ExportFormat::Tsv,
                crate::app::model::ExportFormat::Json,
                crate::app::model::ExportFormat::Ndjson,
                crate::app::model::ExportFormat::Markdown,
            ]
            .into_iter()
            .map(|format| crate::app::palette::PaletteEntry {
                label: format.label().to_owned(),
                detail: format!("{}; {}", format.description(), format.extension()),
                group: "Formats",
                command: crate::app::palette::PaletteCommand::ChooseExportFormat(format),
            })
            .collect(),
            "3 row(s) ready: retained rows on screen only; choose a shape before naming the destination.",
        ));

        let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 34);
        for expected in [
            "Export format",
            "retained rows on screen",
            "CSV",
            "TSV",
            "JSON",
            "NDJSON",
            "Markdown",
            "Enter to choose",
        ] {
            assert!(text.contains(expected), "{expected:?} missing:\n{text}");
        }

        let compact = render_to_string(&model, &Keymap::new(), &rich(), 80, 24);
        assert!(compact.contains("Export format"), "{compact}");
        let plain = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::HighContrast, false, GlyphTier::Ascii),
            100,
            24,
        );
        assert!(
            plain.is_ascii(),
            "export palette emitted non-ASCII: {plain}"
        );
        assert!(plain.contains("retained rows on screen"), "{plain}");
        let reduced = Presentation::new(
            Theme::new(ThemeChoice::Dark, true),
            Glyphs::new(GlyphTier::Unicode),
            true,
        );
        let reduced_text = render_to_string(&model, &Keymap::new(), &reduced, 100, 24);
        assert!(reduced_text.contains("Export format"), "{reduced_text}");
    }

    #[test]
    fn connection_details_explain_identity_transport_and_posture_without_secrets() {
        let mut model = connected_model(Environment::Production);
        model.credential_presentation = crate::connection::cloud::built_in()
            .into_iter()
            .find(|provider| provider.name == "entra")
            .map(|provider| provider.presentation());
        model.connection_details = true;

        let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 34);
        for expected in [
            "Connection details",
            "Target",
            "Database",
            "Role",
            "PROD",
            "read-write",
            "Microsoft Entra ID",
            "az account get-access-token",
            "TLS required",
            "token is not shown, stored, logged, or written",
            "Esc closes",
        ] {
            assert!(text.contains(expected), "{expected:?} missing:\n{text}");
        }
        assert!(
            !text.contains("access-token --resource"),
            "raw arguments leaked"
        );

        // The same facts survive the bare, no-colour presentation.
        let plain = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::HighContrast, false, GlyphTier::Ascii),
            140,
            34,
        );
        for expected in ["Microsoft Entra ID", "TLS required", "read-write"] {
            assert!(plain.contains(expected), "{expected:?} missing:\n{plain}");
        }
        assert!(
            plain.is_ascii(),
            "ASCII presentation emitted non-ASCII text"
        );
    }

    #[test]
    fn connection_details_never_guess_a_cloud_identity_for_password_sessions() {
        let mut model = connected_model(Environment::Local);
        model.connection_details = true;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 140, 34);
        assert!(text.contains("No cloud token was requested"), "{text}");
        assert!(
            text.contains(".pgpass"),
            "supported password routes are stated"
        );
        assert!(!text.contains("Microsoft Entra"), "identity was invented");
    }

    #[test]
    fn connection_details_sanitise_custom_provider_display_text() {
        let mut model = connected_model(Environment::Local);
        model.credential_provider = Some("corp\x1b[31m".into());
        model.credential_presentation = Some(
            crate::connection::cloud::Provider {
                name: "corp\x1b[31m".into(),
                command: vec!["token\x1b[2J".into(), "--secret".into()],
                extract: crate::connection::cloud::Extract::Raw,
                timeout: std::time::Duration::from_secs(60),
                remedy: "run corp login".into(),
            }
            .presentation(),
        );
        model.connection_details = true;
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            140,
            34,
        );
        assert!(
            !text.contains('\x1b'),
            "escape reached the rendered terminal"
        );
        assert!(
            text.contains("\\x1B[31m"),
            "sanitised provider name is visible"
        );
        assert!(!text.contains("--secret"), "provider arguments leaked");
    }

    // ------------------------------------------------------------ chords

    #[test]
    fn a_pending_chord_lists_every_key_that_could_follow_it() {
        let mut model = connected_model(Environment::Local);
        model.prefix_pending = true;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);

        assert!(
            text.contains("Ctrl+K"),
            "the popup names the prefix: {text}"
        );
        for (key, _, description) in crate::ui::keymap::CHORDS {
            assert!(text.contains(*description), "{key} is missing from {text}");
        }
    }

    #[test]
    fn a_failed_transaction_takes_over_the_results_and_names_the_way_out() {
        let mut model = connected_model(Environment::Local);
        with_rows(&mut model, &["n"], &[&[Cell::Text("1".into())]]);
        model.transaction = crate::query::result::TransactionState::Failed;

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Transaction failed"), "{text}");
        assert!(text.contains("ROLLBACK"), "the way out is named: {text}");
    }

    #[test]
    fn a_failed_transaction_with_an_error_still_shows_recovery_in_the_results() {
        let mut model = connected_model(Environment::Local);
        with_rows(&mut model, &["n"], &[&[Cell::Text("1".into())]]);
        model.transaction = crate::query::result::TransactionState::Failed;
        model.error = Some(
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Query,
                "current transaction is aborted",
                "running statement 1",
            )
            .likely_cause("an earlier statement failed")
            .next_action("roll back the transaction"),
        );

        for (width, height) in [(100, 30), (60, 20)] {
            let text = render_to_string(&model, &Keymap::new(), &rich(), width, height);
            assert!(
                text.contains("Transaction failed"),
                "{width}x{height}: {text}"
            );
            assert!(
                text.contains("current transaction is aborted"),
                "the original diagnostic remains visible at {width}x{height}: {text}"
            );
            assert!(
                text.contains("ROLLBACK"),
                "the recovery instruction is visible at {width}x{height}: {text}"
            );
        }
    }

    #[test]
    fn the_transaction_state_is_always_in_the_status_line() {
        let mut model = connected_model(Environment::Local);
        for (state, expected) in [
            (
                crate::query::result::TransactionState::Autocommit,
                "Autocommit",
            ),
            (
                crate::query::result::TransactionState::Open,
                "In transaction",
            ),
            (
                crate::query::result::TransactionState::Failed,
                "Transaction failed",
            ),
            (
                crate::query::result::TransactionState::Unknown,
                "Transaction state unknown",
            ),
        ] {
            model.transaction = state;
            let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
            assert!(
                text.contains(expected),
                "{state:?} missing from the status line"
            );
        }
    }

    #[test]
    fn a_write_to_production_asks_before_it_runs() {
        let mut model = connected_model(Environment::Production);
        model.pending_run = Some(crate::app::model::PendingRun {
            sql: "DELETE FROM orders WHERE created_at < now()".into(),
            impact: crate::query::Impact::Destructive,
            typed: String::new(),
            required: "orders".into(),
            source: None,
        });

        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Confirm"), "{text}");
        assert!(
            text.contains("[PROD]"),
            "the classification is on the prompt"
        );
        assert!(
            text.contains("destroys data"),
            "it says what the statement does"
        );
        assert!(
            text.contains("DELETE FROM orders"),
            "and shows the statement"
        );
        assert!(
            text.contains("Type the database name"),
            "a destructive statement is worth typing for: {text}"
        );
        assert!(
            text.contains("Database permissions are the real control"),
            "the prompt must not overstate its own certainty"
        );
    }

    #[test]
    fn a_lesser_change_asks_without_making_anyone_type() {
        let mut model = connected_model(Environment::Production);
        model.pending_run = Some(crate::app::model::PendingRun {
            sql: "UPDATE orders SET total = 0".into(),
            impact: crate::query::Impact::Write,
            typed: String::new(),
            required: "orders".into(),
            source: None,
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Enter to run it"), "{text}");
        assert!(
            !text.contains("Type the database name"),
            "typing a word for every write would train people to type it"
        );
    }

    fn plan_model(analyzed: bool) -> Model {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        model.plan.ready(
            crate::query::PlanDocument::parse(
                r#"[{"Plan":{"Node Type":"Nested Loop","Startup Cost":0.1,"Total Cost":18.4,"Plan Rows":4,"Actual Rows":40,"Actual Total Time":12.0,"Actual Loops":2,"Plans":[{"Node Type":"Seq Scan","Relation Name":"orders","Startup Cost":0,"Total Cost":24,"Plan Rows":100,"Actual Rows":1000,"Actual Total Time":7.0,"Actual Loops":2,"Filter":"owner = 'demo'"},{"Node Type":"Index Scan","Relation Name":"items","Index Name":"items_order_idx","Plan Rows":4,"Actual Rows":4,"Actual Total Time":1.0,"Actual Loops":2}]},"Planning Time":0.3,"Execution Time":25.7}]"#,
                analyzed,
            )
            .expect("plan fixture"),
        );
        model
    }

    #[test]
    fn a_plain_plan_is_a_readable_tree_with_estimate_only_wording() {
        let model = plan_model(false);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("EXPLAIN"), "{text}");
        assert!(text.contains("Nested Loop"), "{text}");
        assert!(text.contains("Seq Scan orders"), "{text}");
        assert!(text.contains("est rows"), "{text}");
        assert!(text.contains("cost units"), "{text}");
        assert!(
            text.contains("not measured"),
            "plain plans do not imply timing"
        );
        assert!(text.contains("Attention:"), "{text}");
    }

    #[test]
    fn an_analyzed_plan_names_observed_metrics_loops_and_mismatch_basis() {
        let model = plan_model(true);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("EXPLAIN ANALYZE"), "{text}");
        assert!(text.contains("actual rows"), "{text}");
        assert!(text.contains("time"), "{text}");
        assert!(text.contains("loops"), "{text}");
        assert!(text.contains("Estimate mismatch"), "{text}");
        assert!(text.contains("measured time"), "{text}");
        assert!(text.contains("25.70 ms"), "{text}");
    }

    #[test]
    fn plan_metrics_keep_width_and_startup_time_distinct_from_cost() {
        let mut model = connected_model(Environment::Local);
        model.focus = Focus::Results;
        model.sidebar_visible = false;
        let document = crate::query::PlanDocument::parse(
            r#"[{"Plan":{"Node Type":"Result","Startup Cost":0.1,"Total Cost":2.5,"Plan Rows":3,"Plan Width":32,"Actual Startup Time":0.25,"Actual Total Time":1.5,"Actual Rows":3,"Actual Loops":2}}]"#,
            true,
        )
        .expect("plan fixture");
        let metrics = plan_metrics(&document.root, true);
        model.plan.ready(document);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 34);
        assert!(text.contains("width 32"), "{text}");
        assert!(text.contains("actual startup 0.25 ms"), "{text}");
        assert!(text.contains("actual total 1.50 ms/loop"), "{text}");
        assert!(metrics.contains("cost 0.10..2.50 cost units"), "{metrics}");
    }

    #[test]
    fn plan_text_survives_narrow_ascii_and_hostile_server_fields() {
        let mut model = plan_model(false);
        model.plan.ready(
            crate::query::PlanDocument::parse(
                r#"[{"Plan":{"Node Type":"Seq\nScan\u001b[31m","Relation Name":"orders","Filter":"password=hunter2-not-a-real-password"}}]"#,
                false,
            )
            .expect("hostile plan fixture"),
        );
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, false, GlyphTier::Ascii),
            60,
            20,
        );
        assert!(
            text.contains("Seq\\nScan"),
            "control text is escaped: {text}"
        );
        assert!(
            text.contains("[redacted]"),
            "secret-shaped text is redacted: {text}"
        );
        for line in text.lines() {
            assert!(
                display_width(line.trim_end()) <= 60,
                "line overflowed: {line:?}"
            );
        }
    }

    #[test]
    fn analyze_confirmation_states_execution_and_side_effects_before_enter() {
        let mut model = connected_model(Environment::Local);
        model.pending_plan = Some(crate::app::model::PendingPlan {
            sql: "SELECT do_work()".into(),
            impact: crate::query::Impact::Read,
            typed: String::new(),
            required: String::new(),
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 120, 30);
        assert!(text.contains("will execute the statement"), "{text}");
        assert!(text.contains("Side effects are possible"), "{text}");
        assert!(text.contains("not automatically roll it back"), "{text}");
        assert!(text.contains("Enter to execute once"), "{text}");
    }

    #[test]
    fn the_layout_is_chosen_from_the_available_space() {
        assert_eq!(layout_mode(Rect::new(0, 0, 120, 40)), LayoutMode::Full);
        assert_eq!(layout_mode(Rect::new(0, 0, 80, 24)), LayoutMode::Full);
        assert_eq!(layout_mode(Rect::new(0, 0, 60, 20)), LayoutMode::Compact);
        assert_eq!(layout_mode(Rect::new(0, 0, 30, 20)), LayoutMode::TooSmall);
        assert_eq!(layout_mode(Rect::new(0, 0, 100, 5)), LayoutMode::TooSmall);
    }

    #[test]
    fn a_narrow_terminal_gets_one_pane_not_a_clipped_layout() {
        let mut model = connected_model(Environment::Production);
        model.focus = Focus::Editor;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 60, 20);

        assert!(text.contains("Editor"), "{text}");
        assert!(
            !text.contains("Results"),
            "the unfocused pane is dropped, not squeezed"
        );
        assert!(text.contains("[PROD]"), "the production marker survives");
        for line in text.lines() {
            assert!(
                display_width(line.trim_end()) <= 60,
                "line overflowed: {line:?}"
            );
        }
    }

    #[test]
    fn a_tiny_terminal_explains_itself_and_offers_a_way_out() {
        let model = connected_model(Environment::Local);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 30, 6);
        assert!(text.contains("Terminal too small"), "{text}");
        assert!(text.contains("80x24"), "the minimum is stated");
        assert!(text.contains("30x6"), "the current size is stated");
        assert!(
            text.contains("ignatius query"),
            "a line-oriented route is offered"
        );
    }

    #[test]
    fn help_lists_every_binding_and_how_to_close_it() {
        let mut model = connected_model(Environment::Local);
        model.help_open = true;
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Help"), "{text}");
        assert!(text.contains("Esc to close"));
        assert!(text.contains("Run the whole buffer"));
        assert!(text.contains("Cancel the running statement"));
    }

    #[test]
    fn wide_characters_do_not_overflow_the_line() {
        let mut model = connected_model(Environment::Local);
        with_rows(
            &mut model,
            &["\u{540d}\u{524d}"],
            &[&[Cell::Text(
                "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}".into(),
            )]],
        );
        let text = render_to_string(&model, &Keymap::new(), &rich(), 80, 24);
        for line in text.lines() {
            assert!(
                display_width(line.trim_end()) <= 80,
                "line is {} cells: {line:?}",
                display_width(line.trim_end())
            );
        }
    }

    #[test]
    fn a_disconnected_model_still_renders_without_panicking() {
        let model = Model::new(100);
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Not connected"), "{text}");
        assert!(text.contains("[UNCLASSIFIED]"));
    }

    #[test]
    fn every_size_renders_without_panicking() {
        // Resize storms and odd geometries must not be able to crash the client.
        let mut model = connected_model(Environment::Production);
        with_rows(
            &mut model,
            &["a", "b", "c"],
            &[&[Cell::Text("1".into()), Cell::Null, Cell::Text("x".into())]],
        );
        model.help_open = true;
        model.phase = QueryPhase::Running {
            job: JobId(1),
            statements: 1,
        };
        for width in [1u16, 2, 8, 39, 40, 41, 79, 80, 81, 200] {
            for height in [1u16, 2, 7, 8, 9, 23, 24, 25, 60] {
                let _ = render_to_string(&model, &Keymap::new(), &rich(), width, height);
            }
        }
    }
}
