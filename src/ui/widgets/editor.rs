pub(super) mod view;

use super::{action_key_label, overlay_block, pane_block, syntax_token};
use crate::app::model::{Focus, Model};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{
    LayoutMode, OverlayAnchor, Presentation, layout_mode, overlay_box, pane_geometry,
};
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

/// The editor's content rectangle in the current layout.
fn editor_content_area(model: &Model, presentation: &Presentation, area: Rect) -> Rect {
    let editor = pane_geometry(model, area).editor.unwrap_or_default();
    pane_block(String::new(), model.focus == Focus::Editor, presentation).inner(editor)
}

/// Text rows inside the pane border; zero means no text can be drawn.
pub(crate) fn content_capacity(pane: Rect) -> usize {
    let inner = super::inner_rect(pane);
    if inner.width == 0 {
        0
    } else {
        usize::from(inner.height)
    }
}

/// Draws the local completion menu next to the editor caret.
///
/// This is deliberately a static overlay. Completion is frequent keyboard
/// feedback, not a transition that needs to take attention away from the text
/// being written. The status lines carry the catalogue facts that colour or an
/// icon must never be asked to carry alone.
pub(crate) fn render_completion(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let Some(menu) = &model.completion.menu else {
        return;
    };
    if layout_mode(area) == LayoutMode::TooSmall || area.width < 12 || area.height < 5 {
        return;
    }

    let inner = editor_content_area(model, presentation, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let (line, _) = model.editor.position();
    let gutter = model.editor.line_count().to_string().len().max(2);
    let capacity = usize::from(inner.width).saturating_sub(gutter + 2);
    if capacity == 0 {
        return;
    }
    let view = view::viewport(&model.editor, capacity);
    let offset = (line.saturating_sub(1)).saturating_sub((inner.height as usize).saturating_sub(1));
    let cursor_y = inner
        .y
        .saturating_add(u16::try_from(line.saturating_sub(1).saturating_sub(offset)).unwrap_or(0))
        .min(inner.y.saturating_add(inner.height.saturating_sub(1)));
    let cursor_x = inner
        .x
        .saturating_add(u16::try_from(gutter + 2 + view.caret_cell).unwrap_or(u16::MAX))
        .min(inner.x.saturating_add(inner.width.saturating_sub(1)));

    let unicode = !presentation.glyphs.is_ascii();
    let safe_names = menu
        .result
        .candidates
        .iter()
        .map(|candidate| sanitize_for_display(&candidate.label))
        .collect::<Vec<_>>();
    let name_width = safe_names
        .iter()
        .map(|name| display_width(name))
        .max()
        .unwrap_or(12)
        .clamp(12, 28);
    let bounds = overlay_box(
        area,
        presentation,
        (u16::MAX, u16::MAX),
        5,
        OverlayAnchor::Center,
    );
    let max_width = bounds.width;
    let desired_width = u16::try_from(name_width + 44).unwrap_or(max_width);
    let width = desired_width.clamp(28, 78).min(max_width);
    let desired_height = u16::try_from(menu.result.candidates.len() + 7).unwrap_or(16);
    let max_height = bounds.height;
    let height = desired_height.clamp(5, 20).min(max_height);

    let right = bounds.right();
    let left_limit = right.saturating_sub(width).max(bounds.x);
    let x = cursor_x.saturating_sub(1).clamp(bounds.x, left_limit);
    let below = cursor_y.saturating_add(1);
    let bottom = bounds.bottom();
    let y = if below.saturating_add(height) <= bottom {
        below
    } else {
        cursor_y.saturating_sub(height).max(bounds.y)
    };
    let popup = Rect::new(x, y, width, height);
    ratatui::widgets::Clear.render(popup, buf);

    let content_width = width.saturating_sub(2) as usize;
    let label_width = name_width.min(content_width.saturating_sub(8));
    let mut lines = vec![Line::from(Span::styled(
        sanitize_for_display(&menu.result.scope.label()),
        presentation.theme.style(Token::Muted),
    ))];
    for (index, candidate) in menu.result.candidates.iter().enumerate() {
        let marker = if index == menu.selected {
            if unicode { "▸" } else { ">" }
        } else {
            " "
        };
        let name = truncate_to_width(&safe_names[index], label_width, unicode);
        let name = pad_to_width(&name, label_width);
        let detail = truncate_to_width(
            &sanitize_for_display(&candidate.plain_detail()),
            content_width.saturating_sub(label_width + 5),
            unicode,
        );
        let text = truncate_to_width(
            &format!("{marker} {name}  {detail}"),
            content_width,
            unicode,
        );
        let style = if index == menu.selected {
            presentation.theme.style(Token::Selection)
        } else {
            presentation.theme.style(Token::Text)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    if menu.result.candidates.is_empty() {
        lines.push(Line::from(Span::styled(
            "No candidates for this position.",
            presentation.theme.style(Token::Warning),
        )));
    }
    lines.push(Line::from(Span::styled(
        format!(
            "Showing {} of {} matching candidates ({} available).",
            menu.result.candidates.len(),
            menu.result.matching_count,
            menu.result.total_count
        ),
        presentation.theme.style(Token::Muted),
    )));
    lines.push(Line::from(Span::styled(
        sanitize_for_display(&model.completion.catalog.message()),
        presentation.theme.style(Token::Info),
    )));
    lines.push(Line::from(Span::styled(
        "Enter accepts, Esc cancels, Up/Down moves.",
        presentation.theme.style(Token::Muted),
    )));

    Paragraph::new(lines)
        .block(overlay_block(" Complete ".to_owned(), presentation))
        .wrap(Wrap { trim: true })
        .render(popup, buf);
}
pub(crate) fn render_editor(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Editor;
    let (line, column) = model.editor.position();
    let mut title = format!(
        " {}Editor  line {line}, column {column} ",
        presentation.icon(Icon::Editor)
    );
    if model.splits.zoomed && focused {
        title.insert_str(0, " (zoomed)");
    }
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
        let ctx = crate::app::discovery::context(model);
        let palette_key = action_key_label(keymap, &crate::app::Action::OpenPalette);
        let help_key = action_key_label(keymap, &crate::app::Action::ToggleHelp);
        let mut lines = Vec::new();
        match ctx.connection {
            crate::app::discovery::ConnectionPosture::Disconnected => {
                lines.push(Line::from(Span::styled(
                    "Connection required",
                    theme.style(Token::Warning),
                )));
                lines.push(Line::from(Span::styled(
                    "Connect before running SQL. Results appear after a query.",
                    theme.style(Token::Muted),
                )));
                lines.push(Line::from(Span::styled(
                    format!("{palette_key} Command palette shows available actions."),
                    theme.style(Token::Info),
                )));
            }
            crate::app::discovery::ConnectionPosture::Connecting => {
                lines.push(Line::from(Span::styled(
                    "Connecting",
                    theme.style(Token::Info),
                )));
                lines.push(Line::from(Span::styled(
                    "The editor is ready. Wait for the connection before running SQL.",
                    theme.style(Token::Muted),
                )));
                lines.push(Line::from(Span::styled(
                    format!("{help_key} Help explains the keyboard while it connects."),
                    theme.style(Token::Info),
                )));
            }
            crate::app::discovery::ConnectionPosture::Connected => {
                let run_key = action_key_label(keymap, &crate::app::Action::RunBuffer);
                lines.push(Line::from(Span::styled(
                    "Start here",
                    theme.style(Token::Focus),
                )));
                lines.push(Line::from(Span::styled(
                    "Type SQL in this editor.",
                    theme.style(Token::Text),
                )));
                lines.push(Line::from(Span::styled(
                    format!("{run_key} Run the buffer once SQL is ready."),
                    theme.style(Token::Info),
                )));
                lines.push(Line::from(Span::styled(
                    format!("{palette_key} Command palette answers what else is possible."),
                    theme.style(Token::Muted),
                )));
            }
            crate::app::discovery::ConnectionPosture::Lost
            | crate::app::discovery::ConnectionPosture::Failed => {
                lines.push(Line::from(Span::styled(
                    ctx.connection.label(),
                    theme.style(Token::Danger),
                )));
                lines.push(Line::from(Span::styled(
                    "Reconnect before running SQL. Results remain from the last known query.",
                    theme.style(Token::Muted),
                )));
                lines.push(Line::from(Span::styled(
                    format!("{palette_key} Command palette shows safe actions."),
                    theme.style(Token::Info),
                )));
            }
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(inner, buf);
        return;
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let gutter = lines.len().to_string().len().max(2);
    let (cursor_line, _) = model.editor.position();
    let capacity = usize::from(inner.width).saturating_sub(gutter + 2);
    let viewport = view::viewport(&model.editor, capacity);
    let (_, cursor) = view::caret_line(&model.editor);

    // The window follows the cursor rather than being stored, exactly as the
    // result grid's does. A scroll offset in the model would be a second source
    // of truth for where the cursor is.
    let height = inner.height as usize;
    let offset = cursor_line
        .saturating_sub(1)
        .saturating_sub(height.saturating_sub(1));

    // Colouring is decoration over the same bytes, so it is computed from the
    // buffer here rather than stored anywhere.
    let syntax = crate::query::highlight::tokens(text);
    let error_location = model.error_location.as_ref();
    // The statement Ctrl+T would run, marked in the gutter. Knowing which one
    // that is before pressing the key is the point.
    let current = crate::query::statements::statement_at(text, model.editor.cursor());
    let marker = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "\u{258e}"
    };

    let mut line_start = 0usize;

    for (index, source) in lines.iter().enumerate() {
        let number = index + 1;
        let start = line_start;
        // Every line advances the offset, including the ones scrolled past, so
        // the highlighting stays aligned with the text.
        line_start += source.len() + 1;
        if number <= offset || number > offset + height {
            continue;
        }

        let in_statement = current
            .as_ref()
            .is_some_and(|s| start < s.end.max(s.start + 1) && start + source.len() >= s.start);
        let error_line = error_location.is_some_and(|location| location.line == number);
        let line_marker = if error_line {
            "!"
        } else if in_statement {
            marker
        } else {
            " "
        };
        let line_marker_token = if error_line {
            Token::Danger
        } else {
            Token::Focus
        };
        let mut spans = vec![
            Span::styled(
                format!("{number:>gutter$} "),
                theme.style(if number == cursor_line {
                    Token::Text
                } else {
                    Token::Muted
                }),
            ),
            Span::styled(line_marker, theme.style(line_marker_token)),
        ];

        let y = inner.y + u16::try_from(number - offset - 1).unwrap_or(0);
        Line::from(std::mem::take(&mut spans)).render(Rect::new(inner.x, y, inner.width, 1), buf);
        if capacity == 0 {
            continue;
        }
        let text_x = inner.x + u16::try_from(gutter + 2).unwrap_or(0);
        let line = view::LineView::project(
            source,
            viewport.offset,
            capacity,
            (number == cursor_line).then_some(cursor),
        );
        for unit in &line.units {
            let is_cursor = focused && number == cursor_line && unit.source.contains(&cursor);
            let in_error_token = error_location
                .and_then(|location| location.token.as_ref())
                .is_some_and(|token| {
                    start + unit.source.start < token.end && token.start < start + unit.source.end
                });
            let style = if is_cursor {
                theme.style(Token::Selection)
            } else if in_error_token {
                theme.style(Token::Danger)
            } else {
                theme.style(syntax_token(crate::query::highlight::kind_at(
                    &syntax,
                    start + unit.source.start,
                )))
            };
            if let Some((cell, visible)) = unit.clipped(viewport.offset, capacity) {
                buf.set_stringn(
                    text_x + u16::try_from(cell).unwrap_or(0),
                    y,
                    visible,
                    capacity - cell,
                    style,
                );
            }
        }
        if focused && number == cursor_line && viewport.blank_caret {
            buf.set_stringn(
                text_x + u16::try_from(viewport.caret_cell).unwrap_or(0),
                y,
                " ",
                1,
                theme.style(Token::Selection),
            );
        }
    }
}
