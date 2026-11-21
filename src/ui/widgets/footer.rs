use crate::app::model::{Model, QueryPhase};
use crate::diagnostics::DiagnosticKind;
use crate::query::result::ExecutionStatus;
use crate::query::value::{display_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

trait FooterHintCap {
    fn footer_hint_cap(&self) -> usize;
}

impl FooterHintCap for Presentation {
    fn footer_hint_cap(&self) -> usize {
        self.spacing().footer_hint_cap
    }
}

/// Returns false only when layout feedback needs the body's border rail.
pub(crate) fn render_footer(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) -> bool {
    let theme = &presentation.theme;
    let server_cancelled = model.phase == QueryPhase::Idle
        && model.connection.is_usable()
        && model
            .last_execution
            .as_ref()
            .filter(|execution| execution.status == ExecutionStatus::Cancelled)
            .and_then(|execution| execution.error.as_ref())
            .is_some_and(|error| {
                // The same diagnostic kind can also describe a local deadline.
                model.error.as_ref() == Some(error)
                    && error.kind == DiagnosticKind::Cancelled
                    && error
                        .technical
                        .iter()
                        .any(|field| field.label == "SQLSTATE" && field.value == "57014")
            });
    let (status_icon, status_token) = match model.phase {
        QueryPhase::Idle if server_cancelled => (Icon::Warning, Token::Warning),
        QueryPhase::Idle => (Icon::Success, Token::Muted),
        QueryPhase::Running { .. } => (Icon::Info, Token::Info),
        QueryPhase::CancellationRequested { .. } => (Icon::Warning, Token::Warning),
    };

    // While a statement runs the status carries the live frame, so the footer is
    // never a static word that could be mistaken for a hung client.
    let leader = if model.phase.is_busy() && !presentation.reduced_motion {
        format!("{} ", presentation.glyphs.spinner(model.frame))
    } else {
        presentation.icon(status_icon)
    };
    let divider = if usize::from(area.width) <= 44 {
        if presentation.glyphs.is_ascii() {
            "|"
        } else {
            "\u{2502}"
        }
    } else {
        presentation.glyphs.separator()
    };
    let divider_width = display_width(divider);
    let transaction = model.transaction.label();
    let mut status = format!(
        " {leader}{}",
        if server_cancelled {
            ExecutionStatus::Cancelled.label()
        } else {
            model.phase.label()
        }
    );
    if server_cancelled
        && display_width(&status) + display_width(transaction) + divider_width + 1
            > usize::from(area.width)
    {
        status = format!(" {leader}Cancelled");
    }
    if let Some(elapsed) = model.running_for.filter(|_| model.phase.is_busy()) {
        status.push_str(&format!(" {:.1}s", elapsed.as_secs_f64()));
    }
    let mut spans = vec![Span::styled(status, theme.on_stripe(status_token))];

    // The transaction state is the one thing a user can be wrong about at real
    // cost, so it sits next to the status rather than behind a panel.
    let transaction_token = match model.transaction {
        crate::query::result::TransactionState::Failed => Token::TransactionFailed,
        crate::query::result::TransactionState::Open => Token::TransactionActive,
        _ => Token::Muted,
    };
    spans.push(Span::styled(divider, theme.on_stripe(Token::Border)));
    spans.push(Span::styled(
        transaction.to_owned(),
        theme.on_stripe(transaction_token),
    ));

    let showing_transient_notice = model.retained_data_loss.limited_sets > 0
        || model.notice_state.pending.needs_report()
        || model.format_notice.is_some()
        || model.paste_notice.is_some()
        || model.layout_notice.is_some();
    if let Some(info) = model
        .connection
        .info()
        .filter(|_| !showing_transient_notice)
    {
        spans.push(Span::styled(divider, theme.on_stripe(Token::Border)));
        spans.push(Span::styled(
            format!(
                "{}{}",
                presentation.icon(Icon::SearchPath),
                info.search_path
            ),
            theme.on_stripe(Token::Muted),
        ));
    }

    // The footer is a rail, not a second help page. Its candidates are derived
    // from focus and state, while the keymap supplies the exact active key.
    // Stop before the next complete pair would be clipped so every shown hint
    // retains both its key and its meaning in a narrow terminal.
    let mut used: usize = spans.iter().map(|s| display_width(&s.content)).sum();

    if let Some(badge) =
        crate::ui::notices::badge(model, usize::from(area.width).saturating_sub(used + 1))
    {
        used += display_width(&badge) + 1;
        spans.push(Span::styled(
            format!(" {badge}"),
            theme.on_stripe(Token::Warning),
        ));
    }

    if model.retained_data_loss.limited_sets > 0 && !crate::ui::notices::covered(model) {
        let badge = format!("Limited sets: {}", model.retained_data_loss.limited_sets);
        if display_width(&badge) <= usize::from(area.width).saturating_sub(used + 1) {
            used += display_width(&badge) + 1;
            spans.push(Span::styled(
                format!(" {badge}"),
                theme.on_stripe(Token::Warning),
            ));
        }
    }

    if let Some(notice) = model.layout_notice {
        let remaining = usize::from(area.width).saturating_sub(used + 1);
        let wordings = notice.wordings();
        let wording = wordings
            .into_iter()
            .find(|wording| display_width(wording) <= remaining);
        if let Some(wording) = wording {
            spans.push(Span::styled(
                format!(" {wording}"),
                theme.on_stripe(Token::Info),
            ));
        }
        Paragraph::new(Line::from(spans))
            .style(theme.surface(Token::SurfaceAlt))
            .render(area, buf);
        return wording.is_some();
    }

    // A paste that did not reach the editor travels the same rail as a format
    // result: it is rendered through the display-sanitising rule, wears the same
    // refusal or information colour, and falls back to its short form rather
    // than losing the message in a narrow terminal. It is checked first because
    // it is the newest feedback when both exist.
    if let Some(notice) = &model.paste_notice {
        let token = if matches!(notice, crate::app::model::PasteNotice::Refused) {
            Token::Warning
        } else {
            Token::Info
        };
        let remaining = usize::from(area.width).saturating_sub(used + 1);
        let full = sanitize_for_display(&notice.message());
        let compact = match notice {
            crate::app::model::PasteNotice::Refused => "Paste refused",
            crate::app::model::PasteNotice::Ignored => "Paste ignored",
        };
        let message = if display_width(&full) <= remaining {
            full
        } else {
            truncate_to_width(compact, remaining, !presentation.glyphs.is_ascii())
        };
        if !message.is_empty() {
            spans.push(Span::styled(format!(" {message}"), theme.on_stripe(token)));
        }
        Paragraph::new(Line::from(spans))
            .style(theme.surface(Token::SurfaceAlt))
            .render(area, buf);
        return true;
    }

    // A formatting result is transient feedback for the editor. Give it the
    // rail before contextual hints so a refusal or a successful edit cannot be
    // hidden behind the next action label.
    if let Some(notice) = &model.format_notice {
        let token = if matches!(notice, crate::app::model::FormatNotice::Refused { .. }) {
            Token::Warning
        } else {
            Token::Info
        };
        let remaining = usize::from(area.width).saturating_sub(used + 1);
        let full = notice.message();
        let compact = match notice {
            crate::app::model::FormatNotice::Applied { .. } => "Formatted SQL",
            crate::app::model::FormatNotice::AlreadyFormatted => "already formatted",
            crate::app::model::FormatNotice::Empty => "no SQL to format",
            crate::app::model::FormatNotice::Refused { .. } => "Format skipped",
        };
        let message = if display_width(&full) <= remaining {
            full
        } else {
            truncate_to_width(compact, remaining, !presentation.glyphs.is_ascii())
        };
        if !message.is_empty() {
            spans.push(Span::styled(format!(" {message}"), theme.on_stripe(token)));
        }
        Paragraph::new(Line::from(spans))
            .style(theme.surface(Token::SurfaceAlt))
            .render(area, buf);
        return true;
    }

    if let Some(notice) = &model.cell_update_notice {
        let remaining = usize::from(area.width).saturating_sub(used + 1);
        let full = sanitize_for_display(notice);
        let message = if display_width(&full) <= remaining {
            full
        } else {
            truncate_to_width(
                "Cell update status",
                remaining,
                !presentation.glyphs.is_ascii(),
            )
        };
        if !message.is_empty() {
            spans.push(Span::styled(
                format!(" {message}"),
                theme.on_stripe(Token::Warning),
            ));
        }
    }

    let used: usize = spans.iter().map(|s| display_width(&s.content)).sum();
    let separator = if presentation.glyphs.is_ascii() {
        " | "
    } else {
        " \u{00b7} "
    };
    let separator_width = display_width(separator);
    let keycap_style = theme.keycap(Token::Focus);
    let label_style = theme.on_stripe(Token::Text);
    let mut hint_spans: Vec<Span<'static>> = Vec::new();
    let mut hints_width = 0usize;
    let mut hints_rendered = 0usize;

    for action in crate::app::discovery::hint_actions_for(model, presentation)
        .into_iter()
        .take(presentation.footer_hint_cap())
    {
        let Some((key, label)) = keymap.contextual_hint(&action) else {
            continue;
        };
        let key_width = display_width(&key);
        let label_width = display_width(label);
        let pair_width = key_width + 2 + label_width;
        let added_width = if hints_rendered == 0 {
            pair_width
        } else {
            separator_width + pair_width
        };
        if used
            .saturating_add(1)
            .saturating_add(hints_width)
            .saturating_add(added_width)
            > usize::from(area.width)
        {
            break;
        }
        if hints_rendered > 0 {
            hint_spans.push(Span::styled(separator, theme.on_stripe(Token::Border)));
        }
        hint_spans.push(Span::styled(format!(" {key} "), keycap_style));
        hint_spans.push(Span::styled(label, label_style));
        hints_width += added_width;
        hints_rendered += 1;
    }

    if hints_rendered > 0 {
        let total_width = usize::from(area.width);
        if used + hints_width + 2 <= total_width {
            let right_pad = usize::from(used + hints_width + 3 <= total_width);
            let gap = total_width - used - hints_width - right_pad;
            spans.push(Span::raw(" ".repeat(gap)));
            spans.extend(hint_spans);
            if right_pad == 1 {
                spans.push(Span::raw(" "));
            }
        } else {
            spans.push(Span::raw(" "));
            spans.extend(hint_spans);
        }
    }

    Paragraph::new(Line::from(spans))
        .style(theme.surface(Token::SurfaceAlt))
        .render(area, buf);
    true
}

#[cfg(test)]
mod tests;
