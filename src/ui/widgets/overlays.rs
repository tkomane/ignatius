use super::{action_key_label, input_cursor, overlay_block, pane_block, syntax_token};
use crate::app::model::{Focus, Model};
use crate::query::value::{sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, definition_box, definition_viewport};
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

/// Asks for a name, for the one thing in this client that needs one.
///
/// Visible, unlike the password prompt: seeing what a query will be called is
/// the point of asking, and the name is going to be a file name that the user
/// will see in a directory listing afterwards.
pub(crate) fn render_name_prompt(
    prompt: &crate::app::model::NamePrompt,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(70);
    let height = 7.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let lines = vec![
        Line::from(Span::styled(
            sanitize_for_display(&prompt.subject),
            theme.style(Token::Text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Name: ", theme.style(Token::Text)),
            Span::styled(
                sanitize_for_display(&prompt.typed),
                theme.style(Token::Focus),
            ),
            Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            sanitize_for_display(&prompt.note),
            theme.style(Token::Muted),
        )),
    ];

    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(overlay_block(
            format!(
                " {}Save  Enter to save, Esc to cancel ",
                presentation.icon(Icon::Editor)
            ),
            presentation,
        ))
        .render(box_area, buf);
}

/// Asks for one named value without changing the SQL editor underneath.
pub(crate) fn render_parameter_prompt(
    prompt: &crate::app::model::ParameterPrompt,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(84);
    let height = 11.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let dot = if presentation.glyphs.is_ascii() {
        "*"
    } else {
        "\u{2022}"
    };
    let name = prompt
        .active_name()
        .map_or_else(|| "unknown".to_owned(), sanitize_for_display);
    let progress = format!(
        "Parameter {} of {}: :{name}",
        prompt.active_index() + 1,
        prompt.total()
    );
    let value = if prompt.typed_length() == 0 {
        "(empty)".to_owned()
    } else {
        dot.repeat(prompt.typed_length())
    };
    let lines = vec![
        Line::from(Span::styled(progress, theme.style(Token::Focus))),
        Line::from(""),
        Line::from(vec![
            Span::styled("Value: ", theme.style(Token::Text)),
            Span::styled(value, theme.style(Token::Focus)),
            Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "The answer is literal text data. Quotes, punctuation and newlines stay data; empty text is valid.",
            theme.style(Token::Muted),
        )),
        Line::from(Span::styled(
            "The SQL template stays unchanged and prompted values are kept nowhere in history.",
            theme.style(Token::Muted),
        )),
    ];

    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(overlay_block(
            format!(
                " {}Parameter  Enter to accept, Esc to cancel ",
                presentation.icon(Icon::Editor)
            ),
            presentation,
        ))
        .render(box_area, buf);
}

/// Asks for a password the server has demanded.
///
/// The field shows how many characters have been typed and nothing else: no
/// characters, no last-character reveal, no strength opinion. What is typed goes
/// into one connection attempt and is dropped with it.
pub(crate) fn render_password_prompt(
    prompt: &crate::app::model::PasswordPrompt,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(72);
    let height = 9.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let dot = if presentation.glyphs.is_ascii() {
        "*"
    } else {
        "\u{2022}"
    };
    let lines = vec![
        Line::from(Span::styled(
            sanitize_for_display(&prompt.reason),
            theme.style(Token::Danger),
        )),
        Line::from(Span::styled(
            sanitize_for_display(&prompt.target),
            theme.style(Token::Muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Password: ", theme.style(Token::Text)),
            Span::styled(dot.repeat(prompt.length()), theme.style(Token::Focus)),
            Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "It is used for this connection and kept nowhere: not in the configuration \
             file, not in the history, not on disk.",
            theme.style(Token::Muted),
        )),
    ];

    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(overlay_block(
            format!(
                " {}Password  Enter to try again, Esc to cancel ",
                presentation.icon(Icon::Role)
            ),
            presentation,
        ))
        .render(box_area, buf);
}

/// Asks before a write reaches a database the user called production.
///
/// It states what will run, what it appears to do, and that the judgement is
/// advisory. A prompt that overstates its own certainty teaches people to
/// distrust it, which costs more than it saves.
pub(crate) fn render_confirmation(
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
            Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
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
        .block(overlay_block(
            " Confirm  Esc to cancel ".to_owned(),
            presentation,
        ))
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
}

/// Explains the one-shot side effect before EXPLAIN ANALYZE is sent.
pub(crate) fn render_plan_confirmation(
    model: &Model,
    pending: &crate::app::model::PendingPlan,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(84);
    let height = 14.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let target = model
        .connection
        .info()
        .map_or_else(|| "this database".to_owned(), |info| info.target.clone());
    let mut lines = vec![
        Line::from(Span::styled(
            "EXPLAIN ANALYZE will execute the statement to measure it.",
            theme.style(Token::Danger),
        )),
        Line::from(Span::styled(
            "Side effects are possible. This client will not automatically roll it back.",
            theme.style(Token::Warning),
        )),
        Line::from(Span::styled(
            format!("Target: {}", safe_plan_text(&target)),
            theme.style(Token::Muted),
        )),
        Line::from(""),
    ];
    for line in pending.sql.lines().take(4) {
        lines.push(Line::from(Span::styled(
            format!("  {}", safe_plan_text(line)),
            theme.style(Token::Muted),
        )));
    }
    lines.push(Line::from(""));
    if pending.impact.needs_typed_confirmation() {
        lines.push(Line::from(Span::styled(
            format!(
                "Type the database name to continue: {}",
                safe_plan_text(&pending.required)
            ),
            theme.style(Token::Warning),
        )));
        lines.push(Line::from(vec![
            Span::styled("  ", theme.style(Token::Text)),
            Span::styled(
                safe_plan_text(&pending.typed),
                theme.style(if pending.is_satisfied() {
                    Token::Success
                } else {
                    Token::Text
                }),
            ),
            Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Enter to execute once and measure it. Esc to cancel.",
            theme.style(Token::Info),
        )));
    }
    lines.push(Line::from(Span::styled(
        "Classification is advisory: database permissions remain the real control.",
        theme.style(Token::Muted),
    )));

    Paragraph::new(lines)
        .block(overlay_block(
            " Confirm analyzed plan  Esc to cancel ".to_owned(),
            presentation,
        ))
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
}

/// Asks before a result value leaves through the terminal.
///
/// The candidate contains only location and size metadata. Looking up the
/// column name here is safe display metadata; the value itself is intentionally
/// never rendered in this overlay.
pub(crate) fn render_copy_confirmation(
    model: &Model,
    pending: &crate::app::model::PendingCopy,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(84);
    let height = 11.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let column = model
        .visible_result()
        .and_then(|set| set.columns.get(pending.column))
        .map_or_else(
            || format!("column {}", pending.column + 1),
            |name| {
                format!(
                    "column {} ({})",
                    pending.column + 1,
                    sanitize_for_display(name)
                )
            },
        );
    let target = model.connection.info().map_or_else(
        || "this terminal".to_owned(),
        |info| safe_plan_text(&info.target),
    );
    let lines = vec![
        Line::from(Span::styled(
            "Send one selected text value through the terminal?",
            theme.style(Token::Warning),
        )),
        Line::from(Span::styled(
            format!("Cell: row {}, {column}", pending.source_row + 1),
            theme.style(Token::Text),
        )),
        Line::from(Span::styled(
            format!(
                "Size: {} UTF-8 bytes, {} characters",
                pending.bytes, pending.characters
            ),
            theme.style(Token::Info),
        )),
        Line::from(Span::styled(
            format!("Path: {target} -> terminal clipboard sequence"),
            theme.style(Token::Muted),
        )),
        Line::from(Span::styled(
            "The terminal, SSH path, or multiplexer may observe or retain it.",
            theme.style(Token::Danger),
        )),
        Line::from(Span::styled(
            "Ignatius writes only; it never reads or clears the clipboard.",
            theme.style(Token::Muted),
        )),
        Line::from(Span::styled(
            "Terminal acceptance is unconfirmed: this client can only know its write and flush.",
            theme.style(Token::Muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter to send once. Esc to cancel.",
            theme.style(Token::Info),
        )),
    ];

    Paragraph::new(lines)
        .block(overlay_block(
            " Confirm copy  Esc to cancel ".to_owned(),
            presentation,
        ))
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
}

/// Collects the replacement before showing the exact generated statement.
pub(crate) fn render_update_value_prompt(
    prompt: &crate::app::model::UpdatePrompt,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(92);
    let height = 12.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let relation = format!(
        "{}.{}",
        sanitize_for_display(&prompt.relation.schema),
        sanitize_for_display(&prompt.relation.relation)
    );
    let replacement = if prompt.typed_length() == 0 {
        "(empty string)".to_owned()
    } else {
        sanitize_for_display(prompt.replacement())
    };
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "Target: {relation}.{}",
                sanitize_for_display(&prompt.source.target_column)
            ),
            theme.style(Token::Focus),
        )),
        Line::from(Span::styled(
            format!(
                "Selected result row {}, column {}",
                prompt.candidate.source_row + 1,
                prompt.candidate.result_column + 1
            ),
            theme.style(Token::Muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Replacement: ", theme.style(Token::Text)),
            Span::styled(replacement, theme.style(Token::Focus)),
            Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "The replacement is literal text. Empty text is valid; NULL is not a shorthand here.",
            theme.style(Token::Muted),
        )),
        Line::from(Span::styled(
            "Enter reviews one generated UPDATE. Nothing is sent at this step.",
            theme.style(Token::Info),
        )),
    ];

    Paragraph::new(lines)
        .block(overlay_block(
            " Edit result cell  Enter to review, Esc to cancel ".to_owned(),
            presentation,
        ))
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
}

/// Shows the literal-bound statement immediately before the one write effect.
pub(crate) fn render_update_review(
    pending: &crate::app::model::PendingUpdate,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(6).min(104);
    let height = 17.min(area.height.saturating_sub(2));
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);

    let bound = pending
        .plan
        .bound_sql()
        .map(|sql| sanitize_for_display(&sql))
        .unwrap_or_else(|_| "The generated statement could not be rendered for review.".into());
    let keys = if pending.plan.key_columns.is_empty() {
        "none".to_owned()
    } else {
        pending
            .plan
            .key_columns
            .iter()
            .map(|name| sanitize_for_display(name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let lines = vec![
        Line::from(Span::styled(
            "Review generated UPDATE",
            theme.style(Token::Warning),
        )),
        Line::from(Span::styled(
            format!(
                "Target: {}.{}; column: {}",
                sanitize_for_display(&pending.relation.schema),
                sanitize_for_display(&pending.relation.relation),
                sanitize_for_display(&pending.plan.target_column)
            ),
            theme.style(Token::Text),
        )),
        Line::from(Span::styled(
            format!("Primary-key identity: {keys}"),
            theme.style(Token::Muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Exact bound statement:",
            theme.style(Token::Header),
        )),
        Line::from(Span::styled(bound, theme.style(Token::Text))),
        Line::from(""),
        Line::from(Span::styled(
            "Nothing has been sent yet. Enter runs this UPDATE once; Esc cancels.",
            theme.style(Token::Info),
        )),
        Line::from(Span::styled(
            "The prior SELECT is a snapshot and will not be rerun automatically.",
            theme.style(Token::Muted),
        )),
    ];

    Paragraph::new(lines)
        .block(overlay_block(
            " Confirm cell update  Esc to cancel ".to_owned(),
            presentation,
        ))
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
}
/// Renders the plan surface over the Results pane without changing the retained
/// ordinary execution underneath it.
pub(crate) fn render_plan(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Results;
    let title = match &model.plan.status {
        crate::app::PlanStatus::Loading { analyzed: true, .. } => " Plan  Measuring ",
        crate::app::PlanStatus::Loading {
            analyzed: false, ..
        } => " Plan  Reading estimate ",
        crate::app::PlanStatus::Ready(document) if document.analyzed => " Plan  EXPLAIN ANALYZE ",
        crate::app::PlanStatus::Ready(_) => " Plan  EXPLAIN ",
        crate::app::PlanStatus::Failed { analyzed: true, .. } => " Plan  Analysis failed ",
        crate::app::PlanStatus::Failed {
            analyzed: false, ..
        } => " Plan  Plan failed ",
        crate::app::PlanStatus::Hidden => " Plan ",
    };
    let block = pane_block(
        format!("{}{}", presentation.icon(Icon::Info), title),
        focused,
        presentation,
    );
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    match &model.plan.status {
        crate::app::PlanStatus::Loading { analyzed, .. } => {
            let lines = if *analyzed {
                vec![
                    Line::from(Span::styled(
                        "Measuring the statement once with EXPLAIN ANALYZE.",
                        theme.style(Token::Warning),
                    )),
                    Line::from(Span::styled(
                        format!(
                            "{} cancels the request; the statement may still be running.",
                            action_key_label(keymap, &crate::app::Action::Cancel)
                        ),
                        theme.style(Token::Info),
                    )),
                ]
            } else {
                vec![
                    Line::from(Span::styled(
                        "Reading the planner estimate. The target statement is not executed.",
                        theme.style(Token::Info),
                    )),
                    Line::from(Span::styled(
                        format!(
                            "{} cancels the request.",
                            action_key_label(keymap, &crate::app::Action::Cancel)
                        ),
                        theme.style(Token::Muted),
                    )),
                ]
            };
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
        }
        crate::app::PlanStatus::Failed {
            analyzed,
            headline,
            next_action,
        } => {
            let mut lines = vec![Line::from(Span::styled(
                if *analyzed {
                    "No successful analyzed plan is available."
                } else {
                    "No successful plan is available."
                },
                theme.style(Token::Danger),
            ))];
            lines.push(Line::from(Span::styled(
                safe_plan_text(headline),
                theme.style(Token::Danger),
            )));
            lines.push(Line::from(Span::styled(
                format!("Next: {}", safe_plan_text(next_action)),
                theme.style(Token::Info),
            )));
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .render(inner, buf);
        }
        crate::app::PlanStatus::Ready(document) => {
            render_ready_plan(document, &model.plan, presentation, inner, buf);
        }
        crate::app::PlanStatus::Hidden => {}
    }
}

fn render_ready_plan(
    document: &crate::query::PlanDocument,
    view: &crate::app::PlanView,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let summary = plan_summary(document);
    if area.height < 3 {
        Paragraph::new(Line::from(Span::styled(
            truncate_to_width(
                &summary,
                area.width as usize,
                !presentation.glyphs.is_ascii(),
            ),
            presentation.theme.style(Token::Info),
        )))
        .render(area, buf);
        return;
    }

    if area.height >= 8 {
        let detail_height = 5.min(area.height.saturating_sub(3));
        let tree_height = area.height.saturating_sub(2 + detail_height);
        let [summary_area, tree_area, detail_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(tree_height),
            Constraint::Length(detail_height),
        ])
        .areas(area);
        render_plan_summary(&summary, document, presentation, summary_area, buf);
        render_plan_tree(document, view, presentation, tree_area, buf);
        render_plan_detail(document, view, presentation, detail_area, buf);
    } else {
        let [summary_area, tree_area] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        render_plan_summary(&summary, document, presentation, summary_area, buf);
        render_plan_tree(document, view, presentation, tree_area, buf);
    }
}

fn render_plan_summary(
    summary: &str,
    document: &crate::query::PlanDocument,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut lines = vec![Line::from(Span::styled(
        truncate_to_width(
            summary,
            area.width as usize,
            !presentation.glyphs.is_ascii(),
        ),
        presentation.theme.style(Token::Info),
    ))];
    let attention = document.has_attention().then(|| {
        document.root.at_path(&document.attention_path).map_or_else(
            || "unavailable".to_owned(),
            |node| safe_plan_text(&node.node_type),
        )
    });
    lines.push(Line::from(Span::styled(
        attention.map_or_else(
            || "Attention: unavailable".to_owned(),
            |node| {
                format!(
                    "Attention: {} ({})",
                    node,
                    document.attention_basis().label()
                )
            },
        ),
        presentation.theme.style(Token::Warning),
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn render_plan_tree(
    document: &crate::query::PlanDocument,
    view: &crate::app::PlanView,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let paths = view.visible_paths();
    if paths.is_empty() {
        return;
    }
    let selected = paths
        .iter()
        .position(|path| path == &view.selected_path)
        .unwrap_or(0);
    let height = usize::from(area.height);
    let start = selected
        .saturating_sub(height / 2)
        .min(paths.len().saturating_sub(height));
    let end = (start + height).min(paths.len());
    let mut lines = Vec::with_capacity(end.saturating_sub(start));
    for path in paths.iter().take(end).skip(start) {
        let Some(node) = document.root.at_path(path) else {
            continue;
        };
        let selected = path == &view.selected_path;
        let mut text = plan_node_text(
            document,
            node,
            path,
            view.collapsed.contains(path),
            selected,
            !presentation.glyphs.is_ascii(),
        );
        text = truncate_to_width(&text, area.width as usize, !presentation.glyphs.is_ascii());
        let line = Line::from(Span::styled(
            text,
            if selected {
                presentation.theme.style(Token::Selection)
            } else if document.is_attention_path(path) {
                presentation.theme.style(Token::Warning)
            } else {
                presentation.theme.style(Token::Text)
            },
        ));
        lines.push(line);
    }
    Paragraph::new(lines).render(area, buf);
}

fn render_plan_detail(
    document: &crate::query::PlanDocument,
    view: &crate::app::PlanView,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let Some(node) = view.selected_node() else {
        return;
    };
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "Selected: {}  ({}/{})",
            safe_plan_text(&node.node_type),
            view.visible_paths()
                .iter()
                .position(|path| path == &view.selected_path)
                .map_or(1, |index| index + 1),
            view.visible_paths().len()
        ),
        presentation.theme.style(Token::Header),
    ))];
    lines.push(Line::from(Span::styled(
        plan_metrics(node, document.analyzed),
        presentation.theme.style(Token::Text),
    )));
    if node.estimate_mismatch() {
        lines.push(Line::from(Span::styled(
            "Estimate mismatch: observed rows differ by at least 10x.",
            presentation.theme.style(Token::Warning),
        )));
    }
    for fact in node.facts.iter().take(2) {
        lines.push(Line::from(Span::styled(
            format!(
                "{}: {}",
                safe_plan_text(&fact.label),
                safe_plan_text(&fact.value)
            ),
            presentation.theme.style(Token::Muted),
        )));
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn plan_summary(document: &crate::query::PlanDocument) -> String {
    let mode = if document.analyzed {
        "EXPLAIN ANALYZE"
    } else {
        "EXPLAIN estimate only"
    };
    let planning = document
        .planning_time_ms
        .map_or_else(|| "not supplied".to_owned(), format_ms);
    let execution = if document.analyzed {
        document
            .execution_time_ms
            .map_or_else(|| "not supplied".to_owned(), format_ms)
    } else {
        "not measured".to_owned()
    };
    let bounded = if document.truncated {
        "; view truncated at client bound"
    } else {
        ""
    };
    format!(
        "{mode}; {} node(s); planning {planning}; execution {execution}{bounded}",
        document.node_count
    )
}

fn plan_node_text(
    document: &crate::query::PlanDocument,
    node: &crate::query::PlanNode,
    path: &[usize],
    collapsed: bool,
    selected: bool,
    unicode: bool,
) -> String {
    let marker = if selected { ">" } else { " " };
    let branch = if node.children.is_empty() {
        "  "
    } else if collapsed {
        "[-]"
    } else if unicode {
        "└─"
    } else {
        "+-"
    };
    let depth = path.len().min(10);
    let indent = if path.len() > depth {
        format!("{}... ", "  ".repeat(depth))
    } else {
        "  ".repeat(depth)
    };
    let subject = node_subject(node);
    let attention = if document.is_attention_path(path) {
        format!(" [attention: {}]", document.attention_basis().label())
    } else {
        String::new()
    };
    let mismatch = if node.estimate_mismatch() {
        " [estimate mismatch]"
    } else {
        ""
    };
    format!(
        "{marker}{indent}{branch} {} | {}{}{}",
        safe_plan_text(&subject),
        plan_metrics(node, document.analyzed),
        attention,
        mismatch
    )
}

fn node_subject(node: &crate::query::PlanNode) -> String {
    match (&node.relation, &node.index) {
        (Some(relation), Some(index)) => format!("{} {} via {}", node.node_type, relation, index),
        (Some(relation), None) => format!("{} {}", node.node_type, relation),
        (None, Some(index)) => format!("{} via {}", node.node_type, index),
        (None, None) => node.node_type.clone(),
    }
}

pub(crate) fn plan_metrics(node: &crate::query::PlanNode, analyzed: bool) -> String {
    let rows = number_or(node.plan_rows, "not supplied");
    let width = number_or(node.plan_width, "not supplied");
    let startup_cost = number_or(node.startup_cost, "not supplied");
    let total_cost = number_or(node.total_cost, "not supplied");
    let cost = format!("cost {startup_cost}..{total_cost} cost units");
    if !analyzed {
        return format!("est rows {rows}; width {width}; {cost}");
    }
    let actual_rows = number_or(node.actual_rows, "not measured");
    let startup_time = node
        .actual_startup_time
        .map_or_else(|| "not measured".to_owned(), format_ms);
    let total_time = node
        .actual_total_time
        .map_or_else(|| "not measured".to_owned(), format_ms);
    let loops = number_or(node.actual_loops, "not measured");
    format!(
        "est rows {rows}; width {width}; actual rows {actual_rows}; actual startup {startup_time}; \
         actual total {total_time}/loop; loops {loops}; {cost}"
    )
}

fn number_or(value: Option<f64>, missing: &str) -> String {
    value.map_or_else(|| missing.to_owned(), format_number)
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn format_ms(value: f64) -> String {
    format!("{value:.2} ms")
}

fn safe_plan_text(value: &str) -> String {
    sanitize_for_display(&crate::diagnostics::redact_text(value))
}
/// Shows what an object is, in SQL, coloured the way the editor colours it.
pub(crate) fn render_definition(
    open: &crate::app::model::Definition,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_area = definition_box(area);
    if box_area.width < 12 || box_area.height < 5 {
        return;
    }
    let (_, height) = definition_viewport((area.width, area.height));
    ratatui::widgets::Clear.render(box_area, buf);

    let mut lines = vec![Line::from(Span::styled(
        sanitize_for_display(&open.heading),
        theme.style(Token::Header).add_modifier(Modifier::BOLD),
    ))];

    if let Some(error) = &open.error {
        lines.push(Line::from(Span::styled(
            sanitize_for_display(error),
            theme.style(Token::Danger),
        )));
    } else if let Some(definition) = &open.definition {
        // Where the text came from is stated every time. A description of a
        // table is not a script that recreates it, and the difference matters
        // to anyone about to copy it.
        lines.push(Line::from(Span::styled(
            definition.source.note(),
            theme.style(Token::Muted),
        )));

        let text = &definition.text;
        let syntax = crate::query::highlight::tokens(text);
        let mut start = 0usize;
        for (index, source) in text.lines().enumerate() {
            let line_start = start;
            start += source.len() + 1;
            if index < open.scroll || index >= open.scroll + height {
                continue;
            }
            let mut spans = Vec::new();
            let mut run = String::new();
            let mut run_style: Option<Style> = None;
            for (byte, ch) in source.char_indices() {
                let style = theme.style(syntax_token(crate::query::highlight::kind_at(
                    &syntax,
                    line_start + byte,
                )));
                if run_style != Some(style) {
                    if let Some(previous) = run_style {
                        spans.push(Span::styled(std::mem::take(&mut run), previous));
                    }
                    run_style = Some(style);
                }
                run.push_str(&sanitize_for_display(&ch.to_string()));
            }
            if let Some(previous) = run_style {
                spans.push(Span::styled(run, previous));
            }
            lines.push(Line::from(spans));
        }

        let total = text.lines().count();
        if total > height {
            lines.push(Line::from(Span::styled(
                format!(
                    "showing {}-{} of {total} lines",
                    open.scroll + 1,
                    (open.scroll + height).min(total)
                ),
                theme.style(Token::Info),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Reading the catalogue...",
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines)
        .block(overlay_block(
            format!(
                " {}Definition  arrows scroll, Esc closes ",
                presentation.icon(Icon::Schema)
            ),
            presentation,
        ))
        .render(box_area, buf);
}
/// The popup that appears while a chord is waiting for its second key.
///
/// Nothing here is on a timer: the reducer reads no clock, and a popup that
/// vanishes on its own is a popup that vanishes while being read.
pub(crate) fn render_chords(
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let chords = crate::ui::keymap::CHORDS;
    let width = area.width.saturating_sub(8).min(60);
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

    let prefix = keymap
        .contextual_hint(&crate::app::Action::BeginPrefix)
        .map(|(key, _)| key)
        .unwrap_or_else(|| "Ctrl+K".to_owned());
    Paragraph::new(lines)
        .block(overlay_block(format!(" {prefix}  then "), presentation))
        .render(chord_area, buf);
}
pub(crate) fn render_help(
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(4).min(70);
    let rows = keymap.bindings().len() + crate::ui::keymap::CHORDS.len() + 2;
    let height = (u16::try_from(rows).unwrap_or(24) + 3).min(area.height.saturating_sub(2));
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

    // The chords belong here too. Reading help and still not knowing how to
    // reach half the interface is the failure this overlay exists to prevent.
    let prefix = keymap
        .contextual_hint(&crate::app::Action::BeginPrefix)
        .map(|(key, _)| key)
        .unwrap_or_else(|| "Ctrl+K".to_owned());
    lines.push(Line::from(Span::styled(
        format!("{prefix} then:"),
        theme.style(Token::Muted),
    )));
    for (key, _, description) in crate::ui::keymap::CHORDS {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<12}", format!("{prefix} {key}")),
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ),
            Span::styled(*description, theme.style(Token::Text)),
        ]));
    }

    Paragraph::new(lines)
        .block(overlay_block(
            format!(" {}Help  Esc to close ", presentation.icon(Icon::Help)),
            presentation,
        ))
        .style(theme.style(Token::Text))
        .render(help_area, buf);
}
