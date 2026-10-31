use crate::app::model::{ConnectingStep, ConnectionState, Model};
use crate::connection::Environment;
use crate::query::result::TransactionState;
use crate::query::value::{display_width, sanitize_for_display};
use crate::ui::glyphs::Icon;
use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use std::time::Duration;

/// Constructs a filled capsule style for non-routine badges when contrast permits,
/// or falls back to the theme's capsule style.
pub(crate) fn capsule_style(presentation: &Presentation, token: Token) -> Style {
    presentation.theme.capsule(token)
}

pub(crate) fn render_header(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let theme = &presentation.theme;
    let environment = model.environment();
    let mut left_spans: Vec<Span<'static>> = Vec::new();

    // 1. Product name
    let brand = presentation.icon(Icon::Brand);
    let name = crate::branding::PRODUCT_NAME;
    left_spans.push(Span::styled(
        format!(" {brand}{name}: "),
        theme.style(Token::Header),
    ));

    // 2. Connection state / target
    left_spans.push(Span::styled(
        format!(
            "{}{}",
            presentation.icon(Icon::Database),
            model.connection.label()
        ),
        theme.style(Token::Text),
    ));

    // Failed connection step if any
    if let Some(step) = failed_step(model) {
        left_spans.push(Span::raw("  "));
        left_spans.push(Span::styled(
            format!("failed during {}", step.label()),
            theme.style(Token::Danger),
        ));
    }

    // 3. Active provider badge if any
    if let Some(provider) = &model.credential_provider {
        left_spans.push(Span::raw("  "));
        left_spans.push(Span::styled(
            format!(
                "{}[auth: {}]",
                presentation.icon(Icon::Info),
                sanitize_for_display(provider)
            ),
            theme.style(Token::Info),
        ));
    }

    // 4. Safety posture capsules
    // Environment capsule
    let env_token = if environment.is_production() {
        Token::EnvironmentProduction
    } else if matches!(environment, Environment::Staging) {
        Token::Warning
    } else if matches!(environment, Environment::Local | Environment::Unclassified) {
        Token::Muted
    } else {
        Token::EnvironmentNonProduction
    };

    let env_style = if environment.is_production() || matches!(environment, Environment::Staging) {
        capsule_style(presentation, env_token)
    } else {
        theme.style(env_token)
    };

    left_spans.push(Span::raw("  "));
    left_spans.push(Span::styled(
        format!("[{}]", environment.label()),
        env_style,
    ));

    // Read-only capsule
    if let Some(info) = model.connection.info()
        && info.read_only
    {
        left_spans.push(Span::raw("  "));
        left_spans.push(Span::styled(
            "[READ ONLY]",
            capsule_style(presentation, Token::Info),
        ));
    }

    // History and metadata link notices
    if !model.records_history() {
        left_spans.push(Span::raw("  "));
        left_spans.push(Span::styled(
            format!("{}[history off]", presentation.icon(Icon::Info)),
            theme.style(Token::Info),
        ));
    } else if matches!(
        model.metadata_link,
        crate::app::model::MetadataLink::Unavailable(_)
    ) {
        left_spans.push(Span::raw("  "));
        left_spans.push(Span::styled(
            format!(
                "{}[objects: {}]",
                presentation.icon(Icon::Info),
                model.metadata_link.label()
            ),
            theme.style(Token::Info),
        ));
    } else if model.history_note.is_some() {
        left_spans.push(Span::raw("  "));
        left_spans.push(Span::styled(
            format!("{}[not in history]", presentation.icon(Icon::Info)),
            theme.style(Token::Info),
        ));
    }

    // Right group: active session context
    let mut right_spans: Vec<Span<'static>> = Vec::new();

    // Non-idle transaction capsule
    match model.transaction {
        TransactionState::Open => {
            right_spans.push(Span::styled(
                "[TX ACTIVE]",
                capsule_style(presentation, Token::TransactionActive),
            ));
        }
        TransactionState::Failed => {
            right_spans.push(Span::styled(
                "[TX FAILED]",
                capsule_style(presentation, Token::TransactionFailed),
            ));
        }
        _ => {}
    }

    // Running / completed elapsed indicator
    if let Some(d) = model.running_for {
        let text = if !presentation.reduced_motion {
            format!(
                "{} {:.1}s",
                presentation.glyphs.spinner(model.frame),
                d.as_secs_f64()
            )
        } else {
            format!("{:.1}s", d.as_secs_f64())
        };
        if !right_spans.is_empty() {
            right_spans.push(Span::raw("  "));
        }
        right_spans.push(Span::styled(text, theme.style(Token::Info)));
    } else if let Some(d) = model.last_elapsed
        && d >= Duration::from_millis(200)
    {
        let text = if d >= Duration::from_secs(1) {
            format!("{:.1}s", d.as_secs_f64())
        } else {
            format!("{}ms", d.as_millis())
        };
        if !right_spans.is_empty() {
            right_spans.push(Span::raw("  "));
        }
        right_spans.push(Span::styled(text, theme.style(Token::Muted)));
    }

    // Motion badge
    if presentation.reduced_motion {
        if !right_spans.is_empty() {
            right_spans.push(Span::raw("  "));
        }
        right_spans.push(Span::styled("[MUTED]", theme.style(Token::Muted)));
    }

    let left_width: usize = left_spans.iter().map(|s| display_width(&s.content)).sum();
    let right_width: usize = right_spans.iter().map(|s| display_width(&s.content)).sum();
    let total_width = usize::from(area.width);

    if !right_spans.is_empty() && left_width + 2 + right_width <= total_width {
        let left_area = Rect::new(area.x, area.y, area.width - (right_width as u16), 1);
        let right_x = area.x + area.width - (right_width as u16);
        let right_area = Rect::new(right_x, area.y, right_width as u16, 1);
        Paragraph::new(Line::from(left_spans)).render(left_area, buf);
        Paragraph::new(Line::from(right_spans)).render(right_area, buf);
    } else if !right_spans.is_empty() && left_width + 2 < total_width {
        let mut combined = left_spans;
        combined.push(Span::raw("  "));
        combined.extend(right_spans);
        Paragraph::new(Line::from(combined)).render(area, buf);
    } else {
        Paragraph::new(Line::from(left_spans)).render(area, buf);
    }
}

/// The connection facts a header adds beside the connection label.
///
/// A failed attempt names the stage it failed in, so the diagnostic's "what"
/// is paired with the sequence's "where". The elapsed clock appears only while
/// a connection is in flight and only once the runtime has reported a tick.
/// A stage the runtime never reported is simply absent, so the header can
/// never read as a checklist.
fn connection_state_spans(model: &Model, presentation: &Presentation) -> Vec<Span<'static>> {
    let theme = &presentation.theme;
    let mut spans = Vec::new();

    if let Some(step) = failed_step(model) {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!("failed during {}", step.label()),
            theme.style(Token::Danger),
        ));
    }

    if let Some(elapsed) = connecting_elapsed(model) {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!(
                "{}{:.1}s",
                presentation.icon(Icon::Clock),
                elapsed.as_secs_f64()
            ),
            theme.style(Token::Muted),
        ));
    }

    spans
}

/// The stage a failed connection stopped in, when the runtime reported one.
fn failed_step(model: &Model) -> Option<ConnectingStep> {
    match &model.connection {
        ConnectionState::Failed(_) => model.connecting_step,
        _ => None,
    }
}

/// The elapsed connecting time, once the runtime has reported one.
fn connecting_elapsed(model: &Model) -> Option<std::time::Duration> {
    match &model.connection {
        ConnectionState::Connecting { .. } => model.running_for,
        _ => None,
    }
}

/// The icon and token for a transport state.
///
/// Encryption with a verified identity, encryption without one, and no
/// encryption at all are three different facts and are drawn three ways.
#[allow(dead_code)]
const fn tls_appearance(state: &crate::postgres::TlsState) -> (Icon, Token) {
    use crate::connection::SslMode;
    use crate::postgres::TlsState;
    match state {
        TlsState::Active { mode, .. } => {
            if matches!(mode, SslMode::VerifyFull | SslMode::VerifyCa) {
                (Icon::LockVerified, Token::Success)
            } else {
                (Icon::LockPlain, Token::Warning)
            }
        }
        TlsState::Disabled | TlsState::NotNegotiated | TlsState::Unknown => {
            (Icon::LockOpen, Token::Warning)
        }
    }
}

pub(crate) fn render_compact_header(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let environment = model.environment();
    // A session has a database name to pair with its target. Without a session
    // there is no database yet, so the connection label stands alone instead of
    // claiming "not connected" beside a connecting or failed state.
    let connection = model.connection.info().map_or_else(
        || {
            format!(
                "{}{}",
                presentation.icon(Icon::Database),
                model.connection.label()
            )
        },
        |info| {
            format!(
                "{}{} ({})",
                presentation.icon(Icon::Database),
                info.database,
                model.connection.label()
            )
        },
    );
    let label = format!(" [{}] ", environment.label());
    let mut spans = vec![
        if environment.is_production() {
            Span::styled(
                label,
                capsule_style(presentation, Token::EnvironmentProduction),
            )
        } else {
            Span::styled(label, theme.style(Token::EnvironmentNonProduction))
        },
        Span::styled(connection, theme.style(Token::Text)),
    ];
    spans.extend(connection_state_spans(model, presentation));
    spans.push(Span::styled(
        format!(
            " {}[{}]",
            presentation.icon(Icon::Focus),
            model.focus.label()
        ),
        theme.style(Token::Focus),
    ));
    Paragraph::new(Line::from(spans)).render(area, buf);
}

#[cfg(test)]
mod tests;
