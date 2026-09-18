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
            if model.tree.filtering {
                input_cursor(presentation)
            } else {
                ""
            }
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

#[cfg(test)]
mod tests;
