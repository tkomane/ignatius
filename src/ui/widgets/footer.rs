use crate::app::model::{Model, QueryPhase};
use crate::query::value::{display_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::keymap::Keymap;
use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub(crate) fn render_footer(
    model: &Model,
    keymap: &Keymap,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let (status_icon, status_token) = match model.phase {
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
    let mut status = format!(" {leader}{}", model.phase.label());
    if let Some(elapsed) = model.running_for.filter(|_| model.phase.is_busy()) {
        status.push_str(&format!(" {:.1}s", elapsed.as_secs_f64()));
    }
    let mut spans = vec![Span::styled(
        format!("{status} "),
        theme.style(status_token),
    )];

    // The transaction state is the one thing a user can be wrong about at real
    // cost, so it sits next to the status rather than behind a panel.
    let transaction_token = match model.transaction {
        crate::query::result::TransactionState::Failed => Token::TransactionFailed,
        crate::query::result::TransactionState::Open => Token::TransactionActive,
        _ => Token::Muted,
    };
    spans.push(Span::styled(
        format!("{} ", model.transaction.label()),
        theme.style(transaction_token),
    ));

    let showing_transient_notice = model.format_notice.is_some() || model.paste_notice.is_some();
    if let Some(info) = model
        .connection
        .info()
        .filter(|_| !showing_transient_notice)
    {
        spans.push(Span::styled(
            format!(
                "{}{} ",
                presentation.icon(Icon::SearchPath),
                info.search_path
            ),
            theme.style(Token::Muted),
        ));
    }

    // The footer is a rail, not a second help page. Its candidates are derived
    // from focus and state, while the keymap supplies the exact active key.
    // Stop before the next complete pair would be clipped so every shown hint
    // retains both its key and its meaning in a narrow terminal.
    let mut used = display_width(&status) + display_width(model.transaction.label()) + 2;
    if let Some(info) = model
        .connection
        .info()
        .filter(|_| !showing_transient_notice)
    {
        used += display_width(&info.search_path) + 2;
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
            spans.push(Span::styled(format!(" {message}"), theme.style(token)));
        }
        Paragraph::new(Line::from(spans)).render(area, buf);
        return;
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
            spans.push(Span::styled(format!(" {message}"), theme.style(token)));
        }
        Paragraph::new(Line::from(spans)).render(area, buf);
        return;
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
                theme.style(Token::Warning),
            ));
        }
    }

    for action in crate::app::discovery::hint_actions(model) {
        let Some((key, label)) = keymap.contextual_hint(&action) else {
            continue;
        };
        let segment_width = display_width(&key) + display_width(label) + 3;
        if used.saturating_add(segment_width) > usize::from(area.width) {
            break;
        }
        spans.push(Span::styled(
            format!(" {key} "),
            theme.style(Token::Focus).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(label, theme.style(Token::Muted)));
        used += segment_width;
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

#[cfg(test)]
mod tests;
