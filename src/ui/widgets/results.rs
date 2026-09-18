use super::overlays::render_plan;
use super::{action_key_label, input_cursor, overlay_block, pane_block};
use crate::app::grid;
use crate::app::model::{Focus, Model, QueryPhase};
use crate::query::value::{display_width, pad_to_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, inspector_box, inspector_viewport};
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

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
        let block = pane_block(
            format!(" {}Transaction failed ", presentation.icon(Icon::Warning)),
            focused,
            presentation,
        );
        let inner = block.inner(area);
        block.render(area, buf);
        let mut lines = vec![Line::from(Span::styled(
            "This transaction has failed.",
            theme.style(Token::TransactionFailed),
        ))];
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
        lines.push(Line::from(Span::styled(
            model
                .transaction
                .recovery()
                .unwrap_or("Run ROLLBACK to end it."),
            theme.style(Token::Info),
        )));
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(inner, buf);
        return;
    }

    let summary = execution
        .statements
        .first()
        .map_or_else(|| execution.status.label().to_owned(), |s| s.summary());
    let status = if model.running_refresh {
        "Refreshing retained result".to_owned()
    } else {
        execution.status.label().to_owned()
    };
    let mut title = format!(
        " {}Results  {}  {} ",
        presentation.icon(Icon::Rows),
        status,
        summary
    );
    // What the filter admits is stated in full: how many match, of how many are
    // held, of how many the server sent. Collapsing those into one number is
    // the lie this pane exists not to tell.
    if !model.result_filter.trim().is_empty() {
        title.push_str(&format!("[{}] ", model.result_window_label()));
    }
    if model.result_filtering {
        title.push_str(&format!(
            "[filter: {}{}] ",
            sanitize_for_display(&model.result_filter),
            input_cursor(presentation)
        ));
    }
    if focused {
        title.push_str(&format!(
            "[focused] [{} grid controls] ",
            action_key_label(keymap, &crate::app::Action::OpenResultControls)
        ));
    }

    let block = pane_block(title, focused, presentation);
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

    if inner.height < 2 || inner.width == 0 {
        return;
    }

    if model.expanded_row {
        render_expanded_row(model, set, presentation, inner, buf);
    } else {
        render_grid(model, set, keymap, presentation, focused, inner, buf);
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
        Paragraph::new(Line::from(Span::styled(
            "No rows, so there is no row to expand.",
            theme.style(Token::Muted),
        )))
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
            format!("   {}", set.window_label()),
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
            line = line.style(theme.style(Token::Selection));
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
    let box_area = inspector_box(area);
    if box_area.width < 8 || box_area.height < 4 {
        return;
    }
    let (width, height) = inspector_viewport((area.width, area.height));

    let Some(set) = model.visible_result() else {
        return;
    };
    let Some(row) = model.selected_source_row() else {
        return;
    };
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

fn render_results_placeholder(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let block = pane_block(
        format!(" {}Results ", presentation.icon(Icon::Rows)),
        focused,
        presentation,
    );
    let inner = block.inner(area);
    block.render(area, buf);

    // While a query is in flight this is where the activity indicator lives, so
    // the user is looking at the place the answer will appear.
    if model.phase.is_busy() {
        Paragraph::new(running_lines(model, keymap, presentation)).render(inner, buf);
        return;
    }

    let ctx = crate::app::discovery::context(model);
    let palette_key = action_key_label(keymap, &crate::app::Action::OpenPalette);
    let mut lines = Vec::new();
    match ctx.connection {
        crate::app::discovery::ConnectionPosture::Disconnected => {
            lines.push(Line::from(vec![
                Span::styled(
                    presentation.icon(Icon::Warning),
                    theme.style(Token::Warning),
                ),
                Span::styled(" Connection required.", theme.style(Token::Warning)),
            ]));
            lines.push(Line::from(Span::styled(
                "Connect before running SQL. Results appear here after a query.",
                theme.style(Token::Muted),
            )));
        }
        crate::app::discovery::ConnectionPosture::Connecting => {
            lines.push(Line::from(vec![
                Span::styled(presentation.icon(Icon::Info), theme.style(Token::Info)),
                Span::styled(" Waiting for the connection.", theme.style(Token::Text)),
            ]));
            lines.push(Line::from(Span::styled(
                "Results will appear here after a query finishes.",
                theme.style(Token::Muted),
            )));
        }
        crate::app::discovery::ConnectionPosture::Connected => {
            let run_key = action_key_label(keymap, &crate::app::Action::RunBuffer);
            lines.push(Line::from(vec![
                Span::styled(presentation.icon(Icon::Info), theme.style(Token::Muted)),
                Span::styled(" No query has run yet.", theme.style(Token::Text)),
            ]));
            lines.push(Line::from(Span::styled(
                format!("{run_key} Run the buffer after entering SQL."),
                theme.style(Token::Info),
            )));
        }
        crate::app::discovery::ConnectionPosture::Lost
        | crate::app::discovery::ConnectionPosture::Failed => {
            lines.push(Line::from(vec![
                Span::styled(presentation.icon(Icon::Warning), theme.style(Token::Danger)),
                Span::styled(
                    format!(" {}.", ctx.connection.label()),
                    theme.style(Token::Danger),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "Reconnect before running SQL. Existing results remain a snapshot.",
                theme.style(Token::Muted),
            )));
        }
    }
    if let Some(notice) = &model.clipboard_notice {
        lines.push(Line::from(Span::styled(
            notice.message(),
            theme.style(Token::Warning),
        )));
    }
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
    lines.push(Line::from(Span::styled(
        format!("{palette_key} Command palette shows safe next actions."),
        theme.style(Token::Muted),
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .render(inner, buf);
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

    // The filter and local sort produce source-row indices. The numbers in the
    // gutter stay those rows' own, so a sorted or filtered view still says where
    // each row is in the server result.
    let rows = model.displayed_rows();
    let has_type_row = model.result_grid.show_types && set.column_types.iter().any(Option::is_some);
    let header_rows = 5 + usize::from(has_type_row);
    let visible_rows = (area.height as usize).saturating_sub(header_rows).max(1);
    let needs_scrollbar = rows.len() > visible_rows;
    let grid_width = (area.width as usize).saturating_sub(usize::from(needs_scrollbar));

    // Keep the selection on screen. The window moves; rows never reorder, and
    // hidden columns leave source positions untouched.
    let offset = model
        .selected_row
        .saturating_sub(visible_rows.saturating_sub(1));
    let gutter = set.rows.len().max(1).to_string().len().max(2);
    let available = grid_width.saturating_sub(gutter + 1);
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
    let order_label = model.result_grid.sort.map_or_else(
        || "order: server".to_owned(),
        |sort| {
            format!(
                "order: local {} on column {} (retained rows only)",
                sort.direction.label(),
                sort.column + 1
            )
        },
    );
    let columns_label = format!(
        "shown {}/{} columns",
        visible_columns.len(),
        set.columns.len()
    );
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
    let freeze_label = if model.result_grid.freeze_first {
        "freeze first: on"
    } else {
        "freeze first: off"
    };
    let controls_key = action_key_label(keymap, &crate::app::Action::OpenResultControls);
    let controls_label = if focused {
        format!("{controls_key} grid controls")
    } else {
        format!("focus Results, then {controls_key} for controls")
    };

    let mut lines: Vec<Line> = Vec::with_capacity(visible_rows + header_rows);
    lines.push(Line::from(Span::styled(
        truncate_to_width(
            &format!("{order_label} | selected column {selected_number}: {selected_label}"),
            grid_width,
            !presentation.glyphs.is_ascii(),
        ),
        theme.style(Token::Info),
    )));
    let state_line = format!("{columns_label} | {type_label} | {freeze_label} | {controls_label}");
    lines.push(Line::from(Span::styled(
        truncate_to_width(&state_line, grid_width, !presentation.glyphs.is_ascii()),
        theme.style(Token::Muted),
    )));
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
        lines.push(Line::from(""));
    }

    let mut header_spans = vec![Span::styled(
        format!("{} ", " ".repeat(gutter)),
        theme.style(Token::Muted),
    )];
    for (position, index) in window.iter().enumerate() {
        if position > 0 {
            header_spans.push(Span::styled(format!("{rule} "), theme.style(Token::Border)));
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
        let header_style = if Some(*index) == selected_column {
            theme.style(Token::Selection)
        } else {
            theme.style(Token::Header)
        };
        header_spans.push(Span::styled(
            format!("{} ", pad_to_width(&text, width)),
            header_style,
        ));
    }
    lines.push(Line::from(header_spans));

    if has_type_row {
        let mut type_spans = vec![Span::styled(
            format!("{} ", " ".repeat(gutter)),
            theme.style(Token::Muted),
        )];
        for (position, index) in window.iter().enumerate() {
            if position > 0 {
                type_spans.push(Span::styled(format!("{rule} "), theme.style(Token::Border)));
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
                format!("{} ", pad_to_width(&text, width)),
                theme.style(Token::Muted),
            ));
        }
        lines.push(Line::from(type_spans));
    }

    let rule_char = if presentation.glyphs.is_ascii() {
        "-"
    } else {
        "\u{2500}"
    };
    lines.push(Line::from(Span::styled(
        rule_char.repeat(grid_width),
        theme.style(Token::Border),
    )));

    for (offset_index, source) in rows.iter().skip(offset).take(visible_rows).enumerate() {
        let position = offset + offset_index;
        let Some(row) = set.rows.get(*source) else {
            continue;
        };
        let selected = focused && position == model.selected_row;
        let striped = position % 2 == 1;

        let mut spans = vec![Span::styled(
            format!("{:>gutter$} ", source + 1),
            cell_style(theme, Token::Muted, selected, striped),
        )];

        for (column_position, index) in window.iter().enumerate() {
            if column_position > 0 {
                spans.push(Span::styled(
                    format!("{rule} "),
                    cell_style(theme, Token::Border, selected, striped),
                ));
            }
            let width = widths
                .get(*index)
                .copied()
                .unwrap_or(grid::MIN_COLUMN_WIDTH);
            let Some(cell) = row.get(*index) else {
                spans.push(Span::styled(
                    format!("{} ", pad_to_width("[missing]", width)),
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
                format!("{}{rendered} ", " ".repeat(pad))
            } else {
                format!("{} ", pad_to_width(&rendered, width))
            };
            spans.push(Span::styled(
                padded,
                cell_style(theme, token, selected, striped),
            ));
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
        let message = if model.result_filter.trim().is_empty() {
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
pub(crate) fn render_error(
    model: &Model,
    error: &crate::diagnostics::Diagnostic,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let block = pane_block(
        format!(" {}{} ", presentation.icon(Icon::Error), error.kind.label()),
        false,
        presentation,
    );

    let mut lines = vec![
        Line::from(Span::styled(
            sanitize_for_display(&error.headline),
            theme.style(Token::Danger),
        )),
        Line::from(Span::styled(
            format!("While: {}", sanitize_for_display(&error.attempted)),
            theme.style(Token::Muted),
        )),
    ];
    if let Some(object) = &error.object {
        let facts = object_context_text(object);
        lines.push(Line::from(Span::styled(
            format!("Object context: {facts}"),
            theme.style(Token::Text),
        )));
        lines.push(Line::from(Span::styled(
            catalogue_note(model, object),
            theme.style(Token::Muted),
        )));
    }
    if let Some(location) = &model.error_location {
        lines.push(Line::from(Span::styled(
            format!(
                "Location: statement {}, line {}, column {}",
                location.statement_number, location.line, location.column
            ),
            theme.style(Token::Info),
        )));
        if let Some(marker) =
            crate::query::error_location::render_marker(model.editor.text(), location)
        {
            for line in marker.lines() {
                lines.push(Line::from(Span::styled(
                    line.to_owned(),
                    theme.style(Token::Warning),
                )));
            }
        }
    } else if let Some(note) = &model.error_location_note {
        lines.push(Line::from(Span::styled(
            format!("Location: {}", sanitize_for_display(note)),
            theme.style(Token::Warning),
        )));
    } else if let Some(position) = error.position {
        lines.push(Line::from(Span::styled(
            format!(
                "Location: unavailable for server character {}.",
                position.character
            ),
            theme.style(Token::Warning),
        )));
    } else if let Some(statement_number) = error.statement_number {
        lines.push(Line::from(Span::styled(
            format!("Statement: {statement_number}"),
            theme.style(Token::Info),
        )));
    }
    if let Some(cause) = &error.likely_cause {
        lines.push(Line::from(vec![
            Span::styled(
                presentation.icon(Icon::Warning),
                theme.style(Token::Warning),
            ),
            Span::styled(
                format!("Likely cause: {}", sanitize_for_display(cause)),
                theme.style(Token::Text),
            ),
        ]));
    }
    if let Some(action) = &error.next_action {
        lines.push(Line::from(vec![
            Span::styled(presentation.icon(Icon::Info), theme.style(Token::Info)),
            Span::styled(
                format!("Next: {}", sanitize_for_display(action)),
                theme.style(Token::Info),
            ),
        ]));
    }
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
    } else if !error.technical.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "{} technical field(s) hidden. Ctrl+D to expand.",
                error.technical.len()
            ),
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .render(area, buf);
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
