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

use crate::app::model::{Focus, Model, SplitState};
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
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

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
    if let Some(inspector) = &model.inspector {
        render_inspector(model, *inspector, presentation, area, buf);
    }
    if let Some(definition) = &model.definition {
        render_definition(definition, presentation, area, buf);
    }
    if model.help_open {
        render_help(model, presentation, area, buf);
    }
    if model.notice_details.is_some() {
        crate::ui::notices::render(model, presentation, area, buf);
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
    if model.pending_run.is_some() {
        render_confirmation(model, presentation, area, buf);
    }
    if model.pending_plan.is_some() {
        render_plan_confirmation(model, presentation, area, buf);
    }
    if model.pending_copy.is_some() {
        render_copy_confirmation(model, presentation, area, buf);
    }
    if model.pending_update.is_some() {
        render_update_review(model, presentation, area, buf);
    }
    if let Some(prompt) = &model.update_prompt {
        render_update_value_prompt(prompt, presentation, area, buf);
    }
    if model.parameter_prompt.is_some() {
        render_parameter_prompt(model, presentation, area, buf);
    }
    if let Some(prompt) = &model.name_prompt {
        render_name_prompt(prompt, presentation, area, buf);
    }
    // Topmost, because nothing else can be done until the connection is open.
    if let Some(prompt) = &model.password_prompt {
        render_password_prompt(prompt, presentation, area, buf);
    }
}

/// The initial sidebar heuristic, before any explicit adjustment.
#[must_use]
pub const fn sidebar_width(available: u16) -> u16 {
    let proportional = available as u32 * 2 / 5;
    if proportional < 24 {
        24
    } else if proportional > 34 {
        34
    } else {
        proportional as u16
    }
}

/// Effective pane rectangles shared by drawing, input and navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaneGeometry {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
    pub objects: Option<Rect>,
    pub editor: Option<Rect>,
    pub results: Option<Rect>,
    pub vertical_split: Option<Rect>,
    pub horizontal_split: Option<Rect>,
}

/// The unzoomed editor/results rectangles. Preferences survive small windows;
/// only the effective rectangles clamp to the available space.
pub(crate) fn main_panes(body: Rect, sidebar_visible: bool, splits: &SplitState) -> (Rect, Rect) {
    let width = if sidebar_visible {
        effective_sidebar(body.width, splits)
    } else {
        0
    };
    let main = Rect::new(
        body.x.saturating_add(width),
        body.y,
        body.width.saturating_sub(width),
        body.height,
    );
    let [initial, _] =
        Layout::vertical([Constraint::Percentage(40), Constraint::Min(3)]).areas(main);
    let minimum = 3.min(main.height / 2);
    let editor_height = splits
        .editor_rows
        .unwrap_or(initial.height)
        .clamp(minimum, main.height.saturating_sub(minimum));
    (
        Rect::new(main.x, main.y, main.width, editor_height),
        Rect::new(
            main.x,
            main.y.saturating_add(editor_height),
            main.width,
            main.height.saturating_sub(editor_height),
        ),
    )
}

fn effective_sidebar(available: u16, splits: &SplitState) -> u16 {
    let maximum = 60.min(available.saturating_sub(30));
    splits
        .sidebar_cells
        .unwrap_or_else(|| sidebar_width(available))
        .clamp(20.min(maximum), maximum)
}

/// The body excludes the persistent header and footer.
pub(crate) fn body_area(area: Rect) -> Rect {
    let [_, body, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .areas(area);
    body
}

/// One layout calculation for all consumers, including compact safety views.
pub(crate) fn pane_geometry(model: &Model, area: Rect) -> PaneGeometry {
    let [header, _, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let body = body_area(area);
    let mut geometry = PaneGeometry {
        header,
        body,
        footer,
        objects: None,
        editor: None,
        results: None,
        vertical_split: None,
        horizontal_split: None,
    };
    match layout_mode(area) {
        LayoutMode::TooSmall => {}
        LayoutMode::Compact => {
            if model.transaction == crate::query::result::TransactionState::Failed {
                geometry.results = Some(body);
            } else if model.error.is_some() {
                let error_height = body.height.saturating_sub(4).clamp(3, 8);
                let [error, editor] =
                    Layout::vertical([Constraint::Length(error_height), Constraint::Min(1)])
                        .areas(body);
                geometry.results = Some(error);
                geometry.editor = Some(editor);
            } else {
                match model.focus {
                    Focus::Editor => geometry.editor = Some(body),
                    Focus::Results => geometry.results = Some(body),
                    Focus::Objects => geometry.objects = Some(body),
                }
            }
        }
        LayoutMode::Full if model.splits.zoomed => match model.focus {
            Focus::Editor => geometry.editor = Some(body),
            Focus::Results => geometry.results = Some(body),
            Focus::Objects => geometry.objects = Some(body),
        },
        LayoutMode::Full => {
            let (editor, results) = main_panes(body, model.sidebar_visible, &model.splits);
            geometry.editor = Some(editor);
            geometry.results = Some(results);
            geometry.horizontal_split = Some(Rect::new(
                editor.x,
                results.y.saturating_sub(1),
                editor.width,
                2,
            ));
            if model.sidebar_visible {
                let width = effective_sidebar(body.width, &model.splits);
                geometry.objects = Some(Rect::new(body.x, body.y, width, body.height));
                geometry.vertical_split = Some(Rect::new(
                    body.x.saturating_add(width).saturating_sub(1),
                    body.y,
                    2,
                    body.height,
                ));
            }
        }
    }
    geometry
}

fn model_geometry(model: &Model) -> PaneGeometry {
    pane_geometry(model, Rect::new(0, 0, model.size.0, model.size.1))
}

/// A page of editor text in the currently visible pane.
#[must_use]
pub fn editor_page(model: &Model) -> usize {
    model_geometry(model)
        .editor
        .map_or(0, crate::ui::widgets::editor::content_capacity)
        .max(1)
}

/// A page of drawable result rows or Plan tree nodes.
#[must_use]
pub fn results_page(model: &Model) -> usize {
    model_geometry(model)
        .results
        .map_or(0, |pane| {
            if model.plan.is_visible() {
                model
                    .plan
                    .document()
                    .map_or(0, |_| crate::ui::widgets::overlays::plan_capacity(pane))
            } else if model.expanded_row
                || model.error.is_some()
                || model.transaction == crate::query::result::TransactionState::Failed
            {
                0
            } else {
                crate::ui::widgets::results::grid_capacity(
                    model,
                    pane,
                    model.presentation.spacing(),
                )
            }
        })
        .max(1)
}

/// A page of the tree's visible rows, including its optional filter line.
pub(crate) fn objects_page(model: &Model) -> usize {
    model_geometry(model)
        .objects
        .map_or(0, |pane| {
            crate::ui::widgets::objects::content_capacity(model, pane)
        })
        .max(1)
}

fn render_full(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_panes(model, keymap, presentation, area, buf);
}

/// Compact layout retains the existing failed-transaction and source/error
/// safety views; it shares geometry with navigation and hit testing.
fn render_compact(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_panes(model, keymap, presentation, area, buf);
}

fn render_panes(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let geometry = pane_geometry(model, area);
    if layout_mode(area) == LayoutMode::Compact {
        render_compact_header(model, presentation, geometry.header, buf);
    } else {
        render_header(model, presentation, geometry.header, buf);
    }
    if let Some(sidebar) = geometry.objects {
        render_objects(model, keymap, presentation, sidebar, buf);
    }
    if let Some(editor) = geometry.editor {
        render_editor(model, keymap, presentation, editor, buf);
    }
    if let Some(results) = geometry.results {
        if model.transaction == crate::query::result::TransactionState::Failed {
            render_results(model, keymap, presentation, results, buf);
        } else if let Some(error) = &model.error {
            render_error(model, error, presentation, results, buf);
        } else {
            render_results(model, keymap, presentation, results, buf);
        }
    }
    let notice_fits_footer = render_footer(model, keymap, presentation, geometry.footer, buf);
    if !notice_fits_footer
        && let Some(notice) = model.layout_notice
        && geometry.body.height > 0
        && geometry.body.width > 2
    {
        // Preserve transaction facts and every content row. Only this existing
        // bottom border becomes a temporary caption when the footer is full.
        let rail = Rect::new(
            geometry.body.x.saturating_add(1),
            geometry.body.bottom().saturating_sub(1),
            geometry.body.width.saturating_sub(2),
            1,
        );
        let wordings = notice.wordings();
        let wording = wordings
            .into_iter()
            .find(|wording| display_width(wording) <= usize::from(rail.width))
            .unwrap_or(wordings[2]);
        Clear.render(rail, buf);
        Paragraph::new(wording)
            .style(
                presentation
                    .theme
                    .surface(Token::SurfacePane)
                    .patch(presentation.theme.style(Token::Info)),
            )
            .render(rail, buf);
    }
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

/// Positioning retained by the existing overlay families.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OverlayAnchor {
    Center,
    Top,
    Bottom,
}

/// Preferred clearance yields only when needed for usable interaction rows.
pub(crate) fn overlay_box(
    area: Rect,
    presentation: &Presentation,
    maximum: (u16, u16),
    minimum_rows: u16,
    anchor: OverlayAnchor,
) -> Rect {
    let margin = presentation.spacing().overlay_margin;
    let horizontal = margin.min(area.width.saturating_sub(1) / 2);
    let vertical = margin.min(area.height.saturating_sub(minimum_rows.min(area.height)) / 2);
    let width = area
        .width
        .saturating_sub(horizontal.saturating_mul(2))
        .min(maximum.0);
    let height = area
        .height
        .saturating_sub(vertical.saturating_mul(2))
        .min(maximum.1);
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let y = match anchor {
        OverlayAnchor::Center => area
            .y
            .saturating_add(area.height.saturating_sub(height) / 2),
        OverlayAnchor::Top => area.y.saturating_add(vertical),
        OverlayAnchor::Bottom => area
            .bottom()
            .saturating_sub(vertical)
            .saturating_sub(height),
    };
    Rect::new(x, y, width, height)
}

/// Protected prompts keep every previously usable vertical interaction row.
/// Preferred margins may widen the box, but may not push an input or review
/// disclosure below the viewport merely to make room outside its border.
pub(crate) fn prompt_box(area: Rect, presentation: &Presentation, maximum: (u16, u16)) -> Rect {
    let height = maximum.1.min(area.height.saturating_sub(2));
    overlay_box(
        area,
        presentation,
        (maximum.0, height),
        height,
        OverlayAnchor::Center,
    )
}

/// Widest the inspector is ever drawn, whatever the terminal allows.
const INSPECTOR_MAX_WIDTH: u16 = 92;
/// Tallest the inspector is ever drawn.
const INSPECTOR_MAX_HEIGHT: u16 = 26;
/// Columns the frame and its padding take from the value.
const INSPECTOR_CHROME_COLUMNS: u16 = 4;
/// Rows the frame, the heading and the position line take from the value.
const INSPECTOR_CHROME_ROWS: u16 = 5;

/// The box the inspector occupies inside an area.
pub(crate) fn inspector_box(area: Rect, presentation: &Presentation) -> Rect {
    overlay_box(
        area,
        presentation,
        (INSPECTOR_MAX_WIDTH, INSPECTOR_MAX_HEIGHT),
        INSPECTOR_CHROME_ROWS + 1,
        OverlayAnchor::Center,
    )
}

/// How much of a value the inspector can show at a terminal size: width in
/// cells, height in lines.
///
/// The reducer clamps scrolling with this and the renderer lays out with it, so
/// "how much fits" has one answer rather than two that drift apart.
#[must_use]
pub fn inspector_viewport(size: (u16, u16), presentation: &Presentation) -> (usize, usize) {
    let box_area = inspector_box(
        Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        },
        presentation,
    );
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
pub(crate) fn definition_box(area: Rect, presentation: &Presentation) -> Rect {
    overlay_box(
        area,
        presentation,
        (100, 34),
        DEFINITION_CHROME_ROWS + 1,
        OverlayAnchor::Center,
    )
}

/// How much of a definition is on screen: width in cells, height in lines.
///
/// One function for the renderer and for the reducer that clamps scrolling.
#[must_use]
pub fn definition_viewport(size: (u16, u16), presentation: &Presentation) -> (usize, usize) {
    let box_area = definition_box(
        Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        },
        presentation,
    );
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
mod tests;
