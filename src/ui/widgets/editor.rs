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
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

/// The editor's content rectangle in the current layout.
fn editor_content_area(model: &Model, presentation: &Presentation, area: Rect) -> Rect {
    let editor = pane_geometry(model, area).editor.unwrap_or_default();
    let mut inner =
        pane_block(String::new(), model.focus == Focus::Editor, presentation).inner(editor);
    if model.recovery_banner.is_some() && inner.height > 0 {
        inner.y = inner.y.saturating_add(1);
        inner.height = inner.height.saturating_sub(1);
    }
    if (model.buffers.count() > 1
        || model.buffers.split_mode != crate::buffer::EditorSplitMode::None)
        && inner.height > 2
    {
        inner.y = inner.y.saturating_add(1);
        inner.height = inner.height.saturating_sub(1);
    }
    inner
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
        if index == menu.selected {
            let padded = pad_to_width(&text, content_width);
            lines.push(Line::from(Span::styled(
                padded,
                presentation.theme.style(Token::Selection),
            )));
        } else {
            let prefix_raw = format!("{marker} {name}");
            let kind_token = candidate_kind_token(candidate.kind);
            if text.len() > prefix_raw.len() && text.is_char_boundary(prefix_raw.len()) {
                lines.push(Line::from(vec![
                    Span::styled(
                        text[..prefix_raw.len()].to_owned(),
                        presentation.theme.style(kind_token),
                    ),
                    Span::styled(
                        text[prefix_raw.len()..].to_owned(),
                        presentation.theme.style(Token::Muted),
                    ),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    text,
                    presentation.theme.style(kind_token),
                )));
            }
        }
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

    if menu.selected < menu.result.candidates.len() {
        let sel_y = popup
            .y
            .saturating_add(2)
            .saturating_add(u16::try_from(menu.selected).unwrap_or(0));
        if sel_y < popup.bottom().saturating_sub(1) {
            let sel_style = presentation.theme.style(Token::Selection);
            for cx in (popup.x + 1)..popup.x.saturating_add(popup.width.saturating_sub(1)) {
                buf[(cx, sel_y)].set_style(sel_style);
            }
        }
    }
}

fn candidate_kind_token(kind: crate::query::completion::CandidateKind) -> Token {
    use crate::query::completion::{CandidateKind, CatalogObjectKind};
    match kind {
        CandidateKind::Keyword => Token::SyntaxKeyword,
        CandidateKind::Cte => Token::Focus,
        CandidateKind::Column => Token::Text,
        CandidateKind::Object(CatalogObjectKind::Function) => Token::SyntaxNumber,
        CandidateKind::Object(_) => Token::Info,
    }
}
/// Draws the one-row crash recovery capsule above the buffer tab bar.
///
/// The wording is chosen from the journal itself: unsaved buffers get the
/// louder label, an untouched session keeps the quiet one. Either way the
/// counts, the keys and the two words carry the meaning without colour.
fn render_recovery_capsule(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let Some(banner) = &model.recovery_banner else {
        return;
    };
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let journal = &banner.journal;
    let unsaved = journal.unsaved_buffer_count();
    let (count, noun) = if unsaved > 0 {
        (unsaved, "UNSAVED BUFFER")
    } else {
        (journal.recoverable_buffer_count(), "OPEN BUFFER")
    };
    let plural = if count == 1 { "" } else { "S" };
    let truncated = journal.truncated_buffer_count();
    let truncated_suffix = if truncated > 0 {
        format!(", {truncated} TRUNCATED")
    } else {
        String::new()
    };
    let capsule_style = theme.style(Token::Focus).add_modifier(Modifier::BOLD);
    let wide_label = format!("[CRASH RECOVERY: {count} {noun}{plural}{truncated_suffix}] ");
    let medium_label = format!("[CRASH RECOVERY: {count} {noun}{plural}{truncated_suffix}]");
    let short_label = format!("[CRASH RECOVERY: {count}]");
    let minimal_label = format!("[RECOVERY: {count}]");

    // Degrade in steps that keep the label and the chord meaning. `^K` is the
    // conventional ASCII spelling of Ctrl+K when the full hint does not fit.
    let wide_width =
        wide_label.chars().count() + "[Ctrl+K r] Restore   [Ctrl+K d] Discard".chars().count();
    let medium_width = medium_label.chars().count() + " ^K r Restore  ^K d Discard".chars().count();
    let words_width = short_label.chars().count() + " ^K r Restore  ^K d Discard".chars().count();
    let letters_width = minimal_label.chars().count() + " ^K r/d".chars().count();
    let width = area.width as usize;

    let spans = if width >= wide_width {
        vec![
            Span::styled(wide_label, capsule_style),
            Span::styled("[Ctrl+K r] ", theme.style(Token::Info)),
            Span::styled("Restore   ", theme.style(Token::Muted)),
            Span::styled("[Ctrl+K d] ", theme.style(Token::Info)),
            Span::styled("Discard", theme.style(Token::Muted)),
        ]
    } else if width >= medium_width {
        vec![
            Span::styled(format!("{medium_label} "), capsule_style),
            Span::styled("^K r ", theme.style(Token::Info)),
            Span::styled("Restore  ", theme.style(Token::Muted)),
            Span::styled("^K d ", theme.style(Token::Info)),
            Span::styled("Discard", theme.style(Token::Muted)),
        ]
    } else if width >= words_width {
        vec![
            Span::styled(format!("{short_label} "), capsule_style),
            Span::styled("^K r ", theme.style(Token::Info)),
            Span::styled("Restore  ", theme.style(Token::Muted)),
            Span::styled("^K d ", theme.style(Token::Info)),
            Span::styled("Discard", theme.style(Token::Muted)),
        ]
    } else if width >= letters_width {
        vec![
            Span::styled(format!("{minimal_label} "), capsule_style),
            Span::styled("^K r/d", theme.style(Token::Info)),
        ]
    } else {
        vec![Span::styled(minimal_label, capsule_style)]
    };
    Line::from(spans).render(area, buf);
}

fn render_tab_bar(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    if area.width < 10 || area.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let mut spans = Vec::new();

    // Field Industrial status capsule: [BUFFERS: n/16]
    spans.push(Span::styled(
        format!(
            "[BUFFERS: {}/{}] ",
            model.buffers.count(),
            crate::buffer::MAX_EDITOR_BUFFERS
        ),
        theme.style(Token::Muted),
    ));

    if model.buffers.split_mode != crate::buffer::EditorSplitMode::None {
        spans.push(Span::styled(
            format!("[SPLIT: {}] ", model.buffers.split_mode.label()),
            theme.style(Token::Info).add_modifier(Modifier::BOLD),
        ));
    }

    // Render individual buffer tabs
    for (idx, b) in model.buffers.buffers.iter().enumerate() {
        let is_active = idx == model.buffers.active_idx;
        let is_secondary = model.buffers.split_mode != crate::buffer::EditorSplitMode::None
            && model.buffers.secondary_idx == Some(idx);

        let label = format!("{}: {}", idx + 1, b.display_label());

        if is_active {
            spans.push(Span::styled("[", theme.style(Token::Focus)));
            spans.push(Span::styled(
                label,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled("] ", theme.style(Token::Focus)));
        } else if is_secondary {
            spans.push(Span::styled("[SPLIT: ", theme.style(Token::Info)));
            spans.push(Span::styled(
                label,
                theme.style(Token::Info).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled("] ", theme.style(Token::Info)));
        } else {
            spans.push(Span::styled(format!("{label} "), theme.style(Token::Muted)));
        }
    }

    spans.push(Span::styled("[+] ", theme.style(Token::Focus)));

    Line::from(spans).render(area, buf);
}

#[allow(clippy::too_many_arguments)]
fn render_editor_buffer_pane(
    editor: &crate::app::editor::Editor,
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    inner: Rect,
    buf: &mut Buffer,
    is_active: bool,
    split_badge: Option<&str>,
) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Editor && is_active;
    let text = editor.text();

    if text.is_empty() {
        if is_active {
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
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{palette_key} "),
                            theme
                                .style(Token::Focus)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::styled(
                            "Command palette shows available actions.",
                            theme.style(Token::Info),
                        ),
                    ]));
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
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{help_key} "),
                            theme
                                .style(Token::Focus)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::styled(
                            "Help explains the keyboard while it connects.",
                            theme.style(Token::Info),
                        ),
                    ]));
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
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{run_key} "),
                            theme
                                .style(Token::Focus)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::styled(
                            "Run the buffer once SQL is ready.",
                            theme.style(Token::Info),
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{palette_key} "),
                            theme
                                .style(Token::Focus)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::styled(
                            "Command palette answers what else is possible.",
                            theme.style(Token::Muted),
                        ),
                    ]));
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
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{palette_key} "),
                            theme
                                .style(Token::Focus)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::styled(
                            "Command palette shows safe actions.",
                            theme.style(Token::Info),
                        ),
                    ]));
                }
            }
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
        } else {
            let label = split_badge.unwrap_or("empty buffer");
            let line = Line::from(vec![
                Span::styled(format!("[SPLIT: {label}] "), theme.style(Token::Info)),
                Span::styled("(empty buffer)", theme.style(Token::Muted)),
            ]);
            Paragraph::new(vec![line]).render(inner, buf);
        }
        return;
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let gutter = lines.len().to_string().len().max(2);
    let (cursor_line, _) = editor.position();
    let capacity = usize::from(inner.width).saturating_sub(gutter + 2);
    let viewport = view::viewport(editor, capacity);
    let (_, cursor) = view::caret_line(editor);

    let height = inner.height as usize;
    let offset = cursor_line
        .saturating_sub(1)
        .saturating_sub(height.saturating_sub(1));

    let syntax = crate::query::highlight::tokens(text);
    let error_location = if is_active {
        model.error_location.as_ref()
    } else {
        None
    };
    let current = if is_active {
        crate::query::statements::statement_at(text, editor.cursor())
    } else {
        None
    };
    let marker = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "\u{258e}"
    };

    let mut line_start = 0usize;
    let mut active_cursor_y = None;

    for (index, source) in lines.iter().enumerate() {
        let number = index + 1;
        let start = line_start;
        line_start += source.len() + 1;
        if number <= offset || number > offset + height {
            continue;
        }

        let on_cursor_line = number == cursor_line;
        let y = inner.y + u16::try_from(number - offset - 1).unwrap_or(0);
        if focused && on_cursor_line {
            active_cursor_y = Some(y);
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
        let line_number_style = if on_cursor_line {
            if focused {
                theme.on_stripe(Token::Focus)
            } else {
                theme.style(Token::Header)
            }
        } else {
            theme.style(Token::Muted)
        };
        let mut spans = vec![
            Span::styled(format!("{number:>gutter$} "), line_number_style),
            Span::styled(line_marker, theme.style(line_marker_token)),
        ];

        Line::from(std::mem::take(&mut spans)).render(Rect::new(inner.x, y, inner.width, 1), buf);
        if capacity == 0 {
            continue;
        }
        let text_x = inner.x + u16::try_from(gutter + 2).unwrap_or(0);
        let line = view::LineView::project(
            source,
            viewport.offset,
            capacity,
            (number == cursor_line && is_active).then_some(cursor),
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

    if let Some(cy) = active_cursor_y
        && let Some(stripe_bg) = theme.stripe().bg
    {
        let surface_pane_bg = theme
            .surface(Token::SurfacePane)
            .bg
            .unwrap_or(ratatui::style::Color::Reset);
        for x in inner.x..inner.x.saturating_add(inner.width) {
            if buf[(x, cy)].bg == surface_pane_bg {
                buf[(x, cy)].set_bg(stripe_bg);
            }
        }
    }
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
    if model.buffers.split_mode != crate::buffer::EditorSplitMode::None {
        title.push_str(&format!("[SPLIT: {}] ", model.buffers.split_mode.label()));
    }
    if model.buffers.count() > 1 {
        title.push_str(&format!(
            "[BUFFERS: {}/{}] ",
            model.buffers.count(),
            crate::buffer::MAX_EDITOR_BUFFERS
        ));
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

    let mut text_inner = inner;
    if model.recovery_banner.is_some() && text_inner.height > 0 {
        render_recovery_capsule(
            model,
            presentation,
            Rect::new(inner.x, inner.y, inner.width, 1),
            buf,
        );
        text_inner = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
    }

    let show_tab_bar = (model.buffers.count() > 1
        || model.buffers.split_mode != crate::buffer::EditorSplitMode::None)
        && text_inner.height > 2;

    let text_inner = if show_tab_bar {
        render_tab_bar(
            model,
            presentation,
            Rect::new(text_inner.x, text_inner.y, text_inner.width, 1),
            buf,
        );
        Rect::new(
            text_inner.x,
            text_inner.y.saturating_add(1),
            text_inner.width,
            text_inner.height.saturating_sub(1),
        )
    } else {
        text_inner
    };

    match model.buffers.split_mode {
        crate::buffer::EditorSplitMode::None => {
            render_editor_buffer_pane(
                &model.editor,
                model,
                keymap,
                presentation,
                text_inner,
                buf,
                true,
                None,
            );
        }
        crate::buffer::EditorSplitMode::Vertical if text_inner.width >= 40 => {
            let left_width = text_inner.width / 2;
            let sep_x = text_inner.x.saturating_add(left_width);
            let right_x = sep_x.saturating_add(1);
            let right_width = text_inner.width.saturating_sub(left_width + 1);

            let left_rect = Rect::new(text_inner.x, text_inner.y, left_width, text_inner.height);
            let right_rect = Rect::new(right_x, text_inner.y, right_width, text_inner.height);

            let sep_char = if presentation.glyphs.is_ascii() {
                "|"
            } else {
                "│"
            };
            for y in text_inner.y..text_inner.y.saturating_add(text_inner.height) {
                buf.set_stringn(sep_x, y, sep_char, 1, theme.style(Token::Muted));
            }

            render_editor_buffer_pane(
                &model.editor,
                model,
                keymap,
                presentation,
                left_rect,
                buf,
                true,
                None,
            );

            if let Some(sec_buf) = model.buffers.secondary_buffer() {
                render_editor_buffer_pane(
                    &sec_buf.editor,
                    model,
                    keymap,
                    presentation,
                    right_rect,
                    buf,
                    false,
                    Some(&sec_buf.title),
                );
            }
        }
        crate::buffer::EditorSplitMode::Horizontal if text_inner.height >= 6 => {
            let top_height = text_inner.height / 2;
            let sep_y = text_inner.y.saturating_add(top_height);
            let bottom_y = sep_y.saturating_add(1);
            let bottom_height = text_inner.height.saturating_sub(top_height + 1);

            let top_rect = Rect::new(text_inner.x, text_inner.y, text_inner.width, top_height);
            let bottom_rect = Rect::new(text_inner.x, bottom_y, text_inner.width, bottom_height);

            let sep_char = if presentation.glyphs.is_ascii() {
                "-"
            } else {
                "─"
            };
            for x in text_inner.x..text_inner.x.saturating_add(text_inner.width) {
                buf.set_stringn(x, sep_y, sep_char, 1, theme.style(Token::Muted));
            }

            render_editor_buffer_pane(
                &model.editor,
                model,
                keymap,
                presentation,
                top_rect,
                buf,
                true,
                None,
            );

            if let Some(sec_buf) = model.buffers.secondary_buffer() {
                render_editor_buffer_pane(
                    &sec_buf.editor,
                    model,
                    keymap,
                    presentation,
                    bottom_rect,
                    buf,
                    false,
                    Some(&sec_buf.title),
                );
            }
        }
        _ => {
            render_editor_buffer_pane(
                &model.editor,
                model,
                keymap,
                presentation,
                text_inner,
                buf,
                true,
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests;
