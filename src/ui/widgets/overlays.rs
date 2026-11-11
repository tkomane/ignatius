use super::{action_key_label, input_cursor, overlay_block, pane_block, syntax_token};
use crate::app::model::{Focus, Model};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{OverlayAnchor, overlay_box, prompt_box};
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
    let box_area = prompt_box(area, presentation, (70, 7));
    ratatui::widgets::Clear.render(box_area, buf);

    let lines = vec![
        Line::from(Span::styled(
            sanitize_for_display(&prompt.subject),
            theme.style(Token::Text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Name: ", theme.style(Token::Header)),
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

/// Draws precisely the rows and fixed interaction areas used for admission.
fn render_sending_review(
    model: &Model,
    kind: crate::ui::review::Kind,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let Some(view) = crate::ui::review::build(model, kind, presentation, area) else {
        return;
    };
    ratatui::widgets::Clear.render(view.outer, buf);
    overlay_block(format!(" {} ", view.title), presentation).render(view.outer, buf);
    if !view.usable {
        if view.outer.width > 2 && view.outer.height > 2 {
            let inner = Rect::new(
                view.outer.x.saturating_add(1),
                view.outer.y.saturating_add(1),
                view.outer.width.saturating_sub(2),
                view.outer.height.saturating_sub(2),
            );
            Paragraph::new(
                view.refusal
                    .iter()
                    .map(|line| Line::from(line.as_str()))
                    .collect::<Vec<_>>(),
            )
            .style(presentation.theme.style(Token::Warning))
            .render(inner, buf);
        }
        return;
    }
    let body = view
        .window()
        .iter()
        .map(|row| {
            Line::from(Span::styled(
                row.text.as_str(),
                presentation.theme.style(row.token),
            ))
        })
        .collect::<Vec<_>>();
    Paragraph::new(body).render(view.body, buf);
    if let (Some(input), Some(input_area)) = (&view.input, view.input_area) {
        Paragraph::new(input.as_str())
            .style(presentation.theme.style(Token::Focus))
            .render(input_area, buf);
    }
    Paragraph::new(
        view.rail
            .iter()
            .map(|line| Line::from(line.as_str()))
            .collect::<Vec<_>>(),
    )
    .style(presentation.theme.style(if view.ready {
        Token::Info
    } else {
        Token::Warning
    }))
    .render(view.rail_area, buf);
}

/// Asks for one named value without changing the SQL editor underneath.
pub(crate) fn render_parameter_prompt(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_sending_review(
        model,
        crate::ui::review::Kind::Parameter,
        presentation,
        area,
        buf,
    );
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
    let box_area = prompt_box(area, presentation, (72, 9));
    ratatui::widgets::Clear.render(box_area, buf);
    let full_title = format!(
        " {}Password  Enter to try again, Esc to cancel ",
        presentation.icon(Icon::Role)
    );
    let title_width = display_width(&full_title).saturating_sub(2)
        + usize::from(presentation.spacing().title_padding) * 2;

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
            if title_width > usize::from(box_area.width.saturating_sub(2)) {
                " Password Enter try, Esc cancel ".to_owned()
            } else {
                full_title
            },
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
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_sending_review(
        model,
        crate::ui::review::Kind::Production,
        presentation,
        area,
        buf,
    );
}

/// Explains the one-shot side effect before EXPLAIN ANALYZE is sent.
pub(crate) fn render_plan_confirmation(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_sending_review(
        model,
        crate::ui::review::Kind::Analyze,
        presentation,
        area,
        buf,
    );
}

/// Asks before a result value leaves through the terminal.
///
/// The candidate contains only location and size metadata. Looking up the
/// column name here is safe display metadata; the value itself is intentionally
/// never rendered in this overlay.
pub(crate) fn render_copy_confirmation(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_sending_review(
        model,
        crate::ui::review::Kind::Copy,
        presentation,
        area,
        buf,
    );
}

/// Collects the replacement before showing the exact generated statement.
pub(crate) fn render_update_value_prompt(
    prompt: &crate::app::model::UpdatePrompt,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_area = prompt_box(area, presentation, (92, 12));
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
        Line::from(vec![
            Span::styled("Target: ", theme.style(Token::Header)),
            Span::styled(
                format!(
                    "{relation}.{}",
                    sanitize_for_display(&prompt.source.target_column)
                ),
                theme.style(Token::Focus),
            ),
        ]),
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
            Span::styled("Replacement: ", theme.style(Token::Header)),
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
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_sending_review(
        model,
        crate::ui::review::Kind::Update,
        presentation,
        area,
        buf,
    );
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
        format!(
            "{}{}{}",
            if model.splits.zoomed && focused {
                "(zoomed) "
            } else {
                ""
            },
            presentation.icon(Icon::Info),
            title
        ),
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

    let (summary_area, tree_area, detail_area) = plan_areas(area);
    render_plan_summary(&summary, document, presentation, summary_area, buf);
    render_plan_tree(document, view, presentation, tree_area, buf);
    if let Some(detail) = detail_area {
        render_plan_detail(document, view, presentation, detail, buf);
    }
}

/// Plan sections, shared with keyboard paging through the tree.
fn plan_areas(area: Rect) -> (Rect, Rect, Option<Rect>) {
    if area.height < 3 {
        return (area, Rect::default(), None);
    }
    if area.height >= 8 {
        let detail_height = 5.min(area.height.saturating_sub(3));
        let tree_height = area.height.saturating_sub(2 + detail_height);
        let [summary, tree, detail] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(tree_height),
            Constraint::Length(detail_height),
        ])
        .areas(area);
        (summary, tree, Some(detail))
    } else {
        let [summary, tree] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        (summary, tree, None)
    }
}

pub(crate) fn plan_capacity(pane: Rect) -> usize {
    let inner = super::inner_rect(pane);
    if inner.width == 0 {
        0
    } else {
        usize::from(plan_areas(inner).1.height)
    }
}

fn render_plan_summary(
    summary: &str,
    document: &crate::query::PlanDocument,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let truncated = truncate_to_width(
        summary,
        area.width as usize,
        !presentation.glyphs.is_ascii(),
    );
    let mut first_line_spans = Vec::new();
    let mut parts = truncated.split("; ");
    if let Some(first) = parts.next() {
        first_line_spans.push(Span::styled(
            first.to_owned(),
            presentation.theme.style(Token::Header),
        ));
        for segment in parts {
            first_line_spans.push(Span::styled("; ", presentation.theme.style(Token::Border)));
            first_line_spans.push(Span::styled(
                segment.to_owned(),
                presentation.theme.style(Token::Info),
            ));
        }
    }
    let mut lines = vec![Line::from(first_line_spans)];
    let attention = document.has_attention().then(|| {
        document.root.at_path(&document.attention_path).map_or_else(
            || "unavailable".to_owned(),
            |node| safe_plan_text(&node.node_type),
        )
    });
    let attention_detail = attention.map_or_else(
        || "unavailable".to_owned(),
        |node| format!("{} ({})", node, document.attention_basis().label()),
    );
    let warning_style = presentation.theme.style(Token::Warning);
    lines.push(Line::from(vec![
        Span::styled("Attention: ", warning_style.add_modifier(Modifier::BOLD)),
        Span::styled(attention_detail, warning_style),
    ]));
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
        let line = if selected {
            let padded = pad_to_width(&text, area.width as usize);
            Line::from(Span::styled(
                padded,
                presentation.theme.style(Token::Selection),
            ))
        } else if document.is_attention_path(path) {
            Line::from(Span::styled(text, presentation.theme.style(Token::Warning)))
        } else if let Some((left, right)) = text.split_once(" | ") {
            Line::from(vec![
                Span::styled(left.to_owned(), presentation.theme.style(Token::Header)),
                Span::styled(" | ", presentation.theme.style(Token::Border)),
                Span::styled(
                    right.to_owned(),
                    presentation.theme.style(if node.estimate_mismatch() {
                        Token::Warning
                    } else {
                        Token::Muted
                    }),
                ),
            ])
        } else {
            Line::from(Span::styled(text, presentation.theme.style(Token::Header)))
        };
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
    let mut lines = vec![Line::from(vec![
        Span::styled("Selected: ", presentation.theme.style(Token::Info)),
        Span::styled(
            format!(
                "{}  ({}/{})",
                safe_plan_text(&node.node_type),
                view.visible_paths()
                    .iter()
                    .position(|path| path == &view.selected_path)
                    .map_or(1, |index| index + 1),
                view.visible_paths().len()
            ),
            presentation.theme.style(Token::Header),
        ),
    ])];
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
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", safe_plan_text(&fact.label)),
                presentation.theme.style(Token::Info),
            ),
            Span::styled(
                safe_plan_text(&fact.value),
                presentation.theme.style(Token::Muted),
            ),
        ]));
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
    let box_area = definition_box(area, presentation);
    if box_area.width < 12 || box_area.height < 5 {
        return;
    }
    let (_, height) = definition_viewport((area.width, area.height), presentation);
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

        let text = definition.full_script(open.include_drop);
        let syntax = crate::query::highlight::tokens(&text);
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
                " {}Definition  [e/Enter] Load in Editor  [d] Toggle DROP  arrows scroll, Esc closes ",
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
    let single_col_height = u16::try_from(chords.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);

    let use_two_columns = single_col_height > area.height && area.width >= 90;
    if use_two_columns {
        let split_at = chords.len().div_ceil(2);
        let col1 = &chords[..split_at];
        let col2 = &chords[split_at..];
        let height = u16::try_from(split_at)
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let max_width = 110u16.min(area.width.saturating_sub(2));
        let chord_area = overlay_box(
            area,
            presentation,
            (max_width, height),
            height.min(area.height),
            OverlayAnchor::Bottom,
        );
        ratatui::widgets::Clear.render(chord_area, buf);

        let col_width = usize::from(chord_area.width.saturating_sub(4) / 2);
        let mut lines = Vec::new();
        for i in 0..split_at {
            let mut spans = Vec::new();
            if let Some((key, _, description)) = col1.get(i) {
                spans.push(Span::styled(
                    format!(" {key}  "),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ));
                let pad = col_width.saturating_sub(4);
                let text = format!("{:<pad$}", description);
                spans.push(Span::styled(text, theme.style(Token::Text)));
            }
            if let Some((key, _, description)) = col2.get(i) {
                spans.push(Span::styled(
                    format!(" {key}  "),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(*description, theme.style(Token::Text)));
            }
            lines.push(Line::from(spans));
        }

        let prefix = keymap
            .contextual_hint(&crate::app::Action::BeginPrefix)
            .map(|(key, _)| key)
            .unwrap_or_else(|| "Ctrl+K".to_owned());
        Paragraph::new(lines)
            .block(overlay_block(format!(" {prefix}  then "), presentation))
            .render(chord_area, buf);
    } else {
        let chord_area = overlay_box(
            area,
            presentation,
            (60, single_col_height),
            single_col_height.min(area.height),
            OverlayAnchor::Bottom,
        );
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
}
pub(crate) fn render_help(
    model: &crate::app::Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let view = crate::ui::help::build(model, presentation, area);
    ratatui::widgets::Clear.render(view.outer, buf);
    overlay_block(
        format!(" {}Help ", presentation.icon(Icon::Help)),
        presentation,
    )
    .render(view.outer, buf);
    Paragraph::new(view.window().to_vec())
        .style(presentation.theme.style(Token::Text))
        .render(view.body, buf);
    Paragraph::new(
        view.rail
            .iter()
            .cloned()
            .map(Line::from)
            .collect::<Vec<_>>(),
    )
    .style(presentation.theme.style(Token::Focus))
    .render(view.rail_area, buf);
}

/// Shows an interactive schema entity-relationship (ER) diagram overlay.
pub(crate) fn render_schema_diagram(
    diagram: &crate::app::model::SchemaDiagramState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_width = (area.width.saturating_mul(86) / 100)
        .max(40)
        .min(area.width);
    let box_height = (area.height.saturating_mul(82) / 100)
        .max(12)
        .min(area.height);
    let x = area.x + (area.width.saturating_sub(box_width)) / 2;
    let y = area.y + (area.height.saturating_sub(box_height)) / 2;
    let box_area = Rect::new(x, y, box_width, box_height);
    if box_area.width < 20 || box_area.height < 6 {
        return;
    }
    ratatui::widgets::Clear.render(box_area, buf);

    let title = if diagram.loading {
        format!(
            " {}Schema ER Diagram (reading catalog...) ",
            presentation.icon(Icon::Database)
        )
    } else {
        format!(
            " {}Schema ER Diagram ({} table{}, {} - {}) ",
            presentation.icon(Icon::Database),
            diagram.graph.tables.len(),
            if diagram.graph.tables.len() == 1 {
                ""
            } else {
                "s"
            },
            diagram.depth.label(),
            diagram.format.label(),
        )
    };
    overlay_block(title, presentation).render(box_area, buf);

    let inner = Rect::new(
        box_area.x.saturating_add(1),
        box_area.y.saturating_add(1),
        box_area.width.saturating_sub(2),
        box_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let body_height = inner.height.saturating_sub(1);
    let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);
    let footer_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);

    let left_width = (body_area.width.saturating_mul(28) / 100)
        .max(16)
        .min(body_area.width.saturating_sub(4));
    let right_width = body_area.width.saturating_sub(left_width).saturating_sub(1);
    let left_area = Rect::new(body_area.x, body_area.y, left_width, body_area.height);
    let right_area = Rect::new(
        body_area.x.saturating_add(left_width).saturating_add(1),
        body_area.y,
        right_width,
        body_area.height,
    );

    let visible_rows = usize::from(left_area.height);
    let scroll_offset = if diagram.selected_table >= visible_rows {
        diagram
            .selected_table
            .saturating_sub(visible_rows.saturating_sub(1))
    } else {
        0
    };

    let mut table_lines = Vec::new();
    for (idx, table) in diagram
        .graph
        .tables
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
    {
        let is_selected = idx == diagram.selected_table;
        let prefix = if is_selected { "> " } else { "  " };
        let name = table.qualified_name();
        let display_name = format!("{prefix}{name}");
        let styled_line = if is_selected {
            Line::from(Span::styled(
                display_name,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::from(Span::styled(display_name, theme.style(Token::Text)))
        };
        table_lines.push(styled_line);
    }
    if table_lines.is_empty() {
        table_lines.push(Line::from(Span::styled(
            "No tables",
            theme.style(Token::Muted),
        )));
    }
    Paragraph::new(table_lines).render(left_area, buf);

    let divider_x = body_area.x.saturating_add(left_width);
    let border_sym = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "│"
    };
    for row in body_area.y..body_area.bottom() {
        if let Some(cell) = buf.cell_mut((divider_x, row)) {
            cell.set_symbol(border_sym);
            cell.set_style(theme.style(Token::Border));
        }
    }

    let right_visible_rows = usize::from(right_area.height);
    let mut diagram_lines = Vec::new();
    if let Some(error) = &diagram.error {
        diagram_lines.push(Line::from(Span::styled(
            format!("Error reading schema: {error}"),
            theme.style(Token::Danger),
        )));
    } else if diagram.loading {
        diagram_lines.push(Line::from(Span::styled(
            "Loading schema graph from pg_catalog...",
            theme.style(Token::Muted),
        )));
    } else {
        for line in diagram
            .rendered_lines
            .iter()
            .skip(diagram.scroll_y)
            .take(right_visible_rows)
        {
            let scrolled_line: String = line.chars().skip(diagram.scroll_x).collect();
            diagram_lines.push(Line::from(Span::styled(
                scrolled_line,
                theme.style(Token::Text),
            )));
        }
    }
    Paragraph::new(diagram_lines).render(right_area, buf);

    let footer_hints = vec![
        Span::styled(
            " [↑/↓] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Table  ", theme.style(Token::Muted)),
        Span::styled(
            "[1] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("1-hop  ", theme.style(Token::Muted)),
        Span::styled(
            "[2] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("2-hop  ", theme.style(Token::Muted)),
        Span::styled(
            "[a] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("All  ", theme.style(Token::Muted)),
        Span::styled(
            "[m] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Format  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_hints)).render(footer_area, buf);
}

/// Shows an interactive server activity and lock contention dashboard overlay.
pub(crate) fn render_server_activity(
    activity: &crate::app::model::ServerActivityState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_width = (area.width.saturating_mul(86) / 100)
        .max(40)
        .min(area.width);
    let box_height = (area.height.saturating_mul(82) / 100)
        .max(12)
        .min(area.height);
    let x = area.x + (area.width.saturating_sub(box_width)) / 2;
    let y = area.y + (area.height.saturating_sub(box_height)) / 2;
    let box_area = Rect::new(x, y, box_width, box_height);
    if box_area.width < 20 || box_area.height < 6 {
        return;
    }
    ratatui::widgets::Clear.render(box_area, buf);

    let title = if activity.loading {
        format!(
            " {}Server Activity & Locks (reading telemetry...) ",
            presentation.icon(Icon::Schema)
        )
    } else {
        format!(
            " {}Server Activity & Locks ({}) - {} backends ",
            presentation.icon(Icon::Schema),
            activity.mode.label(),
            activity.snapshot.backends.len()
        )
    };
    overlay_block(title, presentation).render(box_area, buf);

    let inner = Rect::new(
        box_area.x.saturating_add(1),
        box_area.y.saturating_add(1),
        box_area.width.saturating_sub(2),
        box_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let body_height = inner.height.saturating_sub(1);
    let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);
    let footer_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);

    let left_width = 28u16.min(body_area.width.saturating_sub(4)).max(16);
    let right_width = body_area.width.saturating_sub(left_width).saturating_sub(1);
    let left_area = Rect::new(body_area.x, body_area.y, left_width, body_area.height);
    let right_area = Rect::new(
        body_area.x.saturating_add(left_width).saturating_add(1),
        body_area.y,
        right_width,
        body_area.height,
    );

    let visible_rows = usize::from(left_area.height);
    let scroll_offset = if activity.selected_backend >= visible_rows {
        activity
            .selected_backend
            .saturating_sub(visible_rows.saturating_sub(1))
    } else {
        0
    };

    let mut backend_lines = Vec::new();
    for (idx, b) in activity
        .snapshot
        .backends
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_rows)
    {
        let is_selected = idx == activity.selected_backend;
        let prefix = if is_selected { "> " } else { "  " };
        let blocked_tag = if !b.blocked_by.is_empty() {
            " [BLOCKED]"
        } else {
            ""
        };
        let label = format!("{prefix}PID {} ({}){blocked_tag}", b.pid, b.username);
        let styled_line = if is_selected {
            Line::from(Span::styled(
                label,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ))
        } else if !b.blocked_by.is_empty() {
            Line::from(Span::styled(label, theme.style(Token::Danger)))
        } else {
            Line::from(Span::styled(label, theme.style(Token::Text)))
        };
        backend_lines.push(styled_line);
    }
    if backend_lines.is_empty() {
        backend_lines.push(Line::from(Span::styled(
            "No backends",
            theme.style(Token::Muted),
        )));
    }
    Paragraph::new(backend_lines).render(left_area, buf);

    let divider_x = body_area.x.saturating_add(left_width);
    let border_sym = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "│"
    };
    for row in body_area.y..body_area.bottom() {
        if let Some(cell) = buf.cell_mut((divider_x, row)) {
            cell.set_symbol(border_sym);
            cell.set_style(theme.style(Token::Border));
        }
    }

    let right_visible_rows = usize::from(right_area.height);
    let mut activity_lines = Vec::new();
    if let Some(error) = &activity.error {
        activity_lines.push(Line::from(Span::styled(
            format!("Error reading activity: {error}"),
            theme.style(Token::Danger),
        )));
    } else if activity.loading {
        activity_lines.push(Line::from(Span::styled(
            "Loading server activity and locks from server...",
            theme.style(Token::Muted),
        )));
    } else {
        for line in activity
            .rendered_lines
            .iter()
            .skip(activity.scroll_y)
            .take(right_visible_rows)
        {
            let scrolled_line: String = line.chars().skip(activity.scroll_x).collect();
            activity_lines.push(Line::from(Span::styled(
                sanitize_for_display(&scrolled_line),
                theme.style(Token::Text),
            )));
        }
    }
    Paragraph::new(activity_lines).render(right_area, buf);

    let footer_hints = vec![
        Span::styled(
            " [↑/↓] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Backend  ", theme.style(Token::Muted)),
        Span::styled(
            "[l] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Toggle Locks Tree  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[c/Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Draft Cancel SQL  ", theme.style(Token::Muted)),
        Span::styled(
            "[x] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Draft Terminate SQL  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_hints)).render(footer_area, buf);
}

/// Shows an interactive roles, privilege grants, and Row-Level Security (RLS) auditor overlay.
pub(crate) fn render_security_audit(
    audit: &crate::app::model::SecurityAuditState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_width = (area.width.saturating_mul(86) / 100)
        .max(40)
        .min(area.width);
    let box_height = (area.height.saturating_mul(82) / 100)
        .max(12)
        .min(area.height);
    let x = area.x + (area.width.saturating_sub(box_width)) / 2;
    let y = area.y + (area.height.saturating_sub(box_height)) / 2;
    let box_area = Rect::new(x, y, box_width, box_height);
    if box_area.width < 20 || box_area.height < 6 {
        return;
    }
    ratatui::widgets::Clear.render(box_area, buf);

    let title = if audit.loading {
        format!(
            " {}Security & RLS Auditor (reading catalog...) ",
            presentation.icon(Icon::Schema)
        )
    } else {
        format!(
            " {}Security & RLS Auditor ({}) ",
            presentation.icon(Icon::Schema),
            audit.section.label()
        )
    };
    overlay_block(title, presentation).render(box_area, buf);

    let inner = Rect::new(
        box_area.x.saturating_add(1),
        box_area.y.saturating_add(1),
        box_area.width.saturating_sub(2),
        box_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let body_height = inner.height.saturating_sub(1);
    let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);
    let footer_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);

    let left_width = 28u16.min(body_area.width.saturating_sub(4)).max(16);
    let right_width = body_area.width.saturating_sub(left_width).saturating_sub(1);
    let left_area = Rect::new(body_area.x, body_area.y, left_width, body_area.height);
    let right_area = Rect::new(
        body_area.x.saturating_add(left_width).saturating_add(1),
        body_area.y,
        right_width,
        body_area.height,
    );

    let visible_rows = usize::from(left_area.height);
    let scroll_offset = if audit.selected_index >= visible_rows {
        audit
            .selected_index
            .saturating_sub(visible_rows.saturating_sub(1))
    } else {
        0
    };

    let mut item_lines = Vec::new();
    match audit.section {
        crate::audit::AuditSection::Roles => {
            for (idx, r) in audit
                .snapshot
                .roles
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(visible_rows)
            {
                let is_selected = idx == audit.selected_index;
                let prefix = if is_selected { "> " } else { "  " };
                let super_tag = if r.is_superuser { " [SUPER]" } else { "" };
                let label = format!("{prefix}{}{super_tag}", r.name);
                let styled_line = if is_selected {
                    Line::from(Span::styled(
                        label,
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ))
                } else if r.is_superuser {
                    Line::from(Span::styled(label, theme.style(Token::Danger)))
                } else {
                    Line::from(Span::styled(label, theme.style(Token::Text)))
                };
                item_lines.push(styled_line);
            }
        }
        crate::audit::AuditSection::RlsPolicies => {
            for (idx, tbl) in audit
                .snapshot
                .tables
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(visible_rows)
            {
                let is_selected = idx == audit.selected_index;
                let prefix = if is_selected { "> " } else { "  " };
                let badge = if tbl.rls_enabled {
                    " [RLS ON]"
                } else {
                    " [RLS OFF]"
                };
                let label = format!("{prefix}{}{badge}", tbl.table);
                let styled_line = if is_selected {
                    Line::from(Span::styled(
                        label,
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ))
                } else if !tbl.rls_enabled {
                    Line::from(Span::styled(label, theme.style(Token::Warning)))
                } else {
                    Line::from(Span::styled(label, theme.style(Token::Text)))
                };
                item_lines.push(styled_line);
            }
        }
        crate::audit::AuditSection::Grants => {
            for (idx, g) in audit
                .snapshot
                .grants
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(visible_rows)
            {
                let is_selected = idx == audit.selected_index;
                let prefix = if is_selected { "> " } else { "  " };
                let label = format!("{prefix}{} -> {}", g.table, g.grantee);
                let styled_line = if is_selected {
                    Line::from(Span::styled(
                        label,
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(label, theme.style(Token::Text)))
                };
                item_lines.push(styled_line);
            }
        }
    }
    if item_lines.is_empty() {
        item_lines.push(Line::from(Span::styled(
            "No items",
            theme.style(Token::Muted),
        )));
    }
    Paragraph::new(item_lines).render(left_area, buf);

    let divider_x = body_area.x.saturating_add(left_width);
    let border_sym = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "│"
    };
    for row in body_area.y..body_area.bottom() {
        if let Some(cell) = buf.cell_mut((divider_x, row)) {
            cell.set_symbol(border_sym);
            cell.set_style(theme.style(Token::Border));
        }
    }

    let right_visible_rows = usize::from(right_area.height);
    let mut report_lines = Vec::new();
    if let Some(error) = &audit.error {
        report_lines.push(Line::from(Span::styled(
            format!("Error reading security audit: {error}"),
            theme.style(Token::Danger),
        )));
    } else if audit.loading {
        report_lines.push(Line::from(Span::styled(
            "Loading security audit data from server...",
            theme.style(Token::Muted),
        )));
    } else {
        for line in audit
            .rendered_lines
            .iter()
            .skip(audit.scroll_y)
            .take(right_visible_rows)
        {
            let scrolled_line: String = line.chars().skip(audit.scroll_x).collect();
            report_lines.push(Line::from(Span::styled(
                sanitize_for_display(&scrolled_line),
                theme.style(Token::Text),
            )));
        }
    }
    Paragraph::new(report_lines).render(right_area, buf);

    let footer_hints = vec![
        Span::styled(
            " [↑/↓] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", theme.style(Token::Muted)),
        Span::styled(
            "[s/1/2/3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Section (Roles/RLS/Grants)  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[e/Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Draft Reviewed SQL  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_hints)).render(footer_area, buf);
}

/// Shows an interactive Schema Diff, Table Designer, and Migration Generator overlay (`030-visual-schema-designer-and-migrations`).
pub(crate) fn render_schema_diff(
    state: &crate::app::model::SchemaDiffState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_width = (area.width.saturating_mul(88) / 100)
        .max(40)
        .min(area.width);
    let box_height = (area.height.saturating_mul(84) / 100)
        .max(12)
        .min(area.height);
    let x = area.x + (area.width.saturating_sub(box_width)) / 2;
    let y = area.y + (area.height.saturating_sub(box_height)) / 2;
    let box_area = Rect::new(x, y, box_width, box_height);
    if box_area.width < 20 || box_area.height < 6 {
        return;
    }
    ratatui::widgets::Clear.render(box_area, buf);

    let title = if state.loading {
        format!(
            " {}Schema Diff & Migrations (reading metadata...) ",
            presentation.icon(Icon::Schema)
        )
    } else {
        format!(
            " {}Schema Diff & Migrations [{}] ",
            presentation.icon(Icon::Schema),
            state.active_tab.label()
        )
    };
    overlay_block(title, presentation).render(box_area, buf);

    let inner = Rect::new(
        box_area.x.saturating_add(1),
        box_area.y.saturating_add(1),
        box_area.width.saturating_sub(2),
        box_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 3 {
        return;
    }

    let tab_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let body_area = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(2),
    );
    let footer_area = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);

    // Render tab bar
    let tab_spans = vec![
        Span::styled(
            if state.active_tab == crate::app::model::SchemaDiffTab::DiffTree {
                " [1] Schema Diff "
            } else {
                "  1  Schema Diff "
            },
            if state.active_tab == crate::app::model::SchemaDiffTab::DiffTree {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            if state.active_tab == crate::app::model::SchemaDiffTab::TableDesigner {
                " [2] Table Designer "
            } else {
                "  2  Table Designer "
            },
            if state.active_tab == crate::app::model::SchemaDiffTab::TableDesigner {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            if state.active_tab == crate::app::model::SchemaDiffTab::MigrationSql {
                " [3] Migration SQL "
            } else {
                "  3  Migration SQL "
            },
            if state.active_tab == crate::app::model::SchemaDiffTab::MigrationSql {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
    ];
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    let visible_rows = usize::from(body_area.height);

    match state.active_tab {
        crate::app::model::SchemaDiffTab::DiffTree => {
            let mut lines = Vec::new();
            if let Some(error) = &state.error {
                lines.push(Line::from(Span::styled(
                    format!("Error reading schema diff: {error}"),
                    theme.style(Token::Danger),
                )));
            } else if state.loading {
                lines.push(Line::from(Span::styled(
                    "Comparing schemas and analyzing catalog objects...",
                    theme.style(Token::Muted),
                )));
            } else {
                for raw_line in state
                    .tree_lines
                    .iter()
                    .skip(state.scroll_y)
                    .take(visible_rows)
                {
                    let scrolled: String = raw_line.chars().skip(state.scroll_x).collect();
                    let trimmed = scrolled.trim_start();
                    let style = if trimmed.starts_with('+') {
                        theme.style(Token::Success)
                    } else if trimmed.starts_with('~') {
                        theme.style(Token::Warning)
                    } else if trimmed.starts_with('-') {
                        theme.style(Token::Danger)
                    } else {
                        theme.style(Token::Text)
                    };
                    lines.push(Line::from(Span::styled(
                        sanitize_for_display(&scrolled),
                        style,
                    )));
                }
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::SchemaDiffTab::TableDesigner => {
            let left_width = 32u16.min(body_area.width.saturating_sub(6)).max(18);
            let right_width = body_area.width.saturating_sub(left_width).saturating_sub(1);
            let left_area = Rect::new(body_area.x, body_area.y, left_width, body_area.height);
            let right_area = Rect::new(
                body_area.x.saturating_add(left_width).saturating_add(1),
                body_area.y,
                right_width,
                body_area.height,
            );

            let mut col_lines = Vec::new();
            col_lines.push(Line::from(Span::styled(
                format!("Table: {}", state.blueprint.table_name),
                theme.style(Token::Header).add_modifier(Modifier::BOLD),
            )));
            col_lines.push(Line::from(""));
            for (idx, col) in state.blueprint.columns.iter().enumerate() {
                let is_focused = idx == state.blueprint.focused_column;
                let prefix = if is_focused { "> " } else { "  " };
                let pk_tag = if col.is_pk { " [PK]" } else { "" };
                let not_null_tag = if col.not_null { " NOT NULL" } else { "" };
                let label = format!(
                    "{prefix}{}: {}{pk_tag}{not_null_tag}",
                    col.name, col.data_type
                );
                let style = if is_focused {
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                } else {
                    theme.style(Token::Text)
                };
                col_lines.push(Line::from(Span::styled(label, style)));
            }
            Paragraph::new(col_lines).render(left_area, buf);

            // Divider
            let divider_x = body_area.x.saturating_add(left_width);
            let border_sym = if presentation.glyphs.is_ascii() {
                "|"
            } else {
                "│"
            };
            for row in body_area.y..body_area.bottom() {
                if let Some(cell) = buf.cell_mut((divider_x, row)) {
                    cell.set_symbol(border_sym);
                    cell.set_style(theme.style(Token::Border));
                }
            }

            // Right side: live DDL preview
            let ddl = state.blueprint.to_create_table_sql();
            let mut preview_lines = Vec::new();
            preview_lines.push(Line::from(Span::styled(
                "Live DDL Preview:",
                theme.style(Token::Header).add_modifier(Modifier::BOLD),
            )));
            for line in ddl.lines().take(visible_rows.saturating_sub(1)) {
                preview_lines.push(Line::from(Span::styled(
                    sanitize_for_display(line),
                    theme.style(Token::SyntaxKeyword),
                )));
            }
            Paragraph::new(preview_lines).render(right_area, buf);
        }
        crate::app::model::SchemaDiffTab::MigrationSql => {
            let mut sql_lines = Vec::new();
            for raw_line in state
                .migration_sql
                .lines()
                .skip(state.scroll_y)
                .take(visible_rows)
            {
                let scrolled: String = raw_line.chars().skip(state.scroll_x).collect();
                let style = if scrolled.starts_with("--") {
                    theme.style(Token::Muted)
                } else if scrolled.starts_with("BEGIN")
                    || scrolled.starts_with("COMMIT")
                    || scrolled.starts_with("CREATE")
                    || scrolled.starts_with("ALTER")
                    || scrolled.starts_with("DROP")
                {
                    theme
                        .style(Token::SyntaxKeyword)
                        .add_modifier(Modifier::BOLD)
                } else {
                    theme.style(Token::Text)
                };
                sql_lines.push(Line::from(Span::styled(
                    sanitize_for_display(&scrolled),
                    style,
                )));
            }
            Paragraph::new(sql_lines).render(body_area, buf);
        }
    }

    let footer_hints = vec![
        Span::styled(
            " [Tab/1/2/3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tab  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[e/Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Load in Editor  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_hints)).render(footer_area, buf);
}

#[cfg(test)]
mod tests;
