use super::{action_key_label, input_cursor, pane_block};
use crate::app::model::{Focus, Model};
use crate::query::value::sanitize_for_display;
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
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
    if model.splits.zoomed && focused {
        title.insert_str(0, " (zoomed)");
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
        let lines = if model.tree.filter.is_empty() {
            if model.connection.is_usable() {
                vec![
                    "No object rows are loaded yet.".to_owned(),
                    format!(
                        "{} Reload the object tree.",
                        action_key_label(keymap, &crate::app::Action::ReloadObjects)
                    ),
                ]
            } else if matches!(
                model.connection,
                crate::app::model::ConnectionState::Connecting { .. }
            ) {
                vec![
                    "Object tree unavailable until connected.".to_owned(),
                    "The connection is being established.".to_owned(),
                ]
            } else if model.connection_profiles.is_empty() {
                // Nothing to pick from, so the one next action is outside the
                // client: name the configuration route and the command-line
                // route rather than offering a key that would open nothing.
                vec![
                    "Object tree unavailable until connected.".to_owned(),
                    "Add a profile in config.toml or pass a connection target when starting."
                        .to_owned(),
                ]
            } else {
                // The picker is the configured route; use its existing key and
                // wording rather than inventing a second description.
                let picker = keymap
                    .contextual_hint(&crate::app::Action::OpenConnectionPicker)
                    .map_or_else(
                        || "Open the command palette to choose a connection".to_owned(),
                        |(key, label)| format!("{key} {label}"),
                    );
                vec![
                    "Object tree unavailable until connected.".to_owned(),
                    format!("{picker}. Object names appear after reload."),
                ]
            }
        } else {
            vec![
                "No object matches this filter.".to_owned(),
                format!("Searched {:?}.", sanitize_for_display(&model.tree.filter)),
                format!(
                    "{} or {} clears the filter.",
                    action_key_label(keymap, &crate::app::Action::Dismiss),
                    action_key_label(keymap, &crate::app::Action::StartFilter)
                ),
            ]
        };
        Paragraph::new(
            lines
                .into_iter()
                .map(|line| Line::from(Span::styled(line, theme.style(Token::Muted))))
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: true })
        .render(body, buf);
        return;
    }

    let visible = body.height as usize;
    let offset = model
        .tree
        .selected
        .saturating_sub(visible.saturating_sub(1))
        .min(rows.len().saturating_sub(1));

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

            spans.push(Span::styled(
                sanitize_for_display(&row.label),
                selection_style,
            ));

            if row.loading && !presentation.reduced_motion {
                spans.push(Span::styled(
                    format!(" {}", presentation.glyphs.spinner(model.frame)),
                    selection_style,
                ));
            } else if row.loading {
                spans.push(Span::styled(" loading", selection_style));
            } else if let Some(detail) = &row.detail {
                spans.push(Span::styled(
                    format!("  {}", sanitize_for_display(detail)),
                    selection_style,
                ));
            }

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

        match &row.row_kind {
            crate::app::tree::RowKind::Schema { usable } => {
                let (icon_token, label_token) = if *usable {
                    (Token::Focus, Token::Header)
                } else {
                    (Token::Muted, Token::Muted)
                };
                let icon = presentation.icon(Icon::Schema);
                if !icon.is_empty() {
                    spans.push(Span::styled(icon, theme.style(icon_token)));
                }
                spans.push(Span::styled(
                    sanitize_for_display(&row.label),
                    theme.style(label_token),
                ));
            }
            crate::app::tree::RowKind::Group(kind) => {
                let icon = presentation.icon(group_icon(*kind));
                if !icon.is_empty() {
                    spans.push(Span::styled(icon, theme.style(Token::Info)));
                }
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
                    spans.push(Span::styled(group_kind.plural(), theme.style(Token::Text)));
                    spans.push(Span::styled(
                        format!(" ({count})"),
                        theme.style(Token::Muted),
                    ));
                } else {
                    spans.push(Span::styled(
                        sanitize_for_display(&row.label),
                        theme.style(Token::Text),
                    ));
                }
            }
            crate::app::tree::RowKind::Object { kind, readable } => match kind {
                crate::postgres::ObjectKind::Index => {
                    let icon = presentation.icon(Icon::Index);
                    if !icon.is_empty() {
                        spans.push(Span::styled(icon, theme.style(Token::Info)));
                    }
                    spans.push(Span::styled(
                        sanitize_for_display(&row.label),
                        theme.style(Token::Muted),
                    ));
                }
                crate::postgres::ObjectKind::Extension => {
                    let icon = presentation.icon(Icon::Extension);
                    if !icon.is_empty() {
                        spans.push(Span::styled(icon, theme.style(Token::Success)));
                    }
                    spans.push(Span::styled(
                        sanitize_for_display(&row.label),
                        theme.style(Token::Text),
                    ));
                }
                _ => {
                    let icon = presentation.icon(group_icon(*kind));
                    if !icon.is_empty() {
                        spans.push(Span::styled(
                            icon,
                            theme.style(object_semantic_token(*kind)),
                        ));
                    }
                    let label_token = if *readable {
                        Token::Text
                    } else {
                        Token::Warning
                    };
                    spans.push(Span::styled(
                        sanitize_for_display(&row.label),
                        theme.style(label_token),
                    ));
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
                if !icon.is_empty() {
                    spans.push(Span::styled(icon, theme.style(icon_token)));
                }
                spans.push(Span::styled(
                    sanitize_for_display(&row.label),
                    theme.style(label_token),
                ));
            }
            crate::app::tree::RowKind::Message => {
                let icon = presentation.icon(Icon::Info);
                if !icon.is_empty() {
                    spans.push(Span::styled(icon, theme.style(Token::Muted)));
                }
                spans.push(Span::styled(
                    sanitize_for_display(&row.label),
                    theme.style(Token::Muted),
                ));
            }
        }

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
