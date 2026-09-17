use crate::app::model::Model;
use crate::query::value::sanitize_for_display;
use crate::ui::glyphs::Icon;
use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub(crate) fn render_header(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let environment = model.environment();
    let mut spans = vec![Span::styled(
        format!(
            " {}{} ",
            presentation.icon(Icon::Brand),
            crate::branding::PRODUCT_NAME
        ),
        theme.style(Token::Header),
    )];

    // The classification is a bracketed word in every mode, so it is legible
    // with no colour and no icons. Production additionally gets a filled
    // capsule, because it is the one marker that must never be skimmed past.
    let label = format!(
        " {}[{}] ",
        presentation.icon(if environment.is_production() {
            Icon::Production
        } else {
            Icon::Environment
        }),
        environment.label()
    );
    spans.push(if environment.is_production() {
        Span::styled(label, theme.capsule(Token::EnvironmentProduction))
    } else {
        Span::styled(label, theme.style(Token::EnvironmentNonProduction))
    });

    if let Some(info) = model.connection.info() {
        let (icon, token) = if info.read_only {
            (Icon::ReadOnly, Token::Info)
        } else {
            (Icon::ReadWrite, Token::Muted)
        };
        spans.push(Span::styled(
            format!("{}[{}]", presentation.icon(icon), info.posture()),
            theme.style(token),
        ));
    }

    spans.push(Span::styled(
        presentation.glyphs.separator(),
        theme.style(Token::Border),
    ));
    spans.push(Span::styled(
        format!(
            "{}{}",
            presentation.icon(Icon::Database),
            model.connection.label()
        ),
        theme.style(Token::Text),
    ));

    if let Some(info) = model.connection.info() {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        let (icon, token) = tls_appearance(&info.tls);
        spans.push(Span::styled(
            format!("{}{}", presentation.icon(icon), info.tls.label()),
            theme.style(token),
        ));
    }

    // Which credential route opened this session. Said in words rather than
    // shown as a colour, because "who am I connected as, and how" is the
    // question a token makes harder to answer: there was no password to type,
    // so nothing about the act of connecting recorded the answer.
    if let Some(provider) = &model.credential_provider {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!(
                "{}[auth: {}]",
                presentation.icon(Icon::Info),
                sanitize_for_display(provider)
            ),
            theme.style(Token::Info),
        ));
    }

    // A session that keeps no record says so. This is the opposite of a mode
    // that hides: the safe state is the one being announced, and someone who
    // paused recording an hour ago should not have to remember that they did.
    if !model.records_history() {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!("{}[history off]", presentation.icon(Icon::Info)),
            theme.style(Token::Info),
        ));
    } else if matches!(
        model.metadata_link,
        crate::app::model::MetadataLink::Unavailable(_)
    ) {
        // Only said when it is not what was intended. The tree opening its own
        // connection is the normal case and needs no announcement; falling back
        // to sharing one explains why the tree can be slow behind a long query.
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!(
                "{}[objects: {}]",
                presentation.icon(Icon::Info),
                model.metadata_link.label()
            ),
            theme.style(Token::Info),
        ));
    } else if model.history_note.is_some() {
        // A statement the history refused. Saying so where the user already
        // looks is the difference between a rule and a mystery.
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!("{}[not in history]", presentation.icon(Icon::Info)),
            theme.style(Token::Info),
        ));
    }

    if let Some(elapsed) = model.last_elapsed {
        spans.push(Span::styled(
            presentation.glyphs.separator(),
            theme.style(Token::Border),
        ));
        spans.push(Span::styled(
            format!(
                "{}{} ms",
                presentation.icon(Icon::Clock),
                elapsed.as_millis()
            ),
            theme.style(Token::Muted),
        ));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

/// The icon and token for a transport state.
///
/// Encryption with a verified identity, encryption without one, and no
/// encryption at all are three different facts and are drawn three ways.
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
    let database = model
        .connection
        .info()
        .map_or_else(|| "not connected".to_owned(), |i| i.database.clone());
    let label = format!(" [{}] ", environment.label());
    let spans = vec![
        if environment.is_production() {
            Span::styled(label, theme.capsule(Token::EnvironmentProduction))
        } else {
            Span::styled(label, theme.style(Token::EnvironmentNonProduction))
        },
        Span::styled(
            format!(
                "{}{database} ({})",
                presentation.icon(Icon::Database),
                model.connection.label()
            ),
            theme.style(Token::Text),
        ),
        Span::styled(
            format!(
                " {}[{}]",
                presentation.icon(Icon::Focus),
                model.focus.label()
            ),
            theme.style(Token::Focus),
        ),
    ];
    Paragraph::new(Line::from(spans)).render(area, buf);
}
