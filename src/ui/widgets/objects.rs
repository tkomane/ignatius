use super::{
    action_key_label, hairline_rule, input_cursor, inset_empty_area, pane_block_line, rail_keycap,
};
use crate::app::model::{Focus, Model};
use crate::query::value::{display_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

/// The row rectangle after the pane border and optional filter prompt.
pub(crate) fn content_area(model: &Model, inner: Rect) -> Rect {
    if model.tree.filtering || !model.tree.filter.is_empty() {
        let [_, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        rest
    } else {
        inner
    }
}

pub(crate) fn content_capacity(model: &Model, pane: Rect) -> usize {
    let body = content_area(model, super::inner_rect(pane));
    if body.width == 0
        || model.tree.error.is_some()
        || (model.tree.loading && model.tree.roots.is_empty())
    {
        0
    } else {
        usize::from(body.height)
    }
}

pub(crate) fn render_objects(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Objects;
    let unicode = !presentation.glyphs.is_ascii();

    // The title doubles as a breadcrumb, so the path to the selected object is
    // visible without a second row of interface.
    let block = pane_block_line(
        objects_title_line(model, presentation, focused, area.width),
        focused,
        presentation,
    );
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
                error.next_action.clone().unwrap_or_else(|| {
                    format!(
                        "{} Reload the object tree when the connection is usable.",
                        action_key_label(keymap, &crate::app::Action::ReloadObjects)
                    )
                }),
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
    let body = content_area(model, inner);
    if model.tree.filtering || !model.tree.filter.is_empty() {
        let prompt = format!(
            " {}{}{}",
            if presentation.glyphs.is_ascii() {
                "/"
            } else {
                "\u{2315}"
            },
            model.tree.filter,
            if model.tree.filtering {
                input_cursor(presentation)
            } else {
                ""
            }
        );
        let [filter_area, _] =
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
    }

    if rows.is_empty() {
        render_empty_objects(model, keymap, presentation, body, buf);
        return;
    }

    let visible = body.height as usize;
    let offset = model
        .tree
        .selected
        .saturating_sub(visible.saturating_sub(1))
        .min(rows.len().saturating_sub(1));

    let max_row_width = usize::from(body.width);
    let mut selected_y = None;

    for (screen_row, (index, row)) in rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .enumerate()
    {
        let selected = focused && index == model.tree.selected;
        if selected {
            selected_y = Some(body.y + u16::try_from(screen_row).unwrap_or(0));
        }
        let mut spans = Vec::new();
        let node = model.tree.node(&row.path);

        if selected {
            let selection_style = theme.style(Token::Selection);
            for _ in 0..row.depth {
                spans.push(Span::styled(
                    presentation.glyphs.indent_guide(),
                    selection_style,
                ));
            }

            if row.expandable {
                spans.push(Span::styled(
                    format!("{} ", presentation.glyphs.chevron(row.expanded)),
                    selection_style,
                ));
            } else {
                spans.push(Span::styled("  ", selection_style));
            }

            let icon = presentation.icon(row_icon(&row.row_kind));
            if !icon.is_empty() {
                spans.push(Span::styled(icon, selection_style));
            }

            let prefix_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();
            let label_budget = max_row_width.saturating_sub(prefix_width);
            let label = truncate_to_width(&sanitize_for_display(&row.label), label_budget, unicode);
            spans.push(Span::styled(label, selection_style));

            push_bounded_row_detail(
                &mut spans,
                row,
                max_row_width,
                unicode,
                model,
                presentation,
                selection_style,
            );

            let line = Line::from(spans).style(selection_style);
            lines.push(line);
            continue;
        }

        // Indent guides for unselected rows
        for _ in 0..row.depth {
            spans.push(Span::styled(
                presentation.glyphs.indent_guide(),
                theme.style(Token::Muted),
            ));
        }

        if row.expandable {
            spans.push(Span::styled(
                format!("{} ", presentation.glyphs.chevron(row.expanded)),
                theme.style(Token::Muted),
            ));
        } else {
            spans.push(Span::styled("  ", theme.style(Token::Muted)));
        }

        let prefix_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();
        let remaining_after_prefix = max_row_width.saturating_sub(prefix_width);

        match &row.row_kind {
            crate::app::tree::RowKind::Schema { usable } => {
                let (icon_token, label_token) = if *usable {
                    (Token::Focus, Token::Header)
                } else {
                    (Token::Muted, Token::Muted)
                };
                let budget = push_icon_and_budget(
                    &mut spans,
                    presentation.icon(Icon::Schema),
                    theme.style(icon_token),
                    remaining_after_prefix,
                );
                let label = truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                spans.push(Span::styled(label, theme.style(label_token)));
            }
            crate::app::tree::RowKind::Group(kind) => {
                let budget = push_icon_and_budget(
                    &mut spans,
                    presentation.icon(group_icon(*kind)),
                    theme.style(Token::Info),
                    remaining_after_prefix,
                );
                if let Some(crate::app::tree::Node {
                    kind:
                        crate::app::tree::NodeKind::Group {
                            count,
                            kind: group_kind,
                            ..
                        },
                    ..
                }) = node
                {
                    let count_str = format!(" ({count})");
                    let name_budget = budget.saturating_sub(display_width(&count_str));
                    let plural = truncate_to_width(group_kind.plural(), name_budget, unicode);
                    spans.push(Span::styled(plural, theme.style(Token::Text)));
                    spans.push(Span::styled(count_str, theme.style(Token::Muted)));
                } else {
                    let label =
                        truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                    spans.push(Span::styled(label, theme.style(Token::Text)));
                }
            }
            crate::app::tree::RowKind::Object { kind, readable } => match kind {
                crate::postgres::ObjectKind::Index => {
                    let budget = push_icon_and_budget(
                        &mut spans,
                        presentation.icon(Icon::Index),
                        theme.style(Token::Info),
                        remaining_after_prefix,
                    );
                    let label =
                        truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                    spans.push(Span::styled(label, theme.style(Token::Muted)));
                }
                crate::postgres::ObjectKind::Extension => {
                    let budget = push_icon_and_budget(
                        &mut spans,
                        presentation.icon(Icon::Extension),
                        theme.style(Token::Success),
                        remaining_after_prefix,
                    );
                    let label =
                        truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                    spans.push(Span::styled(label, theme.style(Token::Text)));
                }
                _ => {
                    let budget = push_icon_and_budget(
                        &mut spans,
                        presentation.icon(group_icon(*kind)),
                        theme.style(object_semantic_token(*kind)),
                        remaining_after_prefix,
                    );
                    let label_token = if *readable {
                        Token::Text
                    } else {
                        Token::Warning
                    };
                    let label =
                        truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                    spans.push(Span::styled(label, theme.style(label_token)));
                }
            },
            crate::app::tree::RowKind::Column { primary_key, .. } => {
                let icon = presentation.icon(if *primary_key {
                    Icon::KeyColumn
                } else {
                    Icon::Column
                });
                let (icon_token, label_token) = if *primary_key {
                    (Token::Warning, Token::Text)
                } else {
                    (Token::Muted, Token::Muted)
                };
                let budget = push_icon_and_budget(
                    &mut spans,
                    icon,
                    theme.style(icon_token),
                    remaining_after_prefix,
                );
                let label = truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                spans.push(Span::styled(label, theme.style(label_token)));
            }
            crate::app::tree::RowKind::Message => {
                let budget = push_icon_and_budget(
                    &mut spans,
                    presentation.icon(Icon::Info),
                    theme.style(Token::Muted),
                    remaining_after_prefix,
                );
                let label = truncate_to_width(&sanitize_for_display(&row.label), budget, unicode);
                spans.push(Span::styled(label, theme.style(Token::Muted)));
            }
        }

        let detail_style = if row.loading && !presentation.reduced_motion {
            theme.style(Token::Info)
        } else {
            theme.style(Token::Muted)
        };
        push_bounded_row_detail(
            &mut spans,
            row,
            max_row_width,
            unicode,
            model,
            presentation,
            detail_style,
        );

        let line = Line::from(spans);
        lines.push(line);
    }

    Paragraph::new(lines).render(body, buf);

    if let Some(sy) = selected_y {
        let selection_style = theme.style(Token::Selection);
        for x in body.x..body.x.saturating_add(body.width) {
            buf[(x, sy)].set_style(selection_style);
        }
    }
}

fn objects_title_line(
    model: &Model,
    presentation: &Presentation,
    focused: bool,
    area_width: u16,
) -> Line<'static> {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let pad = usize::from(presentation.spacing().title_padding) * 2;
    let leader_w = usize::from(focused) * 2;
    let zoomed = model.splits.zoomed && focused;
    let zoomed_w = if zoomed { 9 } else { 0 };
    let focused_w = if focused { 10 } else { 0 };
    let avail = usize::from(area_width).saturating_sub(2 + pad + leader_w + zoomed_w + focused_w);

    let mut spans = Vec::new();
    if zoomed {
        spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
    }
    let primary_style = if focused {
        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
    } else {
        theme.style(Token::Header)
    };
    let icon = presentation.icon(Icon::View);

    if let Some(row) = model.tree.selected_row() {
        let separator = if unicode { " \u{203a} " } else { " > " };
        let mut trail = Vec::new();
        for depth in 0..=row.depth {
            let ancestor = &row.path[..=depth];
            if let Some(node) = model.tree.node(ancestor) {
                trail.push(sanitize_for_display(&node.label()));
            }
        }
        let raw_path = if trail.is_empty() {
            "Objects".to_owned()
        } else {
            trail.join(separator)
        };
        let full_crumb = format!("{icon}{raw_path}");
        let truncated = truncate_to_width(&full_crumb, avail, unicode);
        let used = display_width(&truncated);
        if let Some(pos) = truncated.rfind(separator) {
            let prefix_end = pos + separator.len();
            spans.push(Span::styled(
                truncated[..prefix_end].to_owned(),
                theme.style(Token::Muted),
            ));
            spans.push(Span::styled(
                truncated[prefix_end..].to_owned(),
                primary_style,
            ));
        } else {
            spans.push(Span::styled(truncated, primary_style));
        }
        let rem = avail.saturating_sub(used);
        if !model.tree.filter.is_empty() && rem >= 8 {
            let hint = truncate_to_width(
                &format!("  filter: {}", sanitize_for_display(&model.tree.filter)),
                rem,
                unicode,
            );
            spans.push(Span::styled(hint, theme.style(Token::Muted)));
        }
    } else {
        let base = truncate_to_width(&format!("{icon}Objects"), avail, unicode);
        let used = display_width(&base);
        spans.push(Span::styled(base, primary_style));
        let rem = avail.saturating_sub(used);
        if !model.tree.filter.is_empty() && rem >= 8 {
            let hint = truncate_to_width(
                &format!("  filter: {}", sanitize_for_display(&model.tree.filter)),
                rem,
                unicode,
            );
            spans.push(Span::styled(hint, theme.style(Token::Muted)));
        } else if !model.tree.roots.is_empty() && rem >= 10 {
            let count = model.tree.roots.len();
            let noun = if count == 1 { "schema" } else { "schemas" };
            let hint = truncate_to_width(&format!("  {count} {noun}"), rem, unicode);
            spans.push(Span::styled(hint, theme.style(Token::Muted)));
        }
    }

    if focused {
        spans.push(Span::styled(
            " [focused]",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn push_icon_and_budget(
    spans: &mut Vec<Span<'static>>,
    icon: String,
    icon_style: Style,
    remaining: usize,
) -> usize {
    let icon_w = display_width(&icon);
    if !icon.is_empty() {
        spans.push(Span::styled(icon, icon_style));
    }
    remaining.saturating_sub(icon_w)
}

fn push_bounded_row_detail(
    spans: &mut Vec<Span<'static>>,
    row: &crate::app::tree::TreeRow,
    max_width: usize,
    unicode: bool,
    model: &Model,
    presentation: &Presentation,
    style: Style,
) {
    let used: usize = spans.iter().map(|s| display_width(&s.content)).sum();
    let remaining = max_width.saturating_sub(used);
    if row.loading && !presentation.reduced_motion {
        let suffix = format!(" {}", presentation.glyphs.spinner(model.frame));
        if display_width(&suffix) <= remaining {
            spans.push(Span::styled(suffix, style));
        }
    } else if row.loading {
        if remaining >= 8 {
            spans.push(Span::styled(" loading", style));
        }
    } else if let Some(detail) = &row.detail {
        let detail_budget = remaining.saturating_sub(2);
        let min_budget = if unicode { 3 } else { 5 };
        if detail_budget >= min_budget {
            let clean = sanitize_for_display(detail);
            let clipped = truncate_to_width(&clean, detail_budget, unicode);
            spans.push(Span::styled(format!("  {clipped}"), style));
        }
    }
}

fn render_empty_objects(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    body: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let empty_area = if body.width >= 36 {
        inset_empty_area(body)
    } else {
        let top_pad = u16::from(body.height >= 7);
        let left_pad = u16::from(body.width >= 4);
        Rect::new(
            body.x + left_pad,
            body.y + top_pad,
            body.width.saturating_sub(left_pad),
            body.height.saturating_sub(top_pad),
        )
    };
    let keycap = rail_keycap(theme, Token::Focus);
    let rule_width = usize::from(empty_area.width).min(28);
    let avail_width = usize::from(empty_area.width);

    let mut prose_lines: Vec<Line<'static>> = Vec::new();
    let mut hint_lines: Vec<Line<'static>> = Vec::new();

    if model.tree.filter.is_empty() {
        if model.connection.is_usable() {
            let reload_key = action_key_label(keymap, &crate::app::Action::ReloadObjects);
            let headline = "No object rows are loaded yet.";
            let prefix = presentation.glyphs.prefix(Icon::Database);
            let mut head_spans = Vec::new();
            if !prefix.is_empty() && display_width(&prefix) + display_width(headline) <= avail_width
            {
                head_spans.push(Span::styled(
                    prefix,
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ));
            }
            head_spans.push(Span::styled(headline, theme.style(Token::Header)));
            prose_lines.push(Line::from(head_spans));
            prose_lines.push(Line::from(Span::styled(
                "Schemas, tables, and views populate here once loaded.",
                theme.style(Token::Muted),
            )));
            hint_lines.push(Line::from(vec![
                Span::styled(format!(" {reload_key} "), keycap),
                Span::styled("Reload the object tree.", theme.style(Token::Text)),
            ]));
            if empty_area.height >= 8 {
                let palette_key = action_key_label(keymap, &crate::app::Action::OpenPalette);
                let run_key = action_key_label(keymap, &crate::app::Action::RunBuffer);
                let filter_key = action_key_label(keymap, &crate::app::Action::StartFilter);
                hint_lines.push(Line::from(vec![
                    Span::styled(format!(" {palette_key} "), keycap),
                    Span::styled("Command palette", theme.style(Token::Muted)),
                ]));
                hint_lines.push(Line::from(vec![
                    Span::styled(format!(" {run_key} "), keycap),
                    Span::styled("Execute query", theme.style(Token::Muted)),
                ]));
                hint_lines.push(Line::from(vec![
                    Span::styled(format!(" {filter_key} "), keycap),
                    Span::styled("Filter objects", theme.style(Token::Muted)),
                ]));
            }
        } else if matches!(
            model.connection,
            crate::app::model::ConnectionState::Connecting { .. }
        ) {
            prose_lines.push(Line::from(vec![
                Span::styled(
                    presentation.glyphs.prefix(Icon::Info),
                    theme.style(Token::Info).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Object tree unavailable until connected.",
                    theme.style(Token::Header),
                ),
            ]));
            prose_lines.push(Line::from(Span::styled(
                "The connection is being established.",
                theme.style(Token::Muted),
            )));
        } else if model.connection_profiles.is_empty() {
            // Nothing to pick from, so the one next action is outside the
            // client: name the configuration route and the command-line
            // route rather than offering a key that would open nothing.
            prose_lines.push(Line::from(vec![
                Span::styled(
                    presentation.glyphs.prefix(Icon::Warning),
                    theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Object tree unavailable until connected.",
                    theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                ),
            ]));
            prose_lines.push(Line::from(Span::styled(
                "Add a profile in config.toml or pass a connection target when starting.",
                theme.style(Token::Muted),
            )));
        } else {
            // The picker is the configured route; use its existing key and
            // wording rather than inventing a second description.
            prose_lines.push(Line::from(vec![
                Span::styled(
                    presentation.glyphs.prefix(Icon::Warning),
                    theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Object tree unavailable until connected.",
                    theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                ),
            ]));
            prose_lines.push(Line::from(Span::styled(
                "Object names appear after reload.",
                theme.style(Token::Muted),
            )));
            if let Some((key, label)) =
                keymap.contextual_hint(&crate::app::Action::OpenConnectionPicker)
            {
                hint_lines.push(Line::from(vec![
                    Span::styled(format!(" {key} "), keycap),
                    Span::styled(format!("{label}."), theme.style(Token::Text)),
                ]));
            } else {
                hint_lines.push(Line::from(Span::styled(
                    "Open the command palette to choose a connection.",
                    theme.style(Token::Info),
                )));
            }
        }
    } else {
        let headline = "No object matches this filter.";
        let prefix = presentation.glyphs.prefix(Icon::Index);
        let mut head_spans = Vec::new();
        if !prefix.is_empty() && display_width(&prefix) + display_width(headline) <= avail_width {
            head_spans.push(Span::styled(
                prefix,
                theme.style(Token::Warning).add_modifier(Modifier::BOLD),
            ));
        }
        head_spans.push(Span::styled(headline, theme.style(Token::Header)));
        prose_lines.push(Line::from(head_spans));
        prose_lines.push(Line::from(Span::styled(
            format!("Searched {:?}.", sanitize_for_display(&model.tree.filter)),
            theme.style(Token::Muted),
        )));
        hint_lines.push(Line::from(vec![
            Span::styled(
                format!(
                    " {} ",
                    action_key_label(keymap, &crate::app::Action::Dismiss)
                ),
                keycap,
            ),
            Span::styled(" or", theme.style(Token::Muted)),
            Span::styled(
                format!(
                    " {} ",
                    action_key_label(keymap, &crate::app::Action::StartFilter)
                ),
                keycap,
            ),
            Span::styled("clears the filter.", theme.style(Token::Muted)),
        ]));
    }

    if hint_lines.is_empty() {
        Paragraph::new(prose_lines)
            .wrap(Wrap { trim: true })
            .render(empty_area, buf);
        return;
    }

    let measured = wrapped_line_count(&prose_lines, empty_area.width);
    let prose = Paragraph::new(prose_lines).wrap(Wrap { trim: true });
    let separator_rows = u16::from(empty_area.height >= 6);
    let hints_height = u16::try_from(hint_lines.len())
        .unwrap_or(u16::MAX)
        .min(empty_area.height);
    let max_prose = empty_area
        .height
        .saturating_sub(hints_height + separator_rows);
    let gap = u16::from(separator_rows > 0 && measured.saturating_add(1) <= max_prose);
    let prose_height = measured.saturating_add(gap).min(max_prose);
    let [prose_rect, sep_rect, hints_rect] = Layout::vertical([
        Constraint::Length(prose_height),
        Constraint::Length(separator_rows),
        Constraint::Length(hints_height),
    ])
    .areas(empty_area);

    prose.render(prose_rect, buf);
    if separator_rows > 0 && rule_width > 0 {
        Paragraph::new(Line::from(Span::styled(
            hairline_rule(presentation).repeat(rule_width),
            theme.style(Token::Border),
        )))
        .render(sep_rect, buf);
    }
    Paragraph::new(hint_lines).render(hints_rect, buf);
}

fn wrapped_line_count(lines: &[Line<'_>], width: u16) -> u16 {
    let cols = usize::from(width.max(1));
    let mut rows = 0usize;
    for line in lines {
        let mut current = 0usize;
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if text.trim().is_empty() {
            rows += 1;
            continue;
        }
        rows += 1;
        for word in text.split_whitespace() {
            let word_w = display_width(word).max(1);
            if current == 0 {
                rows += (word_w - 1) / cols;
                current = ((word_w - 1) % cols) + 1;
            } else if current + 1 + word_w <= cols {
                current += 1 + word_w;
            } else {
                rows += 1 + (word_w - 1) / cols;
                current = ((word_w - 1) % cols) + 1;
            }
        }
    }
    u16::try_from(rows).unwrap_or(u16::MAX)
}

/// The icon for a tree row. Icons are supplementary: the label already carries
/// the kind for groups, and the detail carries it for objects.
const fn row_icon(kind: &crate::app::tree::RowKind) -> Icon {
    use crate::app::tree::RowKind;
    match kind {
        RowKind::Schema { .. } => Icon::Schema,
        RowKind::Group(object) | RowKind::Object { kind: object, .. } => group_icon(*object),
        RowKind::Column {
            primary_key: true, ..
        } => Icon::KeyColumn,
        RowKind::Column { .. } => Icon::Column,
        RowKind::Message => Icon::Info,
    }
}

const fn group_icon(kind: crate::postgres::ObjectKind) -> Icon {
    use crate::postgres::ObjectKind;
    match kind {
        ObjectKind::Table | ObjectKind::PartitionedTable | ObjectKind::ForeignTable => Icon::Table,
        ObjectKind::View => Icon::View,
        ObjectKind::MaterializedView => Icon::MaterializedView,
        ObjectKind::Sequence => Icon::Sequence,
        ObjectKind::Function => Icon::Function,
        ObjectKind::Index => Icon::Index,
        ObjectKind::Extension => Icon::Extension,
        ObjectKind::Column => Icon::Column,
    }
}

const fn object_semantic_token(kind: crate::postgres::ObjectKind) -> Token {
    use crate::postgres::ObjectKind;
    match kind {
        ObjectKind::Table | ObjectKind::PartitionedTable | ObjectKind::ForeignTable => Token::Info,
        ObjectKind::View | ObjectKind::MaterializedView => Token::SyntaxKeyword,
        ObjectKind::Sequence => Token::SyntaxLiteral,
        ObjectKind::Function => Token::SyntaxNumber,
        ObjectKind::Index => Token::Info,
        ObjectKind::Extension => Token::Success,
        ObjectKind::Column => Token::Muted,
    }
}

#[cfg(test)]
mod tests;
