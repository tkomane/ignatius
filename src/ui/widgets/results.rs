use super::overlays::render_plan;
use super::{
    action_key_label, hairline_rule, highlighted_sql_spans, input_cursor, inset_empty_area,
    overlay_block, pane_block_line, syntax_token, wrapped_line_count,
};
use crate::app::grid;
use crate::app::model::{Focus, Model, QueryPhase};
use crate::query::result::ExecutionStatus;
use crate::query::value::{
    display_width, pad_to_width, sanitize_for_display, truncate_to_width, wrap_to_width,
};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, inspector_box, inspector_viewport};
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

/// Grid prelude rows, shared with paging and hit testing.
pub(crate) fn grid_header_rows(
    model: &Model,
    set: &crate::query::result::ResultSet,
    spacing: crate::app::model::Spacing,
    area: Rect,
) -> usize {
    // The column header row, then the optional types row and the rule.
    grid_column_header_row(model, spacing, area)
        + 2
        + usize::from(model.result_grid.show_types && set.column_types.iter().any(Option::is_some))
}

/// The inner row the column header is painted on, shared with hit testing.
///
/// The rows above it are, in paint order: the retention warning, the order
/// line, the state line, and either one notice or the density's blank rows.
/// A click handler that assumed a fixed row would land on the rule or the
/// types row in compact density and under a warning.
pub(crate) fn grid_column_header_row(
    model: &Model,
    spacing: crate::app::model::Spacing,
    area: Rect,
) -> usize {
    let notice = model.cell_update_notice.is_some()
        || model.clipboard_notice.is_some()
        || model.refresh_notice.is_some()
        || model.result_grid.note.is_some();
    retention_warning_lines(model, area).len()
        + 2
        + usize::from(u16::from(notice).max(spacing.toolbar_blank_rows))
}

/// Wrap fixed client prose without splitting a numeric token. An indivisible
/// fact too wide for this viewport is deferred rather than shortened.
fn wrap_retention_fact(text: &str, width: usize) -> Vec<String> {
    if width == 0
        || text
            .split_whitespace()
            .any(|word| display_width(word) > width)
    {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && display_width(&line) + 1 + display_width(word) > width {
            rows.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        rows.push(line);
    }
    rows
}

/// Complete nonnumeric cue when a short pane cannot admit a whole fact.
fn short_retention_cue(area: Rect, unavailable: bool) -> Vec<String> {
    if area.height == 0 {
        return Vec::new();
    }
    let candidates = if unavailable {
        [
            "Data unavailable; enlarge",
            "Data unavailable",
            "Unavailable",
        ]
    } else {
        ["Data limited; enlarge view", "Data limited", "Limited"]
    };
    candidates
        .into_iter()
        .find(|text| display_width(text) <= usize::from(area.width))
        .map_or_else(Vec::new, |text| vec![text.to_owned()])
}

/// Complete fixed-size execution facts, shared by paint and viewport geometry.
fn retention_warning_lines(model: &Model, area: Rect) -> Vec<String> {
    let Some(summary) = model
        .retained_data_loss
        .warning()
        .filter(|_| area.width > 0)
    else {
        return Vec::new();
    };
    let width = usize::from(area.width);
    let mut rows = wrap_retention_fact(&summary, width);
    if rows.is_empty() || rows.len() > usize::from(area.height) {
        return short_retention_cue(area, false);
    }
    if let Some(detail) = model.retained_data_loss.detail() {
        let detail = wrap_retention_fact(&detail, width);
        // Keep the normal grid prelude and at least one data row usable.
        if rows.len().saturating_add(detail.len()).saturating_add(7) <= usize::from(area.height) {
            rows.extend(detail);
        }
    }
    rows
}

/// Actual drawable rows, before imposing the navigation minimum of one.
pub(crate) fn grid_capacity(
    model: &Model,
    pane: Rect,
    spacing: crate::app::model::Spacing,
) -> usize {
    let inner = super::inner_rect(pane);
    if inner.width == 0 {
        return 0;
    }
    model
        .visible_result()
        .filter(|set| set.is_available())
        .map_or(0, |set| {
            usize::from(inner.height).saturating_sub(grid_header_rows(model, set, spacing, inner))
        })
}

pub(crate) fn render_results(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if model.plan.is_visible() {
        render_plan(model, keymap, presentation, area, buf);
        return;
    }
    let theme = &presentation.theme;
    let focused = model.focus == Focus::Results;

    let Some(execution) = &model.last_execution else {
        render_results_placeholder(model, keymap, presentation, focused, area, buf);
        return;
    };

    // A failed transaction is not an ordinary error: nothing else will run until
    // it ends, so the way out is shown rather than left to be remembered.
    if model.transaction == crate::query::result::TransactionState::Failed {
        let mut title_spans = Vec::new();
        if model.splits.zoomed && focused {
            title_spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
        }
        title_spans.push(Span::styled(
            format!("{}Transaction failed", presentation.icon(Icon::Warning)),
            theme
                .style(Token::TransactionFailed)
                .add_modifier(Modifier::BOLD),
        ));
        if focused {
            title_spans.push(Span::styled(
                "  [focused]",
                theme.style(Token::Focus).add_modifier(Modifier::BOLD),
            ));
        }
        let block = pane_block_line(Line::from(title_spans), focused, presentation);
        let inner = block.inner(area);
        block.render(area, buf);
        let mut lines = vec![
            Line::from(Span::styled(
                "This transaction has failed.",
                theme.style(Token::TransactionFailed),
            )),
            Line::from(Span::styled(
                model
                    .transaction
                    .recovery()
                    .unwrap_or("Run ROLLBACK to end it."),
                theme.style(Token::Info),
            )),
        ];
        if let Some(error) = &model.error {
            lines.push(Line::from(Span::styled(
                error.headline.clone(),
                theme.style(Token::Danger),
            )));
            if let Some(cause) = &error.likely_cause {
                lines.push(Line::from(Span::styled(
                    format!("Likely cause: {cause}"),
                    theme.style(Token::Text),
                )));
            }
            if let Some(action) = &error.next_action {
                lines.push(Line::from(Span::styled(
                    format!("Next: {action}"),
                    theme.style(Token::Info),
                )));
            }
        }
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(inner, buf);
        return;
    }

    let previous_cancellation =
        model.phase.is_busy() && execution.status == ExecutionStatus::Cancelled;
    if previous_cancellation && model.visible_result().is_none() {
        render_results_placeholder(model, keymap, presentation, focused, area, buf);
        return;
    }

    let summary = execution.statements.first().map_or_else(
        || execution.status.label().to_owned(),
        |s| {
            if let Some(set) = s.result_set.as_ref().filter(|set| set.is_truncated()) {
                // Metadata counts live in complete body facts, never in a
                // caption whose width can ellipsise a numeric token.
                if set.retention_reason() == Some(crate::query::result::RetentionReason::RowCap) {
                    "row cap reached".to_owned()
                } else {
                    "retained-result limit reached".to_owned()
                }
            } else {
                s.summary()
            }
        },
    );
    let status = if model.running_refresh {
        "Refreshing retained result".to_owned()
    } else if previous_cancellation {
        "Previous result".to_owned()
    } else {
        execution.status.label().to_owned()
    };
    let mut title_spans = Vec::new();
    if model.splits.zoomed && focused {
        title_spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
    }
    let primary_style = if focused {
        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
    } else {
        theme.style(Token::Header)
    };
    title_spans.push(Span::styled(
        format!("{}Results", presentation.icon(Icon::Rows)),
        primary_style,
    ));
    // A complete, successful result settles: its status word starts in the
    // success ink and fades to its resting ink. Only the word of the job that
    // started the settle is tinted, and only while it is that job's own
    // status rather than a refresh or a previous result.
    let settle = model
        .motion()
        .result_settle()
        .filter(|(job, _)| {
            *job == execution.job
                && execution.status == ExecutionStatus::Succeeded
                && !model.running_refresh
                && !previous_cancellation
        })
        .map(|(_, progress)| progress);
    title_spans.push(Span::styled("  ", theme.style(Token::Muted)));
    title_spans.push(Span::styled(status, status_word_style(theme, settle)));
    title_spans.push(Span::styled(
        format!("  {summary}"),
        theme.style(Token::Muted),
    ));
    // What the filter admits is stated in full: how many match, of how many are
    // held, of how many the server sent. Collapsing those into one number is
    // the lie this pane exists not to tell.
    if !model.result_filter.trim().is_empty() {
        let filter_tag = if model.visible_result().is_some_and(|set| set.is_truncated()) {
            "  [filtered retained rows]".to_owned()
        } else {
            format!("  [{}]", model.result_window_label())
        };
        title_spans.push(Span::styled(filter_tag, theme.style(Token::Info)));
    }
    if model.result_filtering {
        title_spans.push(Span::styled(
            format!(
                "  [filter: {}{}]",
                sanitize_for_display(&model.result_filter),
                input_cursor(presentation)
            ),
            theme.style(Token::Focus),
        ));
    }
    if focused {
        title_spans.push(Span::styled(
            "  [focused]",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
        title_spans.push(Span::styled(
            format!(
                " [{} grid controls]",
                action_key_label(keymap, &crate::app::Action::OpenResultControls)
            ),
            theme.style(Token::Muted),
        ));
    }

    let block = pane_block_line(Line::from(title_spans), focused, presentation);
    let inner = block.inner(area);
    block.render(area, buf);

    let Some(set) = model.visible_result() else {
        let text = if execution.statements.is_empty() {
            execution.status.label().to_owned()
        } else {
            execution
                .statements
                .iter()
                .map(crate::query::result::StatementResult::summary)
                .collect::<Vec<_>>()
                .join("; ")
        };
        let mut lines = vec![Line::from(vec![
            Span::styled(presentation.icon(Icon::Info), theme.style(Token::Info)),
            Span::styled(text, theme.style(Token::Text)),
        ])];
        if let Some(notice) = &model.cell_update_notice {
            lines.push(Line::from(Span::styled(
                sanitize_for_display(notice),
                theme.style(Token::Warning),
            )));
        }
        if let Some(notice) = &model.refresh_notice {
            lines.push(Line::from(Span::styled(
                sanitize_for_display(notice),
                theme.style(Token::Info),
            )));
        }
        Paragraph::new(lines).render(inner, buf);
        return;
    };

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if !set.is_available() {
        let width = usize::from(inner.width);
        let mut rows = wrap_to_width(crate::query::result::RETAINED_DATA_UNAVAILABLE, width);
        if let Some(summary) = model.retained_data_loss.warning() {
            rows.extend(wrap_retention_fact(&summary, width));
        }
        if rows.len() > usize::from(inner.height) {
            rows = short_retention_cue(inner, true);
            let compact = format!("Limited sets: {}", model.retained_data_loss.limited_sets);
            if model.retained_data_loss.limited_sets > 0
                && rows.len() < usize::from(inner.height)
                && display_width(&compact) <= width
            {
                rows.push(compact);
            }
        }
        let counts = wrap_retention_fact(
            &format!(
                "{} source columns; {} received rows",
                set.source_column_count(),
                set.rows_seen
            ),
            width,
        );
        // A count is shown as a whole fact or deferred, never vertically clipped
        // into another number. The persistent whole-execution warning precedes
        // optional description detail in the minimum usable pane.
        if rows.len().saturating_add(counts.len()) <= usize::from(inner.height) {
            rows.extend(counts);
        }
        let advice = wrap_retention_fact(crate::query::result::RETAINED_DATA_GUIDANCE, width);
        if rows.len().saturating_add(advice.len()) <= usize::from(inner.height) {
            rows.extend(advice);
        }
        Paragraph::new(rows.into_iter().map(Line::from).collect::<Vec<_>>())
            .style(theme.style(Token::Warning))
            .render(inner, buf);
    } else if model.expanded_row {
        let warning = retention_warning_lines(model, inner);
        let height = u16::try_from(warning.len())
            .unwrap_or(u16::MAX)
            .min(inner.height);
        Paragraph::new(warning.into_iter().map(Line::from).collect::<Vec<_>>())
            .style(theme.style(Token::Warning))
            .render(Rect::new(inner.x, inner.y, inner.width, height), buf);
        render_expanded_row(
            model,
            set,
            presentation,
            Rect::new(
                inner.x,
                inner.y + height,
                inner.width,
                inner.height - height,
            ),
            buf,
        );
    } else {
        render_grid(model, set, keymap, presentation, focused, inner, buf);
    }
}

/// The Results title status word, at rest or `settle` of the way through its
/// settle.
///
/// At rest the word is muted ink without the dim modifier. The settle fades
/// the foreground alone from the success ink to exactly that resting ink, so
/// the modifiers are the same in every frame and the last frame of the fade is
/// the resting frame: nothing changes when the settle ends. `Theme::mix`
/// keeps the fade depth-honest: truecolour interpolates, 256 colours step at
/// the midpoint, and no colour means no tint at all.
pub(crate) fn status_word_style(theme: &Theme, settle: Option<f32>) -> Style {
    let mut style = theme.style(Token::Muted);
    style.add_modifier.remove(Modifier::DIM);
    match settle.and_then(|progress| theme.mix(Token::Success, Token::Muted, progress)) {
        Some(ink) => style.fg(ink),
        None => style,
    }
}

/// One row down the screen, one column per line.
///
/// The grid runs out of width long before a real table runs out of columns. This
/// is the same answer `psql` gives with `\x`, for the same reason.
fn render_expanded_row(
    model: &Model,
    set: &crate::query::result::ResultSet,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    if set.rows.is_empty() {
        let text = if set.rows_seen > 0 {
            format!(
                "No retained row to expand; PostgreSQL delivered {} rows.",
                set.rows_seen
            )
        } else {
            "No rows, so there is no row to expand.".to_owned()
        };
        let rows = wrap_retention_fact(&text, usize::from(area.width));
        let rows = if rows.len() <= usize::from(area.height) {
            rows
        } else {
            vec!["No retained row to expand.".into()]
        };
        Paragraph::new(rows.into_iter().map(Line::from).collect::<Vec<_>>())
            .style(theme.style(Token::Muted))
            .render(area, buf);
        return;
    }

    let row = model
        .selected_source_row()
        .unwrap_or_else(|| model.selected_row.min(set.rows.len().saturating_sub(1)));
    let visible_columns = model.result_grid.visible_columns(set.columns.len());
    let fields = crate::app::inspect::expand_row_columns(
        set,
        row,
        area.width as usize,
        !presentation.glyphs.is_ascii(),
        &visible_columns,
    );

    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("Row {} of {}", row + 1, set.rows.len()),
            theme.style(Token::Text),
        ),
        Span::styled(
            if set.is_truncated() {
                String::new()
            } else {
                format!("   {}", set.window_label())
            },
            theme.style(Token::Muted),
        ),
    ])];
    if fields.iter().any(|field| field.truncated) {
        // Saying that a value was shortened, and where the whole one is, is the
        // difference between an abbreviation and a lie.
        if let Some(first) = lines.first_mut() {
            first.spans.push(Span::styled(
                "   Enter shows one in full",
                theme.style(Token::Info),
            ));
        }
    }

    // The selected column is marked here for the same reason it is in the grid:
    // Enter acts on it, so which one it is has to be visible.
    let selected = visible_columns
        .iter()
        .position(|index| *index == model.selected_column)
        .unwrap_or(0)
        .min(fields.len().saturating_sub(1));
    let visible = (area.height as usize).saturating_sub(1);
    let offset = selected.saturating_sub(visible.saturating_sub(1));
    for (index, field) in fields.iter().enumerate().skip(offset).take(visible) {
        let chosen = index == selected;
        let mut line = Line::from(vec![
            Span::styled(field.name.clone(), theme.style(Token::Header)),
            Span::styled(
                match (chosen, presentation.glyphs.is_ascii()) {
                    (true, true) => " > ".to_owned(),
                    (true, false) => " \u{25b8} ".to_owned(),
                    (false, _) => "   ".to_owned(),
                },
                theme.style(Token::Focus),
            ),
            Span::styled(
                field.value.clone(),
                if field.is_null {
                    theme.style(Token::NullValue)
                } else {
                    theme.style(Token::Text)
                },
            ),
        ]);
        if chosen {
            let selection = theme.style(Token::Selection);
            // Child ink overrides a Line's ink, so apply selection last while
            // retaining the header, focus and NULL emphasis modifiers.
            for span in &mut line.spans {
                span.style = span.style.patch(selection);
            }
            line = line.style(selection);
        }
        lines.push(line);
    }
    let below = fields.len().saturating_sub(offset + visible);
    if below > 0 {
        lines.push(Line::from(Span::styled(
            format!("{below} more column(s) below"),
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines).render(area, buf);
}
/// Shows one value in full: what it is, and every character of it.
pub(crate) fn render_inspector(
    model: &Model,
    inspector: crate::app::inspect::Inspector,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let box_area = inspector_box(area, presentation);
    if box_area.width < 8 || box_area.height < 4 {
        return;
    }
    let (width, height) = inspector_viewport((area.width, area.height), presentation);

    let Some(set) = model.visible_result().filter(|set| set.is_available()) else {
        return;
    };
    let Some(row) = model.selected_source_row() else {
        return;
    };
    let structure = crate::app::inspect::structured_view(set, row, model.selected_column);
    if !inspector.raw()
        && let Some(Ok(tree)) = &structure
    {
        render_structured_inspector(model, tree, inspector.scroll(), presentation, box_area, buf);
        return;
    }
    let Some(view) = crate::app::inspect::CellView::build(set, row, model.selected_column, width)
    else {
        return;
    };

    ratatui::widgets::Clear.render(box_area, buf);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{}  ", view.column),
                theme.style(Token::Header).add_modifier(Modifier::BOLD),
            ),
            Span::styled(view.description.clone(), theme.style(Token::Muted)),
        ]),
        Line::from(Span::styled(
            format!(
                "row {} of {}, column {} of {}",
                view.row.0, view.row.1, view.position.0, view.position.1
            ),
            theme.style(Token::Muted),
        )),
    ];

    let height = height.saturating_sub(usize::from(structure.is_some()));
    if let Some(structure) = structure {
        let status = match structure {
            Ok(_) => "Server text. r shows the tree.",
            Err(error) => error.description(),
        };
        lines.push(Line::from(Span::styled(
            truncate_to_width(status, width, !presentation.glyphs.is_ascii()),
            theme.style(Token::Info),
        )));
    }

    if view.is_null {
        // A NULL has no characters to show. Saying so is the point: this is the
        // one place the difference between NULL, an empty string and the text
        // "NULL" is stated rather than encoded in a marker.
        lines.push(Line::from(Span::styled(
            "There is no value here. This is SQL NULL, not an empty string.",
            theme.style(Token::NullValue),
        )));
    } else {
        let (visible, _, _) = view.window(inspector.scroll(), height);
        for line in visible {
            lines.push(Line::from(Span::styled(
                line.clone(),
                theme.style(Token::Text),
            )));
        }
    }

    if let Some(position) = view.scroll_label(inspector.scroll(), height) {
        lines.push(Line::from(Span::styled(position, theme.style(Token::Info))));
    }

    Paragraph::new(lines)
        .block(overlay_block(
            format!(
                " {}Value  arrows move, Esc closes ",
                presentation.icon(Icon::Rows)
            ),
            presentation,
        ))
        .render(box_area, buf);
}

fn render_structured_inspector(
    model: &Model,
    tree: &crate::structured::Tree<'_>,
    selected: usize,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let width = area.width.saturating_sub(4) as usize;
    let available = area.height.saturating_sub(2) as usize;
    let selected = selected.min(tree.nodes.len().saturating_sub(1));
    let set = model.visible_result();
    let editable = set
        .and_then(|set| set.structured_column(model.selected_column))
        .and_then(|column| column.edit_input(tree, selected))
        .is_some()
        && crate::app::discovery::action_is_available(
            model,
            &crate::app::message::Action::GenerateCellUpdate,
        );
    let title = if editable {
        " Cell Detail  r Raw  e Edit "
    } else {
        " Cell Detail  r Raw "
    };
    let mut lines = Vec::new();
    let status = if tree.truncated {
        format!(
            "Truncated tree  node {} of {} shown",
            selected + 1,
            tree.nodes.len()
        )
    } else {
        format!("Node {} of {}", selected + 1, tree.nodes.len())
    };
    let column = set
        .and_then(|set| set.columns.get(model.selected_column))
        .map_or_else(String::new, |column| sanitize_for_display(column));
    lines.push(Line::from(Span::styled(
        truncate_to_width(&format!("{status}  |  {column}"), width, unicode),
        theme.style(if tree.truncated {
            Token::Warning
        } else {
            Token::Muted
        }),
    )));
    let has_path = available >= 3;
    let visible = available.saturating_sub(lines.len() + usize::from(has_path));
    let start = selected.saturating_sub(visible.saturating_sub(1));
    for (index, node) in tree.nodes.iter().enumerate().skip(start).take(visible) {
        let marker = if index == selected { ">" } else { " " };
        let indent = " ".repeat((node.depth * 2).min(width / 2));
        let label = sanitize_for_display(&node.label);
        let value = sanitize_for_display(tree.preview(index));
        let text = truncate_to_width(
            &format!("{marker} {indent}{label}  {value}"),
            width,
            unicode,
        );
        let style = if index == selected {
            theme.style(Token::Selection)
        } else {
            theme.style(Token::Text)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    if has_path {
        lines.push(Line::from(Span::styled(
            truncate_to_width(
                &format!("Path {}", sanitize_for_display(&tree.path(selected))),
                width,
                unicode,
            ),
            theme.style(Token::Info),
        )));
    }
    ratatui::widgets::Clear.render(area, buf);
    Paragraph::new(lines)
        .block(overlay_block(title.to_owned(), presentation))
        .render(area, buf);
}

fn render_results_placeholder(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let mut title_spans = Vec::new();
    if model.splits.zoomed && focused {
        title_spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
    }
    let primary_style = if focused {
        theme.style(Token::Focus).add_modifier(Modifier::BOLD)
    } else {
        theme.style(Token::Header)
    };
    title_spans.push(Span::styled(
        format!("{}Results", presentation.icon(Icon::Rows)),
        primary_style,
    ));
    if focused {
        title_spans.push(Span::styled(
            "  [focused]",
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
    }
    let block = pane_block_line(Line::from(title_spans), focused, presentation);
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // While a query is in flight this is where the activity indicator lives, so
    // the user is looking at the place the answer will appear.
    if model.phase.is_busy() {
        Paragraph::new(running_lines(model, keymap, presentation)).render(inner, buf);
        return;
    }

    let empty_area = inset_empty_area(inner);
    let keycap = theme.keycap(Token::Focus);
    let ctx = crate::app::discovery::context(model);
    let palette_key = action_key_label(keymap, &crate::app::Action::OpenPalette);
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
                    "Connection required.",
                    theme.style(Token::Warning).add_modifier(Modifier::BOLD),
                ),
            ]));
            prose_lines.push(Line::from(Span::styled(
                "Connect before running SQL. Results appear here after a query.",
                theme.style(Token::Muted),
            )));
        }
        crate::app::discovery::ConnectionPosture::Connecting => {
            prose_lines.push(Line::from(vec![
                Span::styled(
                    presentation.glyphs.prefix(Icon::Info),
                    theme.style(Token::Info).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Waiting for the connection.", theme.style(Token::Header)),
            ]));
            prose_lines.push(Line::from(Span::styled(
                "Results will appear here after a query finishes.",
                theme.style(Token::Muted),
            )));
        }
        crate::app::discovery::ConnectionPosture::Connected => {
            let run_key = action_key_label(keymap, &crate::app::Action::RunBuffer);
            prose_lines.push(Line::from(vec![
                Span::styled(
                    presentation.glyphs.prefix(Icon::Info),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled("No query has run yet.", theme.style(Token::Header)),
            ]));
            // With SQL already in the editor, asking for SQL would be advice
            // the user has already taken: the next step is to run it.
            let (prose, hint) = if ctx.editor_has_text {
                (
                    "SQL is ready to run. Rows, types, and plans appear here.",
                    "Run the buffer",
                )
            } else {
                (
                    "Write SQL in the editor to inspect rows, types, and query plans.",
                    "Run the buffer after entering SQL.",
                )
            };
            prose_lines.push(Line::from(Span::styled(prose, theme.style(Token::Muted))));
            hint_lines.push(Line::from(vec![
                Span::styled(format!(" {run_key} "), keycap),
                Span::styled(hint, theme.style(Token::Text)),
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
                    format!("{}.", ctx.connection.label()),
                    theme.style(Token::Danger).add_modifier(Modifier::BOLD),
                ),
            ]));
            prose_lines.push(Line::from(Span::styled(
                "Reconnect before running SQL. Existing results remain a snapshot.",
                theme.style(Token::Muted),
            )));
        }
    }
    if let Some(notice) = &model.clipboard_notice {
        prose_lines.push(Line::from(Span::styled(
            notice.message(),
            theme.style(Token::Warning),
        )));
    }
    if let Some(notice) = &model.cell_update_notice {
        prose_lines.push(Line::from(Span::styled(
            sanitize_for_display(notice),
            theme.style(Token::Warning),
        )));
    }
    if let Some(notice) = &model.refresh_notice {
        prose_lines.push(Line::from(Span::styled(
            sanitize_for_display(notice),
            theme.style(Token::Info),
        )));
    }
    hint_lines.push(Line::from(vec![
        Span::styled(format!(" {palette_key} "), keycap),
        Span::styled(
            "Command palette shows safe next actions.",
            theme.style(Token::Text),
        ),
    ]));

    // Prose, rule and hints stack from the top, as the Objects empty state
    // does, so a tall pane does not open a gap between the words and the keys.
    let measured = wrapped_line_count(&prose_lines, empty_area.width);
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

    Paragraph::new(prose_lines)
        .wrap(Wrap { trim: true })
        .render(prose_rect, buf);
    if separator_rows > 0 {
        let rule_width = usize::from(empty_area.width).min(48);
        if rule_width > 0 {
            Paragraph::new(Line::from(Span::styled(
                hairline_rule(presentation).repeat(rule_width),
                theme.style(Token::Border),
            )))
            .render(sep_rect, buf);
        }
    }
    Paragraph::new(hint_lines).render(hints_rect, buf);
}

/// The activity indicator: a moving frame, a word, and an honest elapsed time.
fn running_lines<'a>(model: &Model, keymap: &Keymap, presentation: &Presentation) -> Vec<Line<'a>> {
    let theme = &presentation.theme;
    let elapsed = model.running_for.unwrap_or_default();
    let seconds = elapsed.as_secs_f64();

    let leader = if presentation.reduced_motion {
        String::new()
    } else {
        format!("{} ", presentation.glyphs.spinner(model.frame))
    };
    let word = match model.phase {
        QueryPhase::CancellationRequested { .. } => "Cancellation requested",
        _ if model.running_refresh => "Refreshing retained result",
        _ => "Running",
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(leader, theme.style(Token::Info)),
        Span::styled(word.to_owned(), theme.style(Token::Text)),
        Span::styled(
            format!("  {}{seconds:.1}s", presentation.icon(Icon::Clock)),
            theme.style(Token::Muted),
        ),
    ])];

    if !presentation.reduced_motion {
        // A meter that shows time passing, not progress. The server reports no
        // progress, so nothing here may imply a percentage.
        let width = 24usize;
        let filled = usize::try_from(model.frame % (width as u64 + 1)).unwrap_or(0);
        lines.push(Line::from(Span::styled(
            presentation.glyphs.meter(filled, width),
            theme.style(Token::Border),
        )));
    }

    let cancellation = match model.phase {
        QueryPhase::CancellationRequested { .. } => {
            "The server has been asked to stop. It may still be running.".to_owned()
        }
        _ => format!(
            "{} asks the server to cancel.",
            action_key_label(keymap, &crate::app::Action::Cancel)
        ),
    };
    lines.push(Line::from(Span::styled(
        cancellation,
        theme.style(Token::Muted),
    )));

    lines
}

/// Draws the result grid: row numbers, headers, striped rows, and a scroll bar.
fn render_grid(
    model: &Model,
    set: &crate::query::result::ResultSet,
    keymap: &Keymap,
    presentation: &Presentation,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let rule = presentation.glyphs.column_rule();
    let padding = " ".repeat(usize::from(presentation.spacing().grid_cell_padding));

    // The filter and local sort produce source-row indices. The numbers in the
    // gutter stay those rows' own, so a sorted or filtered view still says where
    // each row is in the server result.
    let rows = model.displayed_rows();
    let has_type_row = model.result_grid.show_types && set.column_types.iter().any(Option::is_some);
    let header_rows = grid_header_rows(model, set, presentation.spacing(), area);
    let visible_rows = (area.height as usize).saturating_sub(header_rows).max(1);
    let needs_scrollbar = rows.len() > visible_rows;
    let grid_width = (area.width as usize).saturating_sub(usize::from(needs_scrollbar));

    // Keep the selection on screen. The window moves; rows never reorder, and
    // hidden columns leave source positions untouched.
    let offset = model
        .selected_row
        .saturating_sub(visible_rows.saturating_sub(1));
    let gutter = set.rows.len().max(1).to_string().len().max(2);
    let available =
        grid_width.saturating_sub(gutter + usize::from(presentation.spacing().grid_cell_padding));
    let automatic_widths: Vec<usize> = (0..set.columns.len())
        .map(|index| grid::automatic_column_width(set, index))
        .collect();
    let widths: Vec<usize> = automatic_widths
        .iter()
        .enumerate()
        .map(|(index, automatic)| model.result_grid.width_for(index, *automatic))
        .collect();
    let visible_columns = model.result_grid.visible_columns(set.columns.len());
    let selected_column = visible_columns
        .iter()
        .copied()
        .find(|index| *index == model.selected_column)
        .or_else(|| visible_columns.first().copied());
    let window = grid::column_window(
        &visible_columns,
        selected_column.unwrap_or(0),
        &widths,
        available,
        model.result_grid.freeze_first,
        model.result_grid.horizontal_start,
    );
    let alignment = column_alignment(set);

    let selected_label = selected_column
        .and_then(|index| set.columns.get(index))
        .map_or_else(|| "none".to_owned(), |name| sanitize_for_display(name));
    let selected_number = selected_column.map_or(0, |index| index + 1);
    // The range is the rows actually painted in the viewport, one-based and
    // inclusive. The prelude (order line below) already owns its row, so the
    // range extends that line rather than adding a sixth prelude row and
    // shifting every hit region. A pane with no room for a data row states no
    // range rather than claiming rows that were never drawn.
    let drawable_rows = (area.height as usize).saturating_sub(header_rows);
    let rendered_rows = rows.len().saturating_sub(offset).min(drawable_rows);
    let range_label = grid::viewport_position_label(
        offset + 1,
        offset + rendered_rows,
        rows.len(),
        set.retained(),
        set.rows_seen,
        set.is_truncated(),
        !model.result_filter.trim().is_empty(),
    );
    // Line 1 states where the viewport is, what order the rows are in, and
    // which column Enter acts on. A local sort and its qualifier are one fact,
    // stated once: it sorts the retained rows only, never the server result.
    let selected_segment = format!("selected column {selected_number}: {selected_label}");
    let mut order_candidates = Vec::new();
    match model.result_grid.sort_label(&set.columns) {
        Some(sort) => {
            let sort = format!("{sort} {LOCAL_SORT_QUALIFIER}");
            order_candidates.push(toolbar(&[
                range_label.as_deref(),
                Some(&sort),
                Some(&selected_segment),
            ]));
            order_candidates.push(toolbar(&[range_label.as_deref(), Some(&sort)]));
            order_candidates.push(sort);
            order_candidates.push(LOCAL_SORT_SHORT.to_owned());
        }
        None => {
            order_candidates.push(toolbar(&[
                range_label.as_deref(),
                Some(SERVER_ORDER),
                Some(&selected_segment),
            ]));
            order_candidates.push(toolbar(&[range_label.as_deref(), Some(SERVER_ORDER)]));
            order_candidates.extend(range_label.clone());
            order_candidates.push(SERVER_ORDER.to_owned());
        }
    }
    if set.is_truncated() {
        order_candidates.push("Retained window".to_owned());
    }

    // Line 2 states only what differs from a plain grid, plus the types,
    // which are a fact about the server's answer rather than a setting.
    let columns_label = if visible_columns.len() == set.columns.len() {
        match set.columns.len() {
            1 => "1 column".to_owned(),
            count => format!("{count} columns"),
        }
    } else {
        format!(
            "shown {}/{} columns",
            visible_columns.len(),
            set.columns.len()
        )
    };
    let type_count = set
        .column_types
        .iter()
        .filter(|value| value.is_some())
        .count();
    let type_label = if !model.result_grid.show_types {
        "types: hidden"
    } else if type_count == 0 {
        "types: unavailable"
    } else if type_count == set.columns.len() {
        "types: available"
    } else {
        "types: partial, some unavailable"
    };
    let freeze_label = model.result_grid.freeze_first.then_some("freeze first: on");
    // The controls open only from a focused Results pane, so an unfocused pane
    // does not spend its toolbar teaching a two-step route.
    let controls_label = focused.then(|| {
        format!(
            "{} grid controls",
            action_key_label(keymap, &crate::app::Action::OpenResultControls)
        )
    });
    let filter_window = (set.is_truncated() && !model.result_filter.trim().is_empty())
        .then(|| model.result_window_label());
    let mut state_candidates: Vec<String> = filter_window.into_iter().collect();
    state_candidates.push(toolbar(&[
        Some(&columns_label),
        Some(type_label),
        freeze_label,
        controls_label.as_deref(),
    ]));
    state_candidates.push(toolbar(&[
        Some(&columns_label),
        Some(type_label),
        freeze_label,
    ]));
    state_candidates.push(toolbar(&[Some(&columns_label), Some(type_label)]));
    state_candidates.push(columns_label.clone());
    if set.is_truncated() {
        state_candidates.push("Retained grid".to_owned());
    }

    let mut lines: Vec<Line> = Vec::with_capacity(visible_rows + header_rows);
    lines.extend(
        retention_warning_lines(model, area)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, theme.style(Token::Warning)))),
    );
    let order = first_fitting(order_candidates, grid_width);
    lines.push(styled_toolbar_line(
        &order,
        Token::Info,
        Some("selected column"),
        theme,
    ));
    let state_line = first_fitting(state_candidates, grid_width);
    lines.push(styled_toolbar_line(&state_line, Token::Muted, None, theme));
    let clipboard_note = model
        .clipboard_notice
        .as_ref()
        .map(|notice| notice.message());
    if let Some(note) = model
        .cell_update_notice
        .as_deref()
        .or(clipboard_note.as_deref())
        .or(model.refresh_notice.as_deref())
        .or(model.result_grid.note.as_deref())
    {
        lines.push(Line::from(Span::styled(
            truncate_to_width(
                &sanitize_for_display(note),
                grid_width,
                !presentation.glyphs.is_ascii(),
            ),
            theme.style(Token::Warning),
        )));
    } else {
        // Keep the row/header offsets deterministic even when there is no
        // limitation note. The blank line gives a little breathing room between
        // state and values, like a compact GUI toolbar.
        lines.extend((0..presentation.spacing().toolbar_blank_rows).map(|_| Line::from("")));
    }

    let mut header_spans = vec![Span::styled(
        format!("{:>gutter$}{padding}", "#"),
        theme.on_stripe(Token::Muted),
    )];
    for (position, index) in window.iter().enumerate() {
        if position > 0 {
            header_spans.push(Span::styled(
                format!("{rule}{padding}"),
                theme.on_stripe(Token::Border),
            ));
        }
        let name = set.columns.get(*index).map_or("unknown", String::as_str);
        let width = widths
            .get(*index)
            .copied()
            .unwrap_or(grid::MIN_COLUMN_WIDTH);
        let text = truncate_to_width(
            &sanitize_for_display(name),
            width,
            !presentation.glyphs.is_ascii(),
        );
        // The selection fill follows the same gate as the selected row: it
        // is painted only while Results has focus. An unfocused pane still
        // marks the column Enter would act on, in the focus ink alone.
        let header_style = match (Some(*index) == selected_column, focused) {
            (true, true) => theme.style(Token::Selection),
            (true, false) => theme.on_stripe(Token::Focus).add_modifier(Modifier::BOLD),
            (false, _) => theme.on_stripe(Token::Header),
        };
        // A name sits over its values: right-aligned over a column whose
        // values read as numbers, left-aligned otherwise.
        let cell = if alignment.get(*index).copied().unwrap_or(false) {
            let pad = width.saturating_sub(display_width(&text));
            format!("{}{text}", " ".repeat(pad))
        } else {
            pad_to_width(&text, width)
        };
        header_spans.push(Span::styled(format!("{cell}{padding}"), header_style));
    }
    let header_width: usize = header_spans.iter().map(|s| display_width(&s.content)).sum();
    if header_width < grid_width {
        header_spans.push(Span::styled(
            " ".repeat(grid_width - header_width),
            theme.stripe(),
        ));
    }
    lines.push(Line::from(header_spans).style(theme.stripe()));

    if has_type_row {
        let mut type_spans = vec![Span::styled(
            format!("{}{padding}", " ".repeat(gutter)),
            theme.style(Token::Muted),
        )];
        for (position, index) in window.iter().enumerate() {
            if position > 0 {
                type_spans.push(Span::styled(
                    format!("{rule}{padding}"),
                    theme.style(Token::Border),
                ));
            }
            let width = widths
                .get(*index)
                .copied()
                .unwrap_or(grid::MIN_COLUMN_WIDTH);
            let type_name = set
                .column_type(*index)
                .map_or_else(|| "unavailable".to_owned(), sanitize_for_display);
            let text = truncate_to_width(&type_name, width, !presentation.glyphs.is_ascii());
            type_spans.push(Span::styled(
                format!("{}{padding}", pad_to_width(&text, width)),
                theme.style(Token::Muted),
            ));
        }
        lines.push(Line::from(type_spans));
    }

    let rule_char = hairline_rule(presentation);
    let cross_char = if presentation.glyphs.is_ascii() {
        "+"
    } else {
        "\u{253c}"
    };
    let pad_w = padding.len();
    let mut grid_rule = String::with_capacity(grid_width);
    grid_rule.push_str(&rule_char.repeat(gutter + pad_w));
    for (position, index) in window.iter().enumerate() {
        if position > 0 {
            grid_rule.push_str(cross_char);
            grid_rule.push_str(&rule_char.repeat(pad_w));
        }
        let width = widths
            .get(*index)
            .copied()
            .unwrap_or(grid::MIN_COLUMN_WIDTH);
        grid_rule.push_str(&rule_char.repeat(width + pad_w));
    }
    let current_w = grid_rule.chars().count();
    if current_w < grid_width {
        grid_rule.push_str(&rule_char.repeat(grid_width - current_w));
    } else if current_w > grid_width {
        grid_rule = grid_rule.chars().take(grid_width).collect();
    }
    lines.push(Line::from(Span::styled(
        grid_rule,
        theme.style(Token::Border),
    )));

    for (offset_index, source) in rows.iter().skip(offset).take(visible_rows).enumerate() {
        let position = offset + offset_index;
        let Some(row) = set.rows.get(*source) else {
            continue;
        };
        let selected = focused && position == model.selected_row;
        let striped = position % 2 == 1;

        let mut spans = if selected {
            vec![Span::styled(
                format!("{:>gutter$}{padding}", source + 1),
                theme.style(Token::Selection),
            )]
        } else {
            vec![
                Span::styled(
                    format!("{:>gutter$}", source + 1),
                    theme.on_stripe(Token::Muted),
                ),
                Span::styled(
                    padding.clone(),
                    cell_style(theme, Token::Muted, false, striped),
                ),
            ]
        };

        for (column_position, index) in window.iter().enumerate() {
            if column_position > 0 {
                spans.push(Span::styled(
                    format!("{rule}{padding}"),
                    cell_style(theme, Token::Border, selected, striped),
                ));
            }
            let width = widths
                .get(*index)
                .copied()
                .unwrap_or(grid::MIN_COLUMN_WIDTH);
            let Some(cell) = row.get(*index) else {
                spans.push(Span::styled(
                    format!("{}{padding}", pad_to_width("[missing]", width)),
                    cell_style(theme, Token::Warning, selected, striped),
                ));
                continue;
            };
            let token = if cell.is_null() {
                Token::NullValue
            } else {
                Token::Text
            };
            let rendered =
                truncate_to_width(&cell.display(), width, !presentation.glyphs.is_ascii());
            let padded = if alignment.get(*index).copied().unwrap_or(false) {
                // Right-aligned by the shape of the value, not by its type. This
                // remains a reading aid and is never described as type-aware.
                let pad = width.saturating_sub(display_width(&rendered));
                format!("{}{rendered}{padding}", " ".repeat(pad))
            } else {
                format!("{}{padding}", pad_to_width(&rendered, width))
            };
            spans.push(Span::styled(
                padded,
                cell_style(theme, token, selected, striped),
            ));
        }

        let row_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();
        if row_width < grid_width {
            if selected {
                spans.push(Span::styled(
                    " ".repeat(grid_width - row_width),
                    theme.style(Token::Selection),
                ));
            } else if striped {
                spans.push(Span::styled(
                    " ".repeat(grid_width - row_width),
                    theme.stripe(),
                ));
            }
        }

        let mut line = Line::from(spans);
        if selected {
            line = line.style(theme.style(Token::Selection));
        } else if striped {
            line = line.style(theme.stripe());
        }
        lines.push(line);
    }

    if rows.is_empty() {
        let message = if model.result_filter.trim().is_empty() && set.rows_seen > 0 {
            format!(
                "No rows retained; PostgreSQL delivered {} rows.",
                set.rows_seen
            )
        } else if model.result_filter.trim().is_empty() {
            "No rows returned. Edit the SQL or use the command palette for the next action."
                .to_owned()
        } else {
            format!(
                "No retained row contains {:?}. {} clears the filter.",
                sanitize_for_display(&model.result_filter),
                action_key_label(keymap, &crate::app::Action::Dismiss)
            )
        };
        lines.push(Line::from(Span::styled(message, theme.style(Token::Muted))));
    }

    Paragraph::new(lines).render(area, buf);

    if needs_scrollbar {
        render_scrollbar(
            rows.len(),
            visible_rows,
            offset,
            presentation,
            Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + u16::try_from(header_rows).unwrap_or(u16::MAX),
                width: 1,
                height: area
                    .height
                    .saturating_sub(u16::try_from(header_rows).unwrap_or(u16::MAX)),
            },
            buf,
        );
    }
}

fn cell_style(theme: &Theme, token: Token, selected: bool, striped: bool) -> Style {
    if selected {
        theme.style(Token::Selection)
    } else if striped {
        theme.on_stripe(token)
    } else {
        theme.style(token)
    }
}

/// The wording that ties a local sort to the rows it actually sorts.
const LOCAL_SORT_QUALIFIER: &str = "(local, retained rows only)";

/// A local sort stated in the fewest words that still say both facts.
const LOCAL_SORT_SHORT: &str = "local sort, retained rows only";

/// The order of an unsorted grid: whatever order the server sent.
const SERVER_ORDER: &str = "order: server";

/// Joins the present toolbar segments with the shared delimiter.
fn toolbar(segments: &[Option<&str>]) -> String {
    segments
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The first candidate that fits, or nothing.
///
/// Candidates run from the fullest statement to the shortest, each made of
/// whole segments, so a narrow pane drops the least important fact rather than
/// cutting one mid-word or ellipsising a count.
fn first_fitting(candidates: Vec<String>, width: usize) -> String {
    candidates
        .into_iter()
        .find(|line| display_width(line) <= width)
        .unwrap_or_default()
}

pub(crate) fn styled_toolbar_line(
    text: &str,
    primary: Token,
    accent_prefix: Option<&str>,
    theme: &Theme,
) -> Line<'static> {
    if text.is_empty() {
        return Line::default();
    }
    let parts: Vec<&str> = text.split(" | ").collect();
    let mut spans = Vec::with_capacity(parts.len() * 2);
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", theme.style(Token::Border)));
        }
        let token = if let Some(prefix) = accent_prefix {
            if part.starts_with(prefix) {
                Token::Focus
            } else {
                primary
            }
        } else if primary == Token::Muted && part.contains("controls") {
            Token::Info
        } else {
            primary
        };
        spans.push(Span::styled(part.to_owned(), theme.style(token)));
    }
    Line::from(spans)
}

/// A scroll indicator, so rows below the fold are visibly below the fold.
fn render_scrollbar(
    total: usize,
    visible: usize,
    offset: usize,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height == 0 || total == 0 {
        return;
    }
    let track = area.height as usize;
    let thumb = ((visible * track) / total).clamp(1, track);
    let max_offset = total.saturating_sub(visible).max(1);
    let position = (offset * track.saturating_sub(thumb)) / max_offset;

    let (full, empty) = if presentation.glyphs.is_ascii() {
        ("#", "|")
    } else {
        ("\u{2588}", "\u{2502}")
    };

    for row in 0..track {
        let inside = row >= position && row < position + thumb;
        let symbol = if inside { full } else { empty };
        let style = presentation
            .theme
            .style(if inside { Token::Focus } else { Token::Border });
        buf.set_string(
            area.x,
            area.y + u16::try_from(row).unwrap_or(0),
            symbol,
            style,
        );
    }
}

/// Decides which columns read as numbers and should be right-aligned.
pub(crate) fn column_alignment(set: &crate::query::result::ResultSet) -> Vec<bool> {
    (0..set.columns.len())
        .map(|index| {
            let mut saw_value = false;
            for row in &set.rows {
                let Some(cell) = row.get(index) else { continue };
                let Some(text) = cell.raw() else { continue };
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                saw_value = true;
                if trimmed.parse::<f64>().is_err() {
                    return false;
                }
            }
            saw_value
        })
        .collect()
}
fn error_sql_spans(
    editor_text: &str,
    location: &crate::query::error_location::ErrorLocation,
    sql_part: &str,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let exact_raw_slice = (location.cursor <= editor_text.len()
        && editor_text.is_char_boundary(location.cursor))
    .then(|| {
        let start = editor_text[..location.cursor]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let end = editor_text[location.cursor..]
            .find('\n')
            .map_or(editor_text.len(), |i| location.cursor + i);
        (editor_text.get(start..end) == Some(sql_part)).then_some(start)
    })
    .flatten();

    match (exact_raw_slice, location.token.as_ref()) {
        (Some(line_start), Some(token_range)) => {
            let syntax = crate::query::highlight::tokens(sql_part);
            let mut spans = Vec::new();
            let mut run = String::new();
            let mut run_style: Option<Style> = None;
            for (byte, ch) in sql_part.char_indices() {
                let abs_byte = line_start + byte;
                let style = if token_range.contains(&abs_byte) {
                    theme
                        .on_stripe(Token::Danger)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    theme.on_stripe(syntax_token(crate::query::highlight::kind_at(
                        &syntax, byte,
                    )))
                };
                if run_style != Some(style) {
                    if let Some(previous) = run_style {
                        spans.push(Span::styled(std::mem::take(&mut run), previous));
                    }
                    run_style = Some(style);
                }
                run.push(ch);
            }
            if let Some(previous) = run_style {
                spans.push(Span::styled(run, previous));
            }
            spans
        }
        _ => highlighted_sql_spans(sql_part, theme, true),
    }
}

fn push_error_marker_lines(
    lines: &mut Vec<Line<'static>>,
    editor_text: &str,
    location: &crate::query::error_location::ErrorLocation,
    marker: &str,
    inner_width: usize,
    presentation: &Presentation,
) {
    let theme = &presentation.theme;
    let col_sep = format!(" {} ", presentation.glyphs.column_rule());
    for raw_line in marker.lines() {
        let line_width = display_width(raw_line);
        let mut spans = if let Some((line_num, sql_part)) = raw_line.split_once(" | ") {
            let mut row = vec![
                Span::styled(line_num.to_owned(), theme.on_stripe(Token::Muted)),
                Span::styled(col_sep.clone(), theme.on_stripe(Token::Border)),
            ];
            row.extend(error_sql_spans(editor_text, location, sql_part, theme));
            row
        } else if let Some(caret_pos) = raw_line.rfind('^') {
            vec![
                // Leading spaces must survive Wrap { trim: false } so ^ stays under the column.
                Span::styled(raw_line[..caret_pos].to_owned(), theme.stripe()),
                Span::styled(
                    raw_line[caret_pos..].to_owned(),
                    theme.on_stripe(Token::Danger).add_modifier(Modifier::BOLD),
                ),
            ]
        } else {
            vec![Span::styled(
                raw_line.to_owned(),
                theme.on_stripe(Token::Warning),
            )]
        };
        if line_width < inner_width {
            spans.push(Span::styled(
                " ".repeat(inner_width - line_width),
                theme.stripe(),
            ));
        }
        lines.push(Line::from(spans).style(theme.stripe()));
    }
}

pub(crate) fn render_error(
    model: &Model,
    error: &crate::diagnostics::Diagnostic,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let mut title_spans = Vec::new();
    if model.splits.zoomed && model.focus == Focus::Results {
        title_spans.push(Span::styled("(zoomed) ", theme.style(Token::Info)));
    }
    title_spans.push(Span::styled(
        format!("{}{}", presentation.icon(Icon::Error), error.kind.label()),
        theme.style(Token::Danger).add_modifier(Modifier::BOLD),
    ));
    let block = pane_block_line(Line::from(title_spans), false, presentation);
    let inner_width = usize::from(block.inner(area).width);

    // Every labelled row starts with the same gutter, an icon where the row
    // has one and blank cells where it does not, so `Likely cause:` and
    // `Next:` line up with `While:` and `Location:`. In ASCII icons are empty
    // and so is the gutter, which keeps the rows aligned there too.
    let gutter = |icon: Option<Icon>, token: Token| -> Span<'static> {
        match icon {
            _ if presentation.glyphs.is_ascii() => Span::raw(""),
            Some(icon) => Span::styled(presentation.icon(icon), theme.style(token)),
            None => Span::raw("  "),
        }
    };
    let labelled = |label: &'static str, label_token: Token, value: Span<'static>| {
        Line::from(vec![
            gutter(None, label_token),
            Span::styled(label, theme.style(label_token)),
            value,
        ])
    };
    let mut lines = vec![
        Line::from(Span::styled(
            sanitize_for_display(&error.headline),
            theme.style(Token::Danger),
        )),
        labelled(
            "While: ",
            Token::Header,
            Span::styled(
                sanitize_for_display(&error.attempted),
                theme.style(Token::Muted),
            ),
        ),
    ];
    if let Some(object) = &error.object {
        let facts = object_context_text(object);
        lines.push(labelled(
            "Object context: ",
            Token::Header,
            Span::styled(facts, theme.style(Token::Text)),
        ));
        let note = catalogue_note(model, object);
        if let Some(rest) = note.strip_prefix("Catalogue: ") {
            lines.push(labelled(
                "Catalogue: ",
                Token::Header,
                Span::styled(rest.to_owned(), theme.style(Token::Muted)),
            ));
        } else {
            lines.push(Line::from(vec![
                gutter(None, Token::Muted),
                Span::styled(note, theme.style(Token::Muted)),
            ]));
        }
    }
    if let Some(location) = &model.error_location {
        lines.push(labelled(
            "Location: ",
            Token::Header,
            Span::styled(
                format!(
                    "statement {}, line {}, column {}",
                    location.statement_number, location.line, location.column
                ),
                theme.style(Token::Info),
            ),
        ));
        if let Some(marker) =
            crate::query::error_location::render_marker(model.editor.text(), location)
        {
            push_error_marker_lines(
                &mut lines,
                model.editor.text(),
                location,
                &marker,
                inner_width,
                presentation,
            );
        }
    } else if let Some(note) = &model.error_location_note {
        lines.push(labelled(
            "Location: ",
            Token::Header,
            Span::styled(sanitize_for_display(note), theme.style(Token::Warning)),
        ));
    } else if let Some(position) = error.position {
        lines.push(labelled(
            "Location: ",
            Token::Header,
            Span::styled(
                format!("unavailable for server character {}.", position.character),
                theme.style(Token::Warning),
            ),
        ));
    } else if let Some(statement_number) = error.statement_number {
        lines.push(labelled(
            "Statement: ",
            Token::Header,
            Span::styled(statement_number.to_string(), theme.style(Token::Info)),
        ));
    }
    if let Some(cause) = &error.likely_cause {
        lines.push(Line::from(vec![
            gutter(Some(Icon::Warning), Token::Warning),
            Span::styled("Likely cause: ", theme.style(Token::Warning)),
            Span::styled(sanitize_for_display(cause), theme.style(Token::Text)),
        ]));
    }
    if let Some(action) = &error.next_action {
        lines.push(Line::from(vec![
            gutter(Some(Icon::Info), Token::Info),
            Span::styled("Next: ", theme.style(Token::Header)),
            Span::styled(sanitize_for_display(action), theme.style(Token::Info)),
        ]));
    }
    // The expand key is the one the keymap binds, and once the fields are
    // shown the same key is offered to fold them away again.
    let detail_key = error_detail_key(model);
    let key_hint = |verb: &'static str| -> Vec<Span<'static>> {
        detail_key.clone().map_or_else(Vec::new, |key| {
            vec![
                Span::styled(key, theme.style(Token::Info).add_modifier(Modifier::BOLD)),
                Span::styled(verb, theme.style(Token::Muted)),
            ]
        })
    };
    if model.error_expanded {
        for field in &error.technical {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<12}", sanitize_for_display(&field.label)),
                    theme.style(Token::Muted),
                ),
                Span::styled(sanitize_for_display(&field.value), theme.style(Token::Text)),
            ]));
        }
        if !error.technical.is_empty() {
            let mut spans = vec![gutter(None, Token::Muted)];
            spans.extend(key_hint(" to collapse."));
            if spans.len() > 1 {
                lines.push(Line::from(spans));
            }
        }
    } else if !error.technical.is_empty() {
        let mut spans = vec![
            gutter(None, Token::Muted),
            Span::styled(
                format!("{} technical field(s) hidden. ", error.technical.len()),
                theme.style(Token::Muted),
            ),
        ];
        spans.extend(key_hint(" to expand."));
        lines.push(Line::from(spans));
    }

    Paragraph::new(lines)
        .block(block)
        // Preserve leading indentation on error caret and technical-field lines.
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

/// The key the active keymap binds to expanding error detail, or `None` when
/// nothing is bound, in which case no key is advertised at all.
fn error_detail_key(model: &Model) -> Option<String> {
    let fallback;
    let snapshot = if model.keymap_snapshot.is_empty() {
        fallback = Keymap::new().snapshot();
        &fallback
    } else {
        &model.keymap_snapshot
    };
    snapshot
        .bindings
        .iter()
        .find(|binding| binding.action == crate::app::Action::ToggleErrorDetail)
        .map(|binding| sanitize_for_display(&binding.key))
}

fn object_context_text(object: &crate::diagnostics::ObjectContext) -> String {
    let mut facts = Vec::new();
    match (object.schema.as_deref(), object.table.as_deref()) {
        (Some(schema), Some(table)) => facts.push(format!(
            "relation {}.{}",
            quoted_object(schema),
            quoted_object(table)
        )),
        (None, Some(table)) => facts.push(format!("table {}", quoted_object(table))),
        (Some(schema), None) => facts.push(format!("schema {}", quoted_object(schema))),
        (None, None) => {}
    }
    if let Some(column) = object.column.as_deref() {
        facts.push(format!("column {}", quoted_object(column)));
    }
    if let Some(constraint) = object.constraint.as_deref() {
        facts.push(format!("constraint {}", quoted_object(constraint)));
    }
    if facts.is_empty() {
        "server supplied no named object fields".to_owned()
    } else {
        facts.join(", ")
    }
}

fn quoted_object(value: &str) -> String {
    sanitize_for_display(&crate::query::quote_identifier(value))
}

fn catalogue_note(model: &Model, object: &crate::diagnostics::ObjectContext) -> String {
    let Some(table) = object.table.as_deref() else {
        return "Catalogue: cannot match without a server-supplied relation; no catalogue-derived claim."
            .to_owned();
    };
    let Some(catalog) = model.completion.catalog() else {
        return format!(
            "Catalogue: {}; no catalogue-derived claim.",
            sanitize_for_display(&model.completion.catalog.message())
        );
    };
    let relation = object.schema.as_deref().map_or_else(
        || {
            let matches = catalog.relations_named(table);
            (matches.len() == 1).then(|| matches[0])
        },
        |schema| catalog.relation(Some(schema), table),
    );
    let Some(relation) = relation else {
        return format!(
            "Catalogue: snapshot does not uniquely match relation {}; no catalogue-derived claim.",
            quoted_object(table)
        );
    };
    let Some(column) = object.column.as_deref() else {
        return format!(
            "Catalogue: matched relation {}. No column type was inferred.",
            quoted_object(&relation.name)
        );
    };
    let Some(catalogue_column) = relation
        .columns
        .iter()
        .find(|candidate| candidate.name == column)
    else {
        return format!(
            "Catalogue: matched relation but not column {}; no column type was inferred.",
            quoted_object(column)
        );
    };
    format!(
        "Catalogue: matched relation and column {}; type {}.",
        quoted_object(&catalogue_column.name),
        sanitize_for_display(&catalogue_column.data_type)
    )
}

#[cfg(test)]
mod tests;
