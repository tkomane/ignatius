use super::{input_cursor, overlay_block};
use crate::app::model::Model;
use crate::query::value::sanitize_for_display;
use crate::ui::glyphs::Icon;
use crate::ui::layout::Presentation;
use crate::ui::layout::{OverlayAnchor, overlay_box};
use crate::ui::theme::{Theme, Token};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

pub(crate) fn palette_box(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
) -> Rect {
    let notes = palette
        .context_note
        .as_deref()
        .map_or(0, |note| note.split(". ").count());
    let minimum = u16::try_from(notes).unwrap_or(u16::MAX).saturating_add(4);
    let target_height = ((u32::from(area.height) * 4 / 5) as u16).max(minimum);
    let max_width = if palette.supports_preview() && area.width >= crate::ui::MIN_COLUMNS {
        area.width.saturating_sub(4).clamp(76, 96).min(area.width)
    } else {
        76
    };
    overlay_box(
        area,
        presentation,
        (max_width, target_height),
        minimum,
        OverlayAnchor::Top,
    )
}

/// The number of visible match rows in the palette overlay.
pub(crate) fn visible_rows(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
) -> usize {
    let box_area = palette_box(palette, presentation, area);
    let note_lines = palette
        .context_note
        .as_deref()
        .map_or(0, |note| note.split(". ").count());
    (box_area.height as usize)
        .saturating_sub(3 + note_lines)
        .max(1)
}

/// Resolves a screen cell within the palette match list to its row index.
pub(crate) fn hit_row(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let palette_area = palette_box(palette, presentation, area);
    let inner = Rect {
        x: palette_area.x.saturating_add(1),
        y: palette_area.y.saturating_add(1),
        width: palette_area.width.saturating_sub(2),
        height: palette_area.height.saturating_sub(2),
    };
    if column < inner.x
        || column >= inner.x.saturating_add(inner.width)
        || row < inner.y
        || row >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    if palette.supports_preview() && area.width >= crate::ui::MIN_COLUMNS && inner.width >= 56 {
        let preview_width = inner.width * 60 / 100;
        let list_width = inner.width.saturating_sub(preview_width);
        if column >= inner.x.saturating_add(list_width) {
            return None;
        }
    }

    let matches = palette.matches();
    let note_lines = palette
        .context_note
        .as_deref()
        .map_or(0, |note| note.split(". ").count());
    let visible = visible_rows(palette, presentation, area);
    let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
    let first_item = inner
        .y
        .saturating_add(1)
        .saturating_add(u16::try_from(note_lines).unwrap_or(u16::MAX));
    if row < first_item {
        return None;
    }
    let position = usize::from(row.saturating_sub(first_item));
    if position >= visible {
        return None;
    }
    let index = offset.saturating_add(position);
    if index >= matches.len() {
        return None;
    }
    Some(index)
}

pub(crate) fn render_palette(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let palette_area = palette_box(palette, presentation, area);
    ratatui::widgets::Clear.render(palette_area, buf);

    let inner = Rect {
        x: palette_area.x.saturating_add(1),
        y: palette_area.y.saturating_add(1),
        width: palette_area.width.saturating_sub(2),
        height: palette_area.height.saturating_sub(2),
    };

    let show_preview =
        palette.supports_preview() && area.width >= crate::ui::MIN_COLUMNS && inner.width >= 56;

    let matches = palette.matches();
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

    if !show_preview {
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

        lines.extend(note_lines.iter().map(|note| {
            Line::from(Span::styled(
                format!(" {}", sanitize_for_display(note)),
                theme.style(Token::Muted),
            ))
        }));

        let visible = visible_rows(palette, presentation, area);
        let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
        for (index, entry) in matches.iter().enumerate().skip(offset).take(visible) {
            let selected = index == palette.selected;
            let selection = if selected {
                theme.style(Token::Selection)
            } else {
                Style::default()
            };
            let mut spans = vec![
                Span::styled(
                    format!(" {:<8}", entry.group),
                    theme.style(Token::Muted).patch(selection),
                ),
                Span::styled(
                    sanitize_for_display(&entry.label),
                    if selected {
                        theme.style(Token::Selection)
                    } else {
                        theme.style(Token::Text)
                    }
                    .patch(selection),
                ),
            ];
            if !entry.detail.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", sanitize_for_display(&entry.detail)),
                    theme.style(Token::Muted).patch(selection),
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
        return;
    }

    // Preview-bearing layout
    let block = overlay_block(palette.purpose.title().to_owned(), presentation);
    block.render(palette_area, buf);

    let input_line = Line::from(vec![
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
    ]);
    Paragraph::new(input_line).render(Rect::new(inner.x, inner.y, inner.width, 1), buf);

    let mut top_offset = 1u16;
    for note in &note_lines {
        let note_line = Line::from(Span::styled(
            format!(" {}", sanitize_for_display(note)),
            theme.style(Token::Muted),
        ));
        Paragraph::new(note_line).render(
            Rect::new(inner.x, inner.y + top_offset, inner.width, 1),
            buf,
        );
        top_offset += 1;
    }

    let standing_note = palette.purpose.standing_note();
    let standing_lines: Vec<String> = standing_note
        .map(|standing| {
            standing
                .split(". ")
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
    let bottom_rows = u16::try_from(standing_lines.len()).unwrap_or(0);
    for (idx, line) in standing_lines.iter().enumerate() {
        let y = inner
            .y
            .saturating_add(inner.height)
            .saturating_sub(bottom_rows)
            .saturating_add(u16::try_from(idx).unwrap_or(0));
        let standing_line = Line::from(Span::styled(line.as_str(), theme.style(Token::Muted)));
        Paragraph::new(standing_line).render(Rect::new(inner.x, y, inner.width, 1), buf);
    }

    let content_y = inner.y + top_offset;
    let content_height = inner.height.saturating_sub(top_offset + bottom_rows);

    let preview_width = inner.width * 60 / 100;
    let list_width = inner.width.saturating_sub(preview_width);
    let divider_x = inner.x + list_width;
    let preview_x = divider_x + 1;
    let preview_w = inner.width.saturating_sub(list_width + 1);

    let divider_glyph = presentation.glyphs.column_rule();
    let divider_style = theme
        .surface(Token::SurfaceOverlay)
        .patch(theme.style(Token::Border));
    for y in content_y..content_y + content_height {
        buf[(divider_x, y)]
            .set_symbol(divider_glyph)
            .set_style(divider_style);
    }

    let visible = visible_rows(palette, presentation, area);
    let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
    let mut match_lines = Vec::new();
    for (index, entry) in matches.iter().enumerate().skip(offset).take(visible) {
        let selected = index == palette.selected;
        let selection = if selected {
            theme.style(Token::Selection)
        } else {
            Style::default()
        };
        let mut spans = vec![
            Span::styled(
                format!(" {:<8}", entry.group),
                theme.style(Token::Muted).patch(selection),
            ),
            Span::styled(
                sanitize_for_display(&entry.label),
                if selected {
                    theme.style(Token::Selection)
                } else {
                    theme.style(Token::Text)
                }
                .patch(selection),
            ),
        ];
        if !entry.detail.is_empty() && !show_preview {
            spans.push(Span::styled(
                format!("  {}", sanitize_for_display(&entry.detail)),
                theme.style(Token::Muted).patch(selection),
            ));
        }
        let mut line = Line::from(spans);
        if selected {
            line = line.style(theme.style(Token::Selection));
        }
        match_lines.push(line);
    }

    if palette.loading {
        match_lines.push(Line::from(Span::styled(
            " Reading the catalogue...",
            theme.style(Token::Muted),
        )));
    } else if matches.is_empty() {
        match_lines.push(Line::from(Span::styled(
            palette.purpose.empty_message(),
            theme.style(Token::Muted),
        )));
    }

    let list_area = Rect::new(inner.x, content_y, list_width, content_height);
    Paragraph::new(match_lines)
        .style(theme.surface(Token::SurfaceOverlay))
        .render(list_area, buf);

    if let Some(preview_raw) = palette.selected_preview() {
        let preview_lines: Vec<Line<'static>> = preview_raw
            .into_iter()
            .map(|raw_line| {
                if let Some((key, val)) = raw_line.split_once(": ") {
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(format!("{key}: "), theme.style(Token::Muted)),
                        Span::styled(val.to_owned(), theme.style(Token::Text)),
                    ])
                } else if raw_line.ends_with(':') || raw_line == "No matching entry selected." {
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(raw_line, theme.style(Token::Muted)),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(raw_line, theme.style(Token::Text)),
                    ])
                }
            })
            .collect();
        let preview_area = Rect::new(preview_x, content_y, preview_w, content_height);
        Paragraph::new(preview_lines)
            .style(theme.surface(Token::SurfaceOverlay))
            .wrap(Wrap { trim: false })
            .render(preview_area, buf);
    }
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

    let box_area = overlay_box(area, presentation, (96, 30), 8, OverlayAnchor::Center);
    let width = box_area.width;
    let height = box_area.height;
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

    ratatui::widgets::Clear.render(box_area, buf);
    // The overlay fill comes from the depth-aware surface route, so 256-colour
    // terminals get an indexed colour and 16-colour or colour-off terminals
    // paint nothing rather than a raw `Color::Rgb`.
    let panel_style = theme.surface(Token::SurfaceOverlay);

    let connection_state = match &model.connection {
        crate::app::model::ConnectionState::Disconnected => "Not connected".to_owned(),
        crate::app::model::ConnectionState::Connecting { .. } => "Connecting".to_owned(),
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

#[cfg(test)]
mod tests;
