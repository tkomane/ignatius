//! Derived help content and geometry shared by drawing and local navigation.

use crate::app::discovery::KeymapSnapshot;
use crate::app::message::{Action, Direction};
use crate::app::model::{Model, PasteNotice, Presentation};
use crate::query::value::{display_width, sanitize_for_display, wrap_to_width};
use crate::ui::layout::{OverlayAnchor, overlay_box};
use crate::ui::theme::Token;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

pub(crate) struct View {
    pub(crate) outer: Rect,
    pub(crate) body: Rect,
    pub(crate) rail_area: Rect,
    pub(crate) rows: Vec<Line<'static>>,
    pub(crate) rail: Vec<String>,
    pub(crate) offset: usize,
    pub(crate) max_scroll: usize,
}

impl View {
    pub(crate) fn window(&self) -> &[Line<'static>] {
        let end = self
            .offset
            .saturating_add(usize::from(self.body.height))
            .min(self.rows.len());
        &self.rows[self.offset..end]
    }

    pub(crate) fn navigate(&self, action: &Action) -> Option<usize> {
        let page = usize::from(self.body.height).max(1);
        let next = match action {
            Action::Move(Direction::Up) => self.offset.saturating_sub(1),
            Action::Move(Direction::Down) => self.offset.saturating_add(1),
            Action::MovePage(Direction::Up) => self.offset.saturating_sub(page),
            Action::MovePage(Direction::Down) => self.offset.saturating_add(page),
            Action::MoveLineStart | Action::MoveBufferStart => 0,
            Action::MoveLineEnd | Action::MoveBufferEnd => self.max_scroll,
            _ => return None,
        };
        Some(next.min(self.max_scroll))
    }
}

fn key(snapshot: &KeymapSnapshot, action: &Action) -> String {
    snapshot
        .bindings
        .iter()
        .find(|binding| binding.action == *action)
        .map_or_else(
            || "unbound".into(),
            |binding| sanitize_for_display(&binding.key),
        )
}

fn compact(label: String) -> String {
    label
        .split('+')
        .map(|part| match part {
            "PageUp" => "PgUp",
            "PageDown" => "PgDn",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The model snapshot is the runtime's active key authority, as for reviews.
pub(crate) fn build(model: &Model, presentation: &Presentation, area: Rect) -> View {
    let fallback;
    let snapshot = if model.keymap_snapshot.is_empty() {
        fallback = crate::ui::Keymap::new().snapshot();
        &fallback
    } else {
        &model.keymap_snapshot
    };
    let outer = overlay_box(
        area,
        presentation,
        (90, area.height),
        super::USABLE_ROWS,
        OverlayAnchor::Center,
    );
    let inner = Rect::new(
        outer.x.saturating_add(1),
        outer.y.saturating_add(1),
        outer.width.saturating_sub(2),
        outer.height.saturating_sub(2),
    );
    let width = usize::from(inner.width).max(1);
    let theme = &presentation.theme;
    let usable = area.width >= super::USABLE_COLUMNS && area.height >= super::USABLE_ROWS;
    let mut rows = Vec::new();
    for text in wrap_to_width(
        "Every action is reachable from the keyboard. Hold Shift to select text with the terminal, or set ui.mouse = false in config.toml.",
        width,
    ) {
        rows.push(Line::styled(text, theme.style(Token::Muted)));
    }
    let entries: Vec<(String, String)> = snapshot
        .bindings
        .iter()
        .map(|binding| {
            (
                sanitize_for_display(&binding.key),
                sanitize_for_display(&binding.description),
            )
        })
        .chain(snapshot.chords.iter().map(|chord| {
            (
                format!(
                    "{} {}",
                    snapshot
                        .prefix
                        .as_deref()
                        .map(sanitize_for_display)
                        .unwrap_or_else(|| "unbound".into()),
                    chord.key
                ),
                sanitize_for_display(&chord.description),
            )
        }))
        .collect();
    let key_width = entries
        .iter()
        .map(|(label, _)| display_width(label))
        .max()
        .unwrap_or(1)
        .max(12)
        .min(width.saturating_sub(3) / 2)
        .max(1);
    let description_width = width.saturating_sub(key_width + 2).max(1);
    for (label, description) in entries {
        let labels = wrap_to_width(&label, key_width);
        let descriptions = wrap_to_width(&description, description_width);
        for index in 0..labels.len().max(descriptions.len()) {
            let label = labels.get(index).map_or("", String::as_str);
            let description = descriptions.get(index).map_or("", String::as_str);
            rows.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{label}{}  ",
                        " ".repeat(key_width.saturating_sub(display_width(label)))
                    ),
                    theme.style(Token::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled(description.to_owned(), theme.style(Token::Text)),
            ]));
        }
    }
    if !usable {
        rows = wrap_to_width("Enlarge terminal to read help.", width)
            .into_iter()
            .map(|line| Line::styled(line, theme.style(Token::Muted)))
            .collect();
    }
    let pages = format!(
        "{}/{} scroll",
        compact(key(snapshot, &Action::MovePage(Direction::Up))),
        compact(key(snapshot, &Action::MovePage(Direction::Down)))
    );
    let navigation = if width < 50 {
        pages
    } else {
        format!(
            "{}/{} scroll {}/{} top/end",
            compact(key(snapshot, &Action::MovePage(Direction::Up))),
            compact(key(snapshot, &Action::MovePage(Direction::Down))),
            key(snapshot, &Action::MoveLineStart),
            key(snapshot, &Action::MoveLineEnd)
        )
    };
    let close_only = format!("{} to close", key(snapshot, &Action::Dismiss));
    let close = format!(
        "{} to close  {} search commands",
        key(snapshot, &Action::Dismiss),
        key(snapshot, &Action::OpenPalette)
    );
    let feedback = match model.paste_notice {
        Some(PasteNotice::Ignored) => " | Paste ignored",
        Some(PasteNotice::Refused) => " | Paste too large",
        _ => "",
    };
    let mut rail: Vec<String> = if usable {
        [navigation, close]
            .iter()
            .flat_map(|line| wrap_to_width(line, width))
            .collect()
    } else {
        wrap_to_width(&close_only, width)
    };
    let mut show_position = usable;
    let mut position_reserve = if usable {
        wrap_to_width(&format!("Rows {0}-{0}/{0}{feedback}", rows.len()), width).len()
    } else {
        0
    };
    if usable && rail.len().saturating_add(position_reserve) >= usize::from(inner.height) {
        // A complete long-key rail may need more space than this frame. Keep
        // the actual close key visible rather than clipping it below navigation.
        rows = wrap_to_width("Enlarge terminal to read the active help keys.", width)
            .into_iter()
            .map(|line| Line::styled(line, theme.style(Token::Muted)))
            .collect();
        rail = wrap_to_width(&close_only, width);
        show_position = false;
        position_reserve = 0;
    }
    let reserved = u16::try_from(rail.len().saturating_add(position_reserve))
        .unwrap_or(u16::MAX)
        .min(inner.height);
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(reserved),
    );
    let max_scroll = rows.len().saturating_sub(usize::from(body.height).max(1));
    let offset = model.help_scroll.min(max_scroll);
    if show_position {
        rail.extend(wrap_to_width(
            &format!(
                "Rows {}-{}/{}{feedback}",
                offset + 1,
                offset
                    .saturating_add(usize::from(body.height))
                    .min(rows.len()),
                rows.len()
            ),
            width,
        ));
    }
    let rail_area = Rect::new(inner.x, body.bottom(), inner.width, reserved);
    View {
        outer,
        body,
        rail_area,
        rows,
        rail,
        offset,
        max_scroll,
    }
}

#[cfg(test)]
mod tests;
