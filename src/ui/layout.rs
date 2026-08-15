//! Drawing the full-screen interface.
//!
//! Rendering is a pure function of the model: same model, same pixels. Nothing
//! here reads a clock, a file, or a connection.
//!
//! Three rules shape every decision below:
//!
//! - Every state that colour expresses is also written in words, so the interface
//!   survives `NO_COLOR`, a monochrome terminal, and colour-blind vision.
//! - A narrow terminal gets a deliberately different layout, not a clipped one.
//! - Nothing from the database is drawn without passing through
//!   [`crate::query::value::sanitize_for_display`].

use crate::app::model::{Focus, Model, QueryPhase};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::keymap::Keymap;
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

/// How the interface is being presented.
#[derive(Debug, Clone)]
pub struct Presentation {
    /// Palette and whether colour may be emitted.
    pub theme: Theme,
    /// Whether Unicode box drawing may be used.
    pub unicode: bool,
}

impl Presentation {
    /// The border characters for the current glyph mode.
    fn borders(&self) -> border::Set<'static> {
        if self.unicode {
            border::PLAIN
        } else {
            border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: "-",
                horizontal_bottom: "-",
            }
        }
    }

    /// The separator used between status items.
    const fn separator(&self) -> &'static str {
        if self.unicode { " │ " } else { " | " }
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

    if model.help_open {
        render_help(keymap, presentation, area, buf);
    }
}

fn render_full(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let [header, editor, results, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Percentage(40),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

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
    } else if model.focus == Focus::Editor {
        render_editor(model, presentation, body, buf);
    } else {
        render_results(model, presentation, body, buf);
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

fn render_header(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let environment = model.environment();
    let mut spans = vec![
        Span::styled(
            format!(" {} ", crate::branding::PRODUCT_NAME),
            theme.style(Token::Header),
        ),
        // The classification is a bracketed word, so it is unmistakable with no
        // colour at all.
        Span::styled(
            format!("[{}]", environment.label()),
            theme.style(if environment.is_production() {
                Token::EnvironmentProduction
            } else {
                Token::EnvironmentNonProduction
            }),
        ),
    ];

    if let Some(info) = model.connection.info() {
        spans.push(Span::styled(
            format!(" [{}]", info.posture()),
            theme.style(Token::Muted),
        ));
    }

    spans.push(Span::styled(
        presentation.separator(),
        theme.style(Token::Border),
    ));
    spans.push(Span::styled(
        model.connection.label(),
        theme.style(Token::Text),
    ));

    if let Some(info) = model.connection.info() {
        spans.push(Span::styled(
            presentation.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            info.tls.label(),
            theme.style(if info.tls.is_encrypted() {
                Token::Success
            } else {
                Token::Warning
            }),
        ));
    }

    if let Some(elapsed) = model.last_elapsed {
        spans.push(Span::styled(
            presentation.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!("{} ms", elapsed.as_millis()),
            theme.style(Token::Muted),
        ));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_compact_header(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let environment = model.environment();
    let database = model
        .connection
        .info()
        .map_or_else(|| "not connected".to_owned(), |i| i.database.clone());
    let spans = vec![
        Span::styled(
            format!("[{}] ", environment.label()),
            theme.style(if environment.is_production() {
                Token::EnvironmentProduction
            } else {
                Token::EnvironmentNonProduction
            }),
        ),
        Span::styled(database, theme.style(Token::Text)),
        Span::styled(
            format!(" [{}]", model.focus.label()),
            theme.style(Token::Focus),
        ),
    ];
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_editor(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let focused = model.focus == Focus::Editor;
    let (line, column) = model.editor.position();
    let mut title = format!(" Editor  line {line}, column {column} ");
    if model.editor.is_modified() {
        title.push_str("[modified] ");
    }
    if focused {
        title.push_str("[focused] ");
    }

    let block = pane_block(title, focused, presentation);
    let text: Vec<Line> = model
        .editor
        .text()
        .lines()
        .map(|l| Line::from(sanitize_for_display(l)))
        .collect();
    let text = if text.is_empty() {
        vec![Line::from(Span::styled(
            "Type SQL here, then press F5 to run it.",
            presentation.theme.style(Token::Muted),
        ))]
    } else {
        text
    };

    Paragraph::new(text)
        .block(block)
        .style(presentation.theme.style(Token::Text))
        .render(area, buf);
}

fn render_results(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Results;

    let Some(execution) = &model.last_execution else {
        let block = pane_block(" Results ".to_owned(), focused, presentation);
        let hint = if model.connection.is_usable() {
            "No query has run yet. Press F5 to run the buffer."
        } else {
            "Not connected. Results appear here once a query runs."
        };
        Paragraph::new(Line::from(Span::styled(hint, theme.style(Token::Muted))))
            .block(block)
            .render(area, buf);
        return;
    };

    let summary = execution
        .statements
        .first()
        .map_or_else(|| execution.status.label().to_owned(), |s| s.summary());
    let mut title = format!(" Results  {}  {} ", execution.status.label(), summary);
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
        Paragraph::new(Line::from(Span::styled(text, theme.style(Token::Text)))).render(inner, buf);
        return;
    };

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let widths = column_widths(set, inner.width as usize);
    let mut lines = Vec::new();

    lines.push(Line::from(
        set.columns
            .iter()
            .zip(&widths)
            .map(|(name, width)| {
                Span::styled(
                    format!(
                        "{} ",
                        pad_to_width(
                            &truncate_to_width(
                                &sanitize_for_display(name),
                                *width,
                                presentation.unicode
                            ),
                            *width
                        )
                    ),
                    theme.style(Token::Header),
                )
            })
            .collect::<Vec<_>>(),
    ));

    let visible_rows = (inner.height as usize).saturating_sub(1);
    for (index, row) in set.rows.iter().take(visible_rows).enumerate() {
        let selected = focused && index == model.selected_row;
        let spans = row
            .iter()
            .zip(&widths)
            .map(|(cell, width)| {
                let token = if selected {
                    Token::Selection
                } else if cell.is_null() {
                    Token::NullValue
                } else {
                    Token::Text
                };
                let rendered = truncate_to_width(&cell.display(), *width, presentation.unicode);
                Span::styled(
                    format!("{} ", pad_to_width(&rendered, *width)),
                    theme.style(token),
                )
            })
            .collect::<Vec<_>>();
        lines.push(Line::from(spans));
    }

    if set.rows.len() > visible_rows {
        // Never let the window silently hide rows that are in memory.
        lines.pop();
        lines.push(Line::from(Span::styled(
            format!(
                "{} more row(s) below; {}",
                set.rows.len() - visible_rows + 1,
                set.window_label()
            ),
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines).render(inner, buf);
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

    // Each column also carries a single space of padding.
    let total: usize = widths.iter().map(|w| w + 1).sum();
    if total > available && available > count {
        let budget = available - count;
        let scale = budget as f64 / widths.iter().sum::<usize>() as f64;
        for width in &mut widths {
            *width = ((*width as f64) * scale).floor().max(3.0) as usize;
        }
    }
    widths
}

fn render_error(
    model: &Model,
    error: &crate::diagnostics::Diagnostic,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let block = pane_block(
        format!(" {} ", error.kind.label()),
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
        lines.push(Line::from(format!("Likely cause: {cause}")));
    }
    if let Some(action) = &error.next_action {
        lines.push(Line::from(Span::styled(
            format!("Next: {action}"),
            theme.style(Token::Info),
        )));
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
            lines.push(Line::from(Span::styled(
                format!("{}: {}", field.label, field.value),
                theme.style(Token::Muted),
            )));
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

fn render_footer(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let mut spans = vec![Span::styled(
        format!(" {} ", model.phase.label()),
        theme.style(match model.phase {
            QueryPhase::Idle => Token::Muted,
            QueryPhase::Running { .. } => Token::Info,
            QueryPhase::CancellationRequested { .. } => Token::Warning,
        }),
    )];

    if let Some(info) = model.connection.info() {
        spans.push(Span::styled(
            format!("Search path: {} ", info.search_path),
            theme.style(Token::Muted),
        ));
    }

    for (key, label) in keymap.hints() {
        spans.push(Span::styled(format!(" {key} "), theme.style(Token::Focus)));
        spans.push(Span::styled(label, theme.style(Token::Muted)));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_help(keymap: &Keymap, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let width = area.width.saturating_sub(4).min(60);
    let height = (keymap.bindings().len() as u16 + 4).min(area.height.saturating_sub(2));
    let help_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    ratatui::widgets::Clear.render(help_area, buf);
    let mut lines = vec![Line::from(Span::styled(
        "Every action is reachable from the keyboard.",
        presentation.theme.style(Token::Muted),
    ))];
    for binding in keymap.bindings() {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<12}", binding.key_label()),
                presentation.theme.style(Token::Focus),
            ),
            Span::styled(binding.description, presentation.theme.style(Token::Text)),
        ]));
    }

    Paragraph::new(lines)
        .block(pane_block(
            " Help  Esc to close ".to_owned(),
            true,
            presentation,
        ))
        .style(presentation.theme.style(Token::Text))
        .render(help_area, buf);
}

fn pane_block(title: String, focused: bool, presentation: &Presentation) -> Block<'static> {
    Block::bordered()
        .border_set(presentation.borders())
        .border_style(
            presentation
                .theme
                .style(if focused { Token::Focus } else { Token::Border }),
        )
        .title(Span::styled(
            title,
            presentation
                .theme
                .style(if focused { Token::Focus } else { Token::Muted }),
        ))
}

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
    use std::time::Duration;

    fn presentation(theme: ThemeChoice, color: bool, unicode: bool) -> Presentation {
        Presentation {
            theme: Theme::new(theme, color),
            unicode,
        }
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
        });
    }

    #[test]
    fn the_full_layout_shows_connection_environment_tls_and_hints() {
        let model = connected_model(Environment::Local);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );

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
        assert!(text.contains("F5"), "the run key must be discoverable");
        assert!(text.contains("Ctrl+Q"), "quitting must be discoverable");
    }

    #[test]
    fn a_production_connection_is_unmistakable_without_colour() {
        let model = connected_model(Environment::Production);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            // Colour disabled: the marker must survive on its own.
            &presentation(ThemeChoice::Dark, false, true),
            100,
            30,
        );
        assert!(text.contains("[PROD]"), "production marker missing: {text}");
    }

    #[test]
    fn results_show_headers_values_and_a_row_count() {
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
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );

        assert!(text.contains("customer_id"), "{text}");
        assert!(text.contains("10482"));
        assert!(text.contains("1245.00"));
        assert!(text.contains("2 rows"), "the row count must be visible");
        assert!(text.contains("87 ms"), "elapsed time must be visible");
        assert!(
            text.contains("[null]"),
            "NULL must be distinguishable: {text}"
        );
    }

    #[test]
    fn hostile_values_cannot_emit_escape_sequences_into_the_interface() {
        let mut model = connected_model(Environment::Local);
        with_rows(
            &mut model,
            &["note"],
            &[&[Cell::Text("\x1b[2Jgotcha".into())]],
        );
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
        assert!(!text.contains('\x1b'), "an escape reached the screen");

        // The same applies to a hostile column name.
        let mut model = connected_model(Environment::Local);
        with_rows(&mut model, &["\x1b[31mname"], &[&[Cell::Text("x".into())]]);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
        assert!(
            !text.contains('\x1b'),
            "an escape in a column name reached the screen"
        );
    }

    #[test]
    fn an_empty_state_says_what_to_do_next() {
        let model = connected_model(Environment::Local);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
        assert!(text.contains("No query has run yet"), "{text}");
        assert!(
            text.contains("F5"),
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
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );

        assert!(text.contains("Query error"), "{text}");
        assert!(text.contains("does not exist"));
        assert!(text.contains("Likely cause"));
        assert!(text.contains("Next:"));
        assert!(!text.contains("42P01"), "technical detail starts collapsed");
        assert!(text.contains("Ctrl+D to expand"));

        model.error_expanded = true;
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
        assert!(text.contains("42P01"), "expanding reveals SQLSTATE");
    }

    #[test]
    fn cancellation_is_worded_as_requested_until_the_server_answers() {
        let mut model = connected_model(Environment::Local);
        model.phase = QueryPhase::CancellationRequested { job: JobId(1) };
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
        assert!(text.contains("Cancellation requested"), "{text}");
        assert!(!text.contains("Query cancelled by server"));
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
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            60,
            20,
        );

        assert!(text.contains("Editor"), "{text}");
        assert!(
            !text.contains("Results"),
            "the unfocused pane is dropped, not squeezed"
        );
        assert!(
            text.contains("[PROD]"),
            "the production marker survives the narrow layout"
        );
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
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            30,
            6,
        );
        assert!(text.contains("Terminal too small"), "{text}");
        assert!(text.contains("80x24"), "the minimum is stated");
        assert!(text.contains("30x6"), "the current size is stated");
        assert!(
            text.contains("ignatius query"),
            "a line-oriented route is offered"
        );
    }

    #[test]
    fn ascii_mode_draws_no_unicode_box_characters() {
        let model = connected_model(Environment::Local);
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, false),
            100,
            30,
        );
        for ch in ['┌', '┐', '└', '┘', '─', '│'] {
            assert!(!text.contains(ch), "ASCII mode emitted {ch}");
        }
        assert!(text.contains('+') && text.contains('-') && text.contains('|'));
    }

    #[test]
    fn every_theme_and_colour_mode_renders_the_same_meaning() {
        let mut model = connected_model(Environment::Production);
        with_rows(&mut model, &["n"], &[&[Cell::Null]]);
        for theme in [
            ThemeChoice::Dark,
            ThemeChoice::Light,
            ThemeChoice::HighContrast,
        ] {
            for color in [true, false] {
                for unicode in [true, false] {
                    let text = render_to_string(
                        &model,
                        &Keymap::new(),
                        &presentation(theme, color, unicode),
                        100,
                        30,
                    );
                    assert!(text.contains("[PROD]"), "{theme:?}/{color}/{unicode}");
                    assert!(text.contains("[null]"), "{theme:?}/{color}/{unicode}");
                    assert!(text.contains("Ready"), "{theme:?}/{color}/{unicode}");
                }
            }
        }
    }

    #[test]
    fn help_lists_every_binding_and_how_to_close_it() {
        let mut model = connected_model(Environment::Local);
        model.help_open = true;
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
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
            &["名前"],
            &[&[Cell::Text("日本語のテキストです".into())]],
        );
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            80,
            24,
        );
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
        let text = render_to_string(
            &model,
            &Keymap::new(),
            &presentation(ThemeChoice::Dark, true, true),
            100,
            30,
        );
        assert!(text.contains("Not connected"), "{text}");
        assert!(text.contains("[UNCLASSIFIED]"));
    }
}
