use super::{action_key_label, input_cursor, overlay_block, pane_block, syntax_token};
use crate::app::model::{Focus, Model};
use crate::query::value::{display_width, sanitize_for_display, truncate_to_width};
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
    let height = u16::try_from(chords.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let chord_area = overlay_box(
        area,
        presentation,
        (60, height),
        height.min(area.height),
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
pub(crate) fn render_help(
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let rows = keymap.bindings().len() + crate::ui::keymap::CHORDS.len() + 2;
    let height = u16::try_from(rows).unwrap_or(u16::MAX).saturating_add(3);
    let help_area = overlay_box(
        area,
        presentation,
        (70, height),
        height.min(area.height.saturating_sub(2)),
        OverlayAnchor::Center,
    );

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
