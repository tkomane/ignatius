pub(super) mod view;

use super::{
    action_key_label, bound_key, focus_accent_style, hairline_rule, inset_empty_area,
    overlay_block_line, pane_block, pane_block_line_with_motion, row_wave_progress, syntax_token,
};
use crate::app::model::{Focus, Model};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{
    LayoutMode, OverlayAnchor, Presentation, layout_mode, overlay_box, pane_geometry,
};
use crate::ui::theme::{Theme, Token};
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
    let desired_width = u16::try_from(name_width + 46).unwrap_or(max_width);
    let width = desired_width.clamp(28, 78).min(max_width);
    let max_height = bounds.height;
    let use_rules = max_height >= 7;
    let desired_height = u16::try_from(menu.result.candidates.len() + 7).unwrap_or(20);
    let height = desired_height.clamp(5, 22).min(max_height);

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
    let mut lines = vec![Line::from(Span::styled(
        truncate_to_width(
            &sanitize_for_display(&menu.result.scope.label()),
            content_width,
            unicode,
        ),
        presentation.theme.style(Token::Muted),
    ))];
    if use_rules {
        lines.push(Line::from(Span::styled(
            hairline_rule(presentation).repeat(content_width),
            presentation.theme.style(Token::Border),
        )));
    }
    for (index, candidate) in menu.result.candidates.iter().enumerate() {
        let marker = if index == menu.selected {
            if unicode { "▸" } else { ">" }
        } else {
            " "
        };
        let kind_icon = presentation.icon(candidate_kind_icon(candidate.kind));
        let icon_w = display_width(&kind_icon);
        let label_width = name_width.min(content_width.saturating_sub(8 + icon_w));
        let name = truncate_to_width(&safe_names[index], label_width, unicode);
        let name = pad_to_width(&name, label_width);
        let detail = truncate_to_width(
            &sanitize_for_display(&candidate.plain_detail()),
            content_width.saturating_sub(label_width + 5 + icon_w),
            unicode,
        );
        let prefix_raw = format!("{marker} {kind_icon}{name}");
        let text = truncate_to_width(&format!("{prefix_raw}  {detail}"), content_width, unicode);
        if index == menu.selected {
            let padded = pad_to_width(&text, content_width);
            lines.push(Line::from(Span::styled(
                padded,
                presentation.theme.style(Token::Selection),
            )));
        } else {
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
    if use_rules {
        lines.push(Line::from(Span::styled(
            hairline_rule(presentation).repeat(content_width),
            presentation.theme.style(Token::Border),
        )));
    }
    let summary_sep = if unicode { " \u{00b7} " } else { " | " };
    // Counts read in the order they narrow: what is listed, of what matches,
    // and how many candidates the scope holds at all.
    let catalog_summary = format!(
        "{} of {} shown, {} in scope{summary_sep}{}",
        menu.result.candidates.len(),
        menu.result.matching_count,
        menu.result.total_count,
        sanitize_for_display(&model.completion.catalog.message()),
    );
    lines.push(Line::from(Span::styled(
        truncate_to_width(&catalog_summary, content_width, unicode),
        presentation.theme.style(Token::Muted),
    )));
    lines.push(completion_key_row(model, presentation, content_width));

    let title_line = Line::from(Span::styled(
        format!("{}Complete", presentation.icon(Icon::Editor)),
        presentation
            .theme
            .style(Token::Focus)
            .add_modifier(Modifier::BOLD),
    ));
    Paragraph::new(lines)
        .block(overlay_block_line(title_line, presentation))
        .render(popup, buf);

    if menu.selected < menu.result.candidates.len() {
        let candidate_offset = 2 + u16::from(use_rules);
        let sel_y = popup
            .y
            .saturating_add(candidate_offset)
            .saturating_add(u16::try_from(menu.selected).unwrap_or(0));
        if sel_y < popup.bottom().saturating_sub(1) {
            let sel_style = presentation.theme.style(Token::Selection);
            for cx in (popup.x + 1)..popup.x.saturating_add(popup.width.saturating_sub(1)) {
                buf[(cx, sel_y)].set_style(sel_style);
            }
        }
    }
}

/// The popup's keys as keycaps, the way the footer draws them, recessed onto
/// the popup's own surface. Each key comes from the active keymap, and a pair
/// that does not fit is left out whole rather than cut.
fn completion_key_row(model: &Model, presentation: &Presentation, width: usize) -> Line<'static> {
    use crate::app::message::{Action, Direction};
    let theme = &presentation.theme;
    let vertical = match (
        bound_key(model, &Action::Move(Direction::Up)),
        bound_key(model, &Action::Move(Direction::Down)),
    ) {
        (Some(up), Some(down)) => Some(format!("{up}/{down}")),
        (one, other) => one.or(other),
    };
    let pairs = [
        (bound_key(model, &Action::Activate), "accepts"),
        (bound_key(model, &Action::Dismiss), "cancels"),
        (vertical, "moves"),
    ];
    let mut spans = Vec::new();
    let mut used = 0usize;
    for (key, label) in pairs {
        let Some(key) = key else { continue };
        let keycap = format!(" {key} ");
        let gap = usize::from(used > 0);
        let pair_width = gap + display_width(&keycap) + display_width(label);
        if used + pair_width > width {
            break;
        }
        if gap > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(keycap, theme.recessed_pill(Token::Focus)));
        spans.push(Span::styled(label, theme.style(Token::Text)));
        used += pair_width;
    }
    Line::from(spans)
}

const fn candidate_kind_icon(kind: crate::query::completion::CandidateKind) -> Icon {
    use crate::query::completion::{CandidateKind, CatalogObjectKind};
    match kind {
        CandidateKind::Keyword => Icon::Keyword,
        CandidateKind::Cte => Icon::SearchPath,
        CandidateKind::Column => Icon::Column,
        CandidateKind::Object(
            CatalogObjectKind::Table
            | CatalogObjectKind::ForeignTable
            | CatalogObjectKind::PartitionedTable,
        ) => Icon::Table,
        CandidateKind::Object(CatalogObjectKind::View) => Icon::View,
        CandidateKind::Object(CatalogObjectKind::MaterializedView) => Icon::MaterializedView,
        CandidateKind::Object(CatalogObjectKind::Sequence) => Icon::Sequence,
        CandidateKind::Object(CatalogObjectKind::Function) => Icon::Function,
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
    let capsule_style = if unsaved > 0 {
        theme.capsule(Token::Warning)
    } else {
        theme.pill(Token::Focus)
    };
    let wide_label = format!("[CRASH RECOVERY: {count} {noun}{plural}{truncated_suffix}]");
    let medium_label = format!("[CRASH RECOVERY: {count} {noun}{plural}{truncated_suffix}]");
    let short_label = format!("[CRASH RECOVERY: {count}]");
    let minimal_label = format!("[RECOVERY: {count}]");

    // Degrade in steps that keep the label and the chord meaning. `^K` is the
    // conventional ASCII spelling of Ctrl+K when the full hint does not fit.
    let wide_width =
        wide_label.chars().count() + " [Ctrl+K r] Restore   [Ctrl+K d] Discard".chars().count();
    let medium_width = medium_label.chars().count() + " ^K r Restore  ^K d Discard".chars().count();
    let words_width = short_label.chars().count() + " ^K r Restore  ^K d Discard".chars().count();
    let letters_width = minimal_label.chars().count() + " ^K r/d".chars().count();
    let width = area.width as usize;

    let spans = if width >= wide_width {
        vec![
            Span::styled(wide_label, capsule_style),
            Span::raw(" "),
            Span::styled("[Ctrl+K r] ", theme.style(Token::Info)),
            Span::styled("Restore   ", theme.style(Token::Muted)),
            Span::styled("[Ctrl+K d] ", theme.style(Token::Info)),
            Span::styled("Discard", theme.style(Token::Muted)),
        ]
    } else if width >= medium_width {
        vec![
            Span::styled(medium_label, capsule_style),
            Span::raw(" "),
            Span::styled("^K r ", theme.style(Token::Info)),
            Span::styled("Restore  ", theme.style(Token::Muted)),
            Span::styled("^K d ", theme.style(Token::Info)),
            Span::styled("Discard", theme.style(Token::Muted)),
        ]
    } else if width >= words_width {
        vec![
            Span::styled(short_label, capsule_style),
            Span::raw(" "),
            Span::styled("^K r ", theme.style(Token::Info)),
            Span::styled("Restore  ", theme.style(Token::Muted)),
            Span::styled("^K d ", theme.style(Token::Info)),
            Span::styled("Discard", theme.style(Token::Muted)),
        ]
    } else if width >= letters_width {
        vec![
            Span::styled(minimal_label, capsule_style),
            Span::raw(" "),
            Span::styled("^K r/d", theme.style(Token::Info)),
        ]
    } else {
        vec![Span::styled(minimal_label, capsule_style)]
    };
    Line::from(spans).render(area, buf);
}

fn tab_item_width(label_width: usize, is_active: bool, is_secondary: bool) -> usize {
    if is_active {
        label_width + 3
    } else if is_secondary {
        label_width + 10
    } else {
        label_width + 1
    }
}

fn tab_window_width(start: usize, end: usize, total: usize, widths: &[usize]) -> usize {
    let left = if start > 0 { 2 } else { 0 };
    let right = if end < total { 2 } else { 0 };
    left + widths[start..end].iter().sum::<usize>() + right
}

fn tab_glide_inks_are_legible(theme: &Theme) -> bool {
    let surface_alt = theme.rgb(Token::SurfaceAlt);
    [Token::Info, Token::Focus]
        .into_iter()
        .all(|token| theme.rgb(token).contrast(surface_alt) >= 4.5)
}

fn render_tab_bar(
    model: &Model,
    presentation: &Presentation,
    tab_glide: Option<f32>,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width < 10 || area.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let split_active = model.buffers.split_mode != crate::buffer::EditorSplitMode::None;
    let mut spans = Vec::new();

    // Field Industrial status capsule: [BUFFERS: n/16]
    spans.push(Span::styled(
        format!(
            "[BUFFERS: {}/{}]",
            model.buffers.count(),
            crate::buffer::MAX_EDITOR_BUFFERS
        ),
        theme.pill(Token::Muted),
    ));
    spans.push(Span::raw(" "));

    if split_active {
        spans.push(Span::styled(
            format!("[SPLIT: {}] ", model.buffers.split_mode.label()),
            theme.style(Token::Info).add_modifier(Modifier::BOLD),
        ));
    }

    let total = model.buffers.buffers.len();
    if total == 0 {
        spans.push(Span::styled("[+]", theme.style(Token::Focus)));
        Line::from(spans).render(area, buf);
        return;
    }

    let prefix_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();
    let suffix = "[+]";
    let tab_budget = usize::from(area.width).saturating_sub(prefix_width + display_width(suffix));
    let active_idx = model.buffers.active_idx.min(total.saturating_sub(1));

    let labels: Vec<String> = model
        .buffers
        .buffers
        .iter()
        .enumerate()
        .map(|(idx, b)| format!("{}: {}", idx + 1, sanitize_for_display(&b.display_label())))
        .collect();
    let widths: Vec<usize> = labels
        .iter()
        .enumerate()
        .map(|(idx, label)| {
            let is_active = idx == active_idx;
            let is_secondary = split_active && model.buffers.secondary_idx == Some(idx);
            tab_item_width(display_width(label), is_active, is_secondary)
        })
        .collect();

    let mut start = active_idx;
    let mut end = active_idx + 1;
    loop {
        let mut expanded = false;
        if end < total && tab_window_width(start, end + 1, total, &widths) <= tab_budget {
            end += 1;
            expanded = true;
        }
        if start > 0 && tab_window_width(start - 1, end, total, &widths) <= tab_budget {
            start -= 1;
            expanded = true;
        }
        if !expanded {
            break;
        }
    }

    if start > 0 {
        spans.push(Span::styled("< ", theme.style(Token::Info)));
    }

    let overflow_overhead = if start > 0 { 2 } else { 0 } + if end < total { 2 } else { 0 };
    for (idx, raw_label) in labels.iter().enumerate().take(end).skip(start) {
        let is_active = idx == active_idx;
        let is_secondary = split_active && model.buffers.secondary_idx == Some(idx);
        let label = if start + 1 == end && tab_window_width(start, end, total, &widths) > tab_budget
        {
            let max_label = tab_budget.saturating_sub(overflow_overhead + 3).max(4);
            truncate_to_width(raw_label, max_label, unicode)
        } else {
            raw_label.clone()
        };

        if is_active {
            let mut active_style = theme.active_tab(Token::Focus);
            if let Some(progress) = tab_glide
                && theme.color
                && theme.depth.shows_transitions()
                && tab_glide_inks_are_legible(theme)
                && let Some(ink) = theme.mix(Token::Info, Token::Focus, progress)
            {
                active_style = active_style.fg(ink);
            }
            spans.push(Span::styled(format!("[{label}]"), active_style));
            spans.push(Span::raw(" "));
        } else if is_secondary {
            spans.push(Span::styled(
                format!("[SPLIT: {label}]"),
                theme.recessed_pill(Token::Info),
            ));
            spans.push(Span::raw(" "));
        } else {
            spans.push(Span::styled(format!("{label} "), theme.style(Token::Muted)));
        }
    }

    if end < total {
        spans.push(Span::styled("> ", theme.style(Token::Info)));
    }

    spans.push(Span::styled(suffix, theme.style(Token::Focus)));

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
    split_badge: Option<(usize, &str)>,
) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Editor && is_active;
    let text = editor.text();

    if text.is_empty() {
        if is_active {
            let empty_area = inset_empty_area(inner);
            let keycap = theme.keycap(Token::Focus);
            let ctx = crate::app::discovery::context(model);
            let palette_key = action_key_label(keymap, &crate::app::Action::OpenPalette);
            let help_key = action_key_label(keymap, &crate::app::Action::ToggleHelp);
            let mut prose_lines = Vec::new();
            let mut hint_lines = Vec::new();
            match ctx.connection {
                crate::app::discovery::ConnectionPosture::Disconnected => {
                    prose_lines.push(Line::from(vec![
                        Span::styled(
                            presentation.glyphs.prefix(Icon::Warning),
                            theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "Connection required",
                            theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    prose_lines.push(Line::from(Span::styled(
                        "Connect before running SQL. Results appear after a query.",
                        theme.style(Token::Muted),
                    )));
                    hint_lines.push(Line::from(vec![
                        Span::styled(format!(" {palette_key} "), keycap),
                        Span::styled(
                            "Command palette shows available actions.",
                            theme.style(Token::Info),
                        ),
                    ]));
                }
                crate::app::discovery::ConnectionPosture::Connecting => {
                    prose_lines.push(Line::from(vec![
                        Span::styled(
                            presentation.glyphs.prefix(Icon::Info),
                            theme.style(Token::Info).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "Connecting",
                            theme.style(Token::Info).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    prose_lines.push(Line::from(Span::styled(
                        "The editor is ready. Wait for the connection before running SQL.",
                        theme.style(Token::Muted),
                    )));
                    hint_lines.push(Line::from(vec![
                        Span::styled(format!(" {help_key} "), keycap),
                        Span::styled(
                            "Help explains the keyboard while it connects.",
                            theme.style(Token::Info),
                        ),
                    ]));
                }
                crate::app::discovery::ConnectionPosture::Connected => {
                    let run_key = action_key_label(keymap, &crate::app::Action::RunBuffer);
                    prose_lines.push(Line::from(vec![
                        Span::styled(
                            presentation.glyphs.prefix(Icon::Editor),
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "Start here",
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    prose_lines.push(Line::from(Span::styled(
                        "Type SQL in this editor.",
                        theme.style(Token::Text),
                    )));
                    hint_lines.push(Line::from(vec![
                        Span::styled(format!(" {run_key} "), keycap),
                        Span::styled(
                            "Run the buffer once SQL is ready.",
                            theme.style(Token::Info),
                        ),
                    ]));
                    hint_lines.push(Line::from(vec![
                        Span::styled(format!(" {palette_key} "), keycap),
                        Span::styled(
                            "Command palette answers what else is possible.",
                            theme.style(Token::Muted),
                        ),
                    ]));
                }
                crate::app::discovery::ConnectionPosture::Lost
                | crate::app::discovery::ConnectionPosture::Failed => {
                    prose_lines.push(Line::from(vec![
                        Span::styled(
                            presentation.glyphs.prefix(Icon::Warning),
                            theme.style(Token::Danger).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            ctx.connection.label(),
                            theme.style(Token::Danger).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    prose_lines.push(Line::from(Span::styled(
                        "Reconnect before running SQL. Results remain from the last known query.",
                        theme.style(Token::Muted),
                    )));
                    hint_lines.push(Line::from(vec![
                        Span::styled(format!(" {palette_key} "), keycap),
                        Span::styled(
                            "Command palette shows safe actions.",
                            theme.style(Token::Info),
                        ),
                    ]));
                }
            }
            let min_needed = prose_lines.len().saturating_add(hint_lines.len());
            let show_separator =
                empty_area.height >= 6 && usize::from(empty_area.height) > min_needed;
            if show_separator {
                let rule_width = usize::from(empty_area.width).min(48);
                if rule_width > 0 {
                    prose_lines.push(Line::from(Span::styled(
                        hairline_rule(presentation).repeat(rule_width),
                        theme.style(Token::Border),
                    )));
                }
            }
            prose_lines.extend(hint_lines);
            Paragraph::new(prose_lines)
                .wrap(Wrap { trim: true })
                .render(empty_area, buf);
        } else {
            let badge = split_badge
                .map(|(idx, title)| format!("[SPLIT {}: {}]", idx + 1, sanitize_for_display(title)))
                .unwrap_or_else(|| "[SPLIT: empty buffer]".to_owned());
            let line = Line::from(vec![
                Span::styled(badge, theme.pill(Token::Info)),
                Span::raw(" "),
                Span::styled("(empty buffer)", theme.style(Token::Muted)),
            ]);
            Paragraph::new(vec![line]).render(inner, buf);
        }
        return;
    }

    let (content_area, single_row_badge) = if let Some((idx, title)) = split_badge {
        let badge = format!("[SPLIT {}: {}]", idx + 1, sanitize_for_display(title));
        let badge = truncate_to_width(
            &badge,
            usize::from(inner.width),
            !presentation.glyphs.is_ascii(),
        );
        if inner.height >= 2 {
            Line::from(Span::styled(badge, theme.pill(Token::Info)))
                .render(Rect::new(inner.x, inner.y, inner.width, 1), buf);
            (
                Rect::new(
                    inner.x,
                    inner.y.saturating_add(1),
                    inner.width,
                    inner.height.saturating_sub(1),
                ),
                None,
            )
        } else {
            (inner, Some(badge))
        }
    } else {
        (inner, None)
    };

    let lines: Vec<&str> = text.split('\n').collect();
    let gutter = lines.len().to_string().len().max(2);
    let (cursor_line, _) = editor.position();
    let capacity = usize::from(content_area.width).saturating_sub(gutter + 2);
    let viewport = view::viewport(editor, capacity);
    let (_, cursor) = view::caret_line(editor);

    let height = content_area.height as usize;
    let offset = cursor_line
        .saturating_sub(1)
        .saturating_sub(height.saturating_sub(1));

    let syntax = crate::query::highlight::tokens(text);
    let delimiter_pair = if focused {
        crate::query::highlight::matching_delimiter(text, editor.cursor())
    } else {
        None
    };
    let format_ripple = if is_active {
        model.motion().format_ripple()
    } else {
        None
    };
    let visible_rows = height.min(lines.len().saturating_sub(offset)).max(1);
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
        let visible_row = number - offset - 1;
        let y = content_area.y + u16::try_from(visible_row).unwrap_or(0);
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
            presentation.glyphs.column_rule()
        };
        let line_marker_token = if error_line {
            Token::Danger
        } else if in_statement {
            Token::Focus
        } else {
            Token::Border
        };
        let mut line_marker_style = theme.style(line_marker_token);
        if !error_line && let Some(progress) = format_ripple {
            let row_progress = row_wave_progress(progress, visible_row, visible_rows);
            if let Some(fg) = theme.mix(Token::Success, line_marker_token, row_progress) {
                line_marker_style = line_marker_style.fg(fg);
            }
        }
        let line_number_style = if on_cursor_line {
            if focused {
                theme.on_stripe(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Header)
            }
        } else {
            theme.style(Token::Muted)
        };
        let mut spans = vec![
            Span::styled(format!("{number:>gutter$} "), line_number_style),
            Span::styled(line_marker, line_marker_style),
        ];

        Line::from(std::mem::take(&mut spans))
            .render(Rect::new(content_area.x, y, content_area.width, 1), buf);
        if capacity == 0 {
            continue;
        }
        let text_x = content_area.x + u16::try_from(gutter + 2).unwrap_or(0);
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
            let is_matching_delimiter = delimiter_pair.is_some_and(|(a, b)| {
                let abs = start + unit.source.start;
                abs == a || abs == b
            });
            let style = if is_cursor {
                theme.style(Token::Selection)
            } else if in_error_token {
                theme.style(Token::Danger)
            } else if is_matching_delimiter {
                theme
                    .style(Token::Focus)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
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

    if let Some(badge) = single_row_badge {
        let badge_w = display_width(&badge);
        if usize::from(content_area.width) > badge_w + 4 {
            let badge_x = content_area
                .x
                .saturating_add(content_area.width)
                .saturating_sub(u16::try_from(badge_w).unwrap_or(0));
            buf.set_stringn(
                badge_x,
                content_area.y,
                &badge,
                badge_w,
                theme.pill(Token::Info),
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
        for x in content_area.x..content_area.x.saturating_add(content_area.width) {
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
    let lines_total = model.editor.line_count();
    let lines_noun = if lines_total == 1 { "line" } else { "lines" };
    let line_sep = if presentation.glyphs.is_ascii() {
        ", "
    } else {
        " \u{00b7} "
    };
    let focus_pulse = (focused && !model.reduced_motion && !presentation.reduced_motion)
        .then(|| model.motion().focus_pulse())
        .flatten();
    let tab_glide = (!model.reduced_motion && !presentation.reduced_motion)
        .then(|| model.motion().tab_glide())
        .flatten();
    let has_tab_bar_state = model.buffers.count() > 1
        || model.buffers.split_mode != crate::buffer::EditorSplitMode::None;
    let inner_height_est = area.height.saturating_sub(2);
    let after_recovery_est = if model.recovery_banner.is_some() && inner_height_est > 0 {
        inner_height_est.saturating_sub(1)
    } else {
        inner_height_est
    };
    let tab_bar_visible =
        has_tab_bar_state && after_recovery_est > 2 && area.width.saturating_sub(2) >= 10;

    let mut title_spans = Vec::new();
    if model.splits.zoomed && focused {
        title_spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
    }
    let focus_style = focus_accent_style(theme, focus_pulse).add_modifier(Modifier::BOLD);
    let primary_style = if focused {
        focus_style
    } else {
        theme.style(Token::Header)
    };
    let editor_label = format!("{}Editor", presentation.icon(Icon::Editor));
    let pos_label = format!("  line {line}, column {column}");
    let line_count_suffix = format!("{line_sep}{lines_total} {lines_noun}");
    let split_badge = (!tab_bar_visible
        && model.buffers.split_mode != crate::buffer::EditorSplitMode::None)
        .then(|| format!("  [SPLIT: {}]", model.buffers.split_mode.label()));
    let buffers_badge = (!tab_bar_visible && model.buffers.count() > 1).then(|| {
        format!(
            "  [BUFFERS: {}/{}]",
            model.buffers.count(),
            crate::buffer::MAX_EDITOR_BUFFERS
        )
    });
    let reserved_w = 2
        + usize::from(presentation.spacing().title_padding) * 2
        + usize::from(focused) * 2
        + usize::from(model.splits.zoomed && focused) * 9
        + display_width(&editor_label)
        + display_width(&pos_label)
        + split_badge.as_deref().map_or(0, display_width)
        + buffers_badge.as_deref().map_or(0, display_width)
        + usize::from(model.editor.is_modified()) * 12
        + usize::from(focused) * 11;
    let pos_text = if reserved_w + display_width(&line_count_suffix) <= usize::from(area.width) {
        format!("{pos_label}{line_count_suffix}")
    } else {
        pos_label
    };
    title_spans.push(Span::styled(editor_label, primary_style));
    title_spans.push(Span::styled(pos_text, theme.style(Token::Muted)));
    if let Some(badge) = split_badge {
        title_spans.push(Span::styled(
            badge,
            theme.style(Token::Info).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(badge) = buffers_badge {
        title_spans.push(Span::styled(badge, theme.style(Token::Muted)));
    }
    if model.editor.is_modified() {
        title_spans.push(Span::raw("  "));
        title_spans.push(Span::styled("[modified]", theme.pill(Token::Warning)));
    }
    if focused {
        title_spans.push(Span::styled("  [focused]", focus_style));
    }

    let block =
        pane_block_line_with_motion(Line::from(title_spans), focused, focus_pulse, presentation);
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
            tab_glide,
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
                buf.set_stringn(sep_x, y, sep_char, 1, theme.style(Token::Border));
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
                let sec_badge = model
                    .buffers
                    .secondary_idx
                    .map(|idx| (idx, sec_buf.title.as_str()));
                render_editor_buffer_pane(
                    &sec_buf.editor,
                    model,
                    keymap,
                    presentation,
                    right_rect,
                    buf,
                    false,
                    sec_badge,
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
                "\u{2500}"
            };
            for x in text_inner.x..text_inner.x.saturating_add(text_inner.width) {
                buf.set_stringn(x, sep_y, sep_char, 1, theme.style(Token::Border));
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
                let sec_badge = model
                    .buffers
                    .secondary_idx
                    .map(|idx| (idx, sec_buf.title.as_str()));
                render_editor_buffer_pane(
                    &sec_buf.editor,
                    model,
                    keymap,
                    presentation,
                    bottom_rect,
                    buf,
                    false,
                    sec_badge,
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
