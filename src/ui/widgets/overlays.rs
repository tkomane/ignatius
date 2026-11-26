use super::{
    action_key_label, hairline_rule, highlighted_sql_spans, input_cursor, inset_empty_area,
    overlay_block, overlay_block_line, syntax_token,
};
use crate::app::model::{Focus, Model};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{OverlayAnchor, overlay_box, prompt_box};
use crate::ui::layout::{Presentation, definition_box, definition_viewport};
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
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

fn review_sql_row_mask(
    kind: crate::ui::review::Kind,
    rows: &[crate::ui::review::Row],
) -> Vec<bool> {
    let mut mask = vec![false; rows.len()];
    match kind {
        crate::ui::review::Kind::Update => {
            let mut in_bound = false;
            for (idx, row) in rows.iter().enumerate() {
                if row.token == Token::Header && row.text == "Exact bound statement:" {
                    in_bound = true;
                } else if row.token == Token::Info {
                    in_bound = false;
                } else if in_bound && row.token == Token::Text {
                    mask[idx] = true;
                }
            }
        }
        crate::ui::review::Kind::Production | crate::ui::review::Kind::Analyze => {
            for (idx, row) in rows.iter().enumerate() {
                if row.token == Token::Muted && row.text.starts_with("  ") {
                    mask[idx] = true;
                }
            }
        }
        crate::ui::review::Kind::Copy
        | crate::ui::review::Kind::Parameter
        | crate::ui::review::Kind::Import => {}
    }
    mask
}

fn styled_review_rail_spans(line: &str, base_style: Style, key_style: Style) -> Vec<Span<'static>> {
    const TOKENS: [&str; 9] = [
        "PageDown", "PageUp", "Ctrl+R", "Enter", "Home", "PgDn", "PgUp", "Esc", "End",
    ];
    let mut spans = Vec::new();
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        let prev_is_alnum = line[..i]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric);
        let matched = (!prev_is_alnum)
            .then(|| {
                TOKENS.iter().copied().find(|tok| {
                    line[i..].starts_with(tok)
                        && !line[i + tok.len()..]
                            .chars()
                            .next()
                            .is_some_and(char::is_alphanumeric)
                })
            })
            .flatten();
        if let Some(tok) = matched {
            if i > cursor {
                spans.push(Span::styled(line[cursor..i].to_owned(), base_style));
            }
            spans.push(Span::styled(tok.to_owned(), key_style));
            i += tok.len();
            cursor = i;
        } else if let Some(ch) = line[i..].chars().next() {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    if cursor < line.len() {
        spans.push(Span::styled(line[cursor..].to_owned(), base_style));
    }
    spans
}

/// Draws precisely the rows and fixed interaction areas used for admission.
pub(crate) fn render_sending_review(
    model: &Model,
    kind: crate::ui::review::Kind,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let Some(view) = crate::ui::review::build(model, kind, presentation, area) else {
        return;
    };
    let theme = &presentation.theme;
    ratatui::widgets::Clear.render(view.outer, buf);
    let title_icon = match kind {
        crate::ui::review::Kind::Production
        | crate::ui::review::Kind::Update
        | crate::ui::review::Kind::Analyze
        | crate::ui::review::Kind::Import => Icon::Warning,
        crate::ui::review::Kind::Copy | crate::ui::review::Kind::Parameter => Icon::Info,
    };
    let title_line = Line::from(Span::styled(
        format!("{}{}", presentation.icon(title_icon), view.title),
        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
    ));
    overlay_block_line(title_line, presentation).render(view.outer, buf);
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
            .style(theme.style(Token::Warning))
            .render(inner, buf);
        }
        return;
    }
    let sql_mask = review_sql_row_mask(kind, &view.rows);
    let window_start = view.offset.min(view.rows.len());
    let body_width = usize::from(view.body.width);
    let body = view
        .window()
        .iter()
        .enumerate()
        .map(|(offset_idx, row)| {
            let global_idx = window_start.saturating_add(offset_idx);
            if sql_mask.get(global_idx).copied().unwrap_or(false) {
                let mut spans = highlighted_sql_spans(row.text.as_str(), theme, true);
                let row_w = display_width(&row.text);
                if row_w < body_width {
                    spans.push(Span::styled(" ".repeat(body_width - row_w), theme.stripe()));
                }
                Line::from(spans).style(theme.stripe())
            } else {
                Line::from(Span::styled(row.text.as_str(), theme.style(row.token)))
            }
        })
        .collect::<Vec<_>>();
    Paragraph::new(body).render(view.body, buf);
    if let (Some(input), Some(input_area)) = (&view.input, view.input_area) {
        Paragraph::new(input.as_str())
            .style(theme.style(Token::Focus))
            .render(input_area, buf);
    }
    let rail_token = if view.ready {
        Token::Info
    } else {
        Token::Warning
    };
    let rail_width = usize::from(view.rail_area.width);
    let rail_base = theme.on_stripe(rail_token);
    let rail_key = theme.on_stripe(Token::Focus).add_modifier(Modifier::BOLD);
    let rail_lines = view
        .rail
        .iter()
        .map(|line| {
            let mut spans = styled_review_rail_spans(line, rail_base, rail_key);
            let line_w = display_width(line);
            if line_w < rail_width {
                spans.push(Span::styled(
                    " ".repeat(rail_width - line_w),
                    theme.stripe(),
                ));
            }
            Line::from(spans).style(theme.stripe())
        })
        .collect::<Vec<_>>();
    Paragraph::new(rail_lines)
        .style(theme.stripe())
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
        if prompt.patch.is_some() {
            "(empty input)"
        } else {
            "(empty string)"
        }
        .to_owned()
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
            prompt.patch.map_or(
                "The replacement is literal text. Empty text is valid; NULL is not a shorthand here.",
                |patch| patch.input.hint(),
            ),
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
    let (status_label, status_token) = match &model.plan.status {
        crate::app::PlanStatus::Loading { analyzed: true, .. } => ("  Measuring", Token::Info),
        crate::app::PlanStatus::Loading {
            analyzed: false, ..
        } => ("  Reading estimate", Token::Muted),
        crate::app::PlanStatus::Ready(document) if document.analyzed => {
            ("  EXPLAIN ANALYZE", Token::Info)
        }
        crate::app::PlanStatus::Ready(_) => ("  EXPLAIN", Token::Muted),
        crate::app::PlanStatus::Failed { analyzed: true, .. } => {
            ("  Analysis failed", Token::Danger)
        }
        crate::app::PlanStatus::Failed {
            analyzed: false, ..
        } => ("  Plan failed", Token::Danger),
        crate::app::PlanStatus::Hidden => ("", Token::Muted),
    };
    let focus_pulse = (focused && !model.reduced_motion && !presentation.reduced_motion)
        .then(|| model.motion().focus_pulse())
        .flatten();
    let focus_style = super::focus_accent_style(theme, focus_pulse).add_modifier(Modifier::BOLD);
    let mut title_spans = Vec::new();
    if model.splits.zoomed && focused {
        title_spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
    }
    let primary_style = if focused {
        focus_style
    } else {
        theme.style(Token::Header)
    };
    title_spans.push(Span::styled(
        format!("{}Plan", presentation.icon(Icon::Info)),
        primary_style,
    ));
    if !status_label.is_empty() {
        title_spans.push(Span::styled(status_label, theme.style(status_token)));
    }
    if focused {
        title_spans.push(Span::styled("  [focused]", focus_style));
    }
    let block = super::pane_block_line_with_motion(
        Line::from(title_spans),
        focused,
        focus_pulse,
        presentation,
    );
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    render_plan_body(model, keymap, presentation, inner, buf);
}

fn render_plan_body(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    inner: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;

    match &model.plan.status {
        crate::app::PlanStatus::Loading { analyzed, .. } => {
            let loading_area = inset_empty_area(inner);
            let keycap = theme.keycap(Token::Focus);
            let cancel_key = action_key_label(keymap, &crate::app::Action::Cancel);
            let leader = if presentation.reduced_motion {
                String::new()
            } else {
                format!("{} ", presentation.glyphs.spinner(model.frame))
            };
            let warn_comet = *analyzed
                || matches!(
                    model.phase,
                    crate::app::model::QueryPhase::CancellationRequested { .. }
                );
            let mut lines = if *analyzed {
                vec![Line::from(vec![
                    Span::styled(leader, theme.style(Token::Warning)),
                    Span::styled(
                        "Measuring the statement once with EXPLAIN ANALYZE.",
                        theme.style(Token::Warning),
                    ),
                ])]
            } else {
                vec![Line::from(vec![
                    Span::styled(leader, theme.style(Token::Info)),
                    Span::styled(
                        "Reading the planner estimate. The target statement is not executed.",
                        theme.style(Token::Info),
                    ),
                ])]
            };
            if !presentation.reduced_motion {
                lines.push(super::results::heartbeat_line(
                    model.frame,
                    warn_comet,
                    presentation,
                ));
            }
            lines.push(if *analyzed {
                Line::from(vec![
                    Span::styled(format!(" {cancel_key} "), keycap),
                    Span::styled(
                        "cancels the request; the statement may still be running.",
                        theme.style(Token::Info),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!(" {cancel_key} "), keycap),
                    Span::styled("cancels the request.", theme.style(Token::Muted)),
                ])
            });
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .render(loading_area, buf);
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
            let plan_sweep = (!model.reduced_motion && !presentation.reduced_motion)
                .then(|| model.motion().plan_sweep())
                .flatten();
            render_ready_plan(document, &model.plan, presentation, plan_sweep, inner, buf);
        }
        crate::app::PlanStatus::Hidden => {}
    }
}

fn render_ready_plan(
    document: &crate::query::PlanDocument,
    view: &crate::app::PlanView,
    presentation: &Presentation,
    plan_sweep: Option<f32>,
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

    let (kpi_area, summary_area, tree_area, detail_area) = plan_areas(area);
    if let Some(kpi) = kpi_area {
        render_plan_kpi_strip(document, presentation, kpi, buf);
    }
    render_plan_summary(&summary, document, presentation, summary_area, buf);
    let sweep = f64::from(plan_sweep.unwrap_or(1.0));
    render_plan_tree_with_sweep(document, view, presentation, sweep, tree_area, buf);
    if let Some(detail) = detail_area {
        render_plan_detail(document, view, presentation, detail, buf);
    }
}

/// Plan sections, shared with keyboard paging through the tree.
fn plan_areas(area: Rect) -> (Option<Rect>, Rect, Rect, Option<Rect>) {
    if area.height < 3 {
        return (None, area, Rect::default(), None);
    }
    if area.height >= 10 {
        let detail_height = 5.min(area.height.saturating_sub(4));
        let tree_height = area.height.saturating_sub(3 + detail_height);
        let [kpi, summary, tree, detail] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(tree_height),
            Constraint::Length(detail_height),
        ])
        .areas(area);
        (Some(kpi), summary, tree, Some(detail))
    } else if area.height >= 8 {
        let detail_height = 5.min(area.height.saturating_sub(3));
        let tree_height = area.height.saturating_sub(2 + detail_height);
        let [summary, tree, detail] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(tree_height),
            Constraint::Length(detail_height),
        ])
        .areas(area);
        (None, summary, tree, Some(detail))
    } else {
        let [summary, tree] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        (None, summary, tree, None)
    }
}

pub(crate) fn plan_capacity(pane: Rect) -> usize {
    let inner = super::inner_rect(pane);
    if inner.width == 0 {
        0
    } else {
        usize::from(plan_areas(inner).2.height)
    }
}

fn render_plan_kpi_strip(
    document: &crate::query::PlanDocument,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let sep = if unicode { " \u{00b7} " } else { " | " };
    let planning = document
        .planning_time_ms
        .map_or_else(|| "n/a".to_owned(), format_ms);
    let execution = if document.analyzed {
        document
            .execution_time_ms
            .map_or_else(|| "n/a".to_owned(), format_ms)
    } else {
        "estimate".to_owned()
    };
    let hotspot = if document.has_attention() {
        document
            .root
            .at_path(&document.attention_path)
            .map_or_else(|| "n/a".to_owned(), |node| safe_plan_text(&node.node_type))
    } else {
        "none".to_owned()
    };
    let hotspot_token = if document.has_attention() {
        Token::Warning
    } else {
        Token::Muted
    };
    let segments = vec![
        (" Nodes ".to_owned(), theme.on_stripe(Token::Muted)),
        (
            format!("{}", document.node_count),
            theme.on_stripe(Token::Header).add_modifier(Modifier::BOLD),
        ),
        (sep.to_owned(), theme.on_stripe(Token::Border)),
        ("Planning ".to_owned(), theme.on_stripe(Token::Muted)),
        (planning, theme.on_stripe(Token::Info)),
        (sep.to_owned(), theme.on_stripe(Token::Border)),
        ("Execution ".to_owned(), theme.on_stripe(Token::Muted)),
        (execution, theme.on_stripe(Token::Info)),
        (sep.to_owned(), theme.on_stripe(Token::Border)),
        (
            "Hotspot ".to_owned(),
            theme.on_stripe(hotspot_token).add_modifier(Modifier::BOLD),
        ),
        (hotspot, theme.on_stripe(hotspot_token)),
    ];
    let line = bounded_row(
        segments,
        usize::from(area.width),
        unicode,
        Some(theme.stripe()),
    );
    Paragraph::new(line).style(theme.stripe()).render(area, buf);
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

#[cfg(test)]
fn render_plan_tree(
    document: &crate::query::PlanDocument,
    view: &crate::app::PlanView,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_plan_tree_with_sweep(document, view, presentation, 1.0, area, buf);
}

fn render_plan_tree_with_sweep(
    document: &crate::query::PlanDocument,
    view: &crate::app::PlanView,
    presentation: &Presentation,
    sweep: f64,
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
        let segments = plan_node_segments(
            document,
            node,
            path,
            view.collapsed.contains(path),
            selected,
            usize::from(area.width),
            sweep,
            presentation,
        );
        let line = if selected {
            let selection = presentation.theme.style(Token::Selection);
            bounded_row(
                segments
                    .into_iter()
                    .map(|(text, _)| (text, selection))
                    .collect(),
                usize::from(area.width),
                !presentation.glyphs.is_ascii(),
                Some(selection),
            )
        } else {
            bounded_row(
                segments,
                usize::from(area.width),
                !presentation.glyphs.is_ascii(),
                None,
            )
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
    let theme = &presentation.theme;
    let selected_idx = view
        .visible_paths()
        .iter()
        .position(|path| path == &view.selected_path)
        .map_or(1, |index| index + 1);
    let selected_text = format!(
        "{}  ({}/{})",
        safe_plan_text(&node.node_type),
        selected_idx,
        view.visible_paths().len()
    );
    let mut header_spans = vec![
        Span::styled("Selected: ", theme.on_stripe(Token::Info)),
        Span::styled(selected_text.clone(), theme.on_stripe(Token::Header)),
    ];
    let header_w = "Selected: ".len() + display_width(&selected_text);
    let area_w = usize::from(area.width);
    if header_w < area_w {
        header_spans.push(Span::styled(" ".repeat(area_w - header_w), theme.stripe()));
    }
    let mut lines = vec![Line::from(header_spans).style(theme.stripe())];
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

/// One plan tree row as styled segments.
///
/// The row reads like the Objects tree: Border-toned `│` guides for depth, a
/// chevron only on nodes with children (`▾` open, `▸` closed, `v` and `>` in
/// ASCII), then the operation and a compact metric summary. Width, cost
/// units and loop timing in full stay in the detail block below the tree.
/// The selected row keeps its ASCII `>` marker so selection survives colour
/// loss, and attention and estimate-mismatch flags stay as words.
#[expect(
    clippy::too_many_arguments,
    reason = "plan row segments need document, node, path, collapsed/selected flags, width, sweep progress and presentation"
)]
fn plan_node_segments(
    document: &crate::query::PlanDocument,
    node: &crate::query::PlanNode,
    path: &[usize],
    collapsed: bool,
    selected: bool,
    row_width: usize,
    sweep: f64,
    presentation: &Presentation,
) -> Vec<(String, Style)> {
    let theme = &presentation.theme;
    let glyphs = &presentation.glyphs;
    let attention = document.is_attention_path(path);
    let flag_style = theme.style(Token::Warning);
    let marker = if selected { ">" } else { " " };
    let depth = path.len().min(10);
    let mut guides = glyphs.indent_guide().repeat(depth);
    if path.len() > depth {
        guides.push_str("... ");
    }
    let chevron = if node.children.is_empty() {
        "  ".to_owned()
    } else {
        format!("{} ", glyphs.chevron(!collapsed))
    };
    let subject_token = if attention {
        Token::Warning
    } else {
        Token::Header
    };
    let metrics_token = if attention || node.estimate_mismatch() {
        Token::Warning
    } else {
        Token::Muted
    };
    let unicode = !glyphs.is_ascii();
    let subject = safe_plan_text(&node_subject(node));
    let metrics = plan_row_metrics(node, document.analyzed, unicode);

    let mut badge_segments = Vec::new();
    if attention {
        badge_segments.push((
            format!(" [attention: {}]", document.attention_basis().label()),
            flag_style,
        ));
    }
    if node.estimate_mismatch() {
        badge_segments.push((" [estimate mismatch]".to_owned(), flag_style));
    }
    let badges_w: usize = badge_segments
        .iter()
        .map(|(text, _)| display_width(text))
        .sum();

    let bar_segments = if row_width >= 72
        && unicode
        && let Some(share) = node.share_of_root(&document.root, document.analyzed)
    {
        let scaled = share * sweep.clamp(0.0, 1.0);
        let mut filled = (scaled * 5.0).round().clamp(0.0, 5.0) as usize;
        if scaled > 0.0 && filled == 0 {
            filled = 1;
        }
        let bar_token = if attention || share >= 0.5 {
            Token::Warning
        } else {
            Token::Info
        };
        let mut bar = Vec::new();
        if filled > 0 {
            bar.push(("\u{2501}".repeat(filled), theme.style(bar_token)));
        }
        if filled < 5 {
            bar.push(("\u{2500}".repeat(5 - filled), theme.style(Token::Border)));
        }
        bar.push((" ".to_owned(), theme.style(Token::Muted)));
        Some(bar)
    } else {
        None
    };

    let prefix_w = display_width(marker) + display_width(&guides) + display_width(&chevron);
    let subject_w = display_width(&subject);
    let sep = " | ";
    let sep_w = display_width(sep);
    let metrics_w = display_width(&metrics);
    let bar_w: usize = bar_segments
        .as_ref()
        .map_or(0, |bar| bar.iter().map(|(s, _)| display_width(s)).sum());

    let fits_with_bar = prefix_w + subject_w + sep_w + bar_w + metrics_w + badges_w <= row_width;
    let fits_without_bar = prefix_w + subject_w + sep_w + metrics_w + badges_w <= row_width;

    let (rendered_subject, include_bar, rendered_metrics) = if fits_with_bar {
        (subject, bar_segments.is_some(), metrics)
    } else if fits_without_bar || badges_w == 0 || row_width < prefix_w + sep_w + badges_w + 16 {
        (subject, false, metrics)
    } else {
        let content_budget = row_width - prefix_w - sep_w - badges_w;
        let min_metrics = metrics_w.min(16);
        let subject_cap = subject_w.min(
            content_budget
                .saturating_sub(min_metrics)
                .max(content_budget.min(20)),
        );
        let metrics_cap = content_budget.saturating_sub(subject_cap);
        (
            truncate_to_width(&subject, subject_cap, unicode),
            false,
            truncate_to_width(&metrics, metrics_cap, unicode),
        )
    };

    let mut segments = vec![
        (marker.to_owned(), theme.style(Token::Focus)),
        (guides, theme.style(Token::Border)),
        (chevron, theme.style(Token::Muted)),
        (rendered_subject, theme.style(subject_token)),
        (sep.to_owned(), theme.style(Token::Border)),
    ];
    if include_bar && let Some(bar) = bar_segments {
        segments.extend(bar);
    }
    segments.push((rendered_metrics, theme.style(metrics_token)));
    segments.extend(badge_segments);
    segments
}

/// The compact metric summary a plan tree row carries.
///
/// A plain `EXPLAIN` row shows only what the planner believed. An analysed
/// row sets the estimate against what was observed, with the per-loop time
/// and the loop count when there was more than one loop. Anything the server
/// did not supply is said in words.
fn plan_row_metrics(node: &crate::query::PlanNode, analyzed: bool, unicode: bool) -> String {
    let separator = if unicode { " \u{00b7} " } else { ", " };
    let estimate = number_or(node.plan_rows, "not supplied");
    if !analyzed {
        let startup = number_or(node.startup_cost, "not supplied");
        let total = number_or(node.total_cost, "not supplied");
        return format!("est {estimate} rows{separator}cost {startup}..{total}");
    }
    let actual = number_or(node.actual_rows, "not measured");
    let time = node
        .actual_total_time
        .map_or_else(|| "time not measured".to_owned(), format_ms);
    let loops = node.actual_loops.filter(|loops| *loops > 1.0);
    match loops {
        Some(loops) => format!(
            "est {estimate} -> actual {actual} rows{separator}{time}/loop{separator}{} loops",
            format_number(loops)
        ),
        None => format!("est {estimate} -> actual {actual} rows{separator}{time}"),
    }
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
        let self_cost = node
            .exclusive_cost()
            .map_or_else(|| "not supplied".to_owned(), |c| format!("{c:.2}"));
        return format!("est rows {rows}; width {width}; {cost}; self cost {self_cost}");
    }
    let actual_rows = number_or(node.actual_rows, "not measured");
    let startup_time = node
        .actual_startup_time
        .map_or_else(|| "not measured".to_owned(), format_ms);
    let total_time = node
        .actual_total_time
        .map_or_else(|| "not measured".to_owned(), format_ms);
    let loops = number_or(node.actual_loops, "not measured");
    let self_time = node
        .exclusive_time_ms()
        .map_or_else(|| "not measured".to_owned(), format_ms);
    format!(
        "est rows {rows}; width {width}; actual rows {actual_rows}; actual startup {startup_time}; \
         actual total {total_time}/loop; loops {loops}; self time {self_time}; {cost}"
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
    let unicode = !presentation.glyphs.is_ascii();
    let chords = crate::ui::keymap::CHORDS;
    let single_col_height = u16::try_from(chords.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);

    let prefix = keymap
        .contextual_hint(&crate::app::Action::BeginPrefix)
        .map(|(key, _)| key)
        .unwrap_or_else(|| "Ctrl+K".to_owned());
    let title_line = Line::from(vec![
        Span::styled(
            presentation.icon(Icon::Key).to_owned(),
            theme.style(Token::Info),
        ),
        Span::styled(
            format!("{prefix} "),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("then", theme.style(Token::Muted)),
    ]);

    let push_keycap = |spans: &mut Vec<Span<'static>>, key: char| {
        if unicode {
            spans.push(Span::styled(
                format!(" {key} "),
                theme.recessed_pill(Token::Focus),
            ));
            spans.push(Span::raw(" "));
        } else {
            spans.push(Span::styled(
                format!(" {key}  "),
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
        }
    };

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
        if chord_area.width < 4 || chord_area.height < 3 {
            return;
        }
        ratatui::widgets::Clear.render(chord_area, buf);

        let inner_width = usize::from(chord_area.width.saturating_sub(2));
        let divider_width = 3usize;
        let col1_needed = col1
            .iter()
            .map(|(_, _, desc)| 4 + display_width(desc))
            .max()
            .unwrap_or(0);
        let col2_needed = col2
            .iter()
            .map(|(_, _, desc)| 4 + display_width(desc))
            .max()
            .unwrap_or(0);
        let even_col = inner_width.saturating_sub(divider_width) / 2;
        let col_width =
            even_col.min(col1_needed.max(inner_width.saturating_sub(divider_width + col2_needed)));
        let divider_str = format!(" {} ", presentation.glyphs.column_rule());
        let mut lines = Vec::new();
        for i in 0..split_at {
            let mut spans = Vec::new();
            if let Some((key, _, description)) = col1.get(i) {
                push_keycap(&mut spans, *key);
                let pad = col_width.saturating_sub(4);
                let text = format!("{:<pad$}", description);
                spans.push(Span::styled(text, theme.style(Token::Text)));
            }
            spans.push(Span::styled(
                divider_str.clone(),
                theme.style(Token::Border),
            ));
            if let Some((key, _, description)) = col2.get(i) {
                push_keycap(&mut spans, *key);
                spans.push(Span::styled(*description, theme.style(Token::Text)));
            }
            lines.push(Line::from(spans));
        }

        Paragraph::new(lines)
            .block(overlay_block_line(title_line, presentation))
            .render(chord_area, buf);
    } else {
        let chord_area = overlay_box(
            area,
            presentation,
            (60, single_col_height),
            single_col_height.min(area.height),
            OverlayAnchor::Bottom,
        );
        if chord_area.width < 4 || chord_area.height < 3 {
            return;
        }
        ratatui::widgets::Clear.render(chord_area, buf);

        let lines: Vec<Line> = chords
            .iter()
            .map(|(key, _, description)| {
                let mut spans = Vec::new();
                push_keycap(&mut spans, *key);
                spans.push(Span::styled(*description, theme.style(Token::Text)));
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines)
            .block(overlay_block_line(title_line, presentation))
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

    let show_summary_pills = !diagram.loading && diagram.error.is_none() && inner.height >= 4;
    let body_top = if show_summary_pills {
        let kpi_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let mut capsules = vec![Span::raw(" ")];
        push_kv_capsule(
            &mut capsules,
            presentation,
            "TABLES: ",
            diagram.graph.tables.len().to_string(),
            Token::Focus,
        );
        push_kv_capsule(
            &mut capsules,
            presentation,
            "EDGES: ",
            diagram.graph.edges.len().to_string(),
            Token::Info,
        );
        push_kv_capsule(
            &mut capsules,
            presentation,
            "DEPTH: ",
            diagram.depth.label().to_owned(),
            Token::Text,
        );
        push_kv_capsule(
            &mut capsules,
            presentation,
            "FORMAT: ",
            diagram.format.label().to_owned(),
            Token::Info,
        );
        render_capsule_bar(capsules, kpi_area, buf);
        inner.y.saturating_add(1)
    } else {
        inner.y
    };

    let body_height = inner
        .height
        .saturating_sub(1)
        .saturating_sub(u16::from(show_summary_pills));
    let body_area = Rect::new(inner.x, body_top, inner.width, body_height);
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
        let is_box_mode = diagram.format != crate::diagram::DiagramFormat::Mermaid;
        for (offset_idx, line) in diagram
            .rendered_lines
            .iter()
            .skip(diagram.scroll_y)
            .take(right_visible_rows)
            .enumerate()
        {
            let abs_idx = diagram.scroll_y.saturating_add(offset_idx);
            if is_box_mode {
                let prev = abs_idx
                    .checked_sub(1)
                    .and_then(|i| diagram.rendered_lines.get(i))
                    .map(String::as_str);
                let next = diagram.rendered_lines.get(abs_idx + 1).map(String::as_str);
                diagram_lines.push(style_er_diagram_line(
                    prev,
                    line,
                    next,
                    diagram.scroll_x,
                    usize::from(right_area.width),
                    theme,
                ));
            } else {
                let scrolled_line: String = line.chars().skip(diagram.scroll_x).collect();
                diagram_lines.push(Line::from(Span::styled(
                    scrolled_line,
                    theme.style(Token::Text),
                )));
            }
        }
    }
    Paragraph::new(diagram_lines).render(right_area, buf);

    let nav_key = if presentation.glyphs.is_ascii() {
        " [Up/Down] "
    } else {
        " [\u{2191}/\u{2193}] "
    };
    let footer_hints = vec![
        Span::styled(
            nav_key,
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

fn is_card_top_rule(line: &str) -> bool {
    let t = line.trim_start();
    (t.starts_with('\u{250c}') && t.ends_with('\u{2510}'))
        || (t.starts_with('+') && t.ends_with('+'))
}

fn is_card_mid_rule(line: &str) -> bool {
    let t = line.trim_start();
    (t.starts_with('\u{251c}') && t.ends_with('\u{2524}'))
        || (t.starts_with('+') && t.ends_with('+'))
}

fn is_card_rule_line(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with('\u{250c}') && t.ends_with('\u{2510}'))
        || (t.starts_with('\u{251c}') && t.ends_with('\u{2524}'))
        || (t.starts_with('\u{2514}') && t.ends_with('\u{2518}'))
        || (t.starts_with('+') && t.ends_with('+') && t.contains('-'))
}

fn slice_styled_spans(
    spans: Vec<Span<'static>>,
    scroll_x: usize,
    max_width: usize,
    line_style: Option<Style>,
) -> Line<'static> {
    let mut styled_chars: Vec<(char, Style)> = Vec::new();
    for span in spans {
        for ch in span.content.chars() {
            styled_chars.push((ch, span.style));
        }
    }
    let mut out_spans = Vec::new();
    let mut run = String::new();
    let mut run_style: Option<Style> = None;
    for (ch, style) in styled_chars.into_iter().skip(scroll_x).take(max_width) {
        if run_style != Some(style) {
            if let Some(prev) = run_style {
                out_spans.push(Span::styled(std::mem::take(&mut run), prev));
            }
            run_style = Some(style);
        }
        run.push(ch);
    }
    if let Some(prev) = run_style {
        out_spans.push(Span::styled(run, prev));
    }
    let line = Line::from(out_spans);
    match line_style {
        Some(style) => line.style(style),
        None => line,
    }
}

fn style_er_diagram_line(
    prev: Option<&str>,
    line: &str,
    next: Option<&str>,
    scroll_x: usize,
    right_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let clean = sanitize_for_display(line);
    if is_card_rule_line(&clean) {
        return slice_styled_spans(
            vec![Span::styled(clean, theme.style(Token::Border))],
            scroll_x,
            right_width,
            None,
        );
    }

    let is_card_body = (clean.starts_with('\u{2502}') && clean.ends_with('\u{2502}'))
        || (clean.starts_with('|') && clean.ends_with('|'));
    if !is_card_body {
        let style = if clean.starts_with("Relationships") {
            theme.style(Token::Header).add_modifier(Modifier::BOLD)
        } else if clean.starts_with("Schema ER Diagram") {
            theme.style(Token::Info)
        } else {
            theme.style(Token::Text)
        };
        return slice_styled_spans(
            vec![Span::styled(clean, style)],
            scroll_x,
            right_width,
            None,
        );
    }

    let mut chars = clean.chars();
    let Some(left_border) = chars.next() else {
        return Line::default();
    };
    let Some(right_border) = chars.next_back() else {
        return Line::default();
    };
    let interior: String = chars.collect();

    let is_table_header = prev.is_some_and(is_card_top_rule) && next.is_some_and(is_card_mid_rule);
    if is_table_header {
        let border_style = theme.style(Token::Border);
        let header_style = theme.on_stripe(Token::Header).add_modifier(Modifier::BOLD);
        return slice_styled_spans(
            vec![
                Span::styled(left_border.to_string(), border_style),
                Span::styled(interior, header_style),
                Span::styled(right_border.to_string(), border_style),
            ],
            scroll_x,
            right_width,
            None,
        );
    }

    let border_style = theme.style(Token::Border);
    let mut spans = vec![Span::styled(left_border.to_string(), border_style)];
    let mut rest = interior.as_str();
    if let Some(after_lead) = rest.strip_prefix(' ') {
        spans.push(Span::raw(" "));
        rest = after_lead;
    }

    if let Some(tail) = rest.strip_prefix("PK,FK") {
        spans.push(Span::styled(
            "PK",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(",", border_style));
        spans.push(Span::styled(
            "FK",
            theme.style(Token::Info).add_modifier(Modifier::BOLD),
        ));
        rest = tail;
    } else if let Some(tail) = rest.strip_prefix("PK") {
        spans.push(Span::styled(
            "PK",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
        rest = tail;
    } else if let Some(tail) = rest.strip_prefix("FK") {
        spans.push(Span::styled(
            "FK",
            theme.style(Token::Info).add_modifier(Modifier::BOLD),
        ));
        rest = tail;
    } else if let Some(tail) = rest.strip_prefix("UQ") {
        spans.push(Span::styled(
            "UQ",
            theme.style(Token::Warning).add_modifier(Modifier::BOLD),
        ));
        rest = tail;
    }

    if let Some((col_part, type_part)) = rest.split_once(" : ") {
        spans.push(Span::styled(col_part.to_owned(), theme.style(Token::Text)));
        spans.push(Span::styled(" : ", border_style));
        spans.push(Span::styled(
            type_part.to_owned(),
            theme.style(Token::Muted),
        ));
    } else {
        spans.push(Span::styled(rest.to_owned(), theme.style(Token::Text)));
    }
    spans.push(Span::styled(right_border.to_string(), border_style));

    slice_styled_spans(spans, scroll_x, right_width, None)
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

    let show_telemetry_header = !activity.loading && activity.error.is_none() && inner.height >= 6;
    let body_top = if show_telemetry_header {
        let kpi_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let bar_area = Rect::new(inner.x, inner.y.saturating_add(1), inner.width, 1);
        render_activity_kpi_and_bar(
            &activity.snapshot.backends,
            presentation,
            kpi_area,
            bar_area,
            buf,
        );
        inner.y.saturating_add(2)
    } else {
        inner.y
    };

    let body_height = inner
        .height
        .saturating_sub(1)
        .saturating_sub(if show_telemetry_header { 2 } else { 0 });
    let body_area = Rect::new(inner.x, body_top, inner.width, body_height);
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
    let right_width = usize::from(right_area.width);
    let unicode = !presentation.glyphs.is_ascii();
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
        for (offset_idx, line) in activity
            .rendered_lines
            .iter()
            .skip(activity.scroll_y)
            .take(right_visible_rows)
            .enumerate()
        {
            let abs_idx = activity.scroll_y.saturating_add(offset_idx);
            let scrolled_line: String = line.chars().skip(activity.scroll_x).collect();
            let clean = sanitize_for_display(&scrolled_line);
            if activity.mode == crate::activity::ActivityViewMode::Backends
                && abs_idx == 2
                && !clean.trim().is_empty()
            {
                activity_lines.push(modal_table_header_row(clean, right_width, unicode, theme));
            } else {
                activity_lines.push(Line::from(Span::styled(clean, theme.style(Token::Text))));
            }
        }
    }
    Paragraph::new(activity_lines).render(right_area, buf);

    let nav_key = if presentation.glyphs.is_ascii() {
        " [Up/Down] "
    } else {
        " [\u{2191}/\u{2193}] "
    };
    let footer_hints = vec![
        Span::styled(
            nav_key,
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

fn render_activity_kpi_and_bar(
    backends: &[crate::activity::BackendProcess],
    presentation: &Presentation,
    kpi_area: Rect,
    bar_area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let active_count = backends.iter().filter(|b| b.state == "active").count();
    let idle_count = backends
        .iter()
        .filter(|b| b.state.starts_with("idle"))
        .count();
    let blocked_count = backends.iter().filter(|b| !b.blocked_by.is_empty()).count();
    let waiting_count = backends
        .iter()
        .filter(|b| {
            b.wait_event_type
                .as_deref()
                .is_some_and(|w| !w.is_empty() && w != "Client")
        })
        .count();

    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "Active: ",
        active_count.to_string(),
        count_token(active_count, Token::Success),
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "Idle: ",
        idle_count.to_string(),
        Token::Muted,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "Blocked: ",
        blocked_count.to_string(),
        count_token(blocked_count, Token::Danger),
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "Waiting: ",
        waiting_count.to_string(),
        count_token(waiting_count, Token::Warning),
    );
    render_capsule_bar(capsules, kpi_area, buf);

    let bar_w = usize::from(bar_area.width).saturating_sub(26).clamp(10, 36);
    let waiting_bar_count = backends
        .iter()
        .filter(|b| {
            !b.blocked_by.is_empty()
                || b.wait_event_type
                    .as_deref()
                    .is_some_and(|w| !w.is_empty() && w != "Client")
        })
        .count();
    let active_bar_count = backends
        .iter()
        .filter(|b| {
            b.blocked_by.is_empty()
                && !b
                    .wait_event_type
                    .as_deref()
                    .is_some_and(|w| !w.is_empty() && w != "Client")
                && b.state == "active"
        })
        .count();
    let idle_bar_count = backends
        .len()
        .saturating_sub(waiting_bar_count + active_bar_count);

    let mut spans = vec![
        Span::styled(" State mix: ", theme.style(Token::Muted)),
        Span::styled("[", theme.style(Token::Border)),
    ];
    let total = active_bar_count + waiting_bar_count + idle_bar_count;
    if total == 0 {
        let empty_ch = if unicode { "\u{2500}" } else { "-" };
        spans.push(Span::styled(
            empty_ch.repeat(bar_w),
            theme.style(Token::Border),
        ));
    } else {
        let mut active_cells = (active_bar_count * bar_w).checked_div(total).unwrap_or(0);
        let mut waiting_cells = (waiting_bar_count * bar_w).checked_div(total).unwrap_or(0);
        let mut idle_cells = (idle_bar_count * bar_w).checked_div(total).unwrap_or(0);
        if active_bar_count > 0 && active_cells == 0 {
            active_cells = 1;
        }
        if waiting_bar_count > 0 && waiting_cells == 0 {
            waiting_cells = 1;
        }
        if idle_bar_count > 0 && idle_cells == 0 {
            idle_cells = 1;
        }
        while active_cells + waiting_cells + idle_cells > bar_w {
            if idle_cells >= active_cells && idle_cells >= waiting_cells && idle_cells > 1 {
                idle_cells -= 1;
            } else if active_cells >= waiting_cells && active_cells > 1 {
                active_cells -= 1;
            } else if waiting_cells > 1 {
                waiting_cells -= 1;
            } else {
                break;
            }
        }
        let assigned = active_cells + waiting_cells + idle_cells;
        if assigned < bar_w {
            let rem = bar_w - assigned;
            if active_bar_count >= waiting_bar_count && active_bar_count >= idle_bar_count {
                active_cells += rem;
            } else if waiting_bar_count >= idle_bar_count {
                waiting_cells += rem;
            } else {
                idle_cells += rem;
            }
        }

        let (active_sym, wait_sym, idle_sym) = if unicode {
            ("\u{2588}", "\u{2593}", "\u{2591}")
        } else {
            ("#", "~", "-")
        };
        let wait_token = if blocked_count > 0 {
            Token::Danger
        } else {
            Token::Warning
        };
        if active_cells > 0 {
            spans.push(Span::styled(
                active_sym.repeat(active_cells),
                theme.style(Token::Success),
            ));
        }
        if waiting_cells > 0 {
            spans.push(Span::styled(
                wait_sym.repeat(waiting_cells),
                theme.style(wait_token),
            ));
        }
        if idle_cells > 0 {
            spans.push(Span::styled(
                idle_sym.repeat(idle_cells),
                theme.style(Token::Muted),
            ));
        }
    }
    spans.push(Span::styled("]", theme.style(Token::Border)));
    spans.push(Span::styled(
        "  active / wait / idle",
        theme.style(Token::Muted),
    ));
    Paragraph::new(Line::from(spans)).render(bar_area, buf);
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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "HIT RATIO: ",
        format!("{hit_ratio:.1}%"),
        hit_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TRACKED: ",
        format!("{tracked_count}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "CALLS: ",
        format!("{total_calls}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "EXEC TIME: ",
        crate::perf::format_duration_ms(total_time_ms),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "SORT: ",
        state.sort_order.display_name().to_owned(),
        Token::Focus,
    );
    render_capsule_bar(capsules, capsule_area, buf);

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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "HEALTH: ",
        format!("{health_score}%"),
        score_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "RECLAIMABLE: ",
        reclaimable_str,
        Token::Success,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "REDUNDANT: ",
        format!("{redundant_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "BLOATED: ",
        format!("{bloated_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "UNINDEXED FK: ",
        format!("{unindexed_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "HYPOPG: ",
        if hypo_ready { "ENABLED" } else { "ABSENT" }.to_owned(),
        if hypo_ready {
            Token::Success
        } else {
            Token::Text
        },
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let redundant_tab = format!(" [1] Redundant ({redundant_cnt}) ");
    let bloat_tab = format!(" [2] B-Tree Bloat ({bloated_cnt}) ");
    let fk_tab = format!(" [3] Unindexed Foreign Keys ({unindexed_cnt}) ");
    render_overlay_tabs(
        &[&redundant_tab, &bloat_tab, &fk_tab],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

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
        let unicode = !presentation.glyphs.is_ascii();
        let table_width = usize::from(table_area.width);
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
                    lines.push(modal_table_header_row(
                        format!(
                            "{:<24}{:<26}{:<16}{:>10}{:>8} {}",
                            "TABLE", "INDEX", "REASON", "SIZE", "SCANS", "COVERED BY"
                        ),
                        table_width,
                        unicode,
                        theme,
                    ));

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
                    lines.push(modal_table_header_row(
                        format!(
                            "{:<24}{:<26}{:>10}{:>10}{:>8}",
                            "TABLE", "INDEX", "TOTAL", "WASTED", "BLOAT %"
                        ),
                        table_width,
                        unicode,
                        theme,
                    ));

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
                    lines.push(modal_table_header_row(
                        format!(
                            "{:<28}{:<24}{:<20}{}",
                            "CONSTRAINT", "CHILD TABLE", "COLUMNS", "REFERENCES"
                        ),
                        table_width,
                        unicode,
                        theme,
                    ));

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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "COMMITS: ",
        format!("{commits_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ROLLBACK: ",
        format!("{rollbacks_cnt} ({rollback_pct:.2}%)"),
        rollback_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "WAL WRITE: ",
        wal_str,
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "CHECKPOINTS: ",
        format!("{timed_pct:.1}% TIMED"),
        checkpoint_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "MAX XID AGE: ",
        format!("{max_xid}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "STANDBYS: ",
        format!("{standby_cnt}"),
        if standby_cnt > 0 {
            Token::Success
        } else {
            Token::Text
        },
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let xid_tab = format!(
        " [2] Transactions & XID Freeze ({}) ",
        report_opt.map_or(0, |r| r.databases.len())
    );
    let standby_tab = format!(" [3] Replication & Standbys ({standby_cnt}) ");
    render_overlay_tabs(
        &[" [1] Checkpoints & WAL ", &xid_tab, &standby_tab],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "MODE: ",
        if is_pgbouncer {
            "PgBouncer"
        } else {
            "Direct PostgreSQL"
        }
        .to_owned(),
        Token::Focus,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "SATURATION: ",
        format!("{saturation:.1}% ({total_conns}/{max_conns})"),
        sat_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "HEADROOM: ",
        format!("{headroom}"),
        headroom_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ACTIVE: ",
        format!("{active_conns}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "IDLE: ",
        format!("{idle_conns}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "WAIT QUEUE: ",
        format!("{wait_clients_cnt}"),
        queue_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "POOLS: ",
        format!("{pools_cnt}"),
        Token::Text,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let tab_1 = format!(" [1] Pool Allocations & Queues ({pools_cnt}) ");
    let tab_3 = format!(" [3] Client Wait Forensics ({wait_clients_cnt}) ");
    render_overlay_tabs(
        &[&tab_1, " [2] Server Connections & Saturation ", &tab_3],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "CANDIDATES: ",
        format!("{candidates_cnt}"),
        cand_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "POLICIES: ",
        format!("{policies_cnt} tables"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "VIEW PREFIX: ",
        state.view_prefix.clone(),
        Token::Focus,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ENGINE: ",
        "Virtual Views Overlay".to_owned(),
        Token::Success,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let tab_1 = format!(" [1] Sensitive Columns & Rules ({candidates_cnt}) ");
    let tab_2 = format!(" [2] Generated Masked Views SQL ({policies_cnt}) ");
    render_overlay_tabs(
        &[&tab_1, &tab_2],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TABLES: ",
        format!("{tables_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TOPOLOGY: ",
        cycle_label.to_owned(),
        cycle_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "MODE: ",
        mode_label.to_owned(),
        mode_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TRUNCATE: ",
        trunc_label.to_owned(),
        if state.truncate {
            Token::Warning
        } else {
            Token::Text
        },
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "PROTOCOL: ",
        "Binary COPY Stream".to_owned(),
        Token::Focus,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let report_tables_cnt = state
        .sync_report
        .as_ref()
        .map_or(0, |r| r.table_progress.len());
    let tab_1 = format!(" [1] Dependency Graph & Sync Plan ({tables_cnt}) ");
    let tab_2 = format!(" [2] Live Transfer Telemetry ({report_tables_cnt}) ");
    render_overlay_tabs(
        &[&tab_1, &tab_2],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

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

    let mut capsules = Vec::new();
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TABLES: ",
        format!("{tables_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ROWS/TABLE: ",
        format!("{}", state.rows_per_table),
        Token::Focus,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "PRNG SEED: ",
        format!("{}", state.seed),
        Token::Success,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TOTAL ROWS: ",
        format!("{total_rows}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "INTEGRITY: ",
        "DAG Parent Keys".to_owned(),
        Token::Focus,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let preview_cnt = state.report.as_ref().map_or(0, |r| r.tables.len());
    let tab_1 = format!(" [1] Table Generator Strategies ({tables_cnt}) ");
    let tab_2 = format!(" [2] Generated Fixture Preview ({preview_cnt}) ");
    render_overlay_tabs(
        &[&tab_1, &tab_2],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

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
    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "SNIPPETS: ",
        format!("{}/{}", filtered.len(), total_count),
        Token::Focus,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "CATEGORY: ",
        state.library.category_filter.label().to_owned(),
        Token::Info,
    );

    if let Some(selected) = state.library.selected_snippet() {
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
        push_kv_capsule(
            &mut capsules,
            presentation,
            "SCOPE: ",
            scope_label.to_owned(),
            scope_token,
        );
    }

    render_capsule_bar(capsules, capsule_area, buf);

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
    let unicode = !presentation.glyphs.is_ascii();
    let selection_style = theme.style(Token::Selection);
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
        let list_width = usize::from(list_area.width);
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

            let name = sanitize_for_display(&snippet.name);
            let row = if is_sel {
                bounded_row(
                    vec![
                        (
                            "> ".to_owned(),
                            selection_style.add_modifier(Modifier::BOLD),
                        ),
                        (badge.to_owned(), selection_style),
                        (name, selection_style.add_modifier(Modifier::BOLD)),
                    ],
                    list_width,
                    unicode,
                    Some(selection_style),
                )
            } else {
                bounded_row(
                    vec![
                        ("  ".to_owned(), theme.style(Token::Muted)),
                        (badge.to_owned(), theme.style(badge_token)),
                        (name, theme.style(Token::Text)),
                    ],
                    list_width,
                    unicode,
                    None,
                )
            };
            list_lines.push(row);
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
        let divider =
            if unicode { "\u{2500}" } else { "-" }.repeat(usize::from(preview_area.width));
        preview_lines.push(Line::from(Span::styled(
            divider,
            theme.style(Token::Border),
        )));

        // Syntax-highlighted SQL
        let syntax = crate::query::highlight::tokens(&snippet.sql);
        let mut start = 0usize;
        let available_sql_height = (preview_area.height as usize).saturating_sub(4);

        for (line_idx, source) in snippet.sql.lines().enumerate().take(available_sql_height) {
            let line_start = start;
            start += source.len() + 1;
            let line_nr = format!("{:>3} {sep_char} ", line_idx + 1);
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

/// Builds a single-line row bounded to `max_width`, optionally padded with a full-width selection bar.
fn bounded_row(
    segments: Vec<(String, Style)>,
    max_width: usize,
    unicode: bool,
    selection_style: Option<Style>,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(segments.len() + 1);
    let mut used = 0usize;
    for (text, style) in segments {
        if used >= max_width {
            break;
        }
        let avail = max_width - used;
        let width = display_width(&text);
        if width <= avail {
            spans.push(Span::styled(text, style));
            used += width;
        } else {
            let clipped = truncate_to_width(&text, avail, unicode);
            used += display_width(&clipped);
            spans.push(Span::styled(clipped, style));
            break;
        }
    }
    if let Some(sel) = selection_style {
        if used < max_width {
            spans.push(Span::styled(" ".repeat(max_width - used), sel));
        }
        Line::from(spans).style(sel)
    } else {
        Line::from(spans)
    }
}

/// Paints a full-width recessed `SurfaceAlt` table header row from pre-styled segments.
fn modal_table_header_segments(
    segments: Vec<(String, Style)>,
    max_width: usize,
    unicode: bool,
    theme: &crate::ui::theme::Theme,
) -> Line<'static> {
    bounded_row(segments, max_width, unicode, Some(theme.stripe()))
}

/// Paints a full-width recessed `SurfaceAlt` table header row in bold `Token::Header`.
fn modal_table_header_row(
    text: String,
    max_width: usize,
    unicode: bool,
    theme: &crate::ui::theme::Theme,
) -> Line<'static> {
    let style = theme.on_stripe(Token::Header).add_modifier(Modifier::BOLD);
    modal_table_header_segments(vec![(text, style)], max_width, unicode, theme)
}

/// Draws the offline SQL lint findings overlay.
pub(crate) fn render_lint_findings(
    state: &crate::app::model::LintFindingsState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
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
    let title = format!(" {} SQL LINT - OFFLINE ", presentation.icon(Icon::Warning));
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
    let error_token = if errors > 0 {
        Token::Danger
    } else {
        Token::Success
    };
    let warning_token = if warnings > 0 {
        Token::Warning
    } else {
        Token::Success
    };
    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "FINDINGS: ",
        format!("{}", state.findings.len()),
        Token::Focus,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ERRORS: ",
        format!("{errors}"),
        error_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "WARNINGS: ",
        format!("{warnings}"),
        warning_token,
    );
    render_capsule_bar(capsules, Rect::new(inner.x, inner.y, inner.width, 1), buf);
    Paragraph::new(Line::from(Span::styled(
        hairline_rule(presentation).repeat(usize::from(inner.width)),
        theme.style(Token::Border),
    )))
    .style(theme.surface(Token::SurfaceOverlay))
    .render(
        Rect::new(inner.x, inner.y.saturating_add(1), inner.width, 1),
        buf,
    );

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
        let body_width = usize::from(body_area.width);
        let selection_style = theme.style(Token::Selection);
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
            let marker = if selected { "> " } else { "  " };
            let mut segments = if selected {
                vec![
                    (
                        marker.to_owned(),
                        selection_style.add_modifier(Modifier::BOLD),
                    ),
                    (
                        format!("{:<7} ", finding.severity.label()),
                        selection_style.add_modifier(Modifier::BOLD),
                    ),
                    (
                        format!("{}:{} ", finding.line, finding.column),
                        selection_style,
                    ),
                    (format!("{} ", finding.code), selection_style),
                    (
                        sanitize_for_display(&finding.message),
                        selection_style.add_modifier(Modifier::BOLD),
                    ),
                ]
            } else {
                vec![
                    (marker.to_owned(), theme.style(Token::Muted)),
                    (
                        format!("{:<7} ", finding.severity.label()),
                        severity_style.add_modifier(Modifier::BOLD),
                    ),
                    (
                        format!("{}:{} ", finding.line, finding.column),
                        theme.style(Token::Muted),
                    ),
                    (format!("{} ", finding.code), theme.style(Token::Info)),
                    (
                        sanitize_for_display(&finding.message),
                        theme.style(Token::Text),
                    ),
                ]
            };
            if !finding.excerpt.is_empty() {
                segments.push((
                    format!("  {}", sanitize_for_display(&finding.excerpt)),
                    if selected {
                        selection_style
                    } else {
                        theme.style(Token::Muted)
                    },
                ));
            }
            lines.push(bounded_row(
                segments,
                body_width,
                unicode,
                selected.then_some(selection_style),
            ));
        }
        Paragraph::new(lines).render(body_area, buf);
    }

    render_footer_hints(
        &[
            ("Enter", "Jump"),
            ("r", "Re-lint"),
            ("Up/Down", "Move"),
            ("Esc", "Close"),
        ],
        presentation,
        footer_area,
        buf,
    );
}

fn format_optional_wal_bytes(bytes: Option<i64>) -> String {
    match bytes {
        Some(value) if value >= 0 => crate::telemetry::format_bytes(value as u64),
        _ => "-".to_owned(),
    }
}

fn fit_cell(raw: &str, width: usize, unicode: bool) -> String {
    let sanitized = sanitize_for_display(raw);
    let clipped = truncate_to_width(&sanitized, width, unicode);
    pad_to_width(&clipped, width)
}

/// Rows the modal shell spends around its body: two borders, the capsule
/// bar, the tab strip, the hairline rule, the status line and the key rail.
const MODAL_CHROME_ROWS: u16 = 7;
/// The fewest body rows a fitted modal is given.
const MODAL_MIN_BODY_ROWS: u16 = 4;

/// How tall [`modal_inner_layout`] makes its box, and what it measured.
///
/// A modal is drawn twice: once into scratch at the tallest height the
/// terminal allows, to see how many body rows its content really uses, and
/// then for real with the box fitted to that. The status line and key rail
/// stay pinned to the bottom of the box either way.
struct ModalFit {
    /// Body rows to fit, or `None` for the tallest allowed box.
    rows: Option<u16>,
    /// The body rectangle and its background, recorded by the layout.
    body: std::cell::Cell<Option<(Rect, Color)>>,
}

impl ModalFit {
    const fn tallest() -> Self {
        Self {
            rows: None,
            body: std::cell::Cell::new(None),
        }
    }

    const fn rows(rows: u16) -> Self {
        Self {
            rows: Some(rows),
            body: std::cell::Cell::new(None),
        }
    }
}

/// How many body rows of a drawn modal carry anything: text, or a fill that
/// differs from the body's own.
fn used_body_rows(buf: &Buffer, body: Rect, background: Color) -> u16 {
    (body.top()..body.bottom())
        .rev()
        .find(|&row| {
            (body.left()..body.right()).any(|column| {
                let cell = &buf[(column, row)];
                !cell.symbol().trim().is_empty() || cell.bg != background
            })
        })
        .map_or(0, |row| row - body.top() + 1)
}

/// Draws a modal into scratch at its tallest and returns how many body rows
/// its content uses.
fn measure_modal_body(area: Rect, render: impl FnOnce(&ModalFit, &mut Buffer)) -> u16 {
    let fit = ModalFit::tallest();
    let mut scratch = Buffer::empty(area);
    render(&fit, &mut scratch);
    fit.body.get().map_or(0, |(body, background)| {
        used_body_rows(&scratch, body, background)
    })
}

/// Draws a modal with its box fitted to its content, never below `minimum`
/// body rows.
///
/// The fitted box is only used when it shows exactly what the tallest box
/// showed in the rows the content uses. A body that pins something to its
/// bottom edge, or that lays itself out by proportion, would differ, and
/// then keeps the tallest box: fitting may never hide a row.
fn render_fitted_modal(
    area: Rect,
    buf: &mut Buffer,
    minimum: u16,
    render: impl Fn(&ModalFit, &mut Buffer),
) {
    let tallest = ModalFit::tallest();
    let mut full = Buffer::empty(area);
    render(&tallest, &mut full);
    let Some((body, background)) = tallest.body.get() else {
        render(&tallest, buf);
        return;
    };
    let used = used_body_rows(&full, body, background);
    let fitted = ModalFit::rows(used.max(minimum));
    let mut trial = Buffer::empty(area);
    render(&fitted, &mut trial);
    let keeps_every_row = fitted.body.get().is_some_and(|(fitted_body, _)| {
        fitted_body.x == body.x
            && fitted_body.y == body.y
            && fitted_body.width == body.width
            && fitted_body.height >= used
            && (body.top()..body.top() + used).all(|row| {
                (body.left()..body.right())
                    .all(|column| full[(column, row)] == trial[(column, row)])
            })
    });
    render(if keeps_every_row { &fitted } else { &tallest }, buf);
}

fn modal_title_line(title: &str, presentation: &Presentation) -> Line<'static> {
    let theme = &presentation.theme;
    let trimmed = title.trim();
    let title_style = theme.style(Token::Focus).add_modifier(Modifier::BOLD);
    if !presentation.glyphs.is_ascii()
        && let Some(first) = trimmed.chars().next().filter(|ch| !ch.is_ascii())
    {
        let icon_len = first.len_utf8();
        let rest = &trimmed[icon_len..];
        let (icon, text) = if rest.starts_with(' ') {
            (&trimmed[..icon_len + 1], rest.trim_start())
        } else {
            (&trimmed[..icon_len], rest)
        };
        return Line::from(vec![
            Span::styled(icon.to_owned(), theme.style(Token::Info)),
            Span::styled(text.to_owned(), title_style),
        ]);
    }
    Line::from(Span::styled(trimmed.to_owned(), title_style))
}

fn modal_inner_layout(
    title: String,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) -> Option<(Rect, Rect, Rect, Rect, Rect)> {
    let modal_width = (area.width.saturating_mul(92) / 100)
        .max(76)
        .min(area.width);
    let tallest = (area.height.saturating_mul(88) / 100)
        .max(20)
        .min(area.height);
    let modal_height = fit.rows.map_or(tallest, |rows| {
        MODAL_CHROME_ROWS
            .saturating_add(rows.max(MODAL_MIN_BODY_ROWS))
            .min(tallest)
    });
    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    // The top edge stays where the tallest box would put it, so the title
    // and tab strip never move when content or the active tab changes; only
    // the bottom edge follows the content.
    let modal_y = area.y + (area.height.saturating_sub(tallest)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    if modal_area.width < 30 || modal_area.height < 8 {
        return None;
    }

    ratatui::widgets::Clear.render(modal_area, buf);
    overlay_block_line(modal_title_line(&title, presentation), presentation)
        .render(modal_area, buf);

    let inner = Rect::new(
        modal_area.x.saturating_add(1),
        modal_area.y.saturating_add(1),
        modal_area.width.saturating_sub(2),
        modal_area.height.saturating_sub(2),
    );
    if inner.width < 4 || inner.height < 6 {
        return None;
    }

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    Paragraph::new(Line::from(Span::styled(
        hairline_rule(presentation).repeat(usize::from(chunks[2].width)),
        presentation.theme.style(Token::Border),
    )))
    .style(presentation.theme.surface(Token::SurfaceOverlay))
    .render(chunks[2], buf);

    let background = buf
        .cell((chunks[3].x, chunks[3].y))
        .map_or(Color::Reset, |cell| cell.bg);
    fit.body.set(Some((chunks[3], background)));
    Some((chunks[0], chunks[1], chunks[3], chunks[4], chunks[5]))
}

/// Renders the Logical Replication & Replication Slots Manager overlay (`051`).
pub(crate) fn render_logical_replication(
    state: &crate::app::model::LogicalReplicationState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_logical_replication_frame(state, presentation, area, target, fit);
    });
}

fn render_logical_replication_frame(
    state: &crate::app::model::LogicalReplicationState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let title = format!(
        " {} LOGICAL REPLICATION & SLOTS ",
        presentation.icon(Icon::Database)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let report_opt = state.report.as_ref();
    let slots_cnt = report_opt.map_or(0, |r| r.slots.len());
    let pubs_cnt = report_opt.map_or(0, |r| r.publications.len());
    let subs_cnt = report_opt.map_or(0, |r| r.subscriptions.len());
    let inactive_cnt = report_opt.map_or(0, |r| r.inactive_slots_count());
    let max_retained = format_optional_wal_bytes(report_opt.and_then(|r| r.max_retained_bytes()));
    let risk_token = if report_opt.is_some_and(|r| r.has_critical_findings()) {
        Token::Danger
    } else if report_opt.is_some_and(|r| r.has_warning_findings()) {
        Token::Warning
    } else {
        Token::Success
    };

    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "SLOTS: ",
        format!("{slots_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "INACTIVE: ",
        format!("{inactive_cnt}"),
        risk_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "MAX WAL: ",
        max_retained,
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "PUBS: ",
        format!("{pubs_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "SUBS: ",
        format!("{subs_cnt}"),
        Token::Text,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let tab_1 = format!(" [1] Slots ({slots_cnt}) ");
    let tab_2 = format!(" [2] Publications ({pubs_cnt}) ");
    let tab_3 = format!(" [3] Subscriptions ({subs_cnt}) ");
    render_overlay_tabs(
        &[&tab_1, &tab_2, &tab_3],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

    let body_width = usize::from(body_area.width);
    let body_rows = usize::from(body_area.height).saturating_sub(1);
    let selection_style = theme.style(Token::Selection);

    if state.loading {
        Paragraph::new(Line::from(Span::styled(
            "Loading replication slots, publications, and subscriptions from PostgreSQL catalogs...",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    } else if let Some(report) = report_opt {
        match state.active_tab {
            0 => {
                if report.slots.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No replication slots found.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<22} {:<9} {:<12} {:<9} {:<10} {:>10} {:<6} {}",
                                "SLOT",
                                "TYPE",
                                "PLUGIN",
                                "STATE",
                                "WAL",
                                "RETAINED",
                                "RISK",
                                "SUMMARY"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, slot) in report.slots.iter().enumerate().skip(start).take(body_rows) {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let active_label = if slot.active { "ACTIVE" } else { "INACTIVE" };
                        let active_token = if slot.active {
                            Token::Success
                        } else {
                            Token::Warning
                        };
                        let (risk_badge, risk_token) =
                            if slot.wal_status.trim().eq_ignore_ascii_case("lost") {
                                ("LOST", Token::Danger)
                            } else {
                                match slot.risk {
                                    crate::replication::SlotRiskSeverity::Critical => {
                                        ("CRIT", Token::Danger)
                                    }
                                    crate::replication::SlotRiskSeverity::Warning => {
                                        ("WARN", Token::Warning)
                                    }
                                    crate::replication::SlotRiskSeverity::Ok => {
                                        ("OK", Token::Success)
                                    }
                                }
                            };
                        let retained = format_optional_wal_bytes(slot.retained_bytes);
                        let summary_raw = slot
                            .finding
                            .as_deref()
                            .or(slot.database.as_deref())
                            .unwrap_or("-");
                        let summary = sanitize_for_display(summary_raw);
                        let slot_name = fit_cell(&slot.slot_name, 22, unicode);
                        let slot_type = fit_cell(&slot.slot_type, 9, unicode);
                        let plugin = fit_cell(slot.plugin.as_deref().unwrap_or("-"), 12, unicode);
                        let wal_status = fit_cell(&slot.wal_status, 10, unicode);
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{slot_name} "), selection_style),
                                (format!("{slot_type} "), selection_style),
                                (format!("{plugin} "), selection_style),
                                (format!("{active_label:<9} "), selection_style),
                                (format!("{wal_status} "), selection_style),
                                (format!("{retained:>10} "), selection_style),
                                (
                                    format!("{risk_badge:<6} "),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (summary, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{slot_name} "), theme.style(Token::Text)),
                                (format!("{slot_type} "), theme.style(Token::Info)),
                                (format!("{plugin} "), theme.style(Token::Muted)),
                                (format!("{active_label:<9} "), theme.style(active_token)),
                                (format!("{wal_status} "), theme.style(Token::Muted)),
                                (format!("{retained:>10} "), theme.style(Token::Text)),
                                (
                                    format!("{risk_badge:<6} "),
                                    theme.style(risk_token).add_modifier(Modifier::BOLD),
                                ),
                                (summary, theme.style(Token::Muted)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            1 => {
                if report.publications.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No logical publications found in this database.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<24} {:<11} {:<9} {:<16} {}",
                                "PUBLICATION", "SCOPE", "OPS", "OWNER", "TABLES"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, pub_item) in report
                        .publications
                        .iter()
                        .enumerate()
                        .skip(start)
                        .take(body_rows)
                    {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let scope = if pub_item.all_tables {
                            "ALL TABLES"
                        } else {
                            "SELECTED"
                        };
                        let ops = format!(
                            "{}/{}/{}/{}",
                            if pub_item.insert { "I" } else { "-" },
                            if pub_item.update { "U" } else { "-" },
                            if pub_item.delete { "D" } else { "-" },
                            if pub_item.truncate { "T" } else { "-" },
                        );
                        let tables_summary = if pub_item.all_tables {
                            "ALL TABLES".to_owned()
                        } else if pub_item.tables.is_empty() {
                            "0 tables".to_owned()
                        } else {
                            let preview = pub_item
                                .tables
                                .iter()
                                .take(3)
                                .map(|t| sanitize_for_display(t))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let suffix = if pub_item.tables_truncated || pub_item.tables.len() > 3 {
                                ", ..."
                            } else {
                                ""
                            };
                            format!("{} ({}{})", pub_item.tables.len(), preview, suffix)
                        };
                        let name = fit_cell(&pub_item.name, 24, unicode);
                        let owner = fit_cell(&pub_item.owner, 16, unicode);
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{name} "), selection_style),
                                (format!("{scope:<11} "), selection_style),
                                (format!("{ops:<9} "), selection_style),
                                (format!("{owner} "), selection_style),
                                (tables_summary, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{name} "), theme.style(Token::Text)),
                                (
                                    format!("{scope:<11} "),
                                    theme.style(if pub_item.all_tables {
                                        Token::Info
                                    } else {
                                        Token::Muted
                                    }),
                                ),
                                (format!("{ops:<9} "), theme.style(Token::Focus)),
                                (format!("{owner} "), theme.style(Token::Muted)),
                                (tables_summary, theme.style(Token::Text)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            2 => {
                if report.subscription_access_restricted {
                    Paragraph::new(Line::from(Span::styled(
                        "Subscription catalog (pg_subscription) is restricted for the current role (SQLSTATE 42501).",
                        theme.style(Token::Warning),
                    )))
                    .render(body_area, buf);
                } else if report.subscriptions.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No logical subscriptions found in this database.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<20} {:<9} {:<13} {:<18} {:>10} {}",
                                "SUBSCRIPTION", "STATUS", "STATE", "SLOT", "LAG", "CONNINFO"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, sub) in report
                        .subscriptions
                        .iter()
                        .enumerate()
                        .skip(start)
                        .take(body_rows)
                    {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let enabled_label = if sub.enabled { "ENABLED" } else { "DISABLED" };
                        let enabled_token = if sub.enabled {
                            Token::Success
                        } else {
                            Token::Warning
                        };
                        let name = fit_cell(&sub.name, 20, unicode);
                        let sync_state = fit_cell(&sub.sync_state, 13, unicode);
                        let slot = fit_cell(sub.slot_name.as_deref().unwrap_or("-"), 18, unicode);
                        let lag = format_optional_wal_bytes(sub.lag_bytes);
                        let conninfo = sanitize_for_display(&sub.conninfo_redacted);
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{name} "), selection_style),
                                (format!("{enabled_label:<9} "), selection_style),
                                (format!("{sync_state} "), selection_style),
                                (format!("{slot} "), selection_style),
                                (format!("{lag:>10} "), selection_style),
                                (conninfo, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{name} "), theme.style(Token::Text)),
                                (format!("{enabled_label:<9} "), theme.style(enabled_token)),
                                (format!("{sync_state} "), theme.style(Token::Info)),
                                (format!("{slot} "), theme.style(Token::Muted)),
                                (format!("{lag:>10} "), theme.style(Token::Text)),
                                (conninfo, theme.style(Token::Muted)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            _ => {}
        }
    } else {
        Paragraph::new(Line::from(Span::styled(
            "No logical replication snapshot loaded. Press [r] to refresh.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        (sanitize_for_display(msg), Token::Info)
    } else if let Some(report) = report_opt {
        let mut notices = Vec::new();
        if report.subscription_access_restricted {
            notices.push("pg_subscription restricted (42501)");
        }
        if report.slots_truncated || report.publications_truncated || report.subscriptions_truncated
        {
            notices.push("results truncated at catalog bound");
        }
        if !notices.is_empty() {
            (format!("Notice: {}", notices.join("; ")), Token::Warning)
        } else if state.active_tab == 0
            && let Some(slot) = report.slots.get(state.selected_idx)
            && let Some(finding) = &slot.finding
        {
            (
                sanitize_for_display(finding),
                match slot.risk {
                    crate::replication::SlotRiskSeverity::Critical => Token::Danger,
                    crate::replication::SlotRiskSeverity::Warning => Token::Warning,
                    crate::replication::SlotRiskSeverity::Ok => Token::Muted,
                },
            )
        } else {
            (
                "Press [Enter] or [g] to load reviewed SQL into the editor (never auto-executed)."
                    .to_owned(),
                Token::Muted,
            )
        }
    } else {
        (String::new(), Token::Muted)
    };
    Paragraph::new(Line::from(Span::styled(
        truncate_to_width(&status_text, usize::from(status_area.width), unicode),
        theme.style(status_token),
    )))
    .render(status_area, buf);

    Paragraph::new(Line::from(vec![
        Span::styled(
            "[1-3] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", theme.style(Token::Muted)),
        Span::styled(
            "[Enter/g] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Load Reviewed SQL  ", theme.style(Token::Muted)),
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
    ]))
    .render(footer_area, buf);
}

/// Renders the Server Settings, HBA Auth, Extensions, Tablespaces & FDWs overlay (`052`).
pub(crate) fn render_cluster_config(
    state: &crate::app::model::ClusterConfigState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_cluster_config_frame(state, presentation, area, target, fit);
    });
}

fn render_cluster_config_frame(
    state: &crate::app::model::ClusterConfigState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let title = format!(
        " {} CLUSTER CONFIG, HBA & INFRASTRUCTURE ",
        presentation.icon(Icon::Database)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let report_opt = state.report.as_ref();
    let filter = (!state.filter_text.is_empty()).then_some(state.filter_text.as_str());

    let settings_rows = report_opt
        .map(|r| r.filtered_settings(state.modified_only, filter))
        .unwrap_or_default();
    let hba_rows = report_opt
        .map(|r| r.filtered_hba_rules(filter))
        .unwrap_or_default();
    let ext_rows = report_opt
        .map(|r| r.filtered_extensions(filter))
        .unwrap_or_default();
    let ts_rows = report_opt
        .map(|r| r.filtered_tablespaces(filter))
        .unwrap_or_default();
    let fdw_rows = report_opt
        .map(|r| r.filtered_fdws(filter))
        .unwrap_or_default();

    let non_default_cnt = report_opt.map_or(0, |r| r.non_default_settings_count());
    let pending_restart_cnt = report_opt.map_or(0, |r| r.pending_restart_count());
    let hba_warn_cnt = report_opt.map_or(0, |r| r.hba_warnings_count());
    let hba_crit_cnt = report_opt.map_or(0, |r| r.hba_critical_count());
    let ext_upgrade_cnt = report_opt.map_or(0, |r| r.upgradable_extensions_count());
    let servers_cnt: usize = report_opt.map_or(0, |r| r.fdws.iter().map(|f| f.servers.len()).sum());

    let hba_token = if hba_crit_cnt > 0 {
        Token::Danger
    } else if hba_warn_cnt > 0 {
        Token::Warning
    } else {
        Token::Success
    };

    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "MODIFIED: ",
        format!("{non_default_cnt}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "RESTART: ",
        format!("{pending_restart_cnt}"),
        if pending_restart_cnt > 0 {
            Token::Warning
        } else {
            Token::Success
        },
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "HBA RISKS: ",
        format!("{hba_warn_cnt}"),
        hba_token,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "EXT UPGRADES: ",
        format!("{ext_upgrade_cnt}"),
        if ext_upgrade_cnt > 0 {
            Token::Warning
        } else {
            Token::Success
        },
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "FOREIGN SERVERS: ",
        format!("{servers_cnt}"),
        Token::Text,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    let tab_labels = [
        format!(" [1] Settings ({}) ", settings_rows.len()),
        format!(" [2] HBA Auth ({}) ", hba_rows.len()),
        format!(" [3] Extensions ({}) ", ext_rows.len()),
        format!(" [4] Tablespaces ({}) ", ts_rows.len()),
        format!(" [5] FDWs ({}) ", fdw_rows.len()),
    ];
    let mut tab_spans = Vec::new();
    for (idx, label) in tab_labels.iter().enumerate() {
        if idx > 0 {
            tab_spans.push(Span::styled(" ", theme.style(Token::Border)));
        }
        let style = if idx == state.active_tab {
            theme.active_tab(Token::Focus)
        } else {
            theme.style(Token::Text)
        };
        tab_spans.push(Span::styled(label.clone(), style));
    }
    tab_spans.push(Span::raw("  "));
    let modified_badge = if state.modified_only {
        " [m] Modified-only: ON "
    } else {
        " [m] Modified-only: OFF "
    };
    tab_spans.push(Span::styled(
        modified_badge,
        if state.modified_only {
            theme.recessed_pill(Token::Warning)
        } else {
            theme.recessed_pill(Token::Text)
        },
    ));
    if !state.filter_text.is_empty() {
        tab_spans.push(Span::raw("  "));
        tab_spans.push(Span::styled(
            format!(" [Filter: {}] ", sanitize_for_display(&state.filter_text)),
            theme.recessed_pill(Token::Info),
        ));
    }
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    let body_width = usize::from(body_area.width);
    let body_rows = usize::from(body_area.height).saturating_sub(1);
    let selection_style = theme.style(Token::Selection);

    if state.loading {
        Paragraph::new(Line::from(Span::styled(
            "Loading settings, HBA rules, extensions, tablespaces, and foreign data wrappers...",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    } else if let Some(report) = report_opt {
        match state.active_tab {
            0 => {
                if settings_rows.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No settings match the current filter.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<32} {:<20} {:<12} {:<18} {}",
                                "NAME", "VALUE", "CONTEXT", "SOURCE", "STATE"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, setting) in
                        settings_rows.iter().enumerate().skip(start).take(body_rows)
                    {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let name = fit_cell(&setting.name, 32, unicode);
                        let value = fit_cell(&setting.display_value(), 20, unicode);
                        let context = fit_cell(&setting.context, 12, unicode);
                        let source = fit_cell(&setting.source, 18, unicode);
                        let (badge, badge_token) = if setting.pending_restart {
                            ("[RESTART]", Token::Danger)
                        } else if setting.is_non_default {
                            ("[MODIFIED]", Token::Warning)
                        } else {
                            ("default", Token::Muted)
                        };
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{name} "), selection_style),
                                (format!("{value} "), selection_style),
                                (format!("{context} "), selection_style),
                                (format!("{source} "), selection_style),
                                (
                                    badge.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{name} "), theme.style(Token::Text)),
                                (format!("{value} "), theme.style(Token::Info)),
                                (format!("{context} "), theme.style(Token::Muted)),
                                (format!("{source} "), theme.style(Token::Muted)),
                                (
                                    badge.to_owned(),
                                    theme.style(badge_token).add_modifier(Modifier::BOLD),
                                ),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            1 => {
                if report.hba_access_restricted {
                    Paragraph::new(Line::from(Span::styled(
                        "pg_hba_file_rules is restricted for the current role (requires pg_read_all_settings or superuser).",
                        theme.style(Token::Warning),
                    )))
                    .render(body_area, buf);
                } else if hba_rows.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No pg_hba.conf rules found.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<5} {:<9} {:<12} {:<12} {:<18} {:<13} {:<6} {}",
                                "LINE",
                                "TYPE",
                                "DATABASE",
                                "USER",
                                "ADDRESS",
                                "METHOD",
                                "RISK",
                                "REASON"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, rule) in hba_rows.iter().enumerate().skip(start).take(body_rows) {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let line_num = rule
                            .line_number
                            .map_or_else(|| "-".to_owned(), |n| n.to_string());
                        let rtype = fit_cell(&rule.rule_type, 9, unicode);
                        let dbs = fit_cell(&rule.databases.join(","), 12, unicode);
                        let users = fit_cell(&rule.users.join(","), 12, unicode);
                        let addr = fit_cell(rule.address.as_deref().unwrap_or("-"), 18, unicode);
                        let method = fit_cell(&rule.auth_method, 13, unicode);
                        let (risk_badge, risk_token) = match rule.risk {
                            crate::cluster::HbaRisk::Critical => ("HIGH", Token::Danger),
                            crate::cluster::HbaRisk::Warning => ("MED", Token::Warning),
                            crate::cluster::HbaRisk::Ok => ("LOW", Token::Muted),
                        };
                        let reason = sanitize_for_display(
                            rule.risk_reason
                                .as_deref()
                                .or(rule.error.as_deref())
                                .unwrap_or("-"),
                        );
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{line_num:<5} "), selection_style),
                                (format!("{rtype} "), selection_style),
                                (format!("{dbs} "), selection_style),
                                (format!("{users} "), selection_style),
                                (format!("{addr} "), selection_style),
                                (format!("{method} "), selection_style),
                                (
                                    format!("{risk_badge:<6} "),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (reason, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{line_num:<5} "), theme.style(Token::Muted)),
                                (format!("{rtype} "), theme.style(Token::Text)),
                                (format!("{dbs} "), theme.style(Token::Text)),
                                (format!("{users} "), theme.style(Token::Text)),
                                (format!("{addr} "), theme.style(Token::Info)),
                                (format!("{method} "), theme.style(Token::Text)),
                                (
                                    format!("{risk_badge:<6} "),
                                    theme.style(risk_token).add_modifier(Modifier::BOLD),
                                ),
                                (reason, theme.style(Token::Muted)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            2 => {
                if ext_rows.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No extensions match the current filter.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<26} {:<11} {:<11} {:<11} {}",
                                "EXTENSION", "INSTALLED", "DEFAULT", "STATUS", "COMMENT"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, ext) in ext_rows.iter().enumerate().skip(start).take(body_rows) {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let name = fit_cell(&ext.name, 26, unicode);
                        let installed =
                            fit_cell(ext.installed_version.as_deref().unwrap_or("-"), 11, unicode);
                        let default_ver =
                            fit_cell(ext.default_version.as_deref().unwrap_or("-"), 11, unicode);
                        let (status_badge, status_token) = if ext.upgrade_available {
                            ("[UPGRADE]", Token::Warning)
                        } else if ext.is_installed() {
                            ("installed", Token::Success)
                        } else {
                            ("available", Token::Muted)
                        };
                        let comment = sanitize_for_display(ext.comment.as_deref().unwrap_or("-"));
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{name} "), selection_style),
                                (format!("{installed} "), selection_style),
                                (format!("{default_ver} "), selection_style),
                                (
                                    format!("{status_badge:<11} "),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (comment, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{name} "), theme.style(Token::Text)),
                                (format!("{installed} "), theme.style(Token::Info)),
                                (format!("{default_ver} "), theme.style(Token::Muted)),
                                (
                                    format!("{status_badge:<11} "),
                                    theme.style(status_token).add_modifier(Modifier::BOLD),
                                ),
                                (comment, theme.style(Token::Muted)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            3 => {
                if ts_rows.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No tablespaces found.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<20} {:<16} {:>10} {:<26} {}",
                                "TABLESPACE", "OWNER", "SIZE", "LOCATION", "OPTIONS"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, ts) in ts_rows.iter().enumerate().skip(start).take(body_rows) {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let name = fit_cell(&ts.name, 20, unicode);
                        let owner = fit_cell(&ts.owner, 16, unicode);
                        let size_pretty = ts.size_bytes.map_or_else(
                            || "-".to_owned(),
                            |b| {
                                if b >= 0 {
                                    crate::cluster::format_bytes(b as u64)
                                } else {
                                    "-".to_owned()
                                }
                            },
                        );
                        let location = fit_cell(ts.location.as_deref().unwrap_or("-"), 26, unicode);
                        let options = if ts.options.is_empty() {
                            "-".to_owned()
                        } else {
                            sanitize_for_display(
                                &crate::cluster::mask_options(&ts.options).join(", "),
                            )
                        };
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{name} "), selection_style),
                                (format!("{owner} "), selection_style),
                                (format!("{size_pretty:>10} "), selection_style),
                                (format!("{location} "), selection_style),
                                (options, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{name} "), theme.style(Token::Text)),
                                (format!("{owner} "), theme.style(Token::Muted)),
                                (format!("{size_pretty:>10} "), theme.style(Token::Info)),
                                (format!("{location} "), theme.style(Token::Text)),
                                (options, theme.style(Token::Muted)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            4 => {
                if fdw_rows.is_empty() {
                    Paragraph::new(Line::from(Span::styled(
                        "No foreign data wrappers found.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                } else {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  {:<18} {:<14} {:<18} {:<18} {}",
                                "FDW", "OWNER", "HANDLER", "VALIDATOR", "SERVERS"
                            ),
                            theme.style(Token::Header).add_modifier(Modifier::BOLD),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    let start = state
                        .selected_idx
                        .saturating_sub(body_rows.saturating_sub(1));
                    for (idx, fdw) in fdw_rows.iter().enumerate().skip(start).take(body_rows) {
                        let selected = idx == state.selected_idx;
                        let marker = if selected { "> " } else { "  " };
                        let name = fit_cell(&fdw.name, 18, unicode);
                        let owner = fit_cell(&fdw.owner, 14, unicode);
                        let handler = fit_cell(fdw.handler.as_deref().unwrap_or("-"), 18, unicode);
                        let validator =
                            fit_cell(fdw.validator.as_deref().unwrap_or("-"), 18, unicode);
                        let servers_summary = if fdw.servers.is_empty() {
                            "0 servers".to_owned()
                        } else {
                            let details = fdw
                                .servers
                                .iter()
                                .map(|srv| {
                                    let masked = crate::cluster::mask_options(&srv.options);
                                    if masked.is_empty() {
                                        sanitize_for_display(&srv.name)
                                    } else {
                                        format!(
                                            "{}({})",
                                            sanitize_for_display(&srv.name),
                                            sanitize_for_display(&masked.join(", "))
                                        )
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            format!("{}: {}", fdw.servers.len(), details)
                        };
                        let segments = if selected {
                            vec![
                                (
                                    marker.to_owned(),
                                    selection_style.add_modifier(Modifier::BOLD),
                                ),
                                (format!("{name} "), selection_style),
                                (format!("{owner} "), selection_style),
                                (format!("{handler} "), selection_style),
                                (format!("{validator} "), selection_style),
                                (servers_summary, selection_style),
                            ]
                        } else {
                            vec![
                                (marker.to_owned(), theme.style(Token::Muted)),
                                (format!("{name} "), theme.style(Token::Text)),
                                (format!("{owner} "), theme.style(Token::Muted)),
                                (format!("{handler} "), theme.style(Token::Info)),
                                (format!("{validator} "), theme.style(Token::Muted)),
                                (servers_summary, theme.style(Token::Text)),
                            ]
                        };
                        lines.push(bounded_row(
                            segments,
                            body_width,
                            unicode,
                            selected.then_some(selection_style),
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                }
            }
            _ => {}
        }
    } else {
        Paragraph::new(Line::from(Span::styled(
            "No cluster configuration report loaded. Press [r] to refresh.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        (sanitize_for_display(msg), Token::Info)
    } else if let Some(report) = report_opt {
        let mut notices = Vec::new();
        if report.hba_access_restricted {
            notices.push("pg_hba_file_rules restricted");
        }
        if report.truncated_settings
            || report.truncated_hba_rules
            || report.truncated_extensions
            || report.truncated_tablespaces
            || report.truncated_fdws
        {
            notices.push("results truncated at catalog bound");
        }
        if !notices.is_empty() {
            (format!("Notice: {}", notices.join("; ")), Token::Warning)
        } else {
            (
                "Press [Enter] or [g] to load reviewed SQL into the editor (never auto-executed)."
                    .to_owned(),
                Token::Muted,
            )
        }
    } else {
        (String::new(), Token::Muted)
    };
    Paragraph::new(Line::from(Span::styled(
        truncate_to_width(&status_text, usize::from(status_area.width), unicode),
        theme.style(status_token),
    )))
    .render(status_area, buf);

    Paragraph::new(Line::from(vec![
        Span::styled(
            "[1-5] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Tabs  ", theme.style(Token::Muted)),
        Span::styled(
            "[m] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Modified/Risk  ", theme.style(Token::Muted)),
        Span::styled(
            "[j/k] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Select  ", theme.style(Token::Muted)),
        Span::styled(
            "[Enter/g] ",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled("Load Reviewed SQL  ", theme.style(Token::Muted)),
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
    ]))
    .render(footer_area, buf);
}

/// Renders the Live Data Browser & Batch CRUD overlay (`053`).
pub(crate) fn render_data_browser(
    state: &crate::app::model::DataBrowserState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_data_browser_frame(state, presentation, area, target, fit);
    });
}

fn render_data_browser_frame(
    state: &crate::app::model::DataBrowserState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let title = format!(
        " {} LIVE DATA BROWSER & CRUD ",
        presentation.icon(Icon::Table)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let page_opt = state.page.as_ref();
    let pk_vec = page_opt
        .map(|p| &p.pk_columns)
        .filter(|v| !v.is_empty())
        .unwrap_or(&state.request.pk_columns);
    let pk_label = if pk_vec.is_empty() {
        "ctid".to_owned()
    } else {
        sanitize_for_display(&pk_vec.join(","))
    };
    let rows_count = page_opt.map_or(0, |p| p.rows.len());
    let sort_col = state
        .request
        .sort_column
        .as_deref()
        .or_else(|| pk_vec.first().map(String::as_str))
        .unwrap_or("ctid");
    let sort_label = format!(
        "{} {}",
        sanitize_for_display(sort_col),
        state.request.sort_order.sql_keyword()
    );
    let (page_badge, page_token) = if page_opt.is_some_and(|p| p.has_more) {
        ("MORE", Token::Info)
    } else {
        ("END", Token::Muted)
    };
    let table_label = sanitize_for_display(&format!("{}.{}", state.schema, state.table));

    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TABLE: ",
        table_label,
        Token::Text,
    );
    push_kv_capsule(&mut capsules, presentation, "PK: ", pk_label, Token::Info);
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ROWS: ",
        format!("{rows_count}"),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "SORT: ",
        sort_label,
        Token::Focus,
    );
    capsules.push(Span::styled(
        format!(" [{page_badge}] "),
        theme.recessed_pill(page_token),
    ));
    render_capsule_bar(capsules, capsule_area, buf);

    let active_col_name = page_opt
        .and_then(|p| p.columns.get(state.selected_col))
        .map(|c| sanitize_for_display(c))
        .unwrap_or_else(|| "-".to_owned());
    let mut tab_spans = vec![
        Span::styled(
            format!(" [CELL: {active_col_name}] "),
            theme.recessed_pill(Token::Focus),
        ),
        Span::raw("  "),
        Span::styled(
            format!(" [PAGE SIZE: {}] ", state.request.effective_page_size()),
            theme.recessed_pill(Token::Text),
        ),
        Span::raw("  "),
        Span::styled(
            if page_opt.is_some_and(|p| p.has_prev) {
                " [PREV: YES] "
            } else {
                " [PREV: NO] "
            },
            theme.recessed_pill(Token::Text),
        ),
    ];
    if let Some(filter) = &state.request.filter_predicate {
        tab_spans.push(Span::raw("  "));
        tab_spans.push(Span::styled(
            format!(" [FILTER: {}] ", sanitize_for_display(filter)),
            theme.recessed_pill(Token::Info),
        ));
    }
    Paragraph::new(Line::from(tab_spans)).render(tab_area, buf);

    let body_width = usize::from(body_area.width);
    let body_rows = usize::from(body_area.height).saturating_sub(1);
    let selection_style = theme.style(Token::Selection);

    if state.loading {
        Paragraph::new(Line::from(Span::styled(
            "Loading keyset-paginated data page from PostgreSQL...",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    } else if let Some(page) = page_opt {
        if page.columns.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                "Table has no visible columns.",
                theme.style(Token::Muted),
            )))
            .render(body_area, buf);
        } else {
            let active_col = state.selected_col.min(page.columns.len() - 1);
            let mut col_widths = Vec::with_capacity(page.columns.len());
            for (col_idx, col_name) in page.columns.iter().enumerate() {
                let header_label = if col_idx == active_col {
                    format!("[{}]", sanitize_for_display(col_name))
                } else {
                    sanitize_for_display(col_name)
                };
                let mut width = display_width(&header_label);
                for row in &page.rows {
                    let cell_text =
                        format_browser_cell(row.get(col_idx).and_then(|v| v.as_deref()));
                    width = width.max(display_width(&cell_text));
                }
                col_widths.push(width.clamp(8, 24));
            }

            let (col_start, col_end) =
                browser_column_window(&col_widths, active_col, body_width.saturating_sub(2));

            let mut lines = Vec::new();
            let mut header_segments = vec![(
                "  ".to_owned(),
                theme.on_stripe(Token::Header).add_modifier(Modifier::BOLD),
            )];
            for (col_idx, (col_name, &col_width)) in page
                .columns
                .iter()
                .zip(col_widths.iter())
                .enumerate()
                .take(col_end)
                .skip(col_start)
            {
                let raw_name = sanitize_for_display(col_name);
                let label = if col_idx == active_col {
                    format!("[{raw_name}]")
                } else {
                    raw_name
                };
                let fitted = fit_cell(&label, col_width, unicode);
                let style = if col_idx == active_col {
                    theme.on_stripe(Token::Focus).add_modifier(Modifier::BOLD)
                } else {
                    theme.on_stripe(Token::Header).add_modifier(Modifier::BOLD)
                };
                header_segments.push((format!("{fitted} "), style));
            }
            lines.push(modal_table_header_segments(
                header_segments,
                body_width,
                unicode,
                theme,
            ));

            if page.rows.is_empty() {
                lines.push(bounded_row(
                    vec![(
                        "  No rows on this page. Press [Enter/g] to load a reviewed INSERT template."
                            .to_owned(),
                        theme.style(Token::Muted),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
            } else {
                let start = state
                    .selected_row
                    .saturating_sub(body_rows.saturating_sub(1));
                for (row_idx, row) in page.rows.iter().enumerate().skip(start).take(body_rows) {
                    let selected = row_idx == state.selected_row;
                    let marker = if selected { "> " } else { "  " };
                    let mut segments = Vec::with_capacity((col_end - col_start) + 1);
                    if selected {
                        segments.push((
                            marker.to_owned(),
                            selection_style.add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        segments.push((marker.to_owned(), theme.style(Token::Muted)));
                    }

                    for (col_idx, &width) in
                        col_widths.iter().enumerate().take(col_end).skip(col_start)
                    {
                        let raw = row.get(col_idx).and_then(|v| v.as_deref());
                        let cell_text = format_browser_cell(raw);
                        let fitted = fit_cell(&cell_text, width, unicode);
                        let style = if selected && col_idx == active_col {
                            selection_style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                        } else if selected {
                            selection_style
                        } else {
                            let token = match raw {
                                None => Token::NullValue,
                                Some("") => Token::Muted,
                                Some(_) => Token::Text,
                            };
                            if col_idx == active_col {
                                theme.style(token).add_modifier(Modifier::BOLD)
                            } else {
                                theme.style(token)
                            }
                        };
                        segments.push((format!("{fitted} "), style));
                    }

                    lines.push(bounded_row(
                        segments,
                        body_width,
                        unicode,
                        selected.then_some(selection_style),
                    ));
                }
            }
            Paragraph::new(lines).render(body_area, buf);
        }
    } else {
        Paragraph::new(Line::from(Span::styled(
            "No browse page loaded. Press [r] to refresh.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        (sanitize_for_display(msg), Token::Info)
    } else if let Some(page) = page_opt
        && let Some(row) = page.rows.get(state.selected_row)
        && let Some(col_name) = page.columns.get(state.selected_col)
    {
        let raw = row.get(state.selected_col).and_then(|v| v.as_deref());
        let preview = format_browser_cell(raw);
        (
            format!(
                "Cell [{}] = {}  |  Reviewed CRUD loads SQL into editor (never auto-executed).",
                sanitize_for_display(col_name),
                preview
            ),
            Token::Muted,
        )
    } else {
        (
            "Press [Enter/g] to load reviewed CRUD SQL or [d] for DELETE SQL (never auto-executed)."
                .to_owned(),
            Token::Muted,
        )
    };
    Paragraph::new(Line::from(Span::styled(
        truncate_to_width(&status_text, usize::from(status_area.width), unicode),
        theme.style(status_token),
    )))
    .render(status_area, buf);

    let footer_segments = vec![
        (
            "[j/k] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Row  ".to_owned(), theme.style(Token::Muted)),
        (
            "[h/l] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Col  ".to_owned(), theme.style(Token::Muted)),
        (
            "[n/p] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        (
            "Next/Prev Keyset Page  ".to_owned(),
            theme.style(Token::Muted),
        ),
        (
            "[s] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Sort  ".to_owned(), theme.style(Token::Muted)),
        (
            "[Enter/g] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Load CRUD SQL  ".to_owned(), theme.style(Token::Muted)),
        (
            "[d] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Load DELETE SQL  ".to_owned(), theme.style(Token::Muted)),
        (
            "[r] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Refresh  ".to_owned(), theme.style(Token::Muted)),
        (
            "[Esc] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Close".to_owned(), theme.style(Token::Muted)),
    ];
    Paragraph::new(bounded_row(
        footer_segments,
        usize::from(footer_area.width),
        unicode,
        None,
    ))
    .render(footer_area, buf);
}

fn format_browser_cell(raw: Option<&str>) -> String {
    match raw {
        None => "[null]".to_owned(),
        Some("") => "\"\"".to_owned(),
        Some(text) => sanitize_for_display(text),
    }
}

fn browser_column_window(
    widths: &[usize],
    selected_col: usize,
    max_width: usize,
) -> (usize, usize) {
    if widths.is_empty() {
        return (0, 0);
    }
    let selected = selected_col.min(widths.len() - 1);
    let mut start = selected;
    let mut end = selected + 1;
    let mut used = widths[selected] + 1;

    while end < widths.len() && used + widths[end] < max_width {
        used += widths[end] + 1;
        end += 1;
    }
    while start > 0 && used + widths[start - 1] < max_width {
        start -= 1;
        used += widths[start] + 1;
    }
    (start, end)
}

/// Renders the Extended EXPLAIN (BUFFERS, WAL, SETTINGS) & Plan Diff overlay (`054`).
pub(crate) fn render_plan_diff(
    state: &crate::app::model::PlanDiffState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_plan_diff_frame(state, presentation, area, target, fit);
    });
}

fn render_plan_diff_frame(
    state: &crate::app::model::PlanDiffState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let title = format!(
        " {} EXTENDED EXPLAIN & PLAN DIFF ",
        presentation.icon(Icon::Database)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let report_opt = state.report.as_ref();
    let single_opt = state.baseline.as_ref().or(state.candidate.as_ref());

    let mut capsules = vec![Span::raw(" ")];
    if let Some(report) = report_opt {
        let (verdict_label, verdict_token) = match report.verdict {
            crate::plandiff::DiffVerdict::Improvement => ("IMPROVEMENT", Token::Success),
            crate::plandiff::DiffVerdict::Regression => ("REGRESSION", Token::Danger),
            crate::plandiff::DiffVerdict::Mixed => ("MIXED", Token::Warning),
            crate::plandiff::DiffVerdict::Unchanged => ("UNCHANGED", Token::Muted),
        };
        let cost_delta = match report.total_cost_pct {
            Some(pct) => format!("{:+.2} ({:+.1}%)", report.total_cost_delta, pct),
            None => format!("{:+.2}", report.total_cost_delta),
        };
        let time_delta = match (report.execution_time_delta_ms, report.execution_time_pct) {
            (Some(ms), Some(pct)) => format!("{ms:+.2}ms ({pct:+.1}%)"),
            (Some(ms), None) => format!("{ms:+.2}ms"),
            _ => "n/a".to_owned(),
        };
        let buf_delta = format!(
            "{:+} / {:+}",
            report.shared_hit_delta, report.shared_read_delta
        );
        let wal_delta = format_signed_bytes(report.wal_bytes_delta);
        capsules.push(Span::styled(
            format!(" [{verdict_label}] "),
            theme.recessed_pill(verdict_token),
        ));
        capsules.push(Span::raw(" "));
        push_kv_capsule(
            &mut capsules,
            presentation,
            "COST: ",
            cost_delta,
            Token::Text,
        );
        push_kv_capsule(
            &mut capsules,
            presentation,
            "TIME: ",
            time_delta,
            Token::Text,
        );
        push_kv_capsule(
            &mut capsules,
            presentation,
            "HIT/READ: ",
            buf_delta,
            Token::Info,
        );
        push_kv_capsule(&mut capsules, presentation, "WAL: ", wal_delta, Token::Text);
    } else if let Some(snap) = single_opt {
        let time_str = snap
            .execution_time_ms
            .map_or_else(|| "n/a".to_owned(), |ms| format!("{ms:.2}ms"));
        let wal_str = crate::telemetry::format_bytes(snap.wal_bytes);
        capsules.push(Span::styled(
            " [SINGLE PLAN] ",
            theme.recessed_pill(Token::Text),
        ));
        capsules.push(Span::raw(" "));
        push_kv_capsule(
            &mut capsules,
            presentation,
            "COST: ",
            format!("{:.2}", snap.total_cost),
            Token::Text,
        );
        push_kv_capsule(&mut capsules, presentation, "TIME: ", time_str, Token::Text);
        push_kv_capsule(
            &mut capsules,
            presentation,
            "HIT/READ: ",
            format!("{}/{}", snap.shared_hit_blocks, snap.shared_read_blocks),
            Token::Info,
        );
        push_kv_capsule(&mut capsules, presentation, "WAL: ", wal_str, Token::Text);
    } else {
        capsules.push(Span::styled(
            " [UNCHANGED] ",
            theme.recessed_pill(Token::Text),
        ));
        capsules.push(Span::raw(" "));
        capsules.push(Span::styled(
            " [NO PLANS CAPTURED] ",
            theme.recessed_pill(Token::Text),
        ));
    }
    render_capsule_bar(capsules, capsule_area, buf);

    let (mode_label, mode_token) = if state.analyze_mode {
        ("[MODE: ANALYZE+BUFFERS+WAL]", Token::Warning)
    } else {
        ("[MODE: ESTIMATE]", Token::Info)
    };
    render_overlay_tabs(
        &[
            " [1] Summary & Diff ",
            " [2] Nodes ",
            " [3] Buffers / WAL / Settings ",
        ],
        state.active_tab,
        Some((mode_label.to_owned(), mode_token)),
        presentation,
        tab_area,
        buf,
    );

    let body_width = usize::from(body_area.width);
    let selection_style = theme.style(Token::Selection);

    if state.loading {
        Paragraph::new(Line::from(Span::styled(
            "Capturing extended EXPLAIN (FORMAT JSON, VERBOSE, SETTINGS)...",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    } else {
        match state.active_tab {
            0 => {
                if let Some(report) = report_opt {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![
                            ("  A (Baseline): ".to_owned(), theme.style(Token::Muted)),
                            (
                                sanitize_for_display(&report.baseline.label),
                                theme.style(Token::Text),
                            ),
                            ("  |  B (Candidate): ".to_owned(), theme.style(Token::Muted)),
                            (
                                sanitize_for_display(&report.candidate.label),
                                theme.style(Token::Text),
                            ),
                        ],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  Cost: {:.2} -> {:.2} ({:+.2})  |  Exec Time: {} -> {} ({})",
                                report.baseline.total_cost,
                                report.candidate.total_cost,
                                report.total_cost_delta,
                                fmt_opt_ms_short(report.baseline.execution_time_ms),
                                fmt_opt_ms_short(report.candidate.execution_time_ms),
                                fmt_opt_ms_delta_short(report.execution_time_delta_ms),
                            ),
                            theme.style(Token::Info),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  Buffers Hit/Read: {}/{} -> {}/{} (Delta {:+}/{:+})  |  Temp Write: {} -> {} ({:+})  |  WAL: {} -> {} ({})",
                                report.baseline.shared_hit_blocks,
                                report.baseline.shared_read_blocks,
                                report.candidate.shared_hit_blocks,
                                report.candidate.shared_read_blocks,
                                report.shared_hit_delta,
                                report.shared_read_delta,
                                report.baseline.temp_written_blocks,
                                report.candidate.temp_written_blocks,
                                report.temp_written_delta,
                                crate::telemetry::format_bytes(report.baseline.wal_bytes),
                                crate::telemetry::format_bytes(report.candidate.wal_bytes),
                                format_signed_bytes(report.wal_bytes_delta),
                            ),
                            theme.style(Token::Text),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(modal_table_header_row(
                        format!(
                            "  FINDINGS ({})  |  NODE CHANGES ({})  |  SETTING CHANGES ({})",
                            report.findings.len(),
                            report.node_changes.len(),
                            report.setting_changes.len()
                        ),
                        body_width,
                        unicode,
                        theme,
                    ));
                    let list_rows = usize::from(body_area.height).saturating_sub(lines.len());
                    if report.findings.is_empty() {
                        lines.push(bounded_row(
                            vec![(
                                "  No findings reported.".to_owned(),
                                theme.style(Token::Muted),
                            )],
                            body_width,
                            unicode,
                            None,
                        ));
                    } else {
                        let start = state
                            .selected_idx
                            .saturating_sub(list_rows.saturating_sub(1));
                        for (idx, finding) in report
                            .findings
                            .iter()
                            .enumerate()
                            .skip(start)
                            .take(list_rows)
                        {
                            let selected = idx == state.selected_idx;
                            let marker = if selected { "> " } else { "  " };
                            let text = sanitize_for_display(finding);
                            let segments = if selected {
                                vec![
                                    (
                                        marker.to_owned(),
                                        selection_style.add_modifier(Modifier::BOLD),
                                    ),
                                    (text, selection_style),
                                ]
                            } else {
                                vec![
                                    (marker.to_owned(), theme.style(Token::Muted)),
                                    (text, theme.style(Token::Text)),
                                ]
                            };
                            lines.push(bounded_row(
                                segments,
                                body_width,
                                unicode,
                                selected.then_some(selection_style),
                            ));
                        }
                    }
                    Paragraph::new(lines).render(body_area, buf);
                } else if let Some(snap) = single_opt {
                    let slot_name = if state.baseline.is_some() {
                        "Baseline (A)"
                    } else {
                        "Candidate (B)"
                    };
                    let lines = vec![
                        bounded_row(
                            vec![(
                                format!(
                                    "  {slot_name}: {}  [analyzed={}]",
                                    sanitize_for_display(&snap.label),
                                    if snap.analyzed { "yes" } else { "no" }
                                ),
                                theme.style(Token::Header).add_modifier(Modifier::BOLD),
                            )],
                            body_width,
                            unicode,
                            None,
                        ),
                        bounded_row(
                            vec![(
                                format!(
                                    "  Cost: {:.2} (startup {:.2})  |  Plan Rows: {}  |  Actual Rows: {}",
                                    snap.total_cost,
                                    snap.startup_cost,
                                    snap.plan_rows,
                                    snap.actual_rows
                                        .map_or_else(|| "-".to_owned(), |r| r.to_string())
                                ),
                                theme.style(Token::Text),
                            )],
                            body_width,
                            unicode,
                            None,
                        ),
                        bounded_row(
                            vec![(
                                format!(
                                    "  Planning: {}  |  Execution: {}  |  Buffer Hit Ratio: {}",
                                    fmt_opt_ms_short(snap.planning_time_ms),
                                    fmt_opt_ms_short(snap.execution_time_ms),
                                    snap.buffer_hit_ratio_pct
                                        .map_or_else(|| "-".to_owned(), |p| format!("{p:.1}%"))
                                ),
                                theme.style(Token::Info),
                            )],
                            body_width,
                            unicode,
                            None,
                        ),
                        bounded_row(
                            vec![(
                                format!(
                                    "  Shared Hit/Read/Dirtied/Written: {}/{}/{}/{}  |  Temp R/W: {}/{}",
                                    snap.shared_hit_blocks,
                                    snap.shared_read_blocks,
                                    snap.shared_dirtied_blocks,
                                    snap.shared_written_blocks,
                                    snap.temp_read_blocks,
                                    snap.temp_written_blocks,
                                ),
                                theme.style(Token::Text),
                            )],
                            body_width,
                            unicode,
                            None,
                        ),
                        bounded_row(
                            vec![(
                                format!(
                                    "  WAL Records: {}  |  WAL FPI: {}  |  WAL Bytes: {}  |  Settings: {}",
                                    snap.wal_records,
                                    snap.wal_fpi,
                                    crate::telemetry::format_bytes(snap.wal_bytes),
                                    snap.settings.len()
                                ),
                                theme.style(Token::Text),
                            )],
                            body_width,
                            unicode,
                            None,
                        ),
                        bounded_row(
                            vec![(
                                "  Capture the second plan with [a] or [b] to compute A/B Plan Diff."
                                    .to_owned(),
                                theme.style(Token::Muted),
                            )],
                            body_width,
                            unicode,
                            None,
                        ),
                    ];
                    Paragraph::new(lines).render(body_area, buf);
                } else {
                    Paragraph::new(Line::from(Span::styled(
                        "No EXPLAIN snapshot captured yet. Press [a] for Baseline A or [b] for Candidate B.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                }
            }
            1 => {
                let body_rows = usize::from(body_area.height).saturating_sub(1);
                if let Some(report) = report_opt {
                    if report.node_changes.is_empty() {
                        Paragraph::new(Line::from(Span::styled(
                            "No material node changes detected between Baseline A and Candidate B.",
                            theme.style(Token::Muted),
                        )))
                        .render(body_area, buf);
                    } else {
                        let mut lines = Vec::new();
                        lines.push(modal_table_header_row(
                            format!(
                                "  {:<8} {:>10} {:>10} {}",
                                "VERDICT", "COST_D", "TIME_D", "NODE CHANGE"
                            ),
                            body_width,
                            unicode,
                            theme,
                        ));
                        let start = state
                            .selected_idx
                            .saturating_sub(body_rows.saturating_sub(1));
                        for (idx, change) in report
                            .node_changes
                            .iter()
                            .enumerate()
                            .skip(start)
                            .take(body_rows)
                        {
                            let selected = idx == state.selected_idx;
                            let marker = if selected { "> " } else { "  " };
                            let (sev_label, sev_token) = match change.severity {
                                crate::plandiff::DiffVerdict::Improvement => {
                                    ("IMPR", Token::Success)
                                }
                                crate::plandiff::DiffVerdict::Regression => ("REGR", Token::Danger),
                                crate::plandiff::DiffVerdict::Mixed => ("MIXED", Token::Warning),
                                crate::plandiff::DiffVerdict::Unchanged => ("SAME", Token::Muted),
                            };
                            let cost_d = format!("{:+.2}", change.cost_delta);
                            let time_d = change
                                .time_delta_ms
                                .map_or_else(|| "-".to_owned(), |t| format!("{t:+.2}ms"));
                            let desc = sanitize_for_display(&change.description);
                            let segments = if selected {
                                vec![
                                    (
                                        marker.to_owned(),
                                        selection_style.add_modifier(Modifier::BOLD),
                                    ),
                                    (
                                        format!("{sev_label:<8} "),
                                        selection_style.add_modifier(Modifier::BOLD),
                                    ),
                                    (format!("{cost_d:>10} "), selection_style),
                                    (format!("{time_d:>10} "), selection_style),
                                    (desc, selection_style),
                                ]
                            } else {
                                vec![
                                    (marker.to_owned(), theme.style(Token::Muted)),
                                    (
                                        format!("{sev_label:<8} "),
                                        theme.style(sev_token).add_modifier(Modifier::BOLD),
                                    ),
                                    (format!("{cost_d:>10} "), theme.style(Token::Text)),
                                    (format!("{time_d:>10} "), theme.style(Token::Info)),
                                    (desc, theme.style(Token::Text)),
                                ]
                            };
                            lines.push(bounded_row(
                                segments,
                                body_width,
                                unicode,
                                selected.then_some(selection_style),
                            ));
                        }
                        Paragraph::new(lines).render(body_area, buf);
                    }
                } else if let Some(snap) = single_opt {
                    let mut lines = Vec::new();
                    lines.push(modal_table_header_row(
                        format!(
                            "  {:<8} {:<20} {:<20} {:>10} {:>10} {:>11}",
                            "PATH", "NODE", "TARGET", "COST", "ROWS", "HIT/READ"
                        ),
                        body_width,
                        unicode,
                        theme,
                    ));
                    for node in snap.nodes.iter().take(body_rows) {
                        let target = node
                            .relation_name
                            .as_deref()
                            .or(node.index_name.as_deref())
                            .unwrap_or("-");
                        let rows_text = node.actual_rows.unwrap_or(node.plan_rows).to_string();
                        let hr = format!("{}/{}", node.shared_hit_blocks, node.shared_read_blocks);
                        lines.push(bounded_row(
                            vec![(
                                format!(
                                    "  {:<8} {} {} {:>10.2} {:>10} {:>11}",
                                    fit_cell(&node.path, 8, unicode),
                                    fit_cell(&node.node_type, 20, unicode),
                                    fit_cell(target, 20, unicode),
                                    node.total_cost,
                                    rows_text,
                                    hr
                                ),
                                theme.style(Token::Text),
                            )],
                            body_width,
                            unicode,
                            None,
                        ));
                    }
                    Paragraph::new(lines).render(body_area, buf);
                } else {
                    Paragraph::new(Line::from(Span::styled(
                        "No plan nodes available yet. Press [a] or [b] to capture an EXPLAIN snapshot.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                }
            }
            2 => {
                if let Some(report) = report_opt {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  Shared Hit/Read: A {}/{} -> B {}/{} (Delta {:+} / {:+})",
                                report.baseline.shared_hit_blocks,
                                report.baseline.shared_read_blocks,
                                report.candidate.shared_hit_blocks,
                                report.candidate.shared_read_blocks,
                                report.shared_hit_delta,
                                report.shared_read_delta
                            ),
                            theme.style(Token::Info),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  Dirtied/Written: A {}/{} -> B {}/{}  |  Temp R/W: A {}/{} -> B {}/{}",
                                report.baseline.shared_dirtied_blocks,
                                report.baseline.shared_written_blocks,
                                report.candidate.shared_dirtied_blocks,
                                report.candidate.shared_written_blocks,
                                report.baseline.temp_read_blocks,
                                report.baseline.temp_written_blocks,
                                report.candidate.temp_read_blocks,
                                report.candidate.temp_written_blocks,
                            ),
                            theme.style(Token::Text),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  WAL (Rec/FPI/Bytes): A {}/{}/{} -> B {}/{}/{} (Delta {})",
                                report.baseline.wal_records,
                                report.baseline.wal_fpi,
                                crate::telemetry::format_bytes(report.baseline.wal_bytes),
                                report.candidate.wal_records,
                                report.candidate.wal_fpi,
                                crate::telemetry::format_bytes(report.candidate.wal_bytes),
                                format_signed_bytes(report.wal_bytes_delta),
                            ),
                            theme.style(Token::Text),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(modal_table_header_row(
                        format!(
                            "  {:<32} {:<20} {}",
                            "SETTING", "BASELINE (A)", "CANDIDATE (B)"
                        ),
                        body_width,
                        unicode,
                        theme,
                    ));
                    let list_rows = usize::from(body_area.height).saturating_sub(lines.len());
                    if report.setting_changes.is_empty() {
                        lines.push(bounded_row(
                            vec![(
                                "  No planner setting differences between Baseline A and Candidate B."
                                    .to_owned(),
                                theme.style(Token::Muted),
                            )],
                            body_width,
                            unicode,
                            None,
                        ));
                    } else {
                        let start = state
                            .selected_idx
                            .saturating_sub(list_rows.saturating_sub(1));
                        for (idx, sc) in report
                            .setting_changes
                            .iter()
                            .enumerate()
                            .skip(start)
                            .take(list_rows)
                        {
                            let selected = idx == state.selected_idx;
                            let marker = if selected { "> " } else { "  " };
                            let name = fit_cell(&sc.name, 32, unicode);
                            let base_val = fit_cell(
                                sc.baseline.as_deref().unwrap_or("(default)"),
                                20,
                                unicode,
                            );
                            let cand_val = sanitize_for_display(
                                sc.candidate.as_deref().unwrap_or("(default)"),
                            );
                            let segments = if selected {
                                vec![
                                    (
                                        marker.to_owned(),
                                        selection_style.add_modifier(Modifier::BOLD),
                                    ),
                                    (format!("{name} "), selection_style),
                                    (format!("{base_val} "), selection_style),
                                    (cand_val, selection_style),
                                ]
                            } else {
                                vec![
                                    (marker.to_owned(), theme.style(Token::Muted)),
                                    (format!("{name} "), theme.style(Token::Text)),
                                    (format!("{base_val} "), theme.style(Token::Muted)),
                                    (cand_val, theme.style(Token::Info)),
                                ]
                            };
                            lines.push(bounded_row(
                                segments,
                                body_width,
                                unicode,
                                selected.then_some(selection_style),
                            ));
                        }
                    }
                    Paragraph::new(lines).render(body_area, buf);
                } else if let Some(snap) = single_opt {
                    let mut lines = Vec::new();
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  Single-Plan Buffers: Shared Hit={} Read={} Dirtied={} Written={}  |  Local Hit/Read={}/{}",
                                snap.shared_hit_blocks,
                                snap.shared_read_blocks,
                                snap.shared_dirtied_blocks,
                                snap.shared_written_blocks,
                                snap.local_hit_blocks,
                                snap.local_read_blocks,
                            ),
                            theme.style(Token::Info),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(bounded_row(
                        vec![(
                            format!(
                                "  Temp Read/Written={}/{}  |  WAL Records={} FPI={} Bytes={}",
                                snap.temp_read_blocks,
                                snap.temp_written_blocks,
                                snap.wal_records,
                                snap.wal_fpi,
                                crate::telemetry::format_bytes(snap.wal_bytes),
                            ),
                            theme.style(Token::Text),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                    lines.push(modal_table_header_row(
                        format!("  {:<36} {}", "NON-DEFAULT PLANNER SETTING", "VALUE"),
                        body_width,
                        unicode,
                        theme,
                    ));
                    let list_rows = usize::from(body_area.height).saturating_sub(lines.len());
                    if snap.settings.is_empty() {
                        lines.push(bounded_row(
                            vec![(
                                "  No non-default planner settings reported by EXPLAIN (SETTINGS)."
                                    .to_owned(),
                                theme.style(Token::Muted),
                            )],
                            body_width,
                            unicode,
                            None,
                        ));
                    } else {
                        let start = state
                            .selected_idx
                            .saturating_sub(list_rows.saturating_sub(1));
                        for (idx, (name, value)) in
                            snap.settings.iter().enumerate().skip(start).take(list_rows)
                        {
                            let selected = idx == state.selected_idx;
                            let marker = if selected { "> " } else { "  " };
                            let col_name = fit_cell(name, 36, unicode);
                            let col_val = sanitize_for_display(value);
                            let segments = if selected {
                                vec![
                                    (
                                        marker.to_owned(),
                                        selection_style.add_modifier(Modifier::BOLD),
                                    ),
                                    (format!("{col_name} "), selection_style),
                                    (col_val, selection_style),
                                ]
                            } else {
                                vec![
                                    (marker.to_owned(), theme.style(Token::Muted)),
                                    (format!("{col_name} "), theme.style(Token::Text)),
                                    (col_val, theme.style(Token::Info)),
                                ]
                            };
                            lines.push(bounded_row(
                                segments,
                                body_width,
                                unicode,
                                selected.then_some(selection_style),
                            ));
                        }
                    }
                    Paragraph::new(lines).render(body_area, buf);
                } else {
                    Paragraph::new(Line::from(Span::styled(
                        "No Buffers / WAL / Settings data yet. Press [a] or [b] to capture a plan.",
                        theme.style(Token::Muted),
                    )))
                    .render(body_area, buf);
                }
            }
            _ => {}
        }
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        (sanitize_for_display(msg), Token::Info)
    } else if let Some(report) = report_opt {
        (
            format!(
                "Diff Verdict: {}  |  A: {}  vs  B: {}",
                report.verdict.label(),
                sanitize_for_display(&report.baseline.label),
                sanitize_for_display(&report.candidate.label)
            ),
            Token::Muted,
        )
    } else {
        (
            "Press [a] to capture Baseline A, [b] to capture Candidate B, [m] for ANALYZE+BUFFERS+WAL."
                .to_owned(),
            Token::Muted,
        )
    };
    Paragraph::new(Line::from(Span::styled(
        truncate_to_width(&status_text, usize::from(status_area.width), unicode),
        theme.style(status_token),
    )))
    .render(status_area, buf);

    let footer_segments = vec![
        (
            "[1-3] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Tabs  ".to_owned(), theme.style(Token::Muted)),
        (
            "[a] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Capture Baseline A  ".to_owned(), theme.style(Token::Muted)),
        (
            "[b] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        (
            "Capture Candidate B  ".to_owned(),
            theme.style(Token::Muted),
        ),
        (
            "[m] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        (
            "Toggle Analyze+Buffers+WAL  ".to_owned(),
            theme.style(Token::Muted),
        ),
        (
            "[j/k] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Select  ".to_owned(), theme.style(Token::Muted)),
        (
            "[r] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Refresh  ".to_owned(), theme.style(Token::Muted)),
        (
            "[Esc] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Close".to_owned(), theme.style(Token::Muted)),
    ];
    Paragraph::new(bounded_row(
        footer_segments,
        usize::from(footer_area.width),
        unicode,
        None,
    ))
    .render(footer_area, buf);
}

fn format_signed_bytes(delta: i64) -> String {
    if delta == 0 {
        return "0 B".to_owned();
    }
    let sign = if delta > 0 { "+" } else { "-" };
    format!(
        "{sign}{}",
        crate::telemetry::format_bytes(delta.unsigned_abs())
    )
}

fn fmt_opt_ms_short(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.2}ms"))
}

fn fmt_opt_ms_delta_short(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |v| format!("{v:+.2}ms"))
}

/// Renders the Zero-Query Result Analytics overlay (`055`).
pub(crate) fn render_result_analytics(
    state: &crate::app::model::ResultAnalyticsState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_result_analytics_frame(state, presentation, area, target, fit);
    });
}

fn render_result_analytics_frame(
    state: &crate::app::model::ResultAnalyticsState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let title = format!(
        " {} RESULT ANALYTICS (ZERO-QUERY) ",
        presentation.icon(Icon::Rows)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let analytics = &state.analytics;
    let mut capsules = vec![Span::raw(" ")];
    push_kv_capsule(
        &mut capsules,
        presentation,
        "ROWS: ",
        format!("{}/{}", analytics.row_count, analytics.rows_seen),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "COLUMNS: ",
        format!("{}", analytics.column_count),
        Token::Text,
    );
    if analytics.truncated {
        capsules.push(Span::styled(
            " [TRUNCATED] ",
            theme.recessed_pill(Token::Warning),
        ));
        capsules.push(Span::raw(" "));
    }
    push_kv_capsule(
        &mut capsules,
        presentation,
        "PROFILE: ",
        "ZERO-QUERY".to_owned(),
        Token::Success,
    );
    render_capsule_bar(capsules, capsule_area, buf);

    render_overlay_tabs(
        &[
            " [1] Column Profiles & Sparklines ",
            " [2] Top Values & Detail ",
        ],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

    let body_width = usize::from(body_area.width);
    let body_rows = usize::from(body_area.height).saturating_sub(1);
    let selection_style = theme.style(Token::Selection);

    match state.active_tab {
        0 => {
            if analytics.columns.is_empty() {
                Paragraph::new(Line::from(Span::styled(
                    "No columns available to profile.",
                    theme.style(Token::Muted),
                )))
                .render(body_area, buf);
            } else {
                let mut lines = Vec::new();
                lines.push(bounded_row(
                    vec![(
                        format!(
                            "  {:<18} {:<11} {:>6} {:>8} {:<11} {:<11} {:>9} {:>9} {:>9} {}",
                            "COLUMN",
                            "TYPE",
                            "NULL%",
                            "DIST",
                            "MIN",
                            "MAX",
                            "MEAN",
                            "P50",
                            "P95",
                            "SPARK"
                        ),
                        theme.style(Token::Header).add_modifier(Modifier::BOLD),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
                let start = state
                    .selected_col
                    .saturating_sub(body_rows.saturating_sub(1));
                for (idx, col) in analytics
                    .columns
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(body_rows)
                {
                    let selected = idx == state.selected_col;
                    let marker = if selected { "> " } else { "  " };
                    let name = fit_cell(&col.name, 18, unicode);
                    let typ = fit_cell(&col.type_name, 11, unicode);
                    let null_pct = format!("{:.1}%", col.null_pct);
                    let distinct = if col.distinct_capped {
                        format!("{}+", col.distinct_count)
                    } else {
                        col.distinct_count.to_string()
                    };
                    let min_str = fit_cell(
                        &format_analytics_opt_str(col.min_value.as_deref()),
                        11,
                        unicode,
                    );
                    let max_str = fit_cell(
                        &format_analytics_opt_str(col.max_value.as_deref()),
                        11,
                        unicode,
                    );
                    let mean_str = format_analytics_opt_f64(col.mean);
                    let p50_str = format_analytics_opt_f64(col.p50);
                    let p95_str = format_analytics_opt_f64(col.p95);
                    let spark = if presentation.glyphs.is_ascii() {
                        col.sparkline_ascii.clone()
                    } else {
                        col.sparkline_unicode.clone()
                    };
                    let segments = if selected {
                        vec![
                            (
                                marker.to_owned(),
                                selection_style.add_modifier(Modifier::BOLD),
                            ),
                            (format!("{name} "), selection_style),
                            (format!("{typ} "), selection_style),
                            (format!("{null_pct:>6} "), selection_style),
                            (format!("{distinct:>8} "), selection_style),
                            (format!("{min_str} "), selection_style),
                            (format!("{max_str} "), selection_style),
                            (format!("{mean_str:>9} "), selection_style),
                            (format!("{p50_str:>9} "), selection_style),
                            (format!("{p95_str:>9} "), selection_style),
                            (spark, selection_style),
                        ]
                    } else {
                        vec![
                            (marker.to_owned(), theme.style(Token::Muted)),
                            (format!("{name} "), theme.style(Token::Text)),
                            (format!("{typ} "), theme.style(Token::Info)),
                            (
                                format!("{null_pct:>6} "),
                                theme.style(if col.null_count > 0 {
                                    Token::Warning
                                } else {
                                    Token::Muted
                                }),
                            ),
                            (format!("{distinct:>8} "), theme.style(Token::Text)),
                            (format!("{min_str} "), theme.style(Token::Muted)),
                            (format!("{max_str} "), theme.style(Token::Muted)),
                            (format!("{mean_str:>9} "), theme.style(Token::Text)),
                            (format!("{p50_str:>9} "), theme.style(Token::Text)),
                            (format!("{p95_str:>9} "), theme.style(Token::Text)),
                            (spark, theme.style(Token::Focus)),
                        ]
                    };
                    lines.push(bounded_row(
                        segments,
                        body_width,
                        unicode,
                        selected.then_some(selection_style),
                    ));
                }
                Paragraph::new(lines).render(body_area, buf);
            }
        }
        1 => {
            if let Some(col) = analytics.columns.get(state.selected_col) {
                let mut lines = Vec::new();
                let distinct_label = if col.distinct_capped {
                    format!("{}+", col.distinct_count)
                } else {
                    col.distinct_count.to_string()
                };
                let spark = if presentation.glyphs.is_ascii() {
                    col.sparkline_ascii.as_str()
                } else {
                    col.sparkline_unicode.as_str()
                };
                lines.push(bounded_row(
                    vec![(
                        format!(
                            "  Column: {} ({})  |  Rows: {}  Non-Null: {}  Nulls: {} ({:.1}%)  Empty: {}",
                            sanitize_for_display(&col.name),
                            sanitize_for_display(&col.type_name),
                            col.total_rows,
                            col.non_null_count,
                            col.null_count,
                            col.null_pct,
                            col.empty_count
                        ),
                        theme.style(Token::Header).add_modifier(Modifier::BOLD),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
                lines.push(bounded_row(
                    vec![(
                        format!(
                            "  Distinct: {}  |  Min: {}  Max: {}  |  Sparkline: [{}]",
                            distinct_label,
                            format_analytics_opt_str(col.min_value.as_deref()),
                            format_analytics_opt_str(col.max_value.as_deref()),
                            spark
                        ),
                        theme.style(Token::Info),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
                lines.push(bounded_row(
                    vec![(
                        format!(
                            "  Numeric ({}): Mean={}  StdDev={}  p50={}  p95={}",
                            col.numeric_count,
                            format_analytics_opt_f64(col.mean),
                            format_analytics_opt_f64(col.stddev),
                            format_analytics_opt_f64(col.p50),
                            format_analytics_opt_f64(col.p95),
                        ),
                        theme.style(Token::Text),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
                lines.push(bounded_row(
                    vec![(
                        "  TOP FREQUENT VALUES".to_owned(),
                        theme.style(Token::Header).add_modifier(Modifier::BOLD),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
                if col.top_values.is_empty() {
                    lines.push(bounded_row(
                        vec![(
                            "    (no non-null values observed)".to_owned(),
                            theme.style(Token::Muted),
                        )],
                        body_width,
                        unicode,
                        None,
                    ));
                } else {
                    let max_top = col
                        .top_values
                        .iter()
                        .map(|(_, count)| *count)
                        .max()
                        .unwrap_or(0);
                    for (rank, (val, count)) in col.top_values.iter().enumerate() {
                        let pct = if col.non_null_count > 0 {
                            (*count as f64) * 100.0 / (col.non_null_count as f64)
                        } else {
                            0.0
                        };
                        let display_val = if val.is_empty() {
                            "\"\"".to_owned()
                        } else {
                            sanitize_for_display(val)
                        };
                        let meter = proportional_meter(*count, max_top, 16, unicode);
                        lines.push(bounded_row(
                            vec![
                                (format!("    #{:<2} ", rank + 1), theme.style(Token::Muted)),
                                (
                                    format!("{:<28} ", fit_cell(&display_val, 28, unicode)),
                                    theme.style(Token::Text),
                                ),
                                (
                                    format!("{count:>6} ({pct:>5.1}%)  "),
                                    theme.style(Token::Info),
                                ),
                                (meter, theme.style(Token::Info)),
                            ],
                            body_width,
                            unicode,
                            None,
                        ));
                    }
                }
                lines.push(bounded_row(
                    vec![(
                        "  HISTOGRAM BUCKETS".to_owned(),
                        theme.style(Token::Header).add_modifier(Modifier::BOLD),
                    )],
                    body_width,
                    unicode,
                    None,
                ));
                let max_bucket = col.histogram_counts.iter().copied().max().unwrap_or(0);
                for (bucket_idx, count) in col.histogram_counts.iter().enumerate() {
                    let bar = proportional_meter(*count, max_bucket, 20, unicode);
                    lines.push(bounded_row(
                        vec![
                            (
                                format!("    B{}: ", bucket_idx + 1),
                                theme.style(Token::Muted),
                            ),
                            (format!("{count:>6}  "), theme.style(Token::Text)),
                            (bar, theme.style(Token::Focus)),
                        ],
                        body_width,
                        unicode,
                        None,
                    ));
                }
                Paragraph::new(lines).render(body_area, buf);
            } else {
                Paragraph::new(Line::from(Span::styled(
                    "No column selected.",
                    theme.style(Token::Muted),
                )))
                .render(body_area, buf);
            }
        }
        _ => {}
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        (sanitize_for_display(msg), Token::Info)
    } else if analytics.truncated {
        (
            format!(
                "Notice: zero-query profile covers {} retained row(s) of {} seen.",
                analytics.row_count, analytics.rows_seen
            ),
            Token::Warning,
        )
    } else if let Some(col) = analytics.columns.get(state.selected_col) {
        (
            format!(
                "Selected: {} ({})  |  nulls={}  empty=\"\"={}  distinct={}",
                sanitize_for_display(&col.name),
                sanitize_for_display(&col.type_name),
                col.null_count,
                col.empty_count,
                col.distinct_count
            ),
            Token::Muted,
        )
    } else {
        (String::new(), Token::Muted)
    };
    Paragraph::new(Line::from(Span::styled(
        truncate_to_width(&status_text, usize::from(status_area.width), unicode),
        theme.style(status_token),
    )))
    .render(status_area, buf);

    let footer_segments = vec![
        (
            "[1-2] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Tabs  ".to_owned(), theme.style(Token::Muted)),
        (
            "[j/k] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Column  ".to_owned(), theme.style(Token::Muted)),
        (
            "[e] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        (
            "Export CSV/HTML/Parquet  ".to_owned(),
            theme.style(Token::Muted),
        ),
        (
            "[Esc] ".to_owned(),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        ("Close".to_owned(), theme.style(Token::Muted)),
    ];
    Paragraph::new(bounded_row(
        footer_segments,
        usize::from(footer_area.width),
        unicode,
        None,
    ))
    .render(footer_area, buf);
}

fn proportional_meter(value: usize, max: usize, width_cells: usize, unicode: bool) -> String {
    if value == 0 || max == 0 || width_cells == 0 {
        return String::new();
    }
    if !unicode {
        let len = ((value.saturating_mul(width_cells)) / max).max(1);
        return "#".repeat(len.min(width_cells));
    }
    const EIGHTHS: [char; 9] = [
        ' ', '\u{258f}', '\u{258e}', '\u{258d}', '\u{258c}', '\u{258b}', '\u{258a}', '\u{2589}',
        '\u{2588}',
    ];
    let total_eighths = ((value.saturating_mul(width_cells).saturating_mul(8)) / max)
        .clamp(1, width_cells.saturating_mul(8));
    let full = total_eighths / 8;
    let rem = total_eighths % 8;
    let mut out = "\u{2588}".repeat(full);
    if rem > 0 {
        out.push(EIGHTHS[rem]);
    }
    out
}

fn format_analytics_opt_str(value: Option<&str>) -> String {
    match value {
        None => "-".to_owned(),
        Some("") => "\"\"".to_owned(),
        Some(text) => sanitize_for_display(text),
    }
}

fn format_analytics_opt_f64(value: Option<f64>) -> String {
    match value {
        Some(v) if v.is_finite() => format!("{v:.2}"),
        _ => "-".to_owned(),
    }
}

fn push_kv_capsule(
    spans: &mut Vec<Span<'static>>,
    presentation: &Presentation,
    label: &str,
    value: String,
    value_token: Token,
) {
    let theme = &presentation.theme;
    let mut label_style = theme.recessed_pill(Token::Text);
    label_style.add_modifier.remove(Modifier::BOLD);
    spans.push(Span::styled(format!(" [{label}"), label_style));
    spans.push(Span::styled(value, theme.recessed_pill(value_token)));
    spans.push(Span::styled("] ", label_style));
    spans.push(Span::raw(" "));
}

/// Secondary modal metadata: identity, counts and outcomes rest on a
/// recessed pill (Principle XI, tier two), so the full capsule stays rare
/// enough to mean something.
fn push_pill_capsule(
    spans: &mut Vec<Span<'static>>,
    presentation: &Presentation,
    text: String,
    token: Token,
) {
    spans.push(Span::styled(
        format!(" {text} "),
        presentation.theme.recessed_pill(token),
    ));
    spans.push(Span::raw(" "));
}

/// A high-salience safety state inside a modal, such as a failed gate,
/// breaking drift, a live run that commits, or writes allowed. Only these
/// take the full capsule (Principle XI, tier three); colour off reverses it.
fn push_safety_capsule(
    spans: &mut Vec<Span<'static>>,
    presentation: &Presentation,
    text: String,
    token: Token,
) {
    spans.push(Span::styled(
        format!(" {text} "),
        super::header::capsule_style(presentation, token),
    ));
    spans.push(Span::raw(" "));
}

/// A count badge whose token falls back to `Muted` at zero, so an empty
/// category never reads as an alarm.
const fn count_token(count: usize, token: Token) -> Token {
    if count == 0 { Token::Muted } else { token }
}

fn render_capsule_bar(capsules: Vec<Span<'static>>, area: Rect, buf: &mut Buffer) {
    let total_width: usize = capsules.iter().map(|s| display_width(&s.content)).sum();
    let max_width = usize::from(area.width);
    let fitted = if total_width <= max_width {
        capsules
    } else {
        capsules
            .into_iter()
            .map(|mut span| {
                if span.content.starts_with(" [") {
                    span.content = span.content[1..].to_owned().into();
                }
                if span.content.ends_with("] ") {
                    let end = span.content.len() - 1;
                    span.content = span.content[..end].to_owned().into();
                }
                span
            })
            .collect()
    };
    Paragraph::new(Line::from(fitted)).render(area, buf);
}

fn render_overlay_tabs(
    labels: &[&str],
    active_tab: usize,
    trailing: Option<(String, Token)>,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let mut tab_spans = Vec::new();
    for (idx, label) in labels.iter().enumerate() {
        if idx > 0 {
            tab_spans.push(Span::styled(" | ", theme.style(Token::Border)));
        }
        let style = if idx == active_tab {
            theme.active_tab(Token::Focus)
        } else {
            theme.style(Token::Text)
        };
        tab_spans.push(Span::styled((*label).to_owned(), style));
    }
    if let Some((badge, token)) = trailing {
        tab_spans.push(Span::raw("  "));
        tab_spans.push(Span::styled(
            format!(" {badge} "),
            theme.recessed_pill(token),
        ));
    }
    Paragraph::new(Line::from(tab_spans)).render(area, buf);
}

fn render_footer_hints(
    hints: &[(&str, &str)],
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let keycap = theme.recessed_pill(Token::Focus);
    // The workbench footer's separator, so key and action still pair up when
    // the keycap fill is gone at 16 colours, colour off or ASCII. The keycap's
    // own leading pad supplies the space after it.
    let separator = if unicode { " \u{00b7}" } else { " |" };
    let mut segments = Vec::with_capacity(hints.len() * 3);
    for (idx, (key, desc)) in hints.iter().enumerate() {
        if idx > 0 {
            segments.push((separator.to_owned(), theme.style(Token::Border)));
        }
        segments.push((format!(" {key} "), keycap));
        segments.push(((*desc).to_owned(), theme.style(Token::Text)));
    }
    Paragraph::new(bounded_row(
        segments,
        usize::from(area.width),
        unicode,
        None,
    ))
    .render(area, buf);
}

fn render_overlay_status_styled(
    text: &str,
    style: Style,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let unicode = !presentation.glyphs.is_ascii();
    Paragraph::new(Line::from(Span::styled(
        truncate_to_width(text, usize::from(area.width), unicode),
        style,
    )))
    .render(area, buf);
}

fn render_overlay_status(
    text: &str,
    token: Token,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_overlay_status_styled(
        text,
        presentation.theme.style(token),
        presentation,
        area,
        buf,
    );
}

fn latency_delta_token(delta_ms: f64) -> Token {
    if delta_ms < -1e-9 {
        Token::Success
    } else if delta_ms > 1e-9 {
        Token::Danger
    } else {
        Token::Muted
    }
}

fn qps_delta_token(delta_qps: f64) -> Token {
    if delta_qps > 1e-9 {
        Token::Success
    } else if delta_qps < -1e-9 {
        Token::Danger
    } else {
        Token::Muted
    }
}

fn bench_settle_style(
    theme: &crate::ui::theme::Theme,
    resting: Token,
    settle: Option<f32>,
) -> Style {
    let base = theme.style(resting);
    let overlay = theme.rgb(Token::SurfaceOverlay);
    let legible = [Token::Success, resting]
        .into_iter()
        .all(|token| theme.rgb(token).contrast(overlay) >= 4.5);
    match settle
        .filter(|_| legible)
        .and_then(|progress| theme.mix(Token::Success, resting, progress))
    {
        Some(ink) => base.fg(ink),
        None => base,
    }
}

fn single_line_preview(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    sanitize_for_display(&collapsed)
}

fn bench_sparkline<'a>(
    stats: &'a crate::bench::BenchStats,
    presentation: &Presentation,
) -> &'a str {
    if presentation.glyphs.is_ascii() {
        stats.sparkline_ascii.as_str()
    } else {
        stats.sparkline_unicode.as_str()
    }
}

fn fmt_bench_ms_delta(base: f64, cand: Option<f64>) -> Option<String> {
    cand.map(|c| {
        let delta = c - base;
        if base.abs() > 1e-9 {
            let pct = (delta / base) * 100.0;
            format!("{delta:+.3} ms ({pct:+.1}%)")
        } else {
            format!("{delta:+.3} ms")
        }
    })
}

/// Renders the Query Latency Benchmark & A/B Regression Suite overlay (`056`).
#[cfg(test)]
pub(crate) fn render_query_bench(
    state: &crate::app::model::QueryBenchState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_query_bench_with_motion(state, 0, None, presentation, area, buf);
}

/// Renders the Query Latency Benchmark overlay with live spinner/heartbeat frame
/// and optional `BenchSettle` transition progress.
pub(crate) fn render_query_bench_with_motion(
    state: &crate::app::model::QueryBenchState,
    frame: u64,
    settle: Option<f32>,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    // While a run is in flight, hold the box at the size its retained
    // results need, so a re-run does not shrink the modal and grow it back.
    let held = if state.loading {
        let settled = crate::app::model::QueryBenchState {
            loading: false,
            cancelling: false,
            ..state.clone()
        };
        measure_modal_body(area, |fit, scratch| {
            render_query_bench_frame(&settled, frame, settle, presentation, area, scratch, fit);
        })
    } else {
        0
    };
    render_fitted_modal(area, buf, held, |fit, target| {
        render_query_bench_frame(state, frame, settle, presentation, area, target, fit);
    });
}

fn render_query_bench_frame(
    state: &crate::app::model::QueryBenchState,
    frame: u64,
    settle: Option<f32>,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let title = format!(
        " {}Query benchmark & A/B regression suite ",
        presentation.icon(Icon::Clock)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let mut capsules = vec![Span::raw(" ")];
    push_pill_capsule(
        &mut capsules,
        presentation,
        "[BENCHMARK]".to_owned(),
        Token::Focus,
    );
    if state.config.rollback {
        push_kv_capsule(
            &mut capsules,
            presentation,
            "MODE: ",
            "ROLLBACK".to_owned(),
            Token::Success,
        );
    } else {
        // Every run commits in this mode, so it is a safety state, not
        // metadata.
        push_safety_capsule(
            &mut capsules,
            presentation,
            "[MODE: LIVE]".to_owned(),
            Token::Warning,
        );
    }
    push_kv_capsule(
        &mut capsules,
        presentation,
        "WARMUP: ",
        state.config.warmup_runs.to_string(),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "RUNS: ",
        state.config.measured_runs.to_string(),
        Token::Text,
    );
    if let Some(report) = &state.report {
        if let Some(cmp) = &report.comparison {
            let (verdict_label, verdict_token) = match cmp.verdict {
                crate::bench::BenchVerdict::Faster => ("[FASTER]", Token::Success),
                crate::bench::BenchVerdict::Slower => ("[SLOWER]", Token::Danger),
                crate::bench::BenchVerdict::Unchanged => ("[UNCHANGED]", Token::Muted),
            };
            push_pill_capsule(
                &mut capsules,
                presentation,
                verdict_label.to_owned(),
                verdict_token,
            );
        } else {
            push_pill_capsule(
                &mut capsules,
                presentation,
                "[SINGLE]".to_owned(),
                Token::Info,
            );
        }
        if report.failed_gate {
            push_safety_capsule(
                &mut capsules,
                presentation,
                "[GATE: FAILED]".to_owned(),
                Token::Danger,
            );
        }
    }
    render_capsule_bar(capsules, capsule_area, buf);

    render_overlay_tabs(
        &[" [1] Summary & A/B Compare ", " [2] Latency Samples "],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

    let body_width = usize::from(body_area.width);
    let selection_style = theme.style(Token::Selection);

    if state.loading {
        let (headline, headline_token) = if state.cancelling {
            (
                format!(
                    "Cancelling query benchmark ({} warmup, {} measured, rollback={})...",
                    state.config.warmup_runs,
                    state.config.measured_runs,
                    if state.config.rollback { "on" } else { "off" }
                ),
                Token::Warning,
            )
        } else {
            (
                format!(
                    "Running query benchmark ({} warmup, {} measured, rollback={})...",
                    state.config.warmup_runs,
                    state.config.measured_runs,
                    if state.config.rollback { "on" } else { "off" }
                ),
                Token::Info,
            )
        };
        let mut head_segments = Vec::new();
        if presentation.reduced_motion {
            head_segments.push(("  ".to_owned(), theme.style(headline_token)));
        } else {
            head_segments.push((
                format!("  {} ", presentation.glyphs.spinner(frame)),
                theme.style(headline_token),
            ));
        }
        head_segments.push((headline, theme.style(headline_token)));
        let mut lines = vec![bounded_row(head_segments, body_width, unicode, None)];
        if !presentation.reduced_motion {
            let mut comet_spans = vec![Span::raw("  ")];
            comet_spans.extend(
                super::results::heartbeat_line(frame, state.cancelling, presentation).spans,
            );
            lines.push(Line::from(comet_spans));
        }
        if !state.baseline_sql.trim().is_empty() {
            lines.push(bounded_row(
                vec![
                    ("  Query A: ".to_owned(), theme.style(Token::Muted)),
                    (
                        single_line_preview(&state.baseline_sql),
                        theme.style(Token::Text),
                    ),
                ],
                body_width,
                unicode,
                None,
            ));
        }
        if !state.candidate_sql.trim().is_empty() {
            lines.push(bounded_row(
                vec![
                    ("  Query B: ".to_owned(), theme.style(Token::Muted)),
                    (
                        single_line_preview(&state.candidate_sql),
                        theme.style(Token::Text),
                    ),
                ],
                body_width,
                unicode,
                None,
            ));
        }
        Paragraph::new(lines).render(body_area, buf);
    } else {
        match state.active_tab {
            0 => render_query_bench_summary(state, presentation, body_area, buf, selection_style),
            1 => render_query_bench_samples(state, presentation, body_area, buf, selection_style),
            _ => {}
        }
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        let tok = if state.report.as_ref().is_some_and(|r| r.failed_gate)
            || msg.contains("failed")
            || msg.contains("Could not deliver")
        {
            Token::Danger
        } else if state.cancelling || msg.contains("cancelled") || msg.contains("Cancellation") {
            Token::Warning
        } else {
            Token::Info
        };
        (sanitize_for_display(msg), tok)
    } else if let Some(report) = &state.report {
        if let Some(cmp) = &report.comparison {
            (
                format!(
                    "Verdict: {}  |  Speedup: {:.2}x  |  p50: {:+.3}ms ({:+.1}%)  |  p95: {:+.3}ms ({:+.1}%)",
                    cmp.verdict.label(),
                    cmp.speedup_ratio,
                    cmp.p50_delta_ms,
                    cmp.p50_delta_pct,
                    cmp.p95_delta_ms,
                    cmp.p95_delta_pct,
                ),
                if report.failed_gate {
                    Token::Danger
                } else {
                    Token::Muted
                },
            )
        } else {
            (
                format!(
                    "Single-query benchmark: p50={:.3}ms  p95={:.3}ms  p99={:.3}ms  qps={:.1}",
                    report.baseline.p50_ms,
                    report.baseline.p95_ms,
                    report.baseline.p99_ms,
                    report.baseline.qps,
                ),
                Token::Muted,
            )
        }
    } else {
        (
            "Press r to run the benchmark, m to toggle BEGIN/ROLLBACK isolation.".to_owned(),
            Token::Muted,
        )
    };
    let bench_settle_applies = !state.loading
        && state.report.as_ref().is_some_and(|report| {
            !report.failed_gate
                && report
                    .comparison
                    .as_ref()
                    .is_none_or(|cmp| cmp.verdict != crate::bench::BenchVerdict::Slower)
        })
        && status_token != Token::Danger;
    let status_style = if bench_settle_applies {
        bench_settle_style(theme, status_token, settle)
    } else {
        theme.style(status_token)
    };
    render_overlay_status_styled(&status_text, status_style, presentation, status_area, buf);

    // While a run is in flight m, r and Esc wait, while Ctrl+C cancels.
    let hints: &[(&str, &str)] = if state.loading {
        &[("1-2", "Tabs"), ("Up/Down", "Select"), ("Ctrl+C", "Cancel")]
    } else {
        &[
            ("1-2", "Tabs"),
            ("Up/Down", "Select"),
            ("m", "Toggle rollback"),
            ("r", "Re-run"),
            ("Esc", "Close"),
        ]
    };
    render_footer_hints(hints, presentation, footer_area, buf);
}

fn render_query_bench_summary(
    state: &crate::app::model::QueryBenchState,
    presentation: &Presentation,
    body_area: Rect,
    buf: &mut Buffer,
    selection_style: Style,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let body_width = usize::from(body_area.width);

    let Some(report) = &state.report else {
        let mut lines = vec![bounded_row(
            vec![(
                "  No benchmark results yet. Press r to execute warmup and measured runs."
                    .to_owned(),
                theme.style(Token::Muted),
            )],
            body_width,
            unicode,
            None,
        )];
        if !state.baseline_sql.trim().is_empty() {
            lines.push(bounded_row(
                vec![
                    ("  Query A: ".to_owned(), theme.style(Token::Muted)),
                    (
                        single_line_preview(&state.baseline_sql),
                        theme.style(Token::Text),
                    ),
                ],
                body_width,
                unicode,
                None,
            ));
        }
        Paragraph::new(lines).render(body_area, buf);
        return;
    };

    let base = &report.baseline;
    let cand = report.candidate.as_ref();
    let cmp = report.comparison.as_ref();

    let mut lines = Vec::new();
    lines.push(bounded_row(
        vec![
            ("  A (Baseline): ".to_owned(), theme.style(Token::Muted)),
            (
                sanitize_for_display(&base.sql_preview),
                theme.style(Token::Text),
            ),
            (
                format!("  [rows: {}]", base.total_rows),
                theme.style(Token::Info),
            ),
        ],
        body_width,
        unicode,
        None,
    ));
    if let Some(c) = cand {
        lines.push(bounded_row(
            vec![
                ("  B (Candidate): ".to_owned(), theme.style(Token::Muted)),
                (
                    sanitize_for_display(&c.sql_preview),
                    theme.style(Token::Text),
                ),
                (
                    format!("  [rows: {}]", c.total_rows),
                    theme.style(Token::Info),
                ),
            ],
            body_width,
            unicode,
            None,
        ));
    }
    if let Some(c) = cmp {
        let gate_note = report
            .threshold_pct
            .map_or_else(String::new, |pct| format!("  |  Gate: <={pct:.1}%"));
        let token = match c.verdict {
            crate::bench::BenchVerdict::Faster => Token::Success,
            crate::bench::BenchVerdict::Slower => Token::Danger,
            crate::bench::BenchVerdict::Unchanged => Token::Info,
        };
        lines.push(bounded_row(
            vec![(
                format!(
                    "  A/B Speedup: {:.2}x  |  p50 Delta: {:+.3}ms ({:+.1}%)  |  p95 Delta: {:+.3}ms ({:+.1}%){gate_note}",
                    c.speedup_ratio,
                    c.p50_delta_ms,
                    c.p50_delta_pct,
                    c.p95_delta_ms,
                    c.p95_delta_pct
                ),
                theme.style(token),
            )],
            body_width,
            unicode,
            None,
        ));
    }
    lines.push(modal_table_header_row(
        format!(
            "  {:<12} {:<18} {:<18} {}",
            "METRIC",
            "BASELINE (A)",
            if cand.is_some() { "CANDIDATE (B)" } else { "-" },
            "DELTA / NOTE"
        ),
        body_width,
        unicode,
        theme,
    ));

    type BenchMetricRow = (&'static str, String, Option<String>, Option<String>, Token);
    let metric_rows: [BenchMetricRow; 9] = [
        (
            "min",
            format!("{:.3} ms", base.min_ms),
            cand.map(|c| format!("{:.3} ms", c.min_ms)),
            fmt_bench_ms_delta(base.min_ms, cand.map(|c| c.min_ms)),
            cand.map_or(Token::Muted, |c| {
                latency_delta_token(c.min_ms - base.min_ms)
            }),
        ),
        (
            "mean",
            format!("{:.3} ms", base.mean_ms),
            cand.map(|c| format!("{:.3} ms", c.mean_ms)),
            fmt_bench_ms_delta(base.mean_ms, cand.map(|c| c.mean_ms)),
            cand.map_or(Token::Muted, |c| {
                latency_delta_token(c.mean_ms - base.mean_ms)
            }),
        ),
        (
            "stddev",
            format!("{:.3} ms", base.stddev_ms),
            cand.map(|c| format!("{:.3} ms", c.stddev_ms)),
            fmt_bench_ms_delta(base.stddev_ms, cand.map(|c| c.stddev_ms)),
            cand.map_or(Token::Muted, |c| {
                latency_delta_token(c.stddev_ms - base.stddev_ms)
            }),
        ),
        (
            "p50",
            format!("{:.3} ms", base.p50_ms),
            cand.map(|c| format!("{:.3} ms", c.p50_ms)),
            cmp.map(|c| format!("{:+.3} ms ({:+.1}%)", c.p50_delta_ms, c.p50_delta_pct)),
            cmp.map_or(Token::Muted, |c| latency_delta_token(c.p50_delta_ms)),
        ),
        (
            "p95",
            format!("{:.3} ms", base.p95_ms),
            cand.map(|c| format!("{:.3} ms", c.p95_ms)),
            cmp.map(|c| format!("{:+.3} ms ({:+.1}%)", c.p95_delta_ms, c.p95_delta_pct)),
            cmp.map_or(Token::Muted, |c| latency_delta_token(c.p95_delta_ms)),
        ),
        (
            "p99",
            format!("{:.3} ms", base.p99_ms),
            cand.map(|c| format!("{:.3} ms", c.p99_ms)),
            fmt_bench_ms_delta(base.p99_ms, cand.map(|c| c.p99_ms)),
            cand.map_or(Token::Muted, |c| {
                latency_delta_token(c.p99_ms - base.p99_ms)
            }),
        ),
        (
            "max",
            format!("{:.3} ms", base.max_ms),
            cand.map(|c| format!("{:.3} ms", c.max_ms)),
            fmt_bench_ms_delta(base.max_ms, cand.map(|c| c.max_ms)),
            cand.map_or(Token::Muted, |c| {
                latency_delta_token(c.max_ms - base.max_ms)
            }),
        ),
        (
            "qps",
            format!("{:.1} /s", base.qps),
            cand.map(|c| format!("{:.1} /s", c.qps)),
            cand.map(|c| format!("{:+.1} /s", c.qps - base.qps)),
            cand.map_or(Token::Muted, |c| qps_delta_token(c.qps - base.qps)),
        ),
        (
            "sparkline",
            bench_sparkline(base, presentation).to_owned(),
            cand.map(|c| bench_sparkline(c, presentation).to_owned()),
            Some(format!(
                "{} buckets ({})",
                crate::bench::BENCH_SPARKLINE_BUCKETS,
                if presentation.glyphs.is_ascii() {
                    "ascii"
                } else {
                    "unicode"
                }
            )),
            Token::Muted,
        ),
    ];

    let list_rows = usize::from(body_area.height)
        .saturating_sub(lines.len())
        .max(1);
    let start = state
        .selected_idx
        .saturating_sub(list_rows.saturating_sub(1));
    for (idx, (name, base_val, cand_val, delta_val, delta_tok)) in
        metric_rows.iter().enumerate().skip(start).take(list_rows)
    {
        let selected = idx == state.selected_idx;
        let marker = if selected { "> " } else { "  " };
        let cand_str = cand_val.as_deref().unwrap_or("-");
        let delta_str = delta_val.as_deref().unwrap_or("-");
        let name_cell = fit_cell(name, 12, unicode);
        let base_cell = fit_cell(base_val, 18, unicode);
        let cand_cell = fit_cell(cand_str, 18, unicode);
        let delta_cell = sanitize_for_display(delta_str);
        let segments = if selected {
            vec![
                (
                    marker.to_owned(),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (
                    format!("{name_cell} "),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{base_cell} "), selection_style),
                (format!("{cand_cell} "), selection_style),
                (delta_cell, selection_style),
            ]
        } else {
            let base_tok = if *name == "sparkline" {
                Token::Focus
            } else {
                Token::Text
            };
            let cand_tok = if *name == "sparkline" {
                Token::Info
            } else {
                Token::Text
            };
            vec![
                (marker.to_owned(), theme.style(Token::Muted)),
                (
                    format!("{name_cell} "),
                    theme.style(Token::Info).add_modifier(Modifier::BOLD),
                ),
                (format!("{base_cell} "), theme.style(base_tok)),
                (format!("{cand_cell} "), theme.style(cand_tok)),
                (delta_cell, theme.style(*delta_tok)),
            ]
        };
        lines.push(bounded_row(
            segments,
            body_width,
            unicode,
            selected.then_some(selection_style),
        ));
    }
    Paragraph::new(lines).render(body_area, buf);
}

fn render_query_bench_samples(
    state: &crate::app::model::QueryBenchState,
    presentation: &Presentation,
    body_area: Rect,
    buf: &mut Buffer,
    selection_style: Style,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let body_width = usize::from(body_area.width);

    if state.baseline.is_none() && state.candidate.is_none() {
        Paragraph::new(Line::from(Span::styled(
            "  No latency samples captured yet. Press r to run the benchmark.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
        return;
    }

    let mut lines = Vec::new();
    if let Some(base) = &state.baseline {
        lines.push(bounded_row(
            vec![
                ("  Baseline (A): ".to_owned(), theme.style(Token::Header)),
                (
                    format!(
                        "p50={:.3}ms  p95={:.3}ms  p99={:.3}ms  mean={:.3}ms  stddev={:.3}ms  qps={:.1}  ",
                        base.p50_ms,
                        base.p95_ms,
                        base.p99_ms,
                        base.mean_ms,
                        base.stddev_ms,
                        base.qps
                    ),
                    theme.style(Token::Text),
                ),
                (
                    format!("[{}]", bench_sparkline(base, presentation)),
                    theme.style(Token::Focus),
                ),
            ],
            body_width,
            unicode,
            None,
        ));
    }
    if let Some(cand) = &state.candidate {
        lines.push(bounded_row(
            vec![
                ("  Candidate (B): ".to_owned(), theme.style(Token::Header)),
                (
                    format!(
                        "p50={:.3}ms  p95={:.3}ms  p99={:.3}ms  mean={:.3}ms  stddev={:.3}ms  qps={:.1}  ",
                        cand.p50_ms,
                        cand.p95_ms,
                        cand.p99_ms,
                        cand.mean_ms,
                        cand.stddev_ms,
                        cand.qps
                    ),
                    theme.style(Token::Text),
                ),
                (
                    format!("[{}]", bench_sparkline(cand, presentation)),
                    theme.style(Token::Info),
                ),
            ],
            body_width,
            unicode,
            None,
        ));
    }

    let has_cand = state.candidate.is_some();
    let header_str = if has_cand {
        format!(
            "  {:<6} {:>12} {:>10} {:>12} {:>10} {:>12}",
            "ITER", "BASE (ms)", "BASE ROWS", "CAND (ms)", "CAND ROWS", "DELTA (ms)"
        )
    } else {
        format!(
            "  {:<6} {:>14} {:>14} {}",
            "ITER", "DURATION (ms)", "ROWS RETURNED", "LATENCY PROFILE"
        )
    };
    lines.push(modal_table_header_row(
        header_str, body_width, unicode, theme,
    ));

    let base_samples = state.baseline.as_ref().map_or(&[][..], |b| &b.samples);
    let cand_samples = state.candidate.as_ref().map_or(&[][..], |c| &c.samples);
    let total = base_samples.len().max(cand_samples.len());
    let max_duration = base_samples
        .iter()
        .chain(cand_samples.iter())
        .map(|s| s.duration_ms)
        .fold(0.0f64, f64::max)
        .max(1e-6);
    let bar_char = if presentation.glyphs.is_ascii() {
        '#'
    } else {
        '\u{2588}'
    };

    let list_rows = usize::from(body_area.height)
        .saturating_sub(lines.len())
        .max(1);
    let start = state
        .selected_idx
        .saturating_sub(list_rows.saturating_sub(1));
    for idx in start..total.min(start + list_rows) {
        let selected = idx == state.selected_idx;
        let marker = if selected { "> " } else { "  " };
        let b = base_samples.get(idx);
        let c = cand_samples.get(idx);
        let iter_num = b.or(c).map_or(idx + 1, |s| s.iteration);

        let segments = if has_cand {
            let b_dur = b.map_or_else(|| "-".to_owned(), |s| format!("{:.3}", s.duration_ms));
            let b_rows = b.map_or_else(|| "-".to_owned(), |s| s.rows_returned.to_string());
            let c_dur = c.map_or_else(|| "-".to_owned(), |s| format!("{:.3}", s.duration_ms));
            let c_rows = c.map_or_else(|| "-".to_owned(), |s| s.rows_returned.to_string());
            let (delta, delta_tok) = match (b, c) {
                (Some(bs), Some(cs)) => {
                    let diff = cs.duration_ms - bs.duration_ms;
                    (format!("{diff:+.3}"), latency_delta_token(diff))
                }
                _ => ("-".to_owned(), Token::Muted),
            };
            if selected {
                vec![
                    (
                        marker.to_owned(),
                        selection_style.add_modifier(Modifier::BOLD),
                    ),
                    (format!("#{iter_num:<5} "), selection_style),
                    (format!("{b_dur:>12} "), selection_style),
                    (format!("{b_rows:>10} "), selection_style),
                    (format!("{c_dur:>12} "), selection_style),
                    (format!("{c_rows:>10} "), selection_style),
                    (format!("{delta:>12}"), selection_style),
                ]
            } else {
                vec![
                    (marker.to_owned(), theme.style(Token::Muted)),
                    (format!("#{iter_num:<5} "), theme.style(Token::Info)),
                    (format!("{b_dur:>12} "), theme.style(Token::Text)),
                    (format!("{b_rows:>10} "), theme.style(Token::Muted)),
                    (format!("{c_dur:>12} "), theme.style(Token::Text)),
                    (format!("{c_rows:>10} "), theme.style(Token::Muted)),
                    (format!("{delta:>12}"), theme.style(delta_tok)),
                ]
            }
        } else {
            let dur = b.map_or(0.0, |s| s.duration_ms);
            let rows = b.map_or(0, |s| s.rows_returned);
            let bar_len = ((dur / max_duration) * 24.0).round().clamp(1.0, 24.0) as usize;
            let bar: String = std::iter::repeat_n(bar_char, bar_len).collect();
            if selected {
                vec![
                    (
                        marker.to_owned(),
                        selection_style.add_modifier(Modifier::BOLD),
                    ),
                    (format!("#{iter_num:<5} "), selection_style),
                    (format!("{dur:>14.3} "), selection_style),
                    (format!("{rows:>14} "), selection_style),
                    (bar, selection_style),
                ]
            } else {
                vec![
                    (marker.to_owned(), theme.style(Token::Muted)),
                    (format!("#{iter_num:<5} "), theme.style(Token::Info)),
                    (format!("{dur:>14.3} "), theme.style(Token::Text)),
                    (format!("{rows:>14} "), theme.style(Token::Muted)),
                    (bar, theme.style(Token::Focus)),
                ]
            }
        };
        lines.push(bounded_row(
            segments,
            body_width,
            unicode,
            selected.then_some(selection_style),
        ));
    }
    Paragraph::new(lines).render(body_area, buf);
}

/// Renders the Canonical Schema Snapshot & Drift Guard overlay (`057`).
pub(crate) fn render_schema_snapshot(
    state: &crate::app::model::SchemaSnapshotState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_schema_snapshot_frame(state, presentation, area, target, fit);
    });
}

fn render_schema_snapshot_frame(
    state: &crate::app::model::SchemaSnapshotState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let title = format!(
        " {}Schema snapshot & drift guard ",
        presentation.icon(Icon::Schema)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let snap_opt = state.current.as_ref().or(state.baseline.as_ref());
    let version = snap_opt.map_or(crate::snapshot::SNAPSHOT_SCHEMA_VERSION, |s| {
        s.schema_version
    });
    let sha12 = snap_opt
        .map(|s| s.fingerprint_sha256.chars().take(12).collect::<String>())
        .filter(|fp| !fp.is_empty())
        .unwrap_or_else(|| "none".to_owned());
    let tables_count = snap_opt.map_or(0, |s| s.tables.len());
    let views_count = snap_opt.map_or(0, |s| s.views.len());

    let mut capsules = vec![Span::raw(" ")];
    push_pill_capsule(
        &mut capsules,
        presentation,
        format!("[SNAPSHOT v{version}]"),
        Token::Focus,
    );
    push_kv_capsule(&mut capsules, presentation, "SHA256: ", sha12, Token::Info);
    push_kv_capsule(
        &mut capsules,
        presentation,
        "TABLES: ",
        tables_count.to_string(),
        Token::Text,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "VIEWS: ",
        views_count.to_string(),
        Token::Text,
    );
    if let Some(diff) = &state.diff {
        let breaking = format!("[BREAKING: {}]", diff.breaking_count);
        if diff.breaking_count > 0 {
            push_safety_capsule(&mut capsules, presentation, breaking, Token::Danger);
        } else {
            push_pill_capsule(&mut capsules, presentation, breaking, Token::Muted);
        }
        push_pill_capsule(
            &mut capsules,
            presentation,
            format!("[WARN: {}]", diff.warning_count),
            count_token(diff.warning_count, Token::Warning),
        );
        push_pill_capsule(
            &mut capsules,
            presentation,
            format!("[INFO: {}]", diff.info_count),
            count_token(diff.info_count, Token::Info),
        );
    }
    render_capsule_bar(capsules, capsule_area, buf);

    let baseline_badge = if state.baseline.is_some() {
        ("[BASELINE: PINNED]".to_owned(), Token::Success)
    } else {
        ("[BASELINE: NONE]".to_owned(), Token::Muted)
    };
    render_overlay_tabs(
        &[" [1] Overview & Drift ", " [2] Catalog Tables "],
        state.active_tab,
        Some(baseline_badge),
        presentation,
        tab_area,
        buf,
    );

    let selection_style = theme.style(Token::Selection);

    if state.loading {
        Paragraph::new(Line::from(Span::styled(
            "  Capturing canonical PostgreSQL schema snapshot from pg_catalog...",
            theme.style(Token::Info),
        )))
        .render(body_area, buf);
    } else {
        match state.active_tab {
            0 => render_schema_snapshot_overview(
                state,
                presentation,
                body_area,
                buf,
                selection_style,
            ),
            1 => {
                render_schema_snapshot_tables(state, presentation, body_area, buf, selection_style)
            }
            _ => {}
        }
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        let tok = if msg.contains("failed") {
            Token::Danger
        } else {
            Token::Info
        };
        (sanitize_for_display(msg), tok)
    } else if let Some(diff) = &state.diff {
        (
            format!(
                "Drift: {} breaking, {} warning, {} info  |  Match: {}",
                diff.breaking_count,
                diff.warning_count,
                diff.info_count,
                if diff.fingerprints_match { "YES" } else { "NO" }
            ),
            if diff.breaking_count > 0 {
                Token::Danger
            } else {
                Token::Muted
            },
        )
    } else if let Some(snap) = snap_opt {
        (
            format!(
                "Snapshot v{} ready ({} table(s), {} column(s)). Press [b] to pin baseline.",
                snap.schema_version,
                snap.tables.len(),
                snap.total_columns()
            ),
            Token::Muted,
        )
    } else {
        (
            "Press [r] to capture live schema snapshot.".to_owned(),
            Token::Muted,
        )
    };
    render_overlay_status(&status_text, status_token, presentation, status_area, buf);

    render_footer_hints(
        &[
            ("1-2", "Tabs"),
            ("b", "Pin baseline"),
            ("c", "Copy SHA-256"),
            ("Up/Down", "Select"),
            ("r", "Refresh"),
            ("Esc", "Close"),
        ],
        presentation,
        footer_area,
        buf,
    );
}

fn render_schema_snapshot_overview(
    state: &crate::app::model::SchemaSnapshotState,
    presentation: &Presentation,
    body_area: Rect,
    buf: &mut Buffer,
    selection_style: Style,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let body_width = usize::from(body_area.width);
    let snap_opt = state.current.as_ref().or(state.baseline.as_ref());

    let mut lines = Vec::new();
    if let Some(snap) = snap_opt {
        let filter = snap
            .schema_filter
            .as_deref()
            .map(sanitize_for_display)
            .unwrap_or_else(|| "<all user schemas>".to_owned());
        let curr_fp = state
            .current
            .as_ref()
            .map(|s| s.fingerprint_sha256.chars().take(16).collect::<String>())
            .filter(|fp| !fp.is_empty())
            .unwrap_or_else(|| "-".to_owned());
        let base_fp = state
            .baseline
            .as_ref()
            .map(|s| s.fingerprint_sha256.chars().take(16).collect::<String>())
            .filter(|fp| !fp.is_empty())
            .unwrap_or_else(|| "(not pinned)".to_owned());

        lines.push(bounded_row(
            vec![
                ("  Filter: ".to_owned(), theme.style(Token::Muted)),
                (format!("{filter}  |  "), theme.style(Token::Text)),
                ("Current: ".to_owned(), theme.style(Token::Muted)),
                (format!("{curr_fp}  |  "), theme.style(Token::Info)),
                ("Baseline: ".to_owned(), theme.style(Token::Muted)),
                (base_fp, theme.style(Token::Focus)),
            ],
            body_width,
            unicode,
            None,
        ));
        let trunc_note = if snap.truncated { " [TRUNCATED]" } else { "" };
        lines.push(bounded_row(
            vec![(
                format!(
                    "  Tables: {}  |  Columns: {}  |  Indexes: {}  |  Policies: {}  |  Views: {}  |  Sequences: {}  |  Enums: {}{trunc_note}",
                    snap.tables.len(),
                    snap.total_columns(),
                    snap.total_indexes(),
                    snap.total_policies(),
                    snap.views.len(),
                    snap.sequences.len(),
                    snap.enums.len(),
                ),
                theme.style(Token::Text),
            )],
            body_width,
            unicode,
            None,
        ));
    }

    if let Some(diff) = &state.diff {
        let (status_label, status_tok) = if diff.has_drift() {
            ("DRIFT DETECTED", Token::Warning)
        } else {
            ("NO DRIFT", Token::Success)
        };
        lines.push(bounded_row(
            vec![
                ("  Drift Status: ".to_owned(), theme.style(Token::Muted)),
                (
                    format!("{status_label}  |  "),
                    theme.style(status_tok).add_modifier(Modifier::BOLD),
                ),
                (
                    format!(
                        "Breaking: {}  Warning: {}  Info: {}  Findings: {}",
                        diff.breaking_count,
                        diff.warning_count,
                        diff.info_count,
                        diff.findings.len()
                    ),
                    theme.style(Token::Text),
                ),
            ],
            body_width,
            unicode,
            None,
        ));

        if diff.findings.is_empty() {
            lines.push(bounded_row(
                vec![(
                    "  No schema drift findings between pinned baseline and current snapshot."
                        .to_owned(),
                    theme.style(Token::Success),
                )],
                body_width,
                unicode,
                None,
            ));
        } else {
            lines.push(modal_table_header_row(
                format!(
                    "  {:<9} {:<20} {:<26} {}",
                    "SEVERITY", "KIND", "OBJECT", "DETAIL"
                ),
                body_width,
                unicode,
                theme,
            ));
            let list_rows = usize::from(body_area.height)
                .saturating_sub(lines.len())
                .max(1);
            let start = state
                .selected_idx
                .saturating_sub(list_rows.saturating_sub(1));
            for (idx, finding) in diff.findings.iter().enumerate().skip(start).take(list_rows) {
                let selected = idx == state.selected_idx;
                let marker = if selected { "> " } else { "  " };
                let sev_tok = match finding.severity {
                    crate::snapshot::DriftSeverity::Breaking => Token::Danger,
                    crate::snapshot::DriftSeverity::Warning => Token::Warning,
                    crate::snapshot::DriftSeverity::Info => Token::Info,
                };
                let sev_cell = fit_cell(finding.severity.badge(), 9, unicode);
                let kind_cell = fit_cell(&finding.kind, 20, unicode);
                let obj_cell = fit_cell(&finding.object, 26, unicode);
                let detail_cell = sanitize_for_display(&finding.detail);
                let segments = if selected {
                    vec![
                        (
                            marker.to_owned(),
                            selection_style.add_modifier(Modifier::BOLD),
                        ),
                        (
                            format!("{sev_cell} "),
                            selection_style.add_modifier(Modifier::BOLD),
                        ),
                        (format!("{kind_cell} "), selection_style),
                        (format!("{obj_cell} "), selection_style),
                        (detail_cell, selection_style),
                    ]
                } else {
                    vec![
                        (marker.to_owned(), theme.style(Token::Muted)),
                        (
                            format!("{sev_cell} "),
                            theme.style(sev_tok).add_modifier(Modifier::BOLD),
                        ),
                        (format!("{kind_cell} "), theme.style(Token::Muted)),
                        (format!("{obj_cell} "), theme.style(Token::Text)),
                        (detail_cell, theme.style(Token::Info)),
                    ]
                };
                lines.push(bounded_row(
                    segments,
                    body_width,
                    unicode,
                    selected.then_some(selection_style),
                ));
            }
        }
    } else if snap_opt.is_some() {
        lines.push(bounded_row(
            vec![(
                "  Press [b] to pin current snapshot as baseline, then [r] to check live drift."
                    .to_owned(),
                theme.style(Token::Muted),
            )],
            body_width,
            unicode,
            None,
        ));
    } else {
        lines.push(bounded_row(
            vec![(
                "  No schema snapshot captured yet. Press [r] to capture from PostgreSQL."
                    .to_owned(),
                theme.style(Token::Muted),
            )],
            body_width,
            unicode,
            None,
        ));
    }
    Paragraph::new(lines).render(body_area, buf);
}

fn render_schema_snapshot_tables(
    state: &crate::app::model::SchemaSnapshotState,
    presentation: &Presentation,
    body_area: Rect,
    buf: &mut Buffer,
    selection_style: Style,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let body_width = usize::from(body_area.width);

    let Some(snap) = state.current.as_ref().or(state.baseline.as_ref()) else {
        Paragraph::new(Line::from(Span::styled(
            "  No schema snapshot captured yet. Press [r] to capture.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
        return;
    };

    if snap.tables.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "  No user tables found in this schema snapshot.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
        return;
    }

    let mut lines = Vec::new();
    lines.push(modal_table_header_row(
        format!(
            "  {:<28} {:>5} {:>5} {:>5} {:<7} {:>8} {}",
            "TABLE", "COLS", "CONS", "IDX", "RLS", "POLICIES", "COLUMNS PREVIEW"
        ),
        body_width,
        unicode,
        theme,
    ));

    let detail_rows = usize::from(body_area.height >= 10) * 2;
    let list_rows = usize::from(body_area.height)
        .saturating_sub(1 + detail_rows)
        .max(1);
    let start = state
        .selected_idx
        .saturating_sub(list_rows.saturating_sub(1));
    for (idx, table) in snap.tables.iter().enumerate().skip(start).take(list_rows) {
        let selected = idx == state.selected_idx;
        let marker = if selected { "> " } else { "  " };
        let qname = fit_cell(&table.qualified_name(), 28, unicode);
        let rls_label = if table.rls_forced {
            "FORCED"
        } else if table.rls_enabled {
            "ON"
        } else {
            "off"
        };
        let col_preview = table
            .columns
            .iter()
            .take(4)
            .map(|c| {
                format!(
                    "{}:{}",
                    sanitize_for_display(&c.name),
                    sanitize_for_display(&c.data_type)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let segments = if selected {
            vec![
                (
                    marker.to_owned(),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (
                    format!("{qname} "),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{:>5} ", table.columns.len()), selection_style),
                (format!("{:>5} ", table.constraints.len()), selection_style),
                (format!("{:>5} ", table.indexes.len()), selection_style),
                (format!("{rls_label:<7} "), selection_style),
                (format!("{:>8} ", table.policies.len()), selection_style),
                (col_preview, selection_style),
            ]
        } else {
            let rls_tok = if table.rls_forced || table.rls_enabled {
                Token::Success
            } else {
                Token::Muted
            };
            vec![
                (marker.to_owned(), theme.style(Token::Muted)),
                (format!("{qname} "), theme.style(Token::Text)),
                (
                    format!("{:>5} ", table.columns.len()),
                    theme.style(Token::Info),
                ),
                (
                    format!("{:>5} ", table.constraints.len()),
                    theme.style(Token::Muted),
                ),
                (
                    format!("{:>5} ", table.indexes.len()),
                    theme.style(Token::Muted),
                ),
                (format!("{rls_label:<7} "), theme.style(rls_tok)),
                (
                    format!("{:>8} ", table.policies.len()),
                    theme.style(Token::Muted),
                ),
                (col_preview, theme.style(Token::Muted)),
            ]
        };
        lines.push(bounded_row(
            segments,
            body_width,
            unicode,
            selected.then_some(selection_style),
        ));
    }

    if detail_rows > 0
        && let Some(sel_table) = snap.tables.get(state.selected_idx)
    {
        let idx_names = sel_table
            .indexes
            .iter()
            .take(3)
            .map(|i| sanitize_for_display(&i.name))
            .collect::<Vec<_>>()
            .join(", ");
        let idx_summary = if idx_names.is_empty() {
            "none".to_owned()
        } else {
            idx_names
        };
        lines.push(bounded_row(
            vec![("  ---".to_owned(), theme.style(Token::Border))],
            body_width,
            unicode,
            None,
        ));
        lines.push(bounded_row(
            vec![
                ("  Selected: ".to_owned(), theme.style(Token::Muted)),
                (
                    sanitize_for_display(&sel_table.qualified_name()),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                (
                    format!(
                        "  |  Cols: {}  |  Constraints: {}  |  Indexes: {idx_summary}",
                        sel_table.columns.len(),
                        sel_table.constraints.len()
                    ),
                    theme.style(Token::Text),
                ),
            ],
            body_width,
            unicode,
            None,
        ));
    }
    Paragraph::new(lines).render(body_area, buf);
}

/// Renders the Interactive SQL Markdown Runbook & Write-Guard overlay (`058`).
pub(crate) fn render_sql_runbook(
    state: &crate::app::model::SqlRunbookState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    render_fitted_modal(area, buf, 0, |fit, target| {
        render_sql_runbook_frame(state, presentation, area, target, fit);
    });
}

fn render_sql_runbook_frame(
    state: &crate::app::model::SqlRunbookState,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
    fit: &ModalFit,
) {
    let theme = &presentation.theme;
    let title = format!(
        " {}SQL runbook & write-guard planner ",
        presentation.icon(Icon::Editor)
    );
    let Some((capsule_area, tab_area, body_area, status_area, footer_area)) =
        modal_inner_layout(title, presentation, area, buf, fit)
    else {
        return;
    };

    let report = state.dry_run_report();
    let mut capsules = vec![Span::raw(" ")];
    push_pill_capsule(
        &mut capsules,
        presentation,
        "[RUNBOOK]".to_owned(),
        Token::Focus,
    );
    push_kv_capsule(
        &mut capsules,
        presentation,
        "CELLS: ",
        state.runbook.len().to_string(),
        Token::Text,
    );
    if state.allow_writes {
        push_safety_capsule(
            &mut capsules,
            presentation,
            "[WRITES: ALLOWED]".to_owned(),
            Token::Warning,
        );
    } else {
        push_kv_capsule(
            &mut capsules,
            presentation,
            "WRITES: ",
            "BLOCKED".to_owned(),
            Token::Success,
        );
    }
    if !state.allow_writes {
        let guard_token = if report.blocked_count > 0 {
            Token::Warning
        } else {
            Token::Muted
        };
        push_pill_capsule(
            &mut capsules,
            presentation,
            format!("[WRITE-GUARD: {}]", report.blocked_count),
            guard_token,
        );
    }
    if state.runbook.truncated {
        push_pill_capsule(
            &mut capsules,
            presentation,
            "[TRUNCATED]".to_owned(),
            Token::Warning,
        );
    }
    render_capsule_bar(capsules, capsule_area, buf);

    render_overlay_tabs(
        &[" [1] Notebook Cells ", " [2] Safety Dry-Run Plan "],
        state.active_tab,
        None,
        presentation,
        tab_area,
        buf,
    );

    if state.runbook.cells.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "  No SQL cells found in the active editor buffer.",
            theme.style(Token::Muted),
        )))
        .render(body_area, buf);
    } else {
        let selection_style = theme.style(Token::Selection);
        match state.active_tab {
            0 => render_sql_runbook_cells(state, presentation, body_area, buf, selection_style),
            1 => render_sql_runbook_dry_run(
                state,
                &report,
                presentation,
                body_area,
                buf,
                selection_style,
            ),
            _ => {}
        }
    }

    let (status_text, status_token) = if let Some(msg) = &state.status_message {
        (sanitize_for_display(msg), Token::Info)
    } else if let Some(cell) = state.selected_cell() {
        (
            format!(
                "Cell #{} ({}) at line {}  |  Press [Enter] to jump to editor line {}",
                cell.index + 1,
                sanitize_for_display(&cell.name),
                cell.start_line,
                cell.start_line
            ),
            Token::Muted,
        )
    } else {
        (
            "No runbook cells available in active buffer.".to_owned(),
            Token::Muted,
        )
    };
    render_overlay_status(&status_text, status_token, presentation, status_area, buf);

    render_footer_hints(
        &[
            ("1-2", "Tabs"),
            ("w", "Allow writes"),
            ("Up/Down", "Select cell"),
            ("Enter", "Jump to cell"),
            ("Esc", "Close"),
        ],
        presentation,
        footer_area,
        buf,
    );
}

fn runbook_access_badge(is_write: bool, is_destructive: bool) -> (&'static str, Token) {
    if is_destructive {
        ("DESTRUCTIVE", Token::Danger)
    } else if is_write {
        ("WRITE", Token::Warning)
    } else {
        ("READ", Token::Success)
    }
}

fn runbook_sql_preview(sql: &str) -> String {
    let body: Vec<&str> = sql
        .lines()
        .skip_while(|line| {
            let trimmed = line.trim_start();
            trimmed.is_empty() || trimmed.starts_with("-- @")
        })
        .collect();
    let preview = if body.is_empty() {
        single_line_preview(sql)
    } else {
        single_line_preview(&body.join(" "))
    };
    if preview.is_empty() {
        single_line_preview(sql)
    } else {
        preview
    }
}

fn render_sql_runbook_cells(
    state: &crate::app::model::SqlRunbookState,
    presentation: &Presentation,
    body_area: Rect,
    buf: &mut Buffer,
    selection_style: Style,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let body_width = usize::from(body_area.width);

    let mut lines = Vec::new();
    lines.push(modal_table_header_row(
        format!(
            "  {:<4} {:<16} {:<20} {:<11} {:<15} {}",
            "#", "NAME", "HEADING", "ACCESS", "EXPECT", "SQL PREVIEW"
        ),
        body_width,
        unicode,
        theme,
    ));

    let detail_rows = usize::from(body_area.height >= 10) * 2;
    let list_rows = usize::from(body_area.height)
        .saturating_sub(1 + detail_rows)
        .max(1);
    let start = state
        .selected_idx
        .saturating_sub(list_rows.saturating_sub(1));
    for (idx, cell) in state
        .runbook
        .cells
        .iter()
        .enumerate()
        .skip(start)
        .take(list_rows)
    {
        let selected = idx == state.selected_idx;
        let marker = if selected { "> " } else { "  " };
        let idx_str = format!("#{}", cell.index + 1);
        let idx_cell = fit_cell(&idx_str, 4, unicode);
        let name_cell = fit_cell(&cell.name, 16, unicode);
        let heading_cell = fit_cell(cell.heading.as_deref().unwrap_or("-"), 20, unicode);
        let (access_label, access_tok) = runbook_access_badge(cell.is_write, cell.is_destructive);
        let access_cell = fit_cell(access_label, 11, unicode);
        let expect_raw = cell
            .expect_rows
            .map_or_else(|| "-".to_owned(), |n| format!("@expect-rows:{n}"));
        let expect_cell = fit_cell(&expect_raw, 15, unicode);
        let sql_preview = runbook_sql_preview(&cell.sql);

        let segments = if selected {
            vec![
                (
                    marker.to_owned(),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{idx_cell} "), selection_style),
                (
                    format!("{name_cell} "),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{heading_cell} "), selection_style),
                (
                    format!("{access_cell} "),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{expect_cell} "), selection_style),
                (sql_preview, selection_style),
            ]
        } else {
            vec![
                (marker.to_owned(), theme.style(Token::Muted)),
                (format!("{idx_cell} "), theme.style(Token::Info)),
                (format!("{name_cell} "), theme.style(Token::Text)),
                (format!("{heading_cell} "), theme.style(Token::Muted)),
                (
                    format!("{access_cell} "),
                    theme.style(access_tok).add_modifier(Modifier::BOLD),
                ),
                (format!("{expect_cell} "), theme.style(Token::Info)),
                (sql_preview, theme.style(Token::Text)),
            ]
        };
        lines.push(bounded_row(
            segments,
            body_width,
            unicode,
            selected.then_some(selection_style),
        ));
    }

    if detail_rows > 0
        && let Some(sel) = state.selected_cell()
    {
        let heading_note = sel
            .heading
            .as_deref()
            .map(|h| format!(" ({})", sanitize_for_display(h)))
            .unwrap_or_default();
        let readonly_note = if sel.readonly_hint && sel.is_write {
            "  |  note: @readonly hint ignored by write-guard"
        } else {
            ""
        };
        lines.push(bounded_row(
            vec![("  ---".to_owned(), theme.style(Token::Border))],
            body_width,
            unicode,
            None,
        ));
        lines.push(bounded_row(
            vec![
                (
                    format!(
                        "  Line {} [{}]{heading_note}: ",
                        sel.start_line,
                        sanitize_for_display(&sel.name)
                    ),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                (
                    format!("{}{readonly_note}", runbook_sql_preview(&sel.sql)),
                    theme.style(Token::Text),
                ),
            ],
            body_width,
            unicode,
            None,
        ));
    }
    Paragraph::new(lines).render(body_area, buf);
}

fn render_sql_runbook_dry_run(
    state: &crate::app::model::SqlRunbookState,
    report: &crate::runbook::RunbookReport,
    presentation: &Presentation,
    body_area: Rect,
    buf: &mut Buffer,
    selection_style: Style,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let body_width = usize::from(body_area.width);

    let mut lines = Vec::new();
    lines.push(bounded_row(
        vec![
            ("  Dry-Run Plan: ".to_owned(), theme.style(Token::Muted)),
            (
                format!(
                    "planned={}  blocked={}  skipped={}  writes={}",
                    report.planned_count(),
                    report.blocked_count,
                    report.skipped_count,
                    if report.allow_writes {
                        "ALLOWED"
                    } else {
                        "BLOCKED"
                    }
                ),
                theme.style(if report.blocked_count > 0 {
                    Token::Warning
                } else {
                    Token::Success
                }),
            ),
        ],
        body_width,
        unicode,
        None,
    ));
    lines.push(modal_table_header_row(
        format!(
            "  {:<4} {:<16} {:<14} {:<11} {:<22} {}",
            "#", "NAME", "HEADING", "ACCESS", "STATUS", "EXPECT / SQL"
        ),
        body_width,
        unicode,
        theme,
    ));

    let list_rows = usize::from(body_area.height).saturating_sub(2).max(1);
    let start = state
        .selected_idx
        .saturating_sub(list_rows.saturating_sub(1));
    for (idx, rec) in report.cells.iter().enumerate().skip(start).take(list_rows) {
        let selected = idx == state.selected_idx;
        let marker = if selected { "> " } else { "  " };
        let cell_opt = state.runbook.cells.get(idx);
        let is_destructive = cell_opt.is_some_and(|c| c.is_destructive);
        let (access_label, access_tok) = runbook_access_badge(rec.is_write, is_destructive);
        let status_tok = match rec.status {
            crate::runbook::CellStatus::Planned | crate::runbook::CellStatus::Passed => {
                Token::Success
            }
            crate::runbook::CellStatus::BlockedByWriteGuard
            | crate::runbook::CellStatus::Failed => Token::Danger,
            crate::runbook::CellStatus::Skipped => Token::Muted,
        };
        let expect_part = cell_opt
            .and_then(|c| c.expect_rows)
            .map(|n| format!("@expect-rows:{n}  "))
            .unwrap_or_default();
        let sql_part = cell_opt
            .map(|c| runbook_sql_preview(&c.sql))
            .unwrap_or_default();
        let note_part = rec
            .message
            .as_deref()
            .map(sanitize_for_display)
            .filter(|m| !m.is_empty());
        let tail = match note_part {
            Some(msg) => format!("{expect_part}{msg} | {sql_part}"),
            None => format!("{expect_part}{sql_part}"),
        };

        let idx_cell = fit_cell(&format!("#{}", rec.index + 1), 4, unicode);
        let name_cell = fit_cell(&rec.name, 16, unicode);
        let heading_cell = fit_cell(rec.heading.as_deref().unwrap_or("-"), 14, unicode);
        let access_cell = fit_cell(access_label, 11, unicode);
        let status_cell = fit_cell(rec.status.label(), 22, unicode);

        let segments = if selected {
            vec![
                (
                    marker.to_owned(),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{idx_cell} "), selection_style),
                (
                    format!("{name_cell} "),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{heading_cell} "), selection_style),
                (
                    format!("{access_cell} "),
                    selection_style.add_modifier(Modifier::BOLD),
                ),
                (format!("{status_cell} "), selection_style),
                (tail, selection_style),
            ]
        } else {
            vec![
                (marker.to_owned(), theme.style(Token::Muted)),
                (format!("{idx_cell} "), theme.style(Token::Info)),
                (format!("{name_cell} "), theme.style(Token::Text)),
                (format!("{heading_cell} "), theme.style(Token::Muted)),
                (
                    format!("{access_cell} "),
                    theme.style(access_tok).add_modifier(Modifier::BOLD),
                ),
                (format!("{status_cell} "), theme.style(status_tok)),
                (tail, theme.style(Token::Muted)),
            ]
        };
        lines.push(bounded_row(
            segments,
            body_width,
            unicode,
            selected.then_some(selection_style),
        ));
    }
    Paragraph::new(lines).render(body_area, buf);
}

#[cfg(test)]
mod tests;
