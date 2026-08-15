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

use crate::app::model::{Focus, Model, QueryPhase};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::{Glyphs, Icon};
use crate::ui::keymap::Keymap;
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

/// How the interface is being presented.
#[derive(Debug, Clone)]
pub struct Presentation {
    /// Palette and whether colour may be emitted.
    pub theme: Theme,
    /// Which icons and box characters may be drawn.
    pub glyphs: Glyphs,
    /// When true, indicators are static text rather than animation.
    pub reduced_motion: bool,
}

impl Presentation {
    /// Builds a presentation.
    #[must_use]
    pub const fn new(theme: Theme, glyphs: Glyphs, reduced_motion: bool) -> Self {
        Self {
            theme,
            glyphs,
            reduced_motion,
        }
    }

    fn icon(&self, icon: Icon) -> String {
        self.glyphs.prefix(icon)
    }
}

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

/// Draws the whole interface.
pub fn render(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    Block::new()
        .style(presentation.theme.style(Token::Text))
        .render(area, buf);

    match layout_mode(area) {
        LayoutMode::TooSmall => render_too_small(presentation, area, buf),
        LayoutMode::Compact => render_compact(model, keymap, presentation, area, buf),
        LayoutMode::Full => render_full(model, keymap, presentation, area, buf),
    }

    // Overlays are drawn in the order Esc peels them, so the topmost one is
    // always the one a keypress will act on.
    if model.help_open {
        render_help(keymap, presentation, area, buf);
    }
    if let Some(palette) = &model.palette {
        render_palette(palette, presentation, area, buf);
    }
    if model.prefix_pending {
        render_chords(presentation, area, buf);
    }
    if let Some(pending) = &model.pending_run {
        render_confirmation(model, pending, presentation, area, buf);
    }
}

/// Asks before a write reaches a database the user called production.
///
/// It states what will run, what it appears to do, and that the judgement is
/// advisory. A prompt that overstates its own certainty teaches people to
/// distrust it, which costs more than it saves.
fn render_confirmation(
    model: &Model,
    pending: &crate::app::model::PendingRun,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(78);
    let height = 12.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let target = model
        .connection
        .info()
        .map_or_else(|| "this database".to_owned(), |info| info.target.clone());

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!(
                    " {}[{}] ",
                    presentation.icon(Icon::Production),
                    model.environment().label()
                ),
                theme.capsule(Token::EnvironmentProduction),
            ),
            Span::styled(format!(" {target}"), theme.style(Token::Text)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("This statement {}.", pending.impact.label()),
            theme.style(Token::Danger),
        )),
    ];

    for line in pending.sql.lines().take(4) {
        lines.push(Line::from(Span::styled(
            format!("  {}", sanitize_for_display(line)),
            theme.style(Token::Muted),
        )));
    }

    lines.push(Line::from(""));
    if pending.impact.needs_typed_confirmation() {
        lines.push(Line::from(Span::styled(
            format!("Type the database name to continue: {}", pending.required),
            theme.style(Token::Warning),
        )));
        lines.push(Line::from(vec![
            Span::styled("  ", theme.style(Token::Text)),
            Span::styled(
                sanitize_for_display(&pending.typed),
                theme.style(if pending.is_satisfied() {
                    Token::Success
                } else {
                    Token::Text
                }),
            ),
            Span::styled("\u{2588}", theme.style(Token::Focus)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Enter to run it, Esc to cancel.",
            theme.style(Token::Info),
        )));
    }

    lines.push(Line::from(Span::styled(
        "This reads leading keywords only. Database permissions are the real control.",
        theme.style(Token::Muted),
    )));

    Paragraph::new(lines)
        .block(pane_block(
            " Confirm  Esc to cancel ".to_owned(),
            true,
            presentation,
        ))
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
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
    let main = if model.sidebar_visible {
        let width = sidebar_width(body.width);
        let [sidebar, main] =
            Layout::horizontal([Constraint::Length(width), Constraint::Min(30)]).areas(body);
        render_objects(model, presentation, sidebar, buf);
        main
    } else {
        body
    };

    let [editor, results] =
        Layout::vertical([Constraint::Percentage(40), Constraint::Min(3)]).areas(main);

    render_header(model, presentation, header, buf);
    render_editor(model, presentation, editor, buf);
    if let Some(error) = &model.error {
        render_error(model, error, presentation, results, buf);
    } else {
        render_results(model, presentation, results, buf);
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
    if let Some(error) = &model.error {
        render_error(model, error, presentation, body, buf);
    } else {
        match model.focus {
            Focus::Editor => render_editor(model, presentation, body, buf),
            Focus::Results => render_results(model, presentation, body, buf),
            Focus::Objects => render_objects(model, presentation, body, buf),
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

// ------------------------------------------------------------------- header

fn render_header(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let environment = model.environment();
    let mut spans = vec![Span::styled(
        format!(
            " {}{} ",
            presentation.icon(Icon::Brand),
            crate::branding::PRODUCT_NAME
        ),
        theme.style(Token::Header),
    )];

    // The classification is a bracketed word in every mode, so it is legible
    // with no colour and no icons. Production additionally gets a filled
    // capsule, because it is the one marker that must never be skimmed past.
    let label = format!(
        " {}[{}] ",
        presentation.icon(if environment.is_production() {
            Icon::Production
        } else {
            Icon::Environment
        }),
        environment.label()
    );
    spans.push(if environment.is_production() {
        Span::styled(label, theme.capsule(Token::EnvironmentProduction))
    } else {
        Span::styled(label, theme.style(Token::EnvironmentNonProduction))
    });

    if let Some(info) = model.connection.info() {
        let (icon, token) = if info.read_only {
            (Icon::ReadOnly, Token::Info)
        } else {
            (Icon::ReadWrite, Token::Muted)
        };
        spans.push(Span::styled(
            format!("{}[{}]", presentation.icon(icon), info.posture()),
            theme.style(token),
        ));
    }

    spans.push(Span::styled(
        presentation.glyphs.separator(),
        theme.style(Token::Border),
    ));
    spans.push(Span::styled(
        format!(
            "{}{}",
            presentation.icon(Icon::Database),
            model.connection.label()
        ),
        theme.style(Token::Text),
    ));

    if let Some(info) = model.connection.info() {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        let (icon, token) = tls_appearance(&info.tls);
        spans.push(Span::styled(
            format!("{}{}", presentation.icon(icon), info.tls.label()),
            theme.style(token),
        ));
    }

    if let Some(elapsed) = model.last_elapsed {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!(
                "{}{} ms",
                presentation.icon(Icon::Clock),
                elapsed.as_millis()
            ),
            theme.style(Token::Muted),
        ));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

/// The icon and token for a transport state.
///
/// Encryption with a verified identity, encryption without one, and no
/// encryption at all are three different facts and are drawn three ways.
const fn tls_appearance(state: &crate::postgres::TlsState) -> (Icon, Token) {
    use crate::connection::SslMode;
    use crate::postgres::TlsState;
    match state {
        TlsState::Active { mode, .. } => {
            if matches!(mode, SslMode::VerifyFull | SslMode::VerifyCa) {
                (Icon::LockVerified, Token::Success)
            } else {
                (Icon::LockPlain, Token::Warning)
            }
        }
        TlsState::Disabled | TlsState::NotNegotiated | TlsState::Unknown => {
            (Icon::LockOpen, Token::Warning)
        }
    }
}

fn render_compact_header(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let environment = model.environment();
    let database = model
        .connection
        .info()
        .map_or_else(|| "not connected".to_owned(), |i| i.database.clone());
    let label = format!(" [{}] ", environment.label());
    let spans = vec![
        if environment.is_production() {
            Span::styled(label, theme.capsule(Token::EnvironmentProduction))
        } else {
            Span::styled(label, theme.style(Token::EnvironmentNonProduction))
        },
        Span::styled(
            format!("{}{database}", presentation.icon(Icon::Database)),
            theme.style(Token::Text),
        ),
        Span::styled(
            format!(
                " {}[{}]",
                presentation.icon(Icon::Focus),
                model.focus.label()
            ),
            theme.style(Token::Focus),
        ),
    ];
    Paragraph::new(Line::from(spans)).render(area, buf);
}

// ------------------------------------------------------------------- editor

fn render_editor(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Editor;
    let (line, column) = model.editor.position();
    let mut title = format!(
        " {}Editor  line {line}, column {column} ",
        presentation.icon(Icon::Editor)
    );
    if model.editor.is_modified() {
        title.push_str("[modified] ");
    }
    if focused {
        title.push_str("[focused] ");
    }

    let block = pane_block(title, focused, presentation);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let text = model.editor.text();
    if text.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "Type SQL here, then press Ctrl+R to run it.",
            theme.style(Token::Muted),
        )))
        .render(inner, buf);
        return;
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let gutter = lines.len().to_string().len().max(2);
    let (cursor_line, cursor_column) = model.editor.position();

    let rendered: Vec<Line> = lines
        .iter()
        .enumerate()
        .take(inner.height as usize)
        .map(|(index, source)| {
            let number = index + 1;
            let mut spans = vec![Span::styled(
                format!("{number:>gutter$} "),
                theme.style(Token::Muted),
            )];
            let safe = sanitize_for_display(source);
            // A block cursor drawn by the renderer, because the terminal's own
            // cursor is hidden while the alternate screen is in use. Without it
            // the editor would have no visible caret at all.
            if focused && number == cursor_line {
                let split = safe
                    .char_indices()
                    .nth(cursor_column - 1)
                    .map_or(safe.len(), |(i, _)| i);
                let (before, rest) = safe.split_at(split);
                let mut chars = rest.chars();
                let under = chars.next();
                spans.push(Span::styled(before.to_owned(), theme.style(Token::Text)));
                spans.push(Span::styled(
                    under.map_or_else(|| " ".to_owned(), |c| c.to_string()),
                    theme.style(Token::Selection),
                ));
                spans.push(Span::styled(
                    chars.as_str().to_owned(),
                    theme.style(Token::Text),
                ));
            } else {
                spans.push(Span::styled(safe, theme.style(Token::Text)));
            }
            Line::from(spans)
        })
        .collect();

    Paragraph::new(rendered).render(inner, buf);
}

// ------------------------------------------------------------------ results

fn render_results(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Results;

    let Some(execution) = &model.last_execution else {
        render_results_placeholder(model, presentation, focused, area, buf);
        return;
    };

    // A failed transaction is not an ordinary error: nothing else will run until
    // it ends, so the way out is shown rather than left to be remembered.
    if model.transaction == crate::query::result::TransactionState::Failed && model.error.is_none()
    {
        let block = pane_block(
            format!(" {}Transaction failed ", presentation.icon(Icon::Warning)),
            focused,
            presentation,
        );
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(vec![
            Line::from(Span::styled(
                "This transaction has failed.",
                theme.style(Token::TransactionFailed),
            )),
            Line::from(Span::styled(
                model
                    .transaction
                    .recovery()
                    .unwrap_or("Run ROLLBACK to end it."),
                theme.style(Token::Info),
            )),
        ])
        .wrap(Wrap { trim: true })
        .render(inner, buf);
        return;
    }

    let summary = execution
        .statements
        .first()
        .map_or_else(|| execution.status.label().to_owned(), |s| s.summary());
    let mut title = format!(
        " {}Results  {}  {} ",
        presentation.icon(Icon::Rows),
        execution.status.label(),
        summary
    );
    if focused {
        title.push_str("[focused] ");
    }

    let block = pane_block(title, focused, presentation);
    let inner = block.inner(area);
    block.render(area, buf);

    let Some(set) = model.visible_result() else {
        let text = if execution.statements.is_empty() {
            execution.status.label().to_owned()
        } else {
            execution
                .statements
                .iter()
                .map(crate::query::result::StatementResult::summary)
                .collect::<Vec<_>>()
                .join("; ")
        };
        Paragraph::new(Line::from(vec![
            Span::styled(presentation.icon(Icon::Info), theme.style(Token::Info)),
            Span::styled(text, theme.style(Token::Text)),
        ]))
        .render(inner, buf);
        return;
    };

    if inner.height < 2 || inner.width == 0 {
        return;
    }

    render_grid(model, set, presentation, focused, inner, buf);
}

fn render_results_placeholder(
    model: &Model,
    presentation: &Presentation,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let block = pane_block(
        format!(" {}Results ", presentation.icon(Icon::Rows)),
        focused,
        presentation,
    );
    let inner = block.inner(area);
    block.render(area, buf);

    // While a query is in flight this is where the activity indicator lives, so
    // the user is looking at the place the answer will appear.
    if model.phase.is_busy() {
        Paragraph::new(running_lines(model, presentation)).render(inner, buf);
        return;
    }

    let hint = if model.connection.is_usable() {
        "No query has run yet. Press Ctrl+R to run the buffer."
    } else {
        "Not connected. Results appear here once a query runs."
    };
    Paragraph::new(Line::from(vec![
        Span::styled(presentation.icon(Icon::Info), theme.style(Token::Muted)),
        Span::styled(hint, theme.style(Token::Muted)),
    ]))
    .render(inner, buf);
}

/// The activity indicator: a moving frame, a word, and an honest elapsed time.
fn running_lines<'a>(model: &Model, presentation: &Presentation) -> Vec<Line<'a>> {
    let theme = &presentation.theme;
    let elapsed = model.running_for.unwrap_or_default();
    let seconds = elapsed.as_secs_f64();

    let leader = if presentation.reduced_motion {
        String::new()
    } else {
        format!("{} ", presentation.glyphs.spinner(model.frame))
    };
    let word = match model.phase {
        QueryPhase::CancellationRequested { .. } => "Cancellation requested",
        _ => "Running",
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(leader, theme.style(Token::Info)),
        Span::styled(word.to_owned(), theme.style(Token::Text)),
        Span::styled(
            format!("  {}{seconds:.1}s", presentation.icon(Icon::Clock)),
            theme.style(Token::Muted),
        ),
    ])];

    if !presentation.reduced_motion {
        // A meter that shows time passing, not progress. The server reports no
        // progress, so nothing here may imply a percentage.
        let width = 24usize;
        let filled = usize::try_from(model.frame % (width as u64 + 1)).unwrap_or(0);
        lines.push(Line::from(Span::styled(
            presentation.glyphs.meter(filled, width),
            theme.style(Token::Border),
        )));
    }

    lines.push(Line::from(Span::styled(
        match model.phase {
            QueryPhase::CancellationRequested { .. } => {
                "The server has been asked to stop. It may still be running.".to_owned()
            }
            _ => "Ctrl+C asks the server to cancel.".to_owned(),
        },
        theme.style(Token::Muted),
    )));

    lines
}

/// Draws the result grid: row numbers, headers, striped rows, and a scroll bar.
fn render_grid(
    model: &Model,
    set: &crate::query::result::ResultSet,
    presentation: &Presentation,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let rule = presentation.glyphs.column_rule();

    let visible_rows = (area.height as usize).saturating_sub(2).max(1);
    let needs_scrollbar = set.rows.len() > visible_rows;
    let grid_width = (area.width as usize).saturating_sub(usize::from(needs_scrollbar));

    // Keep the selection on screen. The window moves; rows never reorder.
    let offset = model
        .selected_row
        .saturating_sub(visible_rows.saturating_sub(1));
    let gutter = set.rows.len().max(1).to_string().len().max(2);

    let alignment = column_alignment(set);
    let widths = column_widths(set, grid_width.saturating_sub(gutter + 1));

    let mut lines: Vec<Line> = Vec::with_capacity(visible_rows + 2);

    let mut header_spans = vec![Span::styled(
        format!("{} ", " ".repeat(gutter)),
        theme.style(Token::Muted),
    )];
    for (index, (name, width)) in set.columns.iter().zip(&widths).enumerate() {
        if index > 0 {
            header_spans.push(Span::styled(format!("{rule} "), theme.style(Token::Border)));
        }
        let text = truncate_to_width(
            &sanitize_for_display(name),
            *width,
            !presentation.glyphs.is_ascii(),
        );
        header_spans.push(Span::styled(
            format!("{} ", pad_to_width(&text, *width)),
            theme.style(Token::Header),
        ));
    }
    lines.push(Line::from(header_spans));

    let rule_char = if presentation.glyphs.is_ascii() {
        "-"
    } else {
        "\u{2500}"
    };
    lines.push(Line::from(Span::styled(
        rule_char.repeat(grid_width),
        theme.style(Token::Border),
    )));

    for (offset_index, row) in set.rows.iter().skip(offset).take(visible_rows).enumerate() {
        let row_index = offset + offset_index;
        let selected = focused && row_index == model.selected_row;
        let striped = row_index % 2 == 1;

        let mut spans = vec![Span::styled(
            format!("{:>gutter$} ", row_index + 1),
            cell_style(theme, Token::Muted, selected, striped),
        )];

        for (index, (cell, width)) in row.iter().zip(&widths).enumerate() {
            if index > 0 {
                spans.push(Span::styled(
                    format!("{rule} "),
                    cell_style(theme, Token::Border, selected, striped),
                ));
            }
            let token = if cell.is_null() {
                Token::NullValue
            } else {
                Token::Text
            };
            let rendered =
                truncate_to_width(&cell.display(), *width, !presentation.glyphs.is_ascii());
            let padded = if alignment.get(index).copied().unwrap_or(false) {
                // Right-aligned by the shape of the value, not by its type: the
                // simple query protocol reports no type information, so this is
                // a reading aid and is never described as type-aware.
                let pad = width.saturating_sub(display_width(&rendered));
                format!("{}{rendered} ", " ".repeat(pad))
            } else {
                format!("{} ", pad_to_width(&rendered, *width))
            };
            spans.push(Span::styled(
                padded,
                cell_style(theme, token, selected, striped),
            ));
        }

        let mut line = Line::from(spans);
        if selected {
            line = line.style(theme.style(Token::Selection));
        } else if striped {
            line = line.style(theme.stripe());
        }
        lines.push(line);
    }

    Paragraph::new(lines).render(area, buf);

    if needs_scrollbar {
        render_scrollbar(
            set.rows.len(),
            visible_rows,
            offset,
            presentation,
            Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 2,
                width: 1,
                height: area.height.saturating_sub(2),
            },
            buf,
        );
    }
}

fn cell_style(theme: &Theme, token: Token, selected: bool, striped: bool) -> Style {
    if selected {
        theme.style(Token::Selection)
    } else if striped {
        theme.on_stripe(token)
    } else {
        theme.style(token)
    }
}

/// A scroll indicator, so rows below the fold are visibly below the fold.
fn render_scrollbar(
    total: usize,
    visible: usize,
    offset: usize,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height == 0 || total == 0 {
        return;
    }
    let track = area.height as usize;
    let thumb = ((visible * track) / total).clamp(1, track);
    let max_offset = total.saturating_sub(visible).max(1);
    let position = (offset * track.saturating_sub(thumb)) / max_offset;

    let (full, empty) = if presentation.glyphs.is_ascii() {
        ("#", "|")
    } else {
        ("\u{2588}", "\u{2502}")
    };

    for row in 0..track {
        let inside = row >= position && row < position + thumb;
        let symbol = if inside { full } else { empty };
        let style = presentation
            .theme
            .style(if inside { Token::Focus } else { Token::Border });
        buf.set_string(
            area.x,
            area.y + u16::try_from(row).unwrap_or(0),
            symbol,
            style,
        );
    }
}

/// Decides which columns read as numbers and should be right-aligned.
fn column_alignment(set: &crate::query::result::ResultSet) -> Vec<bool> {
    (0..set.columns.len())
        .map(|index| {
            let mut saw_value = false;
            for row in &set.rows {
                let Some(cell) = row.get(index) else { continue };
                let Some(text) = cell.raw() else { continue };
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                saw_value = true;
                if trimmed.parse::<f64>().is_err() {
                    return false;
                }
            }
            saw_value
        })
        .collect()
}

/// Shares the available width between columns, giving every column something.
fn column_widths(set: &crate::query::result::ResultSet, available: usize) -> Vec<usize> {
    let count = set.columns.len();
    if count == 0 {
        return Vec::new();
    }
    let mut widths: Vec<usize> = set
        .columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let widest_value = set
                .rows
                .iter()
                .filter_map(|row| row.get(i))
                .map(|cell| display_width(&cell.display()))
                .max()
                .unwrap_or(0);
            display_width(&sanitize_for_display(name))
                .max(widest_value)
                .max(3)
        })
        .collect();

    // Each column carries a trailing space, and every column after the first
    // carries a rule and a space as well.
    let overhead = count + (count.saturating_sub(1) * 2);
    let total: usize = widths.iter().sum::<usize>() + overhead;
    if total > available && available > overhead {
        let budget = available - overhead;
        let current: usize = widths.iter().sum();
        if current > 0 {
            #[allow(clippy::cast_precision_loss)]
            let scale = budget as f64 / current as f64;
            for width in &mut widths {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                let scaled = ((*width as f64) * scale).floor() as usize;
                *width = scaled.max(3);
            }
        }
    }
    widths
}

// ------------------------------------------------------------------ objects

fn render_objects(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Objects;

    // The title doubles as a breadcrumb, so the path to the selected object is
    // visible without a second row of interface.
    let mut title = format!(" {}Objects ", presentation.icon(Icon::Database));
    if let Some(row) = model.tree.selected_row() {
        let separator = if presentation.glyphs.is_ascii() {
            " > "
        } else {
            " \u{203a} "
        };
        let mut trail = Vec::new();
        for depth in 0..=row.depth {
            let ancestor = &row.path[..=depth];
            if let Some(node) = model.tree.node(ancestor) {
                trail.push(node.label());
            }
        }
        title = format!(
            " {}{} ",
            presentation.icon(Icon::Database),
            trail.join(separator)
        );
    }
    if focused {
        title.push_str("[focused] ");
    }

    let block = pane_block(title, focused, presentation);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(error) = &model.tree.error {
        Paragraph::new(vec![
            Line::from(Span::styled(
                error.headline.clone(),
                theme.style(Token::Danger),
            )),
            Line::from(Span::styled(
                error
                    .next_action
                    .clone()
                    .unwrap_or_else(|| "Ctrl+K r reloads the tree.".to_owned()),
                theme.style(Token::Muted),
            )),
        ])
        .wrap(Wrap { trim: true })
        .render(inner, buf);
        return;
    }

    if model.tree.loading && model.tree.roots.is_empty() {
        let leader = if presentation.reduced_motion {
            String::new()
        } else {
            format!("{} ", presentation.glyphs.spinner(model.frame))
        };
        Paragraph::new(Line::from(vec![
            Span::styled(leader, theme.style(Token::Info)),
            Span::styled("Reading the catalogue", theme.style(Token::Text)),
        ]))
        .render(inner, buf);
        return;
    }

    let rows = model.tree.rows();
    let mut lines: Vec<Line> = Vec::new();

    // The filter is shown as a line of its own so it is never mistaken for a row.
    let mut body = inner;
    if model.tree.filtering || !model.tree.filter.is_empty() {
        let prompt = format!(
            " {}{}{}",
            if presentation.glyphs.is_ascii() {
                "/"
            } else {
                "\u{2315}"
            },
            model.tree.filter,
            if model.tree.filtering { "\u{2588}" } else { "" }
        );
        let [filter_area, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        Paragraph::new(Line::from(Span::styled(
            prompt,
            theme.style(if model.tree.filtering {
                Token::Focus
            } else {
                Token::Muted
            }),
        )))
        .render(filter_area, buf);
        body = rest;
    }

    if rows.is_empty() {
        let message = if model.tree.filter.is_empty() {
            "Nothing here yet. Ctrl+K r reloads."
        } else {
            "No object matches this filter."
        };
        Paragraph::new(Line::from(Span::styled(message, theme.style(Token::Muted))))
            .render(body, buf);
        return;
    }

    let visible = body.height as usize;
    let offset = model
        .tree
        .selected
        .saturating_sub(visible.saturating_sub(1))
        .min(rows.len().saturating_sub(1));

    for (index, row) in rows.iter().enumerate().skip(offset).take(visible) {
        let selected = focused && index == model.tree.selected;
        let mut spans = Vec::new();

        // Indent guides, the way a file tree draws them.
        for _ in 0..row.depth {
            spans.push(Span::styled(
                presentation.glyphs.indent_guide(),
                theme.style(Token::Border),
            ));
        }

        if row.expandable {
            spans.push(Span::styled(
                format!("{} ", presentation.glyphs.chevron(row.expanded)),
                theme.style(Token::Muted),
            ));
        } else {
            spans.push(Span::raw("  "));
        }

        let icon = presentation.icon(row_icon(&row.row_kind));
        if !icon.is_empty() {
            spans.push(Span::styled(icon, theme.style(row_token(&row.row_kind))));
        }

        spans.push(Span::styled(
            sanitize_for_display(&row.label),
            if selected {
                theme.style(Token::Selection)
            } else {
                theme.style(row_token(&row.row_kind))
            },
        ));

        if row.loading && !presentation.reduced_motion {
            spans.push(Span::styled(
                format!(" {}", presentation.glyphs.spinner(model.frame)),
                theme.style(Token::Info),
            ));
        } else if row.loading {
            spans.push(Span::styled(" loading", theme.style(Token::Muted)));
        } else if let Some(detail) = &row.detail {
            spans.push(Span::styled(
                format!("  {}", sanitize_for_display(detail)),
                theme.style(Token::Muted),
            ));
        }

        let mut line = Line::from(spans);
        if selected {
            line = line.style(theme.style(Token::Selection));
        }
        lines.push(line);
    }

    Paragraph::new(lines).render(body, buf);
}

/// The icon for a tree row. Icons are supplementary: the label already carries
/// the kind for groups, and the detail carries it for objects.
const fn row_icon(kind: &crate::app::tree::RowKind) -> Icon {
    use crate::app::tree::RowKind;
    use crate::postgres::ObjectKind;
    match kind {
        RowKind::Schema { .. } => Icon::Schema,
        RowKind::Group(object) | RowKind::Object { kind: object, .. } => match object {
            ObjectKind::Table | ObjectKind::PartitionedTable | ObjectKind::ForeignTable => {
                Icon::Table
            }
            ObjectKind::View => Icon::View,
            ObjectKind::MaterializedView => Icon::MaterializedView,
            ObjectKind::Sequence => Icon::Sequence,
            ObjectKind::Function => Icon::Function,
            ObjectKind::Index => Icon::Index,
            ObjectKind::Extension => Icon::Extension,
            ObjectKind::Column => Icon::Column,
        },
        RowKind::Column {
            primary_key: true, ..
        } => Icon::KeyColumn,
        RowKind::Column { .. } => Icon::Column,
        RowKind::Message => Icon::Info,
    }
}

const fn row_token(kind: &crate::app::tree::RowKind) -> Token {
    use crate::app::tree::RowKind;
    match kind {
        RowKind::Schema { usable: false } => Token::Muted,
        RowKind::Schema { .. } => Token::Header,
        RowKind::Group(_) => Token::Muted,
        RowKind::Object {
            readable: false, ..
        } => Token::Warning,
        RowKind::Object { .. } | RowKind::Column { .. } => Token::Text,
        RowKind::Message => Token::Muted,
    }
}

// ------------------------------------------------------------------ palette

fn render_palette(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(8).min(76);
    let height = area.height.saturating_sub(6).min(18);
    let palette_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(palette_area, buf);

    let matches = palette.matches();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" {}", presentation.icon(Icon::Focus)),
            theme.style(Token::Focus),
        ),
        Span::styled(
            sanitize_for_display(&palette.query),
            theme.style(Token::Text),
        ),
        Span::styled("\u{2588}", theme.style(Token::Focus)),
        Span::styled(
            format!("   {} match(es)", matches.len()),
            theme.style(Token::Muted),
        ),
    ])];

    let visible = (height as usize).saturating_sub(3);
    let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
    for (index, entry) in matches.iter().enumerate().skip(offset).take(visible) {
        let selected = index == palette.selected;
        let mut spans = vec![
            Span::styled(format!(" {:<8}", entry.group), theme.style(Token::Muted)),
            Span::styled(
                sanitize_for_display(&entry.label),
                if selected {
                    theme.style(Token::Selection)
                } else {
                    theme.style(Token::Text)
                },
            ),
        ];
        if !entry.detail.is_empty() {
            spans.push(Span::styled(
                format!("  {}", sanitize_for_display(&entry.detail)),
                theme.style(Token::Muted),
            ));
        }
        let mut line = Line::from(spans);
        if selected {
            line = line.style(theme.style(Token::Selection));
        }
        lines.push(line);
    }

    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            " Nothing matches that.",
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines)
        .block(pane_block(
            " Go to  Enter to choose, Esc to cancel ".to_owned(),
            true,
            presentation,
        ))
        .render(palette_area, buf);
}

// ------------------------------------------------------------------- chords

/// The popup that appears while a chord is waiting for its second key.
///
/// Nothing here is on a timer: the reducer reads no clock, and a popup that
/// vanishes on its own is a popup that vanishes while being read.
fn render_chords(presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let chords = crate::ui::keymap::CHORDS;
    let width = area.width.saturating_sub(8).min(52);
    let height = (u16::try_from(chords.len()).unwrap_or(6) + 2).min(area.height);
    let chord_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height.saturating_sub(height + 1),
        width,
        height,
    };
    ratatui::widgets::Clear.render(chord_area, buf);

    let lines: Vec<Line> = chords
        .iter()
        .map(|(key, _, description)| {
            Line::from(vec![
                Span::styled(
                    format!(" {key}  "),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled(*description, theme.style(Token::Text)),
            ])
        })
        .collect();

    Paragraph::new(lines)
        .block(pane_block(" Ctrl+K  then ".to_owned(), true, presentation))
        .render(chord_area, buf);
}

// -------------------------------------------------------------------- error

fn render_error(
    model: &Model,
    error: &crate::diagnostics::Diagnostic,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let block = pane_block(
        format!(" {}{} ", presentation.icon(Icon::Error), error.kind.label()),
        model.focus == Focus::Results,
        presentation,
    );

    let mut lines = vec![
        Line::from(Span::styled(
            error.headline.clone(),
            theme.style(Token::Danger),
        )),
        Line::from(Span::styled(
            format!("While: {}", error.attempted),
            theme.style(Token::Muted),
        )),
    ];
    if let Some(cause) = &error.likely_cause {
        lines.push(Line::from(vec![
            Span::styled(
                presentation.icon(Icon::Warning),
                theme.style(Token::Warning),
            ),
            Span::styled(format!("Likely cause: {cause}"), theme.style(Token::Text)),
        ]));
    }
    if let Some(action) = &error.next_action {
        lines.push(Line::from(vec![
            Span::styled(presentation.icon(Icon::Info), theme.style(Token::Info)),
            Span::styled(format!("Next: {action}"), theme.style(Token::Info)),
        ]));
    }
    if let Some(position) = error.position
        && let Some(marker) =
            crate::diagnostics::render_position_marker(model.editor.text(), position)
    {
        for line in marker.lines() {
            lines.push(Line::from(Span::styled(
                line.to_owned(),
                theme.style(Token::Warning),
            )));
        }
    }
    if model.error_expanded {
        for field in &error.technical {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<12}", field.label), theme.style(Token::Muted)),
                Span::styled(field.value.clone(), theme.style(Token::Text)),
            ]));
        }
    } else if !error.technical.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "{} technical field(s) hidden. Ctrl+D to expand.",
                error.technical.len()
            ),
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

// ------------------------------------------------------------------- footer

fn render_footer(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let (status_icon, status_token) = match model.phase {
        QueryPhase::Idle => (Icon::Success, Token::Muted),
        QueryPhase::Running { .. } => (Icon::Info, Token::Info),
        QueryPhase::CancellationRequested { .. } => (Icon::Warning, Token::Warning),
    };

    // While a statement runs the status carries the live frame, so the footer is
    // never a static word that could be mistaken for a hung client.
    let leader = if model.phase.is_busy() && !presentation.reduced_motion {
        format!("{} ", presentation.glyphs.spinner(model.frame))
    } else {
        presentation.icon(status_icon)
    };
    let mut status = format!(" {leader}{}", model.phase.label());
    if let Some(elapsed) = model.running_for.filter(|_| model.phase.is_busy()) {
        status.push_str(&format!(" {:.1}s", elapsed.as_secs_f64()));
    }
    let mut spans = vec![Span::styled(
        format!("{status} "),
        theme.style(status_token),
    )];

    // The transaction state is the one thing a user can be wrong about at real
    // cost, so it sits next to the status rather than behind a panel.
    let transaction_token = match model.transaction {
        crate::query::result::TransactionState::Failed => Token::TransactionFailed,
        crate::query::result::TransactionState::Open => Token::TransactionActive,
        _ => Token::Muted,
    };
    spans.push(Span::styled(
        format!("{} ", model.transaction.label()),
        theme.style(transaction_token),
    ));

    if let Some(info) = model.connection.info() {
        spans.push(Span::styled(
            format!(
                "{}{} ",
                presentation.icon(Icon::SearchPath),
                info.search_path
            ),
            theme.style(Token::Muted),
        ));
    }

    for (key, label) in keymap.hints() {
        spans.push(Span::styled(
            format!(" {key} "),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(label, theme.style(Token::Muted)));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

// --------------------------------------------------------------------- help

fn render_help(keymap: &Keymap, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(4).min(70);
    let height = (u16::try_from(keymap.bindings().len()).unwrap_or(20) + 4)
        .min(area.height.saturating_sub(2));
    let help_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    ratatui::widgets::Clear.render(help_area, buf);
    let mut lines = vec![Line::from(Span::styled(
        "Every action is reachable from the keyboard.",
        theme.style(Token::Muted),
    ))];
    for binding in keymap.bindings() {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<12}", binding.key_label()),
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ),
            Span::styled(binding.description, theme.style(Token::Text)),
        ]));
    }

    Paragraph::new(lines)
        .block(pane_block(
            format!(" {}Help  Esc to close ", presentation.icon(Icon::Help)),
            true,
            presentation,
        ))
        .style(theme.style(Token::Text))
        .render(help_area, buf);
}

fn pane_block(title: String, focused: bool, presentation: &Presentation) -> Block<'static> {
    let theme = &presentation.theme;
    Block::bordered()
        .border_set(presentation.glyphs.borders(focused))
        .border_style(theme.style(if focused { Token::Focus } else { Token::Border }))
        .title(Span::styled(
            title,
            if focused {
                theme.style(Token::Focus)
            } else {
                theme.style(Token::Muted)
            },
        ))
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
    render(model, keymap, presentation, area, &mut buf);
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
    use crate::ui::glyphs::GlyphTier;
    use std::time::Duration;

    fn presentation(theme: ThemeChoice, color: bool, tier: GlyphTier) -> Presentation {
        Presentation::new(Theme::new(theme, color), Glyphs::new(tier), false)
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
        assert!(text.contains("Ctrl+Q"), "quitting must be discoverable");
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
            for color in [true, false] {
                for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
                    let text = render_to_string(
                        &model,
                        &Keymap::new(),
                        &presentation(theme, color, tier),
                        100,
                        30,
                    );
                    let context = format!("{theme:?}/colour={color}/{tier:?}");
                    assert!(text.contains("[PROD]"), "{context}");
                    assert!(text.contains("[null]"), "{context}");
                    assert!(text.contains("Ready"), "{context}");
                    assert!(text.contains("Results"), "{context}");
                }
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
        });
        let text = render_to_string(&model, &Keymap::new(), &rich(), 100, 30);
        assert!(text.contains("Enter to run it"), "{text}");
        assert!(
            !text.contains("Type the database name"),
            "typing a word for every write would train people to type it"
        );
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
