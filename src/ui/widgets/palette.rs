use super::{input_cursor, overlay_block};
use crate::app::model::Model;
use crate::query::value::sanitize_for_display;
use crate::ui::glyphs::Icon;
use crate::ui::layout::Presentation;
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

pub(crate) fn render_palette(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let width = area.width.saturating_sub(8).min(76);
    let height = area.height.saturating_sub(6).min(18);
    let palette_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(palette_area, buf);

    let matches = palette.matches();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" {}", presentation.icon(Icon::Focus)),
            theme.style(Token::Focus),
        ),
        Span::styled(
            sanitize_for_display(&palette.query),
            theme.style(Token::Text),
        ),
        Span::styled(input_cursor(presentation), theme.style(Token::Focus)),
        Span::styled(
            format!("   {} match(es)", matches.len()),
            theme.style(Token::Muted),
        ),
    ])];

    let note_lines: Vec<String> = palette
        .context_note
        .as_deref()
        .map(|note| {
            note.split(". ")
                .map(|part| {
                    if part.ends_with('.') {
                        part.to_owned()
                    } else {
                        format!("{part}.")
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    lines.extend(note_lines.iter().map(|note| {
        Line::from(Span::styled(
            format!(" {}", sanitize_for_display(note)),
            theme.style(Token::Muted),
        ))
    }));

    let visible = (height as usize)
        .saturating_sub(3 + note_lines.len())
        .max(1);
    let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
    for (index, entry) in matches.iter().enumerate().skip(offset).take(visible) {
        let selected = index == palette.selected;
        let mut spans = vec![
            Span::styled(format!(" {:<8}", entry.group), theme.style(Token::Muted)),
            Span::styled(
                sanitize_for_display(&entry.label),
                if selected {
                    theme.style(Token::Selection)
                } else {
                    theme.style(Token::Text)
                },
            ),
        ];
        if !entry.detail.is_empty() {
            spans.push(Span::styled(
                format!("  {}", sanitize_for_display(&entry.detail)),
                theme.style(Token::Muted),
            ));
        }
        let mut line = Line::from(spans);
        if selected {
            line = line.style(theme.style(Token::Selection));
        }
        lines.push(line);
    }

    if palette.loading {
        lines.push(Line::from(Span::styled(
            " Reading the catalogue...",
            theme.style(Token::Muted),
        )));
    } else if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            palette.purpose.empty_message(),
            theme.style(Token::Muted),
        )));
    }
    // The history's rule is stated where someone would go looking for a
    // statement that is not there, whether or not anything matched.
    if let Some(standing) = palette.purpose.standing_note() {
        lines.push(Line::from(Span::styled(
            standing,
            theme.style(Token::Muted),
        )));
    }

    Paragraph::new(lines)
        .block(overlay_block(
            palette.purpose.title().to_owned(),
            presentation,
        ))
        .render(palette_area, buf);
}

// ------------------------------------------------------ connection details

/// Shows the connection's trust boundary without performing any work.
///
/// This is intentionally a read-only surface rather than a settings dialog.
/// It explains the facts that are easy to compress too far in the header - in
/// particular which cloud tool supplied a credential and what TLS actually
/// guarantees - while making it impossible to refresh or mutate anything by
/// opening it.
pub(crate) fn render_connection_details(
    model: &Model,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    // A terminal has no alpha compositing. Clearing and filling the backdrop
    // gives the modal the same focus separation as a scrim without leaving
    // stale text visible around its edges, including in no-colour mode.
    ratatui::widgets::Clear.render(area, buf);
    Block::new().style(theme.stripe()).render(area, buf);

    let width = area.width.saturating_sub(6).min(96);
    let height = area.height.saturating_sub(4).min(30);
    if width < 28 || height < 8 {
        Paragraph::new(Line::from(Span::styled(
            "Connection details need a larger terminal. Esc closes.",
            theme.style(Token::Text),
        )))
        .block(overlay_block(
            " Connection details ".to_owned(),
            presentation,
        ))
        .render(area, buf);
        return;
    }

    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    ratatui::widgets::Clear.render(box_area, buf);
    // The overlay fill comes from the depth-aware surface route, so 256-colour
    // terminals get an indexed colour and 16-colour or colour-off terminals
    // paint nothing rather than a raw `Color::Rgb`.
    let panel_style = theme.surface(Token::SurfaceOverlay);

    let connection_state = match &model.connection {
        crate::app::model::ConnectionState::Disconnected => "Not connected".to_owned(),
        crate::app::model::ConnectionState::Connecting => "Connecting".to_owned(),
        crate::app::model::ConnectionState::Connected(_) => "Connected".to_owned(),
        crate::app::model::ConnectionState::Lost { .. } => "Connection lost".to_owned(),
        crate::app::model::ConnectionState::Failed(_) => "Connection failed".to_owned(),
    };
    let target = model
        .connection
        .info()
        .map_or_else(|| model.connection.label(), |info| info.target.clone());
    let database = model.connection.info().map_or_else(
        || "unknown until connected".to_owned(),
        |info| info.database.clone(),
    );
    let user = model.connection.info().map_or_else(
        || "unknown until connected".to_owned(),
        |info| info.user.clone(),
    );
    let environment = model.environment().label();
    let posture = model.connection.info().map_or_else(
        || "unknown until connected".to_owned(),
        |info| info.posture().to_owned(),
    );

    let mut lines = vec![Line::from(Span::styled(
        "A calm, explicit view of what this session can reach.",
        theme.style(Token::Muted),
    ))];
    push_details_section(&mut lines, "Connection", theme);
    push_detail(
        &mut lines,
        "State",
        connection_state,
        theme.style(Token::Text),
    );
    push_detail(&mut lines, "Target", target, theme.style(Token::Text));
    push_detail(&mut lines, "Database", database, theme.style(Token::Text));
    push_detail(&mut lines, "Role", user, theme.style(Token::Text));
    push_detail(
        &mut lines,
        "Environment",
        environment,
        if model.environment().is_production() {
            theme.style(Token::EnvironmentProduction)
        } else {
            theme.style(Token::EnvironmentNonProduction)
        },
    );
    push_detail(&mut lines, "Posture", posture, theme.style(Token::Info));

    push_details_section(&mut lines, "Transport", theme);
    if let Some(info) = model.connection.info() {
        push_detail(
            &mut lines,
            "TLS",
            info.tls.label(),
            theme.style(if info.tls.is_encrypted() {
                Token::Success
            } else {
                Token::Warning
            }),
        );
        push_detail(
            &mut lines,
            "Guarantee",
            info.tls.description(),
            theme.style(Token::Text),
        );
    } else {
        push_detail(
            &mut lines,
            "TLS",
            "unknown until the server responds".to_owned(),
            theme.style(Token::Muted),
        );
    }
    if let Some(provider) = &model.credential_presentation {
        push_detail(
            &mut lines,
            "Provider rule",
            provider.transport.clone(),
            theme.style(Token::Info),
        );
    }

    push_details_section(&mut lines, "Authentication", theme);
    if let Some(provider) = &model.credential_presentation {
        push_detail(
            &mut lines,
            "Provider",
            provider.display_name.clone(),
            theme.style(Token::Info),
        );
        push_detail(
            &mut lines,
            "Configured name",
            provider.name.clone(),
            theme.style(Token::Muted),
        );
        push_detail(
            &mut lines,
            "Credential source",
            format!("{} via {}", provider.invocation, provider.command_name),
            theme.style(Token::Text),
        );
        push_detail(
            &mut lines,
            "Output",
            provider.extraction.clone(),
            theme.style(Token::Muted),
        );
        push_detail(
            &mut lines,
            "Lifetime",
            provider.lifetime.clone(),
            theme.style(Token::Text),
        );
        push_detail(
            &mut lines,
            "Safety",
            "token is not shown, stored, logged, or written".to_owned(),
            theme.style(Token::Success),
        );
        push_detail(
            &mut lines,
            "If it fails",
            provider.remedy.clone(),
            theme.style(Token::Info),
        );
    } else if let Some(provider) = &model.credential_provider {
        // This can only occur for a provider whose metadata could not be
        // resolved. Keep the name visible, but do not invent a cloud identity or
        // echo an arbitrary configured command.
        push_detail(
            &mut lines,
            "Provider",
            format!("configured provider {provider:?}"),
            theme.style(Token::Warning),
        );
        push_detail(
            &mut lines,
            "Safety",
            "cloud token route selected; provider details unavailable".to_owned(),
            theme.style(Token::Warning),
        );
    } else {
        push_detail(
            &mut lines,
            "Cloud identity",
            "No cloud token was requested for this session".to_owned(),
            theme.style(Token::Muted),
        );
        push_detail(
            &mut lines,
            "Password routes",
            "connection string, environment, .pgpass, or an interactive prompt".to_owned(),
            theme.style(Token::Text),
        );
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Opening this view performs no refresh, network request, or mutation.  Esc closes.",
        theme.style(Token::Muted),
    )));

    Paragraph::new(lines)
        .block(overlay_block(
            format!(
                " {}Connection details  Esc to close ",
                presentation.icon(Icon::Info)
            ),
            presentation,
        ))
        .style(panel_style)
        .wrap(Wrap { trim: true })
        .render(box_area, buf);
}

fn push_details_section(lines: &mut Vec<Line<'static>>, title: &str, theme: &Theme) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        title.to_owned(),
        theme.style(Token::Header),
    )));
}

fn push_detail(lines: &mut Vec<Line<'static>>, label: &str, value: String, style: Style) {
    lines.push(Line::from(vec![
        Span::styled(format!("  {label:<18}"), style.add_modifier(Modifier::BOLD)),
        Span::styled(sanitize_for_display(&value), style),
    ]));
}
