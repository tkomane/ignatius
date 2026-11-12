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
        let max_width = 116u16.min(area.width.saturating_sub(2));
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

/// Renders the interactive Bulk Data Import Wizard overlay (`028-data-import-export-and-backup`).
pub(crate) fn render_import_wizard(
    state: &crate::app::model::ImportWizardState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = area
        .width
        .saturating_mul(85)
        .checked_div(100)
        .unwrap_or(area.width)
        .max(64)
        .min(area.width);
    let modal_height = area
        .height
        .saturating_mul(85)
        .checked_div(100)
        .unwrap_or(area.height)
        .max(18)
        .min(area.height);

    let modal_x = area
        .x
        .saturating_add(area.width.saturating_sub(modal_width) / 2);
    let modal_y = area
        .y
        .saturating_add(area.height.saturating_sub(modal_height) / 2);
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    ratatui::widgets::Clear.render(modal_area, buf);

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_type(if presentation.glyphs.is_ascii() {
            ratatui::widgets::BorderType::Plain
        } else {
            ratatui::widgets::BorderType::Rounded
        })
        .border_style(theme.style(Token::Focus))
        .title(format!(
            " Import & Bulk Data Transfer Wizard: \"{}\".\"{}\" ",
            state.schema, state.table
        ));
    let inner = block.inner(modal_area);
    block.render(modal_area, buf);

    if inner.height < 6 || inner.width < 30 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Step pills
            Constraint::Length(1), // Spacer
            Constraint::Min(4),    // Body
            Constraint::Length(1), // Footer hints
        ])
        .split(inner);

    let step_area = chunks[0];
    let body_area = chunks[2];
    let footer_area = chunks[3];

    let step_spans = vec![
        Span::styled(" Step: ", theme.style(Token::Muted)),
        Span::styled(
            if state.step == crate::app::model::ImportWizardStep::SourceFile {
                " [1] Source File "
            } else {
                "  1  Source File "
            },
            if state.step == crate::app::model::ImportWizardStep::SourceFile {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(
            if state.step == crate::app::model::ImportWizardStep::ColumnMapping {
                " [2] Column Mapping "
            } else {
                "  2  Column Mapping "
            },
            if state.step == crate::app::model::ImportWizardStep::ColumnMapping {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(
            if state.step == crate::app::model::ImportWizardStep::Execute {
                " [3] Review & Run "
            } else {
                "  3  Review & Run "
            },
            if state.step == crate::app::model::ImportWizardStep::Execute {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
    ];
    Paragraph::new(Line::from(step_spans)).render(step_area, buf);

    let visible_rows = usize::from(body_area.height);

    match state.step {
        crate::app::model::ImportWizardStep::SourceFile => {
            let mut lines = Vec::new();
            lines.push(Line::from(vec![
                Span::styled("File path: ", theme.style(Token::Muted)),
                Span::styled(
                    &state.file_path,
                    theme.style(Token::Text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  (Format: {}, Delimiter: '{}')",
                        state.format, state.delimiter as char
                    ),
                    theme.style(Token::Muted),
                ),
            ]));
            lines.push(Line::from(""));

            if let Some(ref err) = state.error {
                lines.push(Line::from(Span::styled(
                    format!("Error: {err}"),
                    theme.style(Token::Danger),
                )));
            } else if state.loading {
                lines.push(Line::from(Span::styled(
                    "Analyzing source file structure and sample data...",
                    theme.style(Token::Muted),
                )));
            } else if let Some(ref prev) = state.preview {
                lines.push(Line::from(vec![
                    Span::styled("Detected Headers: ", theme.style(Token::Muted)),
                    Span::styled(prev.headers.join(", "), theme.style(Token::Focus)),
                ]));
                lines.push(Line::from(Span::styled(
                    format!("Sample rows (showing up to {}):", prev.sample_rows.len()),
                    theme.style(Token::Muted),
                )));
                for (row_idx, row) in prev
                    .sample_rows
                    .iter()
                    .enumerate()
                    .take(visible_rows.saturating_sub(5))
                {
                    let preview_str = row.join(" | ");
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {:2}: ", row_idx + 1), theme.style(Token::Muted)),
                        Span::styled(preview_str, theme.style(Token::Text)),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "No file preview available. Press [r] to analyze file.",
                    theme.style(Token::Muted),
                )));
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::ImportWizardStep::ColumnMapping => {
            let mut lines = Vec::new();
            lines.push(Line::from(vec![
                Span::styled("   ", theme.style(Token::Muted)),
                Span::styled(
                    format!("{:<20}", "Source Column"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", theme.style(Token::Muted)),
                Span::styled(
                    format!("{:<20}", "Target Column"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", theme.style(Token::Muted)),
                Span::styled(
                    format!("{:<14}", "Inferred Type"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", theme.style(Token::Muted)),
                Span::styled(
                    "Skip",
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
            ]));

            if let Some(ref prev) = state.preview {
                for (idx, mapping) in prev
                    .column_mappings
                    .iter()
                    .enumerate()
                    .skip(state.scroll_y)
                    .take(visible_rows.saturating_sub(2))
                {
                    let is_selected = idx == state.selected_column_idx;
                    let cursor = if is_selected { "> " } else { "  " };
                    let skip_box = if mapping.skip { "[x]" } else { "[ ]" };
                    let row_style = if is_selected {
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                    } else if mapping.skip {
                        theme.style(Token::Muted)
                    } else {
                        theme.style(Token::Text)
                    };

                    lines.push(Line::from(vec![
                        Span::styled(
                            cursor,
                            if is_selected {
                                theme.style(Token::Focus)
                            } else {
                                theme.style(Token::Muted)
                            },
                        ),
                        Span::styled(format!("{:<20}", mapping.source_column), row_style),
                        Span::styled("  ", theme.style(Token::Muted)),
                        Span::styled(format!("{:<20}", mapping.target_column), row_style),
                        Span::styled("  ", theme.style(Token::Muted)),
                        Span::styled(format!("{:<14}", mapping.target_type), row_style),
                        Span::styled("  ", theme.style(Token::Muted)),
                        Span::styled(
                            skip_box,
                            if mapping.skip {
                                theme.style(Token::Warning)
                            } else {
                                theme.style(Token::Success)
                            },
                        ),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "No columns mapped. Return to Step 1 to load preview.",
                    theme.style(Token::Muted),
                )));
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::ImportWizardStep::Execute => {
            let mut lines = Vec::new();
            lines.push(Line::from(Span::styled(
                "Transactional Execution Review:",
                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            let dry_run_str = if state.dry_run {
                "[x] Dry-run test mode enabled (will ROLLBACK)"
            } else {
                "[ ] Dry-run (will COMMIT)"
            };
            lines.push(Line::from(vec![
                Span::styled("Safety Mode: ", theme.style(Token::Muted)),
                Span::styled(
                    dry_run_str,
                    if state.dry_run {
                        theme.style(Token::Warning)
                    } else {
                        theme.style(Token::Success)
                    },
                ),
                Span::styled("  (Press 'd' to toggle)", theme.style(Token::Muted)),
            ]));
            lines.push(Line::from(""));

            lines.push(Line::from(Span::styled(
                "Generated SQL DDL / Protocol Statement:",
                theme.style(Token::Muted),
            )));
            lines.push(Line::from(Span::styled(
                &state.copy_sql,
                theme
                    .style(Token::SyntaxKeyword)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            if let Some(ref res) = state.result {
                lines.push(Line::from(vec![
                    Span::styled("Execution Result: ", theme.style(Token::Muted)),
                    Span::styled(
                        &res.status,
                        theme.style(Token::Success).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            " ({} rows, {} bytes in {}ms)",
                            res.rows_imported, res.bytes_processed, res.elapsed_ms
                        ),
                        theme.style(Token::Muted),
                    ),
                ]));
            } else if state.loading {
                lines.push(Line::from(Span::styled(
                    "Streaming data into database via COPY protocol...",
                    theme.style(Token::Focus),
                )));
            } else if let Some(ref err) = state.error {
                lines.push(Line::from(Span::styled(
                    format!("Execution Failed: {err}"),
                    theme.style(Token::Danger),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "Ready to execute. Press [Enter] to run import or [e] to load in SQL editor.",
                    theme.style(Token::Focus),
                )));
            }
            Paragraph::new(lines).render(body_area, buf);
        }
    }

    let footer_hints = vec![
        Span::styled(
            " [Tab/1/2/3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Step  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[Space] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Toggle Skip  ", theme.style(Token::Muted)),
        Span::styled(
            "[d] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Dry-run  ", theme.style(Token::Muted)),
        Span::styled(
            "[e] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Load in Editor  ", theme.style(Token::Muted)),
        Span::styled(
            "[Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Run  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_hints)).render(footer_area, buf);
}

/// Renders the PL/pgSQL Routine Runner and Profiler overlay (`032-plpgsql-routine-runner-and-profiler`).
pub(crate) fn render_routine_runner(
    state: &crate::app::model::RoutineRunnerState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(88) / 100)
        .max(64)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(86) / 100)
        .max(18)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} PL/pgSQL Routine Runner & Profiler [{}] ",
        presentation.icon(Icon::Function),
        match state.step {
            crate::app::model::RoutineRunnerStep::Routines => "Catalog Routines",
            crate::app::model::RoutineRunnerStep::Arguments => "Arguments & Run",
            crate::app::model::RoutineRunnerStep::Profile => "Code Profiler",
        }
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Step tabs
            Constraint::Length(1), // Spacer
            Constraint::Min(4),    // Body
            Constraint::Length(1), // Footer hints
        ])
        .split(inner);

    let step_area = chunks[0];
    let body_area = chunks[2];
    let footer_area = chunks[3];

    // 1. Step Tab bar
    let tab_spans = vec![
        Span::styled(
            if state.step == crate::app::model::RoutineRunnerStep::Routines {
                " [1] Select Routine "
            } else {
                "  1  Select Routine "
            },
            if state.step == crate::app::model::RoutineRunnerStep::Routines {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            if state.step == crate::app::model::RoutineRunnerStep::Arguments {
                " [2] Arguments & Test Harness "
            } else {
                "  2  Arguments & Test Harness "
            },
            if state.step == crate::app::model::RoutineRunnerStep::Arguments {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            if state.step == crate::app::model::RoutineRunnerStep::Profile {
                " [3] Profiler & Findings "
            } else {
                "  3  Profiler & Findings "
            },
            if state.step == crate::app::model::RoutineRunnerStep::Profile {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
    ];
    Paragraph::new(Line::from(tab_spans)).render(step_area, buf);

    // 2. Body Area
    let visible_rows = usize::from(body_area.height);

    match state.step {
        crate::app::model::RoutineRunnerStep::Routines => {
            let mut lines = Vec::new();
            if !state.filter.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Filter: ", theme.style(Token::Focus)),
                    Span::styled(&state.filter, theme.style(Token::Text)),
                ]));
            }
            let filtered = state.filtered_routines();
            if filtered.is_empty() {
                if state.executing {
                    lines.push(Line::from(Span::styled(
                        "Loading routine signatures from pg_proc...",
                        theme.style(Token::Muted),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        "No matching routines found in database catalog.",
                        theme.style(Token::Muted),
                    )));
                }
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        "   SCHEMA    ",
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("│ ", theme.style(Token::Border)),
                    Span::styled(
                        "ROUTINE NAME                   ",
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("│ ", theme.style(Token::Border)),
                    Span::styled(
                        "KIND      ",
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("│ ", theme.style(Token::Border)),
                    Span::styled(
                        "RETURNS                        ",
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ),
                ]));
                for (idx, r) in filtered
                    .iter()
                    .enumerate()
                    .skip(usize::from(state.scroll_y))
                    .take(visible_rows.saturating_sub(lines.len()))
                {
                    let is_sel = idx == state.selected_routine_idx;
                    let cursor = if is_sel { "> " } else { "  " };
                    let row_style = if is_sel {
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Text)
                    };
                    let kind_str = match r.kind {
                        crate::routine::RoutineKind::Function => "FUNCTION",
                        crate::routine::RoutineKind::Procedure => "PROCEDURE",
                        crate::routine::RoutineKind::Aggregate => "AGGREGATE",
                        crate::routine::RoutineKind::Window => "WINDOW",
                    };
                    let unicode = !presentation.glyphs.is_ascii();
                    lines.push(Line::from(vec![
                        Span::styled(cursor, row_style),
                        Span::styled(
                            format!(
                                "{:<11}",
                                sanitize_for_display(&truncate_to_width(&r.schema, 11, unicode))
                            ),
                            row_style,
                        ),
                        Span::styled("│ ", theme.style(Token::Border)),
                        Span::styled(
                            format!(
                                "{:<30}",
                                sanitize_for_display(&truncate_to_width(&r.name, 30, unicode))
                            ),
                            row_style,
                        ),
                        Span::styled("│ ", theme.style(Token::Border)),
                        Span::styled(format!("{:<10}", kind_str), row_style),
                        Span::styled("│ ", theme.style(Token::Border)),
                        Span::styled(
                            sanitize_for_display(&truncate_to_width(&r.return_type, 30, unicode)),
                            row_style,
                        ),
                    ]));
                }
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::RoutineRunnerStep::Arguments => {
            let Some(r) = state.selected_routine() else {
                Paragraph::new(Line::from(Span::styled(
                    "No routine selected. Switch to tab [1] to select one.",
                    theme.style(Token::Muted),
                )))
                .render(body_area, buf);
                return;
            };

            let mut lines = Vec::new();
            let rollback_span = if state.rollback_savepoint {
                Span::styled(
                    "[X] Safe Rollback (Savepoint Protected)",
                    theme.style(Token::Success).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    "[ ] Live Commit (Caution: Mutating)",
                    theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                )
            };
            lines.push(Line::from(vec![
                Span::styled("Target: ", theme.style(Token::Muted)),
                Span::styled(
                    format!("{}.{}", r.schema, r.name),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Mode: ", theme.style(Token::Muted)),
                rollback_span,
            ]));
            lines.push(Line::from(Span::styled(
                format!("Signature: {}", r.display_signature()),
                theme.style(Token::Text),
            )));
            lines.push(Line::from(Span::styled(
                "─────────────────────────────────────────────────────────────────",
                theme.style(Token::Border),
            )));

            let input_params = r.input_params();
            if input_params.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No input arguments required for this routine.",
                    theme.style(Token::Muted),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "Arguments & Bound Values:",
                    theme.style(Token::Focus),
                )));
                for (idx, p) in input_params.iter().enumerate() {
                    let is_sel = idx == state.selected_arg_idx;
                    let cursor = if is_sel { "> " } else { "  " };
                    let val = state
                        .args
                        .get(&p.name)
                        .map(|s| s.as_str())
                        .or(p.default_value.as_deref())
                        .unwrap_or("");
                    let style = if is_sel {
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Text)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(cursor, style),
                        Span::styled(format!("{:<16}", p.name), style),
                        Span::styled(
                            format!("({:?} {}) = ", p.mode, p.data_type),
                            theme.style(Token::Muted),
                        ),
                        Span::styled(
                            if val.is_empty() {
                                "<NULL / empty>"
                            } else {
                                val
                            },
                            if val.is_empty() {
                                theme.style(Token::Muted)
                            } else {
                                theme.style(Token::Success)
                            },
                        ),
                    ]));
                }
            }

            let call_sql = crate::routine::build_routine_call_sql(r, &state.args);
            lines.push(Line::from(vec![
                Span::styled("Generated SQL: ", theme.style(Token::Muted)),
                Span::styled(call_sql, theme.style(Token::Focus)),
            ]));

            if state.executing {
                lines.push(Line::from(Span::styled(
                    "Executing routine within test harness...",
                    theme.style(Token::Focus),
                )));
            } else if let Some(res) = &state.last_result {
                lines.push(Line::from(Span::styled(
                    "─────────────────────────────────────────────────────────────────",
                    theme.style(Token::Border),
                )));
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("Execution Succeeded ({:.2} ms)", res.elapsed_ms),
                        theme.style(Token::Success).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        if res.rolled_back {
                            " [Rolled Back Cleanly]"
                        } else {
                            " [Committed]"
                        },
                        theme.style(Token::Muted),
                    ),
                ]));
                for notice in &res.notices {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  [{}] ", notice.severity),
                            theme.style(Token::Warning),
                        ),
                        Span::styled(&notice.message, theme.style(Token::Text)),
                    ]));
                }
                for row in res.rows.iter().take(5) {
                    let formatted = row.join(" │ ");
                    lines.push(Line::from(Span::styled(
                        format!("  ROW: {formatted}"),
                        theme.style(Token::Text),
                    )));
                }
                if res.rows.len() > 5 {
                    lines.push(Line::from(Span::styled(
                        format!("  ... and {} more rows", res.rows.len() - 5),
                        theme.style(Token::Muted),
                    )));
                }
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::RoutineRunnerStep::Profile => {
            let Some(r) = state.selected_routine() else {
                Paragraph::new(Line::from(Span::styled(
                    "No routine selected. Switch to tab [1] to select one.",
                    theme.style(Token::Muted),
                )))
                .render(body_area, buf);
                return;
            };

            let mut lines = Vec::new();
            lines.push(Line::from(vec![
                Span::styled("Profiling Target: ", theme.style(Token::Muted)),
                Span::styled(
                    format!("{}.{}", r.schema, r.name),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
            ]));
            if state.executing {
                lines.push(Line::from(Span::styled(
                    "Running static and catalog profiler...",
                    theme.style(Token::Focus),
                )));
            } else if let Some(prof) = &state.last_profile {
                let engine_name = if prof.has_plpgsql_check {
                    "plpgsql_check"
                } else {
                    "ignatius-static"
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("Engine: {engine_name}  "),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("Findings: {}", prof.findings.len()),
                        if prof.findings.is_empty() {
                            theme.style(Token::Success)
                        } else {
                            theme.style(Token::Warning)
                        },
                    ),
                ]));
                lines.push(Line::from(Span::styled(
                    "─────────────────────────────────────────────────────────────────",
                    theme.style(Token::Border),
                )));
                if prof.findings.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "✓ No issues, warnings, or anti-patterns detected in routine.",
                        theme.style(Token::Success),
                    )));
                } else {
                    for f in &prof.findings {
                        let sev_style = match f.severity {
                            crate::routine::ProfileFindingSeverity::Error => {
                                theme.style(Token::Danger)
                            }
                            crate::routine::ProfileFindingSeverity::Warning => {
                                theme.style(Token::Warning)
                            }
                            crate::routine::ProfileFindingSeverity::Info
                            | crate::routine::ProfileFindingSeverity::Style => {
                                theme.style(Token::Muted)
                            }
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("[{:?}] ", f.severity),
                                sev_style.add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(
                                    "{}:{} ",
                                    f.code,
                                    f.line.map(|l| format!("line {l}")).unwrap_or_default()
                                ),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(&f.message, theme.style(Token::Text)),
                        ]));
                        if let Some(stmt) = &f.statement {
                            lines.push(Line::from(Span::styled(
                                format!("     Statement: {stmt}"),
                                theme.style(Token::Muted),
                            )));
                        }
                    }
                }
                if !prof.routine.source.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "─── Routine Source Definition ─────────────────────────────────────",
                        theme.style(Token::Border),
                    )));
                    for line in prof.routine.source.lines().take(10) {
                        lines.push(Line::from(Span::styled(line, theme.style(Token::Text))));
                    }
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "Press 'p' to profile routine with plpgsql_check and static rules.",
                    theme.style(Token::Focus),
                )));
            }
            Paragraph::new(lines).render(body_area, buf);
        }
    }

    // 3. Footer hints
    let footer_spans = vec![
        Span::styled(
            "[Tab/1-3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Step  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", theme.style(Token::Muted)),
        Span::styled(
            "[r/Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Run  ", theme.style(Token::Muted)),
        Span::styled(
            "[p] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Profile  ", theme.style(Token::Muted)),
        Span::styled(
            "[s] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Rollback  ", theme.style(Token::Muted)),
        Span::styled(
            "[e] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Editor  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Renders the Maintenance Operations & Pub/Sub Monitor overlay (`033-listen-notify-and-maintenance-ops`).
pub(crate) fn render_maintenance_ops(
    state: &crate::app::model::MaintenanceOpsState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(88) / 100)
        .max(64)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(86) / 100)
        .max(18)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} Maintenance & Pub/Sub Monitor [{}] ",
        presentation.icon(Icon::Database),
        match state.step {
            crate::app::model::MaintenanceOpsStep::PubSub => "Pub/Sub Monitor",
            crate::app::model::MaintenanceOpsStep::Maintain => "Maintenance Runner",
            crate::app::model::MaintenanceOpsStep::Progress => "Live Progress",
        }
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Step tabs
            Constraint::Length(1), // Spacer
            Constraint::Min(4),    // Body
            Constraint::Length(1), // Status message
            Constraint::Length(1), // Footer hints
        ])
        .split(inner);

    let step_area = chunks[0];
    let body_area = chunks[2];
    let status_area = chunks[3];
    let footer_area = chunks[4];

    // 1. Step Tab bar
    let tab_spans = vec![
        Span::styled(
            if state.step == crate::app::model::MaintenanceOpsStep::PubSub {
                " [1] Pub/Sub Monitor "
            } else {
                "  1  Pub/Sub Monitor "
            },
            if state.step == crate::app::model::MaintenanceOpsStep::PubSub {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            if state.step == crate::app::model::MaintenanceOpsStep::Maintain {
                " [2] Maintenance Runner "
            } else {
                "  2  Maintenance Runner "
            },
            if state.step == crate::app::model::MaintenanceOpsStep::Maintain {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            if state.step == crate::app::model::MaintenanceOpsStep::Progress {
                " [3] Live Progress Telemetry "
            } else {
                "  3  Live Progress Telemetry "
            },
            if state.step == crate::app::model::MaintenanceOpsStep::Progress {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
    ];
    Paragraph::new(Line::from(tab_spans)).render(step_area, buf);

    // 2. Body Area
    let visible_rows = usize::from(body_area.height);

    match state.step {
        crate::app::model::MaintenanceOpsStep::PubSub => {
            let mut lines = Vec::new();
            let channel_str = if state.channels.is_empty() {
                "(none - press [l] to listen)".to_owned()
            } else {
                state.channels.join(", ")
            };
            lines.push(Line::from(vec![
                Span::styled("Active Channels: ", theme.style(Token::Muted)),
                Span::styled(
                    channel_str,
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  │  Target: ", theme.style(Token::Muted)),
                Span::styled(&state.channel_input, theme.style(Token::Focus)),
                Span::styled("  │  Total Received: ", theme.style(Token::Muted)),
                Span::styled(
                    state.notifications.len().to_string(),
                    theme.style(Token::Info),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "─".repeat(usize::from(body_area.width)),
                theme.style(Token::Border),
            )));

            if state.notifications.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No notifications received yet. Press [l] to LISTEN on target channel, [n] to NOTIFY, [r] to poll.",
                    theme.style(Token::Muted),
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<20}", "RECEIVED"),
                        theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<16}", "CHANNEL"),
                        theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<8}", "PID"),
                        theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "PAYLOAD",
                        theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                    ),
                ]));

                let skip = if state.selected_idx >= visible_rows.saturating_sub(4) {
                    state
                        .selected_idx
                        .saturating_sub(visible_rows.saturating_sub(5))
                } else {
                    0
                };

                for (idx, n) in state
                    .notifications
                    .iter()
                    .enumerate()
                    .skip(skip)
                    .take(visible_rows.saturating_sub(3))
                {
                    let is_sel = idx == state.selected_idx;
                    let style = if is_sel {
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Text)
                    };

                    let prefix = if is_sel { "▶ " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{}{:<18}", prefix, n.timestamp), style),
                        Span::styled(format!("{:<16}", n.channel), style),
                        Span::styled(format!("{:<8}", n.sender_pid), style),
                        Span::styled(n.payload.clone(), style),
                    ]));
                }
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::MaintenanceOpsStep::Maintain => {
            let mut lines = Vec::new();
            let lock_name = crate::ops::estimate_maintenance_lock_level(&state.maintenance_kind);
            let target_str = if state.target_relation.trim().is_empty() {
                "Entire Database (all tables)".to_owned()
            } else {
                format!("Relation: {}", state.target_relation.trim())
            };
            let opts = crate::ops::MaintenanceOptions {
                kind: state.maintenance_kind.clone(),
                target: if state.target_relation.trim().is_empty() {
                    None
                } else {
                    Some(state.target_relation.trim().to_owned())
                },
                confirm_production: state.confirm_production,
            };
            let generated_sql =
                crate::ops::build_maintenance_sql(&opts).unwrap_or_else(|err| format!("-- {err}"));

            lines.push(Line::from(vec![
                Span::styled("Target: ", theme.style(Token::Muted)),
                Span::styled(
                    target_str,
                    theme.style(Token::Text).add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Operation: ", theme.style(Token::Muted)),
                Span::styled(
                    state.maintenance_kind.display_name(),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  (Press [v] to cycle operation)",
                    theme.style(Token::Muted),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Estimated Lock: ", theme.style(Token::Muted)),
                Span::styled(
                    lock_name,
                    if lock_name.contains("AccessExclusive") {
                        theme.style(Token::Danger).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Success).add_modifier(Modifier::BOLD)
                    },
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Production Guard: ", theme.style(Token::Muted)),
                Span::styled(
                    if state.confirm_production {
                        "[X] Confirmed for production"
                    } else {
                        "[ ] Confirmation required for locking ops"
                    },
                    if state.confirm_production {
                        theme.style(Token::Warning).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Muted)
                    },
                ),
                Span::styled("  (Press [p] to toggle)", theme.style(Token::Muted)),
            ]));
            lines.push(Line::from(Span::styled(
                "─".repeat(usize::from(body_area.width)),
                theme.style(Token::Border),
            )));
            lines.push(Line::from(vec![
                Span::styled("Generated SQL: ", theme.style(Token::Muted)),
                Span::styled(generated_sql, theme.style(Token::Focus)),
            ]));
            lines.push(Line::default());

            if let Some(res) = &state.last_result {
                lines.push(Line::from(vec![
                    Span::styled("Execution Result: ", theme.style(Token::Muted)),
                    Span::styled(
                        &res.message,
                        theme.style(Token::Success).add_modifier(Modifier::BOLD),
                    ),
                ]));
                for notice in &res.notices {
                    lines.push(Line::from(vec![
                        Span::styled("  Notice: ", theme.style(Token::Muted)),
                        Span::styled(notice, theme.style(Token::Text)),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "Press [r/Enter] to run maintenance, or [e] to export SQL to the interactive editor.",
                    theme.style(Token::Muted),
                )));
            }
            Paragraph::new(lines).render(body_area, buf);
        }
        crate::app::model::MaintenanceOpsStep::Progress => {
            let mut lines = Vec::new();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", "PID"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<12}", "COMMAND"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<16}", "PHASE"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<20}", "RELATION"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<16}", "BLOCKS"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "PROGRESS",
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "─".repeat(usize::from(body_area.width)),
                theme.style(Token::Border),
            )));

            if state.progress.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No background maintenance operations currently active in pg_stat_progress_*. Press [r] to refresh.",
                    theme.style(Token::Muted),
                )));
            } else {
                for (idx, entry) in state
                    .progress
                    .iter()
                    .enumerate()
                    .take(visible_rows.saturating_sub(2))
                {
                    let is_sel = idx == state.selected_idx;
                    let style = if is_sel {
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Text)
                    };
                    let blocks = match (entry.heap_blks_scanned, entry.heap_blks_total) {
                        (Some(d), Some(t)) => format!("{d}/{t}"),
                        (Some(d), None) => format!("{d}"),
                        _ => "-".to_owned(),
                    };
                    let pct = entry
                        .percent
                        .map(|p| format!("{p:.1}%"))
                        .unwrap_or_else(|| "-".to_owned());

                    lines.push(Line::from(vec![
                        Span::styled(format!("{:<8}", entry.pid), style),
                        Span::styled(format!("{:<12}", entry.command), style),
                        Span::styled(format!("{:<16}", entry.phase), style),
                        Span::styled(
                            format!("{:<20}", entry.relname.as_deref().unwrap_or("-")),
                            style,
                        ),
                        Span::styled(format!("{:<16}", blocks), style),
                        Span::styled(pct, style),
                    ]));
                }
            }
            Paragraph::new(lines).render(body_area, buf);
        }
    }

    // 3. Status message line
    if let Some(ref msg) = state.status_message {
        Paragraph::new(Line::from(vec![
            Span::styled("Status: ", theme.style(Token::Muted)),
            Span::styled(msg, theme.style(Token::Focus).add_modifier(Modifier::BOLD)),
        ]))
        .render(status_area, buf);
    }

    // 4. Footer Hints
    let footer_spans = vec![
        Span::styled(
            "[Tab/1-3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[k/j] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", theme.style(Token::Muted)),
        Span::styled(
            "[r/Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Run/Poll  ", theme.style(Token::Muted)),
        Span::styled(
            "[v] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Kind  ", theme.style(Token::Muted)),
        Span::styled(
            "[p] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Prod Guard  ", theme.style(Token::Muted)),
        Span::styled(
            "[l] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Listen  ", theme.style(Token::Muted)),
        Span::styled(
            "[n] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Notify  ", theme.style(Token::Muted)),
        Span::styled(
            "[e] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Editor  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Renders the Query Performance & pg_stat_statements Forensics overlay (`034-query-performance-and-stat-statements`).
pub(crate) fn render_query_performance(
    state: &crate::app::model::QueryPerformanceState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} QUERY PERFORMANCE FORENSICS · PG_STAT_STATEMENTS ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace (table + inspector split)
            Constraint::Length(1), // Status / Reset prompt
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let body_area = chunks[2];
    let status_area = chunks[3];
    let footer_area = chunks[4];

    // Hardware Capsules Bar
    let snapshot_opt = state.snapshot.as_ref();
    let hit_ratio = snapshot_opt.map_or(100.0, |s| s.global_hit_ratio);
    let tracked_count = snapshot_opt.map_or(0, |s| s.queries.len());
    let total_calls: u64 = snapshot_opt.map_or(0, |s| s.total_calls);
    let total_time_ms: f64 = snapshot_opt.map_or(0.0, |s| s.total_time_ms);

    let hit_token = if hit_ratio >= 98.0 {
        Token::Success
    } else if hit_ratio >= 90.0 {
        Token::Focus
    } else {
        Token::Warning
    };

    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("HIT RATIO: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{hit_ratio:.1}%"),
            theme.style(hit_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("TRACKED: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{tracked_count}"),
            theme.style(Token::Text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("CALLS: ", theme.style(Token::Muted)),
        Span::styled(format!("{total_calls}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("EXEC TIME: ", theme.style(Token::Muted)),
        Span::styled(
            crate::perf::format_duration_ms(total_time_ms),
            theme.style(Token::Text),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("SORT: ", theme.style(Token::Muted)),
        Span::styled(
            state.sort_order.display_name(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("]", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Body: check if extension unavailable
    if let Some(snapshot) = snapshot_opt {
        if !snapshot.extension_available {
            let mut lines = Vec::new();
            lines.push(Line::from(Span::styled(
                "Telemetry Unavailable: pg_stat_statements is not installed or enabled.",
                theme.style(Token::Warning).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            if let Some(reason) = &snapshot.missing_reason {
                lines.push(Line::from(Span::styled(reason, theme.style(Token::Muted))));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                "To enable performance tracking in PostgreSQL:",
                theme.style(Token::Text).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "  1. Add to postgresql.conf: shared_preload_libraries = 'pg_stat_statements'",
                theme.style(Token::Muted),
            )));
            lines.push(Line::from(Span::styled(
                "  2. Restart PostgreSQL daemon",
                theme.style(Token::Muted),
            )));
            lines.push(Line::from(Span::styled(
                "  3. Run SQL: CREATE EXTENSION IF NOT EXISTS pg_stat_statements;",
                theme.style(Token::Focus),
            )));
            Paragraph::new(lines).render(body_area, buf);
            return;
        }

        if snapshot.queries.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No query execution statistics recorded yet for the current database.",
                    theme.style(Token::Muted),
                )),
                Line::from(Span::styled(
                    "Execute queries in the editor or application to populate pg_stat_statements.",
                    theme.style(Token::Muted),
                )),
            ];
            Paragraph::new(lines).render(body_area, buf);
        } else {
            // Split body area: Table (top 58%) + Selected Query Detail (bottom 42%)
            let body_split = Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .constraints([
                    Constraint::Percentage(58),
                    Constraint::Length(1), // Split rule
                    Constraint::Percentage(42),
                ])
                .split(body_area);

            let table_area = body_split[0];
            let split_rule_area = body_split[1];
            let detail_area = body_split[2];

            // Render horizontal split rule
            let sep = "─".repeat(split_rule_area.width as usize);
            Paragraph::new(Line::from(Span::styled(sep, theme.style(Token::Border))))
                .render(split_rule_area, buf);

            // Table Header
            let mut table_lines = Vec::new();
            table_lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<4}", "#"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<8}", "CALLS"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<11}", "TOTAL TIME"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<11}", "MEAN TIME"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<9}", "CACHE HIT"),
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "QUERY STATEMENT",
                    theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                ),
            ]));

            let visible_rows = table_area.height.saturating_sub(1) as usize;
            let start_idx = if state.selected_idx >= visible_rows {
                state
                    .selected_idx
                    .saturating_sub(visible_rows.saturating_sub(1))
            } else {
                0
            };

            for (idx, entry) in snapshot
                .queries
                .iter()
                .enumerate()
                .skip(start_idx)
                .take(visible_rows)
            {
                let is_sel = idx == state.selected_idx;
                let rank = idx + 1;
                let calls_str = format!("{:<8}", entry.calls);
                let total_str = format!(
                    "{:<11}",
                    crate::perf::format_duration_ms(entry.total_exec_time_ms)
                );
                let mean_str = format!(
                    "{:<11}",
                    crate::perf::format_duration_ms(entry.mean_exec_time_ms)
                );
                let hit_ratio = entry.hit_ratio();
                let hit_str = format!("{:<9}", format!("{hit_ratio:.1}%"));

                let query_snippet = crate::perf::format_query_snippet(&entry.query, 60);

                let line_style = if is_sel {
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                } else {
                    theme.style(Token::Text)
                };

                let marker = if is_sel { "▶ " } else { "  " };

                table_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{marker}{:<2}", rank),
                        if is_sel {
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Muted)
                        },
                    ),
                    Span::styled(calls_str, line_style),
                    Span::styled(total_str, line_style),
                    Span::styled(mean_str, line_style),
                    Span::styled(
                        hit_str,
                        if hit_ratio < 90.0 {
                            theme.style(Token::Warning)
                        } else {
                            line_style
                        },
                    ),
                    Span::styled(
                        query_snippet,
                        if is_sel {
                            theme.style(Token::Text).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Muted)
                        },
                    ),
                ]));
            }
            Paragraph::new(table_lines).render(table_area, buf);

            // Detail Area: Detailed Telemetry for Selected Entry
            if let Some(entry) = state.selected_entry() {
                let mut detail_lines = Vec::new();
                let timing_row = format!(
                    "TIMING: min={} · max={} · mean={} · stddev={}",
                    crate::perf::format_duration_ms(entry.min_exec_time_ms),
                    crate::perf::format_duration_ms(entry.max_exec_time_ms),
                    crate::perf::format_duration_ms(entry.mean_exec_time_ms),
                    crate::perf::format_duration_ms(entry.stddev_exec_time_ms),
                );
                let io_row = format!(
                    "BLOCK I/O: hit={} · read={} · dirtied={} · written={}",
                    entry.shared_blks_hit,
                    entry.shared_blks_read,
                    entry.shared_blks_dirtied,
                    entry.shared_blks_written,
                );

                detail_lines.push(Line::from(vec![
                    Span::styled(timing_row, theme.style(Token::Text)),
                    Span::styled("   ", theme.style(Token::Muted)),
                    Span::styled(io_row, theme.style(Token::Muted)),
                ]));

                detail_lines.push(Line::from(Span::styled(
                    "FULL STATEMENT (press [e] to load into editor):",
                    theme.style(Token::Muted),
                )));

                let max_detail_lines = detail_area.height.saturating_sub(2) as usize;
                for query_line in entry.query.lines().take(max_detail_lines.max(1)) {
                    detail_lines.push(Line::from(Span::styled(
                        query_line,
                        theme.style(Token::Focus),
                    )));
                }

                Paragraph::new(detail_lines).render(detail_area, buf);
            }
        }
    } else if state.loading {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Gathering query telemetry from pg_stat_statements...",
                theme.style(Token::Focus),
            )),
        ];
        Paragraph::new(lines).render(body_area, buf);
    }

    // Status Area / Reset Confirmation Banner
    if state.confirm_reset {
        let warn_line = Line::from(vec![
            Span::styled(
                " HAZARD: ",
                theme.style(Token::Warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Reset all pg_stat_statements metrics? Press ",
                theme.style(Token::Text),
            ),
            Span::styled(
                "[Enter]",
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to execute SELECT pg_stat_statements_reset() or ",
                theme.style(Token::Text),
            ),
            Span::styled(
                "[!]",
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to cancel.", theme.style(Token::Text)),
        ]);
        Paragraph::new(warn_line).render(status_area, buf);
    } else if let Some(status) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(status, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Hints
    let footer_spans = vec![
        Span::styled(
            "[m] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Mean Time  ", theme.style(Token::Muted)),
        Span::styled(
            "[t] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Total Time  ", theme.style(Token::Muted)),
        Span::styled(
            "[c] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Calls  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Rows  ", theme.style(Token::Muted)),
        Span::styled(
            "[h] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Hit Ratio  ", theme.style(Token::Muted)),
        Span::styled(
            "[e] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Editor  ", theme.style(Token::Muted)),
        Span::styled(
            "[!] ",
            theme.style(Token::Warning).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Reset Stats  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Renders the interactive Index Advisor & Bloat Forensics overlay.
pub(crate) fn render_index_advisor(
    state: &crate::app::model::IndexAdvisorState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} INDEX ADVISOR & BLOAT FORENSICS · HYPOPG ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Tab selectors
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace (table + detail split)
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let tab_area = chunks[1];
    let body_area = chunks[3];
    let status_area = chunks[4];
    let footer_area = chunks[5];

    let report_opt = state.report.as_ref();
    let health_score = report_opt.map_or(100, |r| r.health_score());
    let reclaimable_str = report_opt.map_or_else(|| "0 B".to_string(), |r| r.format_reclaimable());
    let redundant_cnt = report_opt.map_or(0, |r| r.redundant_indexes.len());
    let bloated_cnt = report_opt.map_or(0, |r| r.bloated_indexes.len());
    let unindexed_cnt = report_opt.map_or(0, |r| r.unindexed_foreign_keys.len());
    let hypo_ready = report_opt.is_some_and(|r| r.hypopg_installed);

    let score_token = if health_score >= 90 {
        Token::Success
    } else if health_score >= 75 {
        Token::Focus
    } else {
        Token::Warning
    };

    // Hardware Capsules Bar
    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("HEALTH: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{health_score}%"),
            theme.style(score_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("RECLAIMABLE: ", theme.style(Token::Muted)),
        Span::styled(
            reclaimable_str,
            theme.style(Token::Success).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("REDUNDANT: ", theme.style(Token::Muted)),
        Span::styled(format!("{redundant_cnt}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("BLOATED: ", theme.style(Token::Muted)),
        Span::styled(format!("{bloated_cnt}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("UNINDEXED FK: ", theme.style(Token::Muted)),
        Span::styled(format!("{unindexed_cnt}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("HYPOPG: ", theme.style(Token::Muted)),
        Span::styled(
            if hypo_ready { "ENABLED" } else { "ABSENT" },
            theme.style(if hypo_ready {
                Token::Success
            } else {
                Token::Muted
            }),
        ),
        Span::styled("]", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Tab Selectors
    let tab_spans = vec![
        Span::styled(
            format!(" [1] Redundant ({redundant_cnt}) "),
            if state.active_tab == 0 {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            format!(" [2] B-Tree Bloat ({bloated_cnt}) "),
            if state.active_tab == 1 {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            format!(" [3] Unindexed Foreign Keys ({unindexed_cnt}) "),
            if state.active_tab == 2 {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
    ];
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    // Body Workspace
    if state.loading {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Analyzing database catalog for index opportunities, duplicates, and bloat...",
                theme.style(Token::Muted),
            )),
        ];
        Paragraph::new(lines).render(body_area, buf);
    } else if let Some(report) = report_opt {
        let body_split = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                Constraint::Percentage(56),
                Constraint::Length(1), // Split rule
                Constraint::Percentage(44),
            ])
            .split(body_area);

        let table_area = body_split[0];
        let split_rule_area = body_split[1];
        let detail_area = body_split[2];

        let sep = "─".repeat(split_rule_area.width as usize);
        Paragraph::new(Line::from(Span::styled(sep, theme.style(Token::Border))))
            .render(split_rule_area, buf);

        // Render Table according to active tab
        match state.active_tab {
            0 => {
                if report.redundant_indexes.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No duplicate, prefix-covered, or unused indexes detected.",
                        theme.style(Token::Muted),
                    )))
                    .render(table_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(Line::from(vec![
                        Span::styled(format!("{:<24}", "TABLE"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<26}", "INDEX"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<16}", "REASON"), theme.style(Token::Muted)),
                        Span::styled(format!("{:>10}", "SIZE"), theme.style(Token::Muted)),
                        Span::styled(format!("{:>8}", "SCANS"), theme.style(Token::Muted)),
                        Span::raw(" "),
                        Span::styled("COVERED BY", theme.style(Token::Muted)),
                    ]));

                    let visible_rows = table_area.height.saturating_sub(1) as usize;
                    let scroll_offset = state
                        .selected_idx
                        .saturating_sub(visible_rows.saturating_sub(1));

                    for (idx, item) in report
                        .redundant_indexes
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(visible_rows)
                    {
                        let is_selected = idx == state.selected_idx;
                        let row_style = if is_selected {
                            theme.style(Token::Selection).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };

                        let table_str = format!("{}.{}", item.schema, item.table);
                        let covered = item.covering_index.as_deref().unwrap_or("-");

                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<24}", crate::advisor::truncate_str(&table_str, 24)),
                                row_style,
                            ),
                            Span::styled(
                                format!(
                                    "{:<26}",
                                    crate::advisor::truncate_str(&item.index_name, 26)
                                ),
                                row_style,
                            ),
                            Span::styled(
                                format!("{:<16}", item.reason.label()),
                                if is_selected {
                                    row_style
                                } else {
                                    theme.style(Token::Warning)
                                },
                            ),
                            Span::styled(
                                format!("{:>10}", crate::advisor::format_bytes(item.size_bytes)),
                                row_style,
                            ),
                            Span::styled(format!("{:>8}", item.scans), row_style),
                            Span::raw(" "),
                            Span::styled(crate::advisor::truncate_str(covered, 24), row_style),
                        ]));
                    }
                    Paragraph::new(lines).render(table_area, buf);
                }
            }
            1 => {
                if report.bloated_indexes.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No bloated B-Tree indexes exceeding threshold detected.",
                        theme.style(Token::Muted),
                    )))
                    .render(table_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(Line::from(vec![
                        Span::styled(format!("{:<24}", "TABLE"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<26}", "INDEX"), theme.style(Token::Muted)),
                        Span::styled(format!("{:>10}", "TOTAL"), theme.style(Token::Muted)),
                        Span::styled(format!("{:>10}", "WASTED"), theme.style(Token::Muted)),
                        Span::styled(format!("{:>8}", "BLOAT %"), theme.style(Token::Muted)),
                    ]));

                    let visible_rows = table_area.height.saturating_sub(1) as usize;
                    let scroll_offset = state
                        .selected_idx
                        .saturating_sub(visible_rows.saturating_sub(1));

                    for (idx, item) in report
                        .bloated_indexes
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(visible_rows)
                    {
                        let is_selected = idx == state.selected_idx;
                        let row_style = if is_selected {
                            theme.style(Token::Selection).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };

                        let table_str = format!("{}.{}", item.schema, item.table);
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<24}", crate::advisor::truncate_str(&table_str, 24)),
                                row_style,
                            ),
                            Span::styled(
                                format!(
                                    "{:<26}",
                                    crate::advisor::truncate_str(&item.index_name, 26)
                                ),
                                row_style,
                            ),
                            Span::styled(
                                format!("{:>10}", crate::advisor::format_bytes(item.total_bytes)),
                                row_style,
                            ),
                            Span::styled(
                                format!("{:>10}", crate::advisor::format_bytes(item.wasted_bytes)),
                                row_style,
                            ),
                            Span::styled(
                                format!("{:>7.1}%", item.bloat_pct),
                                if is_selected {
                                    row_style
                                } else if item.bloat_pct >= 50.0 {
                                    theme.style(Token::Warning).add_modifier(Modifier::BOLD)
                                } else {
                                    theme.style(Token::Text)
                                },
                            ),
                        ]));
                    }
                    Paragraph::new(lines).render(table_area, buf);
                }
            }
            2 => {
                if report.unindexed_foreign_keys.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "All foreign keys have supporting child indexes.",
                        theme.style(Token::Muted),
                    )))
                    .render(table_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(Line::from(vec![
                        Span::styled(format!("{:<28}", "CONSTRAINT"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<24}", "CHILD TABLE"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<20}", "COLUMNS"), theme.style(Token::Muted)),
                        Span::styled("REFERENCES", theme.style(Token::Muted)),
                    ]));

                    let visible_rows = table_area.height.saturating_sub(1) as usize;
                    let scroll_offset = state
                        .selected_idx
                        .saturating_sub(visible_rows.saturating_sub(1));

                    for (idx, item) in report
                        .unindexed_foreign_keys
                        .iter()
                        .enumerate()
                        .skip(scroll_offset)
                        .take(visible_rows)
                    {
                        let is_selected = idx == state.selected_idx;
                        let row_style = if is_selected {
                            theme.style(Token::Selection).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };

                        let child_str = format!("{}.{}", item.schema, item.child_table);
                        let cols_str = item.columns.join(", ");

                        lines.push(Line::from(vec![
                            Span::styled(
                                format!(
                                    "{:<28}",
                                    crate::advisor::truncate_str(&item.constraint_name, 28)
                                ),
                                row_style,
                            ),
                            Span::styled(
                                format!("{:<24}", crate::advisor::truncate_str(&child_str, 24)),
                                row_style,
                            ),
                            Span::styled(
                                format!("{:<20}", crate::advisor::truncate_str(&cols_str, 20)),
                                row_style,
                            ),
                            Span::styled(
                                crate::advisor::truncate_str(&item.parent_table, 24),
                                row_style,
                            ),
                        ]));
                    }
                    Paragraph::new(lines).render(table_area, buf);
                }
            }
            _ => {}
        }

        // Render Lower Detail Inspector
        let mut detail_lines = Vec::new();
        detail_lines.push(Line::from(Span::styled(
            "Remediation SQL (Press [e] to load into Editor or [c] to copy):",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        )));
        if let Some(sql) = state.selected_sql() {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                sql,
                theme.style(Token::Text).add_modifier(Modifier::BOLD),
            )));
        } else {
            detail_lines.push(Line::from(Span::styled(
                "No item selected",
                theme.style(Token::Muted),
            )));
        }

        if !report.hypo_tests.is_empty() {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "Hypothetical Index Profiling (HypoPG):",
                theme.style(Token::Muted),
            )));
            for t in &report.hypo_tests {
                detail_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:.1}% savings", t.cost_reduction_pct),
                        theme.style(Token::Success),
                    ),
                    Span::styled(
                        format!(
                            " (baseline: {:.1} -> hypo: {:.1})",
                            t.original_cost, t.hypo_cost
                        ),
                        theme.style(Token::Muted),
                    ),
                ]));
            }
        }

        Paragraph::new(detail_lines).render(detail_area, buf);
    }

    // Status Message Line
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(msg, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Bar
    let footer_spans = vec![
        Span::styled(
            "[1/2/3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓/j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[e] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Editor  ", theme.style(Token::Muted)),
        Span::styled(
            "[c] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Copy SQL  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Renders the interactive Transaction, WAL & Checkpointer Observability overlay.
pub(crate) fn render_wal_observability(
    state: &crate::app::model::WalObservabilityState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} TRANSACTION, WAL & CHECKPOINTER OBSERVABILITY ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Tab selectors
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let tab_area = chunks[1];
    let body_area = chunks[3];
    let status_area = chunks[4];
    let footer_area = chunks[5];

    let report_opt = state.report.as_ref();
    let commits_cnt = report_opt.map_or(0, |r| r.total_commits());
    let rollbacks_cnt = report_opt.map_or(0, |r| r.total_rollbacks());
    let rollback_pct = report_opt.map_or(0.0, |r| r.aggregate_rollback_ratio());
    let wal_bytes = report_opt.map_or(0, |r| r.wal.wal_bytes);
    let wal_str = crate::telemetry::format_bytes(wal_bytes);
    let timed_pct = report_opt.map_or(100.0, |r| r.checkpointer.timed_ratio());
    let max_xid = report_opt.map_or(0, |r| r.max_xid_age());
    let standby_cnt = report_opt.map_or(0, |r| r.standbys.len());

    let rollback_token = if rollback_pct < 2.0 {
        Token::Success
    } else if rollback_pct < 10.0 {
        Token::Focus
    } else {
        Token::Warning
    };

    let checkpoint_token = if timed_pct >= 85.0 {
        Token::Success
    } else if timed_pct >= 70.0 {
        Token::Focus
    } else {
        Token::Warning
    };

    // Dieter Rams status capsules
    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("COMMITS: ", theme.style(Token::Muted)),
        Span::styled(format!("{commits_cnt}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("ROLLBACK: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{rollbacks_cnt} ({rollback_pct:.2}%)"),
            theme.style(rollback_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("WAL WRITE: ", theme.style(Token::Muted)),
        Span::styled(wal_str, theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("CHECKPOINTS: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{timed_pct:.1}% TIMED"),
            theme.style(checkpoint_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("MAX XID AGE: ", theme.style(Token::Muted)),
        Span::styled(format!("{max_xid}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("STANDBYS: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{standby_cnt}"),
            theme.style(if standby_cnt > 0 {
                Token::Success
            } else {
                Token::Muted
            }),
        ),
        Span::styled("]", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Tab Selectors
    let tab_spans = vec![
        Span::styled(
            " [1] Checkpoints & WAL ",
            if state.active_tab == 0 {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            format!(
                " [2] Transactions & XID Freeze ({}) ",
                report_opt.map_or(0, |r| r.databases.len())
            ),
            if state.active_tab == 1 {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
        Span::styled(" │ ", theme.style(Token::Border)),
        Span::styled(
            format!(" [3] Replication & Standbys ({standby_cnt}) "),
            if state.active_tab == 2 {
                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
            } else {
                theme.style(Token::Muted)
            },
        ),
    ];
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    // Body Workspace
    if state.loading {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Gathering checkpointer, WAL, transaction, and replication telemetry from catalogs...",
                theme.style(Token::Muted),
            )),
        ];
        Paragraph::new(lines).render(body_area, buf);
    } else if let Some(report) = report_opt {
        match state.active_tab {
            0 => {
                // Tab 0: Checkpoints & WAL (Dual column layout)
                let cols = Layout::default()
                    .direction(ratatui::layout::Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(50),
                        Constraint::Length(1), // Divider
                        Constraint::Percentage(50),
                    ])
                    .split(body_area);

                let left_area = cols[0];
                let divider_area = cols[1];
                let right_area = cols[2];

                // Vertical separator line
                for y in 0..divider_area.height {
                    buf.set_string(
                        divider_area.x,
                        divider_area.y + y,
                        "│",
                        theme.style(Token::Border),
                    );
                }

                // Left Column: Checkpointer & Background Writer
                let chk = &report.checkpointer;
                let mut left_lines = Vec::new();
                left_lines.push(Line::from(Span::styled(
                    "CHECKPOINTER & BACKGROUND WRITER",
                    theme.style(Token::Header).add_modifier(Modifier::BOLD),
                )));
                left_lines.push(Line::from(""));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Timed Checkpoints:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{}", chk.timed_checkpoints),
                        theme.style(Token::Text),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Requested Checkpoints:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{}", chk.requested_checkpoints),
                        theme.style(Token::Text),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(format!("{:<26}", "Timed Ratio:"), theme.style(Token::Muted)),
                    Span::styled(
                        format!("{:.1}%", chk.timed_ratio()),
                        theme.style(if chk.timed_ratio() >= 85.0 {
                            Token::Success
                        } else {
                            Token::Warning
                        }),
                    ),
                    Span::styled(
                        if chk.timed_ratio() < 80.0 {
                            " (low: consider increasing max_wal_size)"
                        } else {
                            " (healthy)"
                        },
                        theme.style(Token::Muted),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(format!("{:<26}", "Write Time:"), theme.style(Token::Muted)),
                    Span::styled(
                        format!("{:.1} ms", chk.write_time_ms),
                        theme.style(Token::Text),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Sync / Fsync Time:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{:.1} ms", chk.sync_time_ms),
                        theme.style(Token::Text),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Buffers Checkpoint:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{}", chk.buffers_checkpoint),
                        theme.style(Token::Text),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Buffers Cleaned:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(format!("{}", chk.buffers_clean), theme.style(Token::Text)),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Buffers Backend Written:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{}", chk.buffers_backend),
                        theme.style(if chk.buffers_backend > 0 {
                            Token::Warning
                        } else {
                            Token::Text
                        }),
                    ),
                ]));
                left_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Backend Write Ratio:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{:.2}%", chk.backend_write_ratio()),
                        theme.style(if chk.backend_write_ratio() > 5.0 {
                            Token::Warning
                        } else {
                            Token::Success
                        }),
                    ),
                ]));
                Paragraph::new(left_lines).render(left_area, buf);

                // Right Column: Write-Ahead Log & Archiver
                let wal = &report.wal;
                let mut right_lines = Vec::new();
                right_lines.push(Line::from(Span::styled(
                    "WRITE-AHEAD LOG (WAL) & ARCHIVER",
                    theme.style(Token::Header).add_modifier(Modifier::BOLD),
                )));
                right_lines.push(Line::from(""));
                right_lines.push(Line::from(vec![
                    Span::styled(format!("{:<26}", "Current LSN:"), theme.style(Token::Muted)),
                    Span::styled(
                        if wal.current_lsn.is_empty() {
                            "0/0"
                        } else {
                            &wal.current_lsn
                        },
                        theme.style(Token::Focus),
                    ),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(format!("{:<26}", "WAL Records:"), theme.style(Token::Muted)),
                    Span::styled(format!("{}", wal.wal_records), theme.style(Token::Text)),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Full Page Images (FPI):"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(format!("{}", wal.wal_fpi), theme.style(Token::Text)),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(format!("{:<26}", "WAL Volume:"), theme.style(Token::Muted)),
                    Span::styled(
                        crate::telemetry::format_bytes(wal.wal_bytes),
                        theme.style(Token::Success),
                    ),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "WAL Buffers Full:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{}", wal.wal_buffers_full),
                        theme.style(if wal.wal_buffers_full > 0 {
                            Token::Warning
                        } else {
                            Token::Success
                        }),
                    ),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "WAL Write Time:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{:.1} ms", wal.wal_write_time_ms),
                        theme.style(Token::Text),
                    ),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "WAL Sync Time:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{:.1} ms", wal.wal_sync_time_ms),
                        theme.style(Token::Text),
                    ),
                ]));
                right_lines.push(Line::from(""));
                right_lines.push(Line::from(Span::styled(
                    "WAL ARCHIVER",
                    theme.style(Token::Header).add_modifier(Modifier::BOLD),
                )));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Archived Segments:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(format!("{}", wal.archived_count), theme.style(Token::Text)),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:<26}", "Failed Archives:"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{}", wal.failed_count),
                        theme.style(if wal.failed_count > 0 {
                            Token::Danger
                        } else {
                            Token::Success
                        }),
                    ),
                ]));
                Paragraph::new(right_lines).render(right_area, buf);
            }
            1 => {
                // Tab 1: Transactions & XID Freeze (Split Top/Bottom)
                let rows = Layout::default()
                    .direction(ratatui::layout::Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(55),
                        Constraint::Length(1), // Split rule
                        Constraint::Percentage(45),
                    ])
                    .split(body_area);

                let xact_area = rows[0];
                let rule_area = rows[1];
                let freeze_area = rows[2];

                let sep = "─".repeat(rule_area.width as usize);
                Paragraph::new(Line::from(Span::styled(sep, theme.style(Token::Border))))
                    .render(rule_area, buf);

                // Top: Database Transactions
                let mut xact_lines = Vec::new();
                xact_lines.push(Line::from(vec![
                    Span::styled(format!("{:<24}", "DATABASE"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>14}", "COMMITS"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>14}", "ROLLBACKS"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>14}", "ROLLBACK %"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>12}", "DEADLOCKS"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>14}", "TEMP BYTES"), theme.style(Token::Muted)),
                ]));

                let db_cursor = state.selected_idx;
                for (idx, db) in report.databases.iter().enumerate() {
                    let is_sel = idx == db_cursor;
                    let rb_pct = db.rollback_ratio();
                    let rb_tok = if rb_pct > 10.0 {
                        Token::Warning
                    } else if rb_pct > 0.0 {
                        Token::Focus
                    } else {
                        Token::Text
                    };

                    let prefix = if is_sel { "> " } else { "  " };
                    let row_token = if is_sel { Token::Focus } else { Token::Text };
                    let db_name = format!("{prefix}{}", db.database_name);

                    xact_lines.push(Line::from(vec![
                        Span::styled(format!("{:<24}", db_name), theme.style(row_token)),
                        Span::styled(format!("{:>14}", db.xact_commit), theme.style(Token::Text)),
                        Span::styled(
                            format!("{:>14}", db.xact_rollback),
                            theme.style(Token::Text),
                        ),
                        Span::styled(format!("{:>13.2}%", rb_pct), theme.style(rb_tok)),
                        Span::styled(
                            format!("{:>12}", db.deadlocks),
                            theme.style(if db.deadlocks > 0 {
                                Token::Warning
                            } else {
                                Token::Text
                            }),
                        ),
                        Span::styled(
                            format!("{:>14}", crate::telemetry::format_bytes(db.temp_bytes)),
                            theme.style(Token::Text),
                        ),
                    ]));
                }
                Paragraph::new(xact_lines).render(xact_area, buf);

                // Bottom: XID Freeze & Wraparound Horizon
                let mut freeze_lines = Vec::new();
                freeze_lines.push(Line::from(vec![
                    Span::styled(format!("{:<20}", "DATABASE"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>14}", "XID AGE"), theme.style(Token::Muted)),
                    Span::styled(format!("{:>12}", "HORIZON %"), theme.style(Token::Muted)),
                    Span::styled(format!(" {:<16}", "STATUS BAR"), theme.style(Token::Muted)),
                    Span::styled(
                        format!("{:<24}", " OLDEST TABLE"),
                        theme.style(Token::Muted),
                    ),
                    Span::styled(
                        format!("{:>14}", "TABLE XID AGE"),
                        theme.style(Token::Muted),
                    ),
                ]));

                for f in &report.xid_freeze {
                    let pct = f.wraparound_pct;
                    let bar_units = ((pct / 100.0) * 12.0).round() as usize;
                    let filled = "=".repeat(bar_units.min(12));
                    let empty = ".".repeat(12usize.saturating_sub(bar_units));
                    let bar_str = format!("[{filled}{empty}]");
                    let status_tok = if pct >= 80.0 {
                        Token::Danger
                    } else if pct >= 50.0 {
                        Token::Warning
                    } else {
                        Token::Success
                    };

                    freeze_lines.push(Line::from(vec![
                        Span::styled(format!("{:<20}", f.database_name), theme.style(Token::Text)),
                        Span::styled(
                            format!("{:>14}", f.datfrozenxid_age),
                            theme.style(Token::Text),
                        ),
                        Span::styled(format!("{:>11.2}%", pct), theme.style(status_tok)),
                        Span::styled(format!(" {:<16}", bar_str), theme.style(status_tok)),
                        Span::styled(
                            format!("{:<24}", f.oldest_table_name.as_deref().unwrap_or("-")),
                            theme.style(Token::Text),
                        ),
                        Span::styled(
                            format!("{:>14}", f.oldest_table_age),
                            theme.style(Token::Text),
                        ),
                    ]));
                }
                Paragraph::new(freeze_lines).render(freeze_area, buf);
            }
            2 => {
                // Tab 2: Replication & Standbys
                if report.standbys.is_empty() {
                    let lines = vec![
                        Line::from(""),
                        Line::from(Span::styled(
                            "No active streaming replication standbys connected (standalone database node).",
                            theme.style(Token::Muted),
                        )),
                        Line::from(""),
                        Line::from(Span::styled(
                            "Query `pg_stat_replication` returned zero rows. When read replicas or standby nodes connect, their LSN lag and sync states will appear here.",
                            theme.style(Token::Muted),
                        )),
                    ];
                    Paragraph::new(lines).render(body_area, buf);
                } else {
                    let split = Layout::default()
                        .direction(ratatui::layout::Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(55),
                            Constraint::Length(1), // Divider
                            Constraint::Percentage(45),
                        ])
                        .split(body_area);

                    let list_area = split[0];
                    let div_area = split[1];
                    let detail_area = split[2];

                    for y in 0..div_area.height {
                        buf.set_string(div_area.x, div_area.y + y, "│", theme.style(Token::Border));
                    }

                    // Standbys Table
                    let mut list_lines = Vec::new();
                    list_lines.push(Line::from(vec![
                        Span::styled(format!("{:<18}", "CLIENT"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<18}", "APPLICATION"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<10}", "SYNC"), theme.style(Token::Muted)),
                        Span::styled(format!("{:<12}", "STATE"), theme.style(Token::Muted)),
                        Span::styled(format!("{:>14}", "REPLAY LAG"), theme.style(Token::Muted)),
                    ]));

                    let sel_idx = state
                        .selected_idx
                        .min(report.standbys.len().saturating_sub(1));
                    for (i, s) in report.standbys.iter().enumerate() {
                        let is_sel = i == sel_idx;
                        let prefix = if is_sel { "> " } else { "  " };
                        let client_str =
                            format!("{prefix}{}", s.client_addr.as_deref().unwrap_or("local"));
                        let row_tok = if is_sel { Token::Focus } else { Token::Text };

                        list_lines.push(Line::from(vec![
                            Span::styled(format!("{:<18}", client_str), theme.style(row_tok)),
                            Span::styled(
                                format!("{:<18}", s.application_name),
                                theme.style(Token::Text),
                            ),
                            Span::styled(format!("{:<10}", s.sync_state), theme.style(Token::Text)),
                            Span::styled(format!("{:<12}", s.state), theme.style(Token::Success)),
                            Span::styled(
                                format!(
                                    "{:>14}",
                                    crate::telemetry::format_bytes(s.replay_lag_bytes)
                                ),
                                theme.style(Token::Text),
                            ),
                        ]));
                    }
                    Paragraph::new(list_lines).render(list_area, buf);

                    // Standby Detail Inspector
                    if let Some(s) = report.standbys.get(sel_idx) {
                        let mut det_lines = Vec::new();
                        det_lines.push(Line::from(Span::styled(
                            "STANDBY NODE INSPECTOR",
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )));
                        det_lines.push(Line::from(""));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Process ID (PID):"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(format!("{}", s.pid), theme.style(Token::Text)),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Client Host:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(
                                s.client_addr.as_deref().unwrap_or("local"),
                                theme.style(Token::Focus),
                            ),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Application Name:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(&s.application_name, theme.style(Token::Text)),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Replication State:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(&s.state, theme.style(Token::Success)),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Sync State:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(&s.sync_state, theme.style(Token::Text)),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(format!("{:<20}", "Sent LSN:"), theme.style(Token::Muted)),
                            Span::styled(
                                s.sent_lsn.as_deref().unwrap_or("-"),
                                theme.style(Token::Text),
                            ),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Write Lag:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(
                                crate::telemetry::format_bytes(s.write_lag_bytes),
                                theme.style(Token::Text),
                            ),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Flush Lag:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(
                                crate::telemetry::format_bytes(s.flush_lag_bytes),
                                theme.style(Token::Text),
                            ),
                        ]));
                        det_lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20}", "Replay Lag:"),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(
                                crate::telemetry::format_bytes(s.replay_lag_bytes),
                                theme.style(Token::Text),
                            ),
                        ]));
                        if let Some(ms) = s.replay_lag_ms {
                            det_lines.push(Line::from(vec![
                                Span::styled(
                                    format!("{:<20}", "Replay Latency:"),
                                    theme.style(Token::Muted),
                                ),
                                Span::styled(format!("{:.1} ms", ms), theme.style(Token::Text)),
                            ]));
                        }
                        Paragraph::new(det_lines).render(detail_area, buf);
                    }
                }
            }
            _ => {}
        }
    }

    // Status Message Line
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(msg, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Bar
    let footer_spans = vec![
        Span::styled(
            "[1/2/3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓/j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Renders the interactive Connection Pool & PgBouncer Telemetry overlay.
pub(crate) fn render_connection_pool(
    state: &crate::app::model::ConnectionPoolState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} CONNECTION POOL & PGBOUNCER TELEMETRY ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Tab selectors
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let tab_area = chunks[1];
    let body_area = chunks[3];
    let status_area = chunks[4];
    let footer_area = chunks[5];

    let report_opt = state.report.as_ref();
    let is_pgbouncer = report_opt.is_some_and(|r| r.is_pgbouncer);
    let total_conns = report_opt.map_or(0, |r| r.server_stats.total_connections);
    let max_conns = report_opt.map_or(0, |r| r.server_stats.max_connections);
    let headroom = report_opt.map_or(0, |r| r.server_stats.headroom());
    let saturation = report_opt.map_or(0.0, |r| r.server_stats.saturation_ratio());
    let active_conns = report_opt.map_or(0, |r| r.server_stats.active);
    let idle_conns = report_opt.map_or(0, |r| r.server_stats.idle);
    let wait_clients_cnt = report_opt.map_or(0, |r| r.waiting_clients.len());
    let pools_cnt = report_opt.map_or(0, |r| r.pools.len());

    let sat_token = if saturation < 70.0 {
        Token::Success
    } else if saturation < 85.0 {
        Token::Focus
    } else {
        Token::Danger
    };

    let headroom_token = if headroom > 20 {
        Token::Success
    } else if headroom > 5 {
        Token::Focus
    } else {
        Token::Danger
    };

    let queue_token = if wait_clients_cnt == 0 {
        Token::Success
    } else {
        Token::Danger
    };

    // Dieter Rams hardware capsules
    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("MODE: ", theme.style(Token::Muted)),
        Span::styled(
            if is_pgbouncer {
                "PgBouncer"
            } else {
                "Direct PostgreSQL"
            },
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("SATURATION: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{saturation:.1}% ({total_conns}/{max_conns})"),
            theme.style(sat_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("HEADROOM: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{headroom}"),
            theme.style(headroom_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("ACTIVE: ", theme.style(Token::Muted)),
        Span::styled(format!("{active_conns}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("IDLE: ", theme.style(Token::Muted)),
        Span::styled(format!("{idle_conns}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("WAIT QUEUE: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{wait_clients_cnt}"),
            theme.style(queue_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("POOLS: ", theme.style(Token::Muted)),
        Span::styled(format!("{pools_cnt}"), theme.style(Token::Text)),
        Span::styled("] ", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Tab Bar
    let tab_titles = [
        format!(" [1] Pool Allocations & Queues ({pools_cnt}) "),
        " [2] Server Connections & Saturation ".to_string(),
        format!(" [3] Client Wait Forensics ({wait_clients_cnt}) "),
    ];
    let mut tab_spans = Vec::new();
    for (i, t) in tab_titles.iter().enumerate() {
        if i == state.active_tab {
            tab_spans.push(Span::styled(
                t,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(t, theme.style(Token::Muted)));
        }
        tab_spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    if state.loading {
        let loading_p = Paragraph::new(Line::from(vec![Span::styled(
            "  Polling connection pool and PostgreSQL telemetry...",
            theme.style(Token::Muted),
        )]));
        loading_p.render(body_area, buf);
    } else if let Some(report) = report_opt {
        match state.active_tab {
            0 => {
                // Tab 0: Pool Allocations & Queues
                if report.pools.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "  No pools detected.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let header_line = Line::from(vec![
                        Span::styled(
                            format!("{:<20} ", "DATABASE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<16} ", "USER"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>8} ", "CL_ACT"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>8} ", "CL_WAIT"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>8} ", "SV_ACT"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>8} ", "SV_IDLE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>8} ", "SV_USED"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>10} ", "MAXWAIT(S)"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<10}", "MODE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    let mut lines = vec![header_line];
                    for (idx, p) in report.pools.iter().enumerate() {
                        let is_selected = idx == state.selected_idx;
                        let line_style = if is_selected {
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };
                        let wait_style = if p.cl_waiting > 0 {
                            theme.style(Token::Danger).add_modifier(Modifier::BOLD)
                        } else {
                            line_style
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<20} ", crate::pool::truncate_str(&p.database, 20)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<16} ", crate::pool::truncate_str(&p.user, 16)),
                                line_style,
                            ),
                            Span::styled(format!("{:>8} ", p.cl_active), line_style),
                            Span::styled(format!("{:>8} ", p.cl_waiting), wait_style),
                            Span::styled(format!("{:>8} ", p.sv_active), line_style),
                            Span::styled(format!("{:>8} ", p.sv_idle), line_style),
                            Span::styled(format!("{:>8} ", p.sv_used), line_style),
                            Span::styled(format!("{:>10.2} ", p.maxwait_seconds), line_style),
                            Span::styled(format!("{:<10}", p.pool_mode), line_style),
                        ]));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            1 => {
                // Tab 1: Server Connections & Saturation
                let s = &report.server_stats;
                let bar_width = 30;
                let filled = ((s.saturation_ratio() / 100.0) * bar_width as f64).round() as usize;
                let filled_clamped = filled.min(bar_width);
                let empty = bar_width.saturating_sub(filled_clamped);
                let progress_bar = format!("[{}{}]", "=".repeat(filled_clamped), ".".repeat(empty));

                let lines = vec![
                    Line::from(vec![
                        Span::styled(
                            "Connection Saturation Bar: ",
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            progress_bar,
                            theme.style(sat_token).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("  {:.1}% used", s.saturation_ratio()),
                            theme.style(sat_token),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Total Established Connections: ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}", s.total_connections),
                            theme.style(Token::Text).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "  /  Configured Max Connections: ",
                            theme.style(Token::Muted),
                        ),
                        Span::styled(
                            format!("{}", s.max_connections),
                            theme.style(Token::Text).add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Superuser Reserved Slots:      ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}", s.superuser_reserved_connections),
                            theme.style(Token::Text),
                        ),
                        Span::styled(
                            "  (Reserved for maintenance and dba recovery)",
                            theme.style(Token::Muted),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Usable Application Limit:      ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}", s.usable_max_connections()),
                            theme.style(Token::Text),
                        ),
                        Span::styled("  | Available Headroom: ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}", s.headroom()),
                            theme.style(headroom_token).add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![Span::styled(
                        "Connection States Breakdown:",
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    )]),
                    Line::from(vec![
                        Span::styled("  • Active Query Executing:    ", theme.style(Token::Muted)),
                        Span::styled(format!("{}", s.active), theme.style(Token::Text)),
                    ]),
                    Line::from(vec![
                        Span::styled("  • Idle (Outside Transaction):", theme.style(Token::Muted)),
                        Span::styled(format!("{}", s.idle), theme.style(Token::Text)),
                    ]),
                    Line::from(vec![
                        Span::styled("  • Idle in Open Transaction:  ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}", s.idle_in_transaction),
                            if s.idle_in_transaction > 0 {
                                theme.style(Token::Warning)
                            } else {
                                theme.style(Token::Text)
                            },
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  • Idle in Aborted Xact:      ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}", s.idle_in_transaction_aborted),
                            if s.idle_in_transaction_aborted > 0 {
                                theme.style(Token::Danger).add_modifier(Modifier::BOLD)
                            } else {
                                theme.style(Token::Text)
                            },
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("  • Fastpath / Disabled Slots: ", theme.style(Token::Muted)),
                        Span::styled(
                            format!("{}/{}", s.fastpath, s.disabled),
                            theme.style(Token::Text),
                        ),
                    ]),
                ];
                Paragraph::new(lines).render(body_area, buf);
            }
            2 => {
                // Tab 2: Client Wait Forensics (Dual Pane Table + Detail)
                if report.waiting_clients.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "  No client connections currently blocked or waiting on pool resources.",
                        theme.style(Token::Success),
                    )))
                    .render(body_area, buf);
                } else {
                    let split_chunks = Layout::default()
                        .direction(ratatui::layout::Direction::Horizontal)
                        .constraints([
                            Constraint::Percentage(55), // Table
                            Constraint::Percentage(45), // Detail Inspector
                        ])
                        .split(body_area);

                    let left_area = split_chunks[0];
                    let right_area = split_chunks[1];

                    // Left Table
                    let header_line = Line::from(vec![
                        Span::styled(
                            format!("{:<7} ", "PID"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<12} ", "DATABASE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<12} ", "USER"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<12} ", "WAIT TYPE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>8}", "DUR(S)"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    let mut lines = vec![header_line];
                    for (idx, w) in report.waiting_clients.iter().enumerate() {
                        let is_selected = idx == state.selected_idx;
                        let line_style = if is_selected {
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };
                        lines.push(Line::from(vec![
                            Span::styled(format!("{:<7} ", w.pid), line_style),
                            Span::styled(
                                format!("{:<12} ", crate::pool::truncate_str(&w.database, 12)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<12} ", crate::pool::truncate_str(&w.user, 12)),
                                line_style,
                            ),
                            Span::styled(
                                format!(
                                    "{:<12} ",
                                    crate::pool::truncate_str(
                                        w.wait_event_type.as_deref().unwrap_or("-"),
                                        12
                                    )
                                ),
                                line_style,
                            ),
                            Span::styled(format!("{:>8.1}", w.duration_seconds), line_style),
                        ]));
                    }
                    Paragraph::new(lines).render(left_area, buf);

                    // Right Detail Inspector
                    if let Some(selected) = report.waiting_clients.get(state.selected_idx) {
                        let det_lines = vec![
                            Line::from(vec![
                                Span::styled("PID: ", theme.style(Token::Muted)),
                                Span::styled(
                                    format!("{}", selected.pid),
                                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled("  Database: ", theme.style(Token::Muted)),
                                Span::styled(&selected.database, theme.style(Token::Text)),
                            ]),
                            Line::from(vec![
                                Span::styled("User: ", theme.style(Token::Muted)),
                                Span::styled(&selected.user, theme.style(Token::Text)),
                                Span::styled("  App: ", theme.style(Token::Muted)),
                                Span::styled(&selected.application_name, theme.style(Token::Text)),
                            ]),
                            Line::from(vec![
                                Span::styled("Client Addr: ", theme.style(Token::Muted)),
                                Span::styled(
                                    selected.client_addr.as_deref().unwrap_or("local"),
                                    theme.style(Token::Text),
                                ),
                            ]),
                            Line::from(vec![
                                Span::styled("Wait Event: ", theme.style(Token::Muted)),
                                Span::styled(
                                    format!(
                                        "{} / {}",
                                        selected.wait_event_type.as_deref().unwrap_or("-"),
                                        selected.wait_event.as_deref().unwrap_or("-")
                                    ),
                                    theme.style(Token::Danger).add_modifier(Modifier::BOLD),
                                ),
                            ]),
                            Line::from(vec![
                                Span::styled("Wait Duration: ", theme.style(Token::Muted)),
                                Span::styled(
                                    format!("{:.2} seconds", selected.duration_seconds),
                                    theme.style(Token::Focus),
                                ),
                            ]),
                            Line::from(""),
                            Line::from(Span::styled(
                                "Current Query:",
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            )),
                            Line::from(Span::styled(&selected.query, theme.style(Token::Text))),
                        ];
                        Paragraph::new(det_lines).render(right_area, buf);
                    }
                }
            }
            _ => {}
        }
    }

    // Status Message Line
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(msg, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Bar
    let footer_spans = vec![
        Span::styled(
            "[1/2/3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓/j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[c] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Copy SQL  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Renders the interactive Data Masking & Anonymization Rules overlay.
pub(crate) fn render_data_masking(
    state: &crate::app::model::DataMaskingState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} DATA MASKING & ANONYMIZATION RULES ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Tab selectors
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let tab_area = chunks[1];
    let body_area = chunks[3];
    let status_area = chunks[4];
    let footer_area = chunks[5];

    let report_opt = state.report.as_ref();
    let candidates_cnt = report_opt.map_or(0, |r| r.candidates.len());
    let policies_cnt = report_opt.map_or(0, |r| r.policies.len());

    let cand_token = if candidates_cnt > 0 {
        Token::Warning
    } else {
        Token::Success
    };

    // Dieter Rams hardware capsules
    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("CANDIDATES: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{candidates_cnt}"),
            theme.style(cand_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("POLICIES: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{policies_cnt} tables"),
            theme.style(Token::Text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("VIEW PREFIX: ", theme.style(Token::Muted)),
        Span::styled(
            &state.view_prefix,
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("ENGINE: ", theme.style(Token::Muted)),
        Span::styled("Virtual Views Overlay", theme.style(Token::Success)),
        Span::styled("] ", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Tab Bar
    let tab_titles = [
        format!(" [1] Sensitive Columns & Rules ({candidates_cnt}) "),
        format!(" [2] Generated Masked Views SQL ({policies_cnt}) "),
    ];
    let mut tab_spans = Vec::new();
    for (i, t) in tab_titles.iter().enumerate() {
        if i == state.active_tab {
            tab_spans.push(Span::styled(
                t,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(t, theme.style(Token::Muted)));
        }
        tab_spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    if state.loading {
        let loading_p = Paragraph::new(Line::from(vec![Span::styled(
            "  Scanning catalog for sensitive columns and analyzing masking policies...",
            theme.style(Token::Muted),
        )]));
        loading_p.render(body_area, buf);
    } else if let Some(report) = report_opt {
        match state.active_tab {
            0 => {
                // Tab 0: Sensitive Columns & Rules
                if report.candidates.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "  No sensitive PII columns detected in analyzed schema.",
                        theme.style(Token::Success),
                    )))
                    .render(body_area, buf);
                } else {
                    let header_line = Line::from(vec![
                        Span::styled(
                            format!("{:<14} ", "SCHEMA"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<18} ", "TABLE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<18} ", "COLUMN"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<12} ", "DATA TYPE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<14} ", "PII CATEGORY"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>6} ", "CONF"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<14}", "ASSIGNED RULE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    let mut lines = vec![header_line];
                    for (idx, c) in report.candidates.iter().enumerate() {
                        let is_selected = idx == state.selected_idx;
                        let line_style = if is_selected {
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };
                        let rule_style = if is_selected {
                            theme.style(Token::Success).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Focus)
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<14} ", crate::mask::truncate_str(&c.schema, 14)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<18} ", crate::mask::truncate_str(&c.table, 18)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<18} ", crate::mask::truncate_str(&c.column, 18)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<12} ", crate::mask::truncate_str(&c.data_type, 12)),
                                line_style,
                            ),
                            Span::styled(format!("{:<14} ", c.pii_type.label()), line_style),
                            Span::styled(
                                format!("{:>5}% ", c.confidence),
                                theme.style(Token::Muted),
                            ),
                            Span::styled(format!("{:<14}", c.rule.name()), rule_style),
                        ]));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            1 => {
                // Tab 1: Generated Masked Views SQL
                let sql = report.render_sql(&state.view_prefix);
                let sql_lines: Vec<Line> = sql
                    .lines()
                    .take(body_area.height as usize)
                    .map(|l| {
                        if l.starts_with("--") {
                            Line::from(Span::styled(l, theme.style(Token::Muted)))
                        } else if l.starts_with("CREATE")
                            || l.starts_with("SELECT")
                            || l.starts_with("FROM")
                        {
                            Line::from(Span::styled(
                                l,
                                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                            ))
                        } else {
                            Line::from(Span::styled(l, theme.style(Token::Text)))
                        }
                    })
                    .collect();
                Paragraph::new(sql_lines).render(body_area, buf);
            }
            _ => {}
        }
    }

    // Status Message Line
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(msg, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Bar
    let footer_spans = vec![
        Span::styled(
            "[1/2] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓/j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[Space/Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Cycle Rule  ", theme.style(Token::Muted)),
        Span::styled(
            "[c] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Copy SQL  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

pub(crate) fn render_data_sync(
    state: &crate::app::model::DataSyncState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} CROSS-DATABASE COPY & SYNC PIPELINES ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Tab selectors
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let tab_area = chunks[1];
    let body_area = chunks[3];
    let status_area = chunks[4];
    let footer_area = chunks[5];

    let plan_opt = state.plan.as_ref();
    let tables_cnt = plan_opt.map_or(0, |p| p.insert_order.len());
    let has_cycles = plan_opt.is_some_and(|p| p.has_cycles);
    let cycle_token = if has_cycles {
        Token::Warning
    } else {
        Token::Success
    };
    let cycle_label = if has_cycles {
        "CYCLES DETECTED"
    } else {
        "STRICT DAG"
    };

    let mode_token = if state.dry_run {
        Token::Success
    } else {
        Token::Warning
    };
    let mode_label = if state.dry_run {
        "DRY-RUN (SAFE)"
    } else {
        "LIVE STREAM"
    };

    let trunc_label = if state.truncate { "ENABLED" } else { "OFF" };

    // Dieter Rams hardware capsules
    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("TABLES: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{tables_cnt}"),
            theme.style(Token::Text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("TOPOLOGY: ", theme.style(Token::Muted)),
        Span::styled(
            cycle_label,
            theme.style(cycle_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("MODE: ", theme.style(Token::Muted)),
        Span::styled(
            mode_label,
            theme.style(mode_token).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("TRUNCATE: ", theme.style(Token::Muted)),
        Span::styled(
            trunc_label,
            theme.style(if state.truncate {
                Token::Warning
            } else {
                Token::Muted
            }),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("PROTOCOL: ", theme.style(Token::Muted)),
        Span::styled("Binary COPY Stream", theme.style(Token::Focus)),
        Span::styled("] ", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Tab Bar
    let report_tables_cnt = state
        .sync_report
        .as_ref()
        .map_or(0, |r| r.table_progress.len());
    let tab_titles = [
        format!(" [1] Dependency Graph & Sync Plan ({tables_cnt}) "),
        format!(" [2] Live Transfer Telemetry ({report_tables_cnt}) "),
    ];
    let mut tab_spans = Vec::new();
    for (i, t) in tab_titles.iter().enumerate() {
        if i == state.active_tab {
            tab_spans.push(Span::styled(
                t,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(t, theme.style(Token::Muted)));
        }
        tab_spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    if state.loading {
        let loading_p = Paragraph::new(Line::from(vec![Span::styled(
            "  Discovering foreign key constraints and computing topological DAG order...",
            theme.style(Token::Muted),
        )]));
        loading_p.render(body_area, buf);
    } else {
        match state.active_tab {
            0 => {
                // Tab 0: Dependency Graph & Sync Plan
                if let (Some(plan), Some(graph)) = (plan_opt, state.graph.as_ref()) {
                    if plan.insert_order.is_empty() {
                        Paragraph::new(Line::from(Span::styled(
                            "  No base tables discovered in analyzed catalog.",
                            theme.style(Token::Muted),
                        )))
                        .render(body_area, buf);
                    } else {
                        let header_line = Line::from(vec![
                            Span::styled(
                                format!("{:<4} ", "#"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<16} ", "SCHEMA"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<24} ", "TABLE"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<10} ", "IN-DEGREE"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<32}", "FOREIGN KEY DEPENDENCIES (PARENTS)"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                        ]);
                        let mut lines = vec![header_line];
                        for (idx, key) in plan.insert_order.iter().enumerate() {
                            let is_selected = idx == state.selected_idx;
                            let line_style = if is_selected {
                                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                            } else {
                                theme.style(Token::Text)
                            };

                            let node = graph.nodes.iter().find(|n| &n.key == key);
                            let parents_str = node.map_or(String::new(), |n| {
                                n.parents
                                    .iter()
                                    .map(|p| p.table.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            });
                            let in_degree = node.map_or(0, |n| n.parents.len());

                            lines.push(Line::from(vec![
                                Span::styled(format!("{:<4} ", idx + 1), theme.style(Token::Muted)),
                                Span::styled(
                                    format!("{:<16} ", crate::sync::truncate_str(&key.schema, 16)),
                                    line_style,
                                ),
                                Span::styled(
                                    format!("{:<24} ", crate::sync::truncate_str(&key.table, 24)),
                                    line_style,
                                ),
                                Span::styled(
                                    format!("{:<10} ", in_degree),
                                    if in_degree > 0 {
                                        theme.style(Token::Focus)
                                    } else {
                                        theme.style(Token::Muted)
                                    },
                                ),
                                Span::styled(
                                    crate::sync::truncate_str(&parents_str, 32),
                                    if in_degree > 0 {
                                        line_style
                                    } else {
                                        theme.style(Token::Muted)
                                    },
                                ),
                            ]));
                        }
                        Paragraph::new(lines).render(body_area, buf);
                    }
                }
            }
            1 => {
                // Tab 1: Live Transfer Telemetry
                if let Some(report) = &state.sync_report {
                    let header_line = Line::from(vec![
                        Span::styled(
                            format!("{:<16} ", "SCHEMA"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<24} ", "TABLE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<12} ", "STATUS"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>10} ", "ROWS"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>12} ", "BYTES"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>10}", "TIME(MS)"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    let mut lines = vec![header_line];
                    for (idx, p) in report.table_progress.iter().enumerate() {
                        let is_selected = idx == state.selected_idx;
                        let line_style = if is_selected {
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };
                        let status_style = match p.status {
                            crate::sync::SyncTableStatus::Completed => {
                                theme.style(Token::Success).add_modifier(Modifier::BOLD)
                            }
                            crate::sync::SyncTableStatus::Streaming => {
                                theme.style(Token::Warning).add_modifier(Modifier::BOLD)
                            }
                            crate::sync::SyncTableStatus::Failed => {
                                theme.style(Token::Danger).add_modifier(Modifier::BOLD)
                            }
                            crate::sync::SyncTableStatus::Pending => theme.style(Token::Muted),
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{:<16} ", crate::sync::truncate_str(&p.key.schema, 16)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<24} ", crate::sync::truncate_str(&p.key.table, 24)),
                                line_style,
                            ),
                            Span::styled(format!("{:<12} ", p.status.label()), status_style),
                            Span::styled(format!("{:>10} ", p.rows_copied), line_style),
                            Span::styled(
                                format!("{:>12} ", crate::sync::format_bytes(p.bytes_transferred)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:>10}", p.duration_ms),
                                theme.style(Token::Muted),
                            ),
                        ]));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                } else {
                    Paragraph::new(Line::from(Span::styled(
                        "  No sync pipeline execution has been triggered yet. Press [s] or pass to CLI.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                }
            }
            _ => {}
        }
    }

    // Status Message Line
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(msg, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Bar
    let footer_spans = vec![
        Span::styled(
            "[1/2] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓/j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[d] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Toggle Dry-Run  ", theme.style(Token::Muted)),
        Span::styled(
            "[t] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Toggle Truncate  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

pub(crate) fn render_synthetic_seed(
    state: &crate::app::model::SyntheticSeedState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} SYNTHETIC TEST FIXTURE GENERATOR ",
        presentation.icon(Icon::Database),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Tab selectors
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main workspace
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkey hints
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let tab_area = chunks[1];
    let body_area = chunks[3];
    let status_area = chunks[4];
    let footer_area = chunks[5];

    let tables_cnt = state.tables.len();
    let total_rows = state.report.as_ref().map_or(0, |r| r.total_rows);

    // Dieter Rams hardware status capsules
    let capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("TABLES: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{tables_cnt}"),
            theme.style(Token::Text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("ROWS/TABLE: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{}", state.rows_per_table),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("PRNG SEED: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{}", state.seed),
            theme.style(Token::Success).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("TOTAL ROWS: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{total_rows}"),
            theme.style(Token::Text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("INTEGRITY: ", theme.style(Token::Muted)),
        Span::styled("DAG Parent Keys", theme.style(Token::Focus)),
        Span::styled("] ", theme.style(Token::Border)),
    ];
    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Tab Bar
    let preview_cnt = state.report.as_ref().map_or(0, |r| r.tables.len());
    let tab_titles = [
        format!(" [1] Table Generator Strategies ({tables_cnt}) "),
        format!(" [2] Generated Fixture Preview ({preview_cnt}) "),
    ];
    let mut tab_spans = Vec::new();
    for (i, t) in tab_titles.iter().enumerate() {
        if i == state.active_tab {
            tab_spans.push(Span::styled(
                t,
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(t, theme.style(Token::Muted)));
        }
        tab_spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    if state.loading {
        let loading_p = Paragraph::new(Line::from(vec![Span::styled(
            "  Analyzing database catalog and inferring synthetic column strategies...",
            theme.style(Token::Muted),
        )]));
        loading_p.render(body_area, buf);
    } else {
        match state.active_tab {
            0 => {
                // Tab 0: Table Generator Strategies & Column Inferences
                if state.tables.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "  No tables discovered in analyzed catalog.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let split = Layout::default()
                        .direction(ratatui::layout::Direction::Horizontal)
                        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                        .split(body_area);

                    // Left side: Tables in topological insertion order
                    let table_header = Line::from(vec![
                        Span::styled(
                            format!("{:<4} ", "#"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<14} ", "SCHEMA"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:<18} ", "TABLE"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{:>6}", "COLS"),
                            theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                        ),
                    ]);
                    let mut table_lines = vec![table_header];
                    for (idx, tbl) in state.tables.iter().enumerate() {
                        let is_selected = idx == state.selected_idx;
                        let line_style = if is_selected {
                            theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                        } else {
                            theme.style(Token::Text)
                        };
                        table_lines.push(Line::from(vec![
                            Span::styled(format!("{:<4} ", idx + 1), theme.style(Token::Muted)),
                            Span::styled(
                                format!("{:<14} ", crate::sync::truncate_str(&tbl.key.schema, 14)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:<18} ", crate::sync::truncate_str(&tbl.key.table, 18)),
                                line_style,
                            ),
                            Span::styled(
                                format!("{:>6}", tbl.columns.len()),
                                theme.style(Token::Muted),
                            ),
                        ]));
                    }
                    Paragraph::new(table_lines).render(split[0], buf);

                    // Right side: Column strategy details for selected table
                    if let Some(selected_table) = state.tables.get(state.selected_idx) {
                        let col_header = Line::from(vec![
                            Span::styled(
                                format!("{:<16} ", "COLUMN"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<14} ", "TYPE"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<6} ", "PK"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{:<24}", "INFERRED STRATEGY"),
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            ),
                        ]);
                        let mut col_lines = vec![col_header];
                        for col in &selected_table.columns {
                            let pk_label = if col.is_primary_key { "YES" } else { "-" };
                            let pk_style = if col.is_primary_key {
                                theme.style(Token::Focus).add_modifier(Modifier::BOLD)
                            } else {
                                theme.style(Token::Muted)
                            };
                            col_lines.push(Line::from(vec![
                                Span::styled(
                                    format!("{:<16} ", crate::sync::truncate_str(&col.name, 16)),
                                    theme.style(Token::Text),
                                ),
                                Span::styled(
                                    format!(
                                        "{:<14} ",
                                        crate::sync::truncate_str(&col.data_type, 14)
                                    ),
                                    theme.style(Token::Muted),
                                ),
                                Span::styled(format!("{:<6} ", pk_label), pk_style),
                                Span::styled(
                                    crate::sync::truncate_str(col.strategy.name(), 24),
                                    theme.style(Token::Success),
                                ),
                            ]));
                        }
                        Paragraph::new(col_lines).render(split[1], buf);
                    }
                }
            }
            1 => {
                // Tab 1: Generated Fixture Preview
                if let Some(report) = &state.report {
                    if let Some(table_data) = report.tables.get(state.selected_idx) {
                        let col_headers: Vec<String> = table_data
                            .columns
                            .iter()
                            .map(|c| crate::sync::truncate_str(c, 16))
                            .collect();
                        let header_str = col_headers
                            .iter()
                            .map(|c| format!("{:<16}", c))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let mut lines = vec![
                            Line::from(Span::styled(
                                format!(
                                    "Table: {} ({} generated rows)",
                                    table_data.key.display(),
                                    table_data.rows.len()
                                ),
                                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                            )),
                            Line::from(Span::styled(
                                header_str,
                                theme.style(Token::Muted).add_modifier(Modifier::BOLD),
                            )),
                        ];

                        for row in table_data.rows.iter().take(12) {
                            let row_vals: Vec<String> = row
                                .iter()
                                .map(|v| crate::sync::truncate_str(v, 16))
                                .collect();
                            let row_str = row_vals
                                .iter()
                                .map(|v| format!("{:<16}", v))
                                .collect::<Vec<_>>()
                                .join(" ");
                            lines.push(Line::from(Span::styled(row_str, theme.style(Token::Text))));
                        }
                        if table_data.rows.len() > 12 {
                            lines.push(Line::from(Span::styled(
                                format!(
                                    "... and {} more rows (export to view all)",
                                    table_data.rows.len() - 12
                                ),
                                theme.style(Token::Muted),
                            )));
                        }
                        Paragraph::new(lines).render(body_area, buf);
                    } else {
                        Paragraph::new(Line::from(Span::styled(
                            "  Select a table in Tab 1 to inspect generated fixtures.",
                            theme.style(Token::Muted),
                        )))
                        .render(body_area, buf);
                    }
                } else {
                    Paragraph::new(Line::from(Span::styled(
                        "  Generating synthetic fixtures...",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                }
            }
            _ => {}
        }
    }

    // Status Message Line
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(msg, theme.style(Token::Focus))))
            .render(status_area, buf);
    }

    // Footer Hotkey Bar
    let footer_spans = vec![
        Span::styled(
            "[1/2] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[↑/↓/j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Navigate  ", theme.style(Token::Muted)),
        Span::styled(
            "[s] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Cycle Seed  ", theme.style(Token::Muted)),
        Span::styled(
            "[n] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Cycle Rows  ", theme.style(Token::Muted)),
        Span::styled(
            "[r] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Refresh  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

pub(crate) fn render_snippet_library(
    state: &crate::app::model::SnippetLibraryState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);

    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);

    let title = format!(
        " {} SQL SNIPPET & SCRATCHPAD LIBRARY ",
        presentation.icon(Icon::Editor),
    );
    overlay_block(title, presentation).render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Hardware capsules bar
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Main split workspace
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Footer hotkeys
        ])
        .split(inner);

    let capsule_area = chunks[0];
    let body_area = chunks[2];
    let status_area = chunks[3];
    let footer_area = chunks[4];

    let filtered = state.library.filtered_snippets();
    let total_count = state.library.total_count();

    // Dieter Rams hardware status capsules
    let mut capsules = vec![
        Span::styled(" [", theme.style(Token::Border)),
        Span::styled("SNIPPETS: ", theme.style(Token::Muted)),
        Span::styled(
            format!("{}/{}", filtered.len(), total_count),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
        Span::styled("[", theme.style(Token::Border)),
        Span::styled("CATEGORY: ", theme.style(Token::Muted)),
        Span::styled(
            state.library.category_filter.label(),
            theme.style(Token::Info).add_modifier(Modifier::BOLD),
        ),
        Span::styled("] ", theme.style(Token::Border)),
    ];

    if let Some(selected) = state.library.selected_snippet() {
        capsules.push(Span::styled("[", theme.style(Token::Border)));
        capsules.push(Span::styled("SCOPE: ", theme.style(Token::Muted)));
        let scope_label = if selected.is_builtin {
            "BUILT-IN"
        } else {
            "USER"
        };
        let scope_token = if selected.is_builtin {
            Token::Text
        } else {
            Token::Focus
        };
        capsules.push(Span::styled(
            scope_label,
            theme.style(scope_token).add_modifier(Modifier::BOLD),
        ));
        capsules.push(Span::styled("] ", theme.style(Token::Border)));
    }

    Paragraph::new(Line::from(capsules)).render(capsule_area, buf);

    // Split body into left (list) and right (preview)
    let body_split = Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(1), // Separator
            Constraint::Percentage(60),
        ])
        .split(body_area);

    let list_area = body_split[0];
    let sep_area = body_split[1];
    let preview_area = body_split[2];

    // Draw vertical separator
    let sep_char = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "│"
    };
    for y in sep_area.y..sep_area.y.saturating_add(sep_area.height) {
        buf.set_stringn(sep_area.x, y, sep_char, 1, theme.style(Token::Border));
    }

    // Left pane: snippet list
    let selected_idx = state
        .library
        .selected_idx
        .min(filtered.len().saturating_sub(1));
    let mut list_lines = Vec::new();

    if filtered.is_empty() {
        list_lines.push(Line::from(Span::styled(
            " No snippets in this category",
            theme.style(Token::Muted),
        )));
    } else {
        let max_visible = list_area.height as usize;
        let scroll = if selected_idx >= max_visible {
            selected_idx.saturating_sub(max_visible - 1)
        } else {
            0
        };

        for (i, snippet) in filtered.iter().enumerate().skip(scroll).take(max_visible) {
            let is_sel = i == selected_idx;
            let (badge, badge_token) = match snippet.category {
                crate::snippet::SnippetCategory::Admin => ("[ADM] ", Token::Warning),
                crate::snippet::SnippetCategory::Performance => ("[PRF] ", Token::Focus),
                crate::snippet::SnippetCategory::Patterns => ("[PAT] ", Token::Info),
                crate::snippet::SnippetCategory::User => ("[USR] ", Token::Success),
                crate::snippet::SnippetCategory::All => ("[ALL] ", Token::Muted),
            };

            let line_spans = if is_sel {
                vec![
                    Span::styled("> ", theme.style(Token::Focus).add_modifier(Modifier::BOLD)),
                    Span::styled(badge, theme.style(badge_token).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        &snippet.name,
                        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                    ),
                ]
            } else {
                vec![
                    Span::styled("  ", theme.style(Token::Muted)),
                    Span::styled(badge, theme.style(badge_token)),
                    Span::styled(&snippet.name, theme.style(Token::Text)),
                ]
            };
            list_lines.push(Line::from(line_spans));
        }
    }
    Paragraph::new(list_lines).render(list_area, buf);

    // Right pane: SQL preview and details
    if let Some(snippet) = state.library.selected_snippet() {
        let mut preview_lines = Vec::new();

        // Header
        preview_lines.push(Line::from(vec![
            Span::styled("TITLE: ", theme.style(Token::Muted)),
            Span::styled(
                &snippet.title,
                theme.style(Token::Header).add_modifier(Modifier::BOLD),
            ),
        ]));
        preview_lines.push(Line::from(vec![
            Span::styled("ABOUT: ", theme.style(Token::Muted)),
            Span::styled(&snippet.description, theme.style(Token::Info)),
        ]));
        preview_lines.push(Line::from(Span::styled(
            "──────────────────────────────────────────────",
            theme.style(Token::Border),
        )));

        // Syntax-highlighted SQL
        let syntax = crate::query::highlight::tokens(&snippet.sql);
        let mut start = 0usize;
        let available_sql_height = (preview_area.height as usize).saturating_sub(4);

        for (line_idx, source) in snippet.sql.lines().enumerate().take(available_sql_height) {
            let line_start = start;
            start += source.len() + 1;
            let line_nr = format!("{:>3} │ ", line_idx + 1);
            let mut spans = vec![Span::styled(line_nr, theme.style(Token::Muted))];

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
            preview_lines.push(Line::from(spans));
        }

        Paragraph::new(preview_lines).render(preview_area, buf);
    }

    // Status message
    if let Some(msg) = &state.status_message {
        Paragraph::new(Line::from(Span::styled(
            format!(" * {msg}"),
            theme.style(Token::Warning),
        )))
        .render(status_area, buf);
    }

    // Hotkey footer
    let footer_spans = vec![
        Span::styled(
            " [Enter] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Insert at Cursor  ", theme.style(Token::Muted)),
        Span::styled(
            "[n] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Open in Tab  ", theme.style(Token::Muted)),
        Span::styled(
            "[Tab] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Cycle Category  ", theme.style(Token::Muted)),
        Span::styled(
            "[a] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Add Buffer  ", theme.style(Token::Muted)),
        Span::styled(
            "[d] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Delete User  ", theme.style(Token::Muted)),
        Span::styled(
            "[Esc] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Close", theme.style(Token::Muted)),
    ];
    Paragraph::new(Line::from(footer_spans)).render(footer_area, buf);
}

/// Draws the offline SQL lint findings overlay.
pub(crate) fn render_lint_findings(
    state: &crate::app::model::LintFindingsState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let modal_width = (area.width.saturating_mul(84) / 100)
        .max(60)
        .min(area.width);
    let modal_height = (area.height.saturating_mul(70) / 100)
        .max(12)
        .min(area.height);
    let modal_area = Rect::new(
        area.x + (area.width.saturating_sub(modal_width)) / 2,
        area.y + (area.height.saturating_sub(modal_height)) / 2,
        modal_width,
        modal_height,
    );
    if modal_area.width < 30 || modal_area.height < 8 {
        return;
    }

    ratatui::widgets::Clear.render(modal_area, buf);
    overlay_block(" SQL LINT - OFFLINE ".to_owned(), presentation).render(modal_area, buf);
    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let errors = state
        .findings
        .iter()
        .filter(|finding| finding.severity == crate::lint::LintSeverity::Error)
        .count();
    let warnings = state
        .findings
        .iter()
        .filter(|finding| finding.severity == crate::lint::LintSeverity::Warning)
        .count();
    let header = format!(
        "[FINDINGS: {}] [ERRORS: {errors}] [WARNINGS: {warnings}]",
        state.findings.len()
    );
    Paragraph::new(Line::from(Span::styled(
        header,
        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
    )))
    .render(Rect::new(inner.x, inner.y, inner.width, 1), buf);

    let footer_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    let body_height = usize::from(inner.height.saturating_sub(3));
    let body_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(3),
    );

    if state.findings.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "No findings. The buffer is clean.",
            theme.style(Token::Success),
        )))
        .render(body_area, buf);
    } else {
        let mut lines = Vec::new();
        let start = state.selected.saturating_sub(body_height.saturating_sub(1));
        for (index, finding) in state
            .findings
            .iter()
            .enumerate()
            .skip(start)
            .take(body_height)
        {
            let selected = index == state.selected;
            let severity_style = match finding.severity {
                crate::lint::LintSeverity::Error => theme.style(Token::Danger),
                crate::lint::LintSeverity::Warning => theme.style(Token::Warning),
                crate::lint::LintSeverity::Info => theme.style(Token::Muted),
            };
            let marker = if selected { ">" } else { " " };
            let mut spans = vec![
                Span::styled(
                    format!("{marker} "),
                    theme.style(if selected {
                        Token::Selection
                    } else {
                        Token::Muted
                    }),
                ),
                Span::styled(
                    format!("{:<7} ", finding.severity.label()),
                    severity_style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}:{} ", finding.line, finding.column),
                    theme.style(Token::Muted),
                ),
                Span::styled(format!("{} ", finding.code), theme.style(Token::Info)),
                Span::styled(
                    sanitize_for_display(&finding.message),
                    if selected {
                        theme.style(Token::Text).add_modifier(Modifier::BOLD)
                    } else {
                        theme.style(Token::Text)
                    },
                ),
            ];
            if !finding.excerpt.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", sanitize_for_display(&finding.excerpt)),
                    theme.style(Token::Muted),
                ));
            }
            lines.push(Line::from(spans));
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(body_area, buf);
    }

    Paragraph::new(Line::from(vec![
        Span::styled("Enter ", theme.style(Token::Info)),
        Span::styled("jump  ", theme.style(Token::Muted)),
        Span::styled("r ", theme.style(Token::Info)),
        Span::styled("re-lint  ", theme.style(Token::Muted)),
        Span::styled("Esc ", theme.style(Token::Info)),
        Span::styled("close", theme.style(Token::Muted)),
    ]))
    .render(footer_area, buf);
}

#[cfg(test)]
mod tests;
