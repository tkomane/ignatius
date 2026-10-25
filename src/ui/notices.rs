//! Pure fixed client-accounting reader and global badge geometry.

use crate::app::message::{Action, Direction};
use crate::app::model::{Model, NoticeDetails, NoticeSource, Presentation};
use crate::query::result::{
    MAX_NOTICE_COUNT, MAX_NOTICE_MESSAGE_BYTES, MAX_NOTICE_TEXT_BYTES, NoticeCount,
};
use crate::query::value::{display_width, sanitize_for_display, wrap_to_width};
use crate::ui::layout::{OverlayAnchor, overlay_box};
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph, Widget};

pub(crate) struct View {
    pub outer: Rect,
    pub body: Rect,
    pub rail_area: Rect,
    pub rows: Vec<String>,
    pub rail: Vec<String>,
    pub offset: usize,
    pub max_scroll: usize,
    pub usable: bool,
}
impl View {
    pub(crate) fn navigate(&self, action: &Action) -> Option<usize> {
        let page = usize::from(self.body.height).max(1);
        Some(match action {
            Action::Move(Direction::Up) => self.offset.saturating_sub(1),
            Action::Move(Direction::Down) => self.offset.saturating_add(1).min(self.max_scroll),
            Action::MovePage(Direction::Up) => self.offset.saturating_sub(page),
            Action::MovePage(Direction::Down) => {
                self.offset.saturating_add(page).min(self.max_scroll)
            }
            Action::MoveLineStart | Action::MoveBufferStart => 0,
            Action::MoveLineEnd | Action::MoveBufferEnd => self.max_scroll,
            _ => return None,
        })
    }
}
fn key(model: &Model, action: Action) -> String {
    let fallback;
    let snapshot = if model.keymap_snapshot.is_empty() {
        fallback = crate::ui::Keymap::new().snapshot();
        &fallback
    } else {
        &model.keymap_snapshot
    };
    snapshot
        .bindings
        .iter()
        .find(|binding| binding.action == action)
        .map_or_else(
            || "unbound".into(),
            |binding| sanitize_for_display(&binding.key),
        )
}
fn page_key(label: String) -> String {
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
fn content(details: &NoticeDetails) -> Vec<String> {
    let mut rows = vec![
        "Client accounting, not a database error.".into(),
        "Observed messages only; SQL outcome is unchanged".into(),
    ];
    let counts = details.snapshot.pending;
    rows.extend([
        format!("Pending omitted: {}", counts.omitted.words()),
        format!("Pending shortened: {}", counts.shortened.words()),
        format!("Pending abandoned: {}", counts.abandoned.words()),
    ]);
    if details.snapshot.previous.needs_report() {
        rows.push("Includes pending counts from previous connections.".into());
    }
    for (index, source) in details.snapshot.sources.iter().enumerate() {
        if let Some(latest) = source.as_ref().and_then(|source| source.latest.as_ref()) {
            let name = if index == 0 {
                NoticeSource::User
            } else {
                NoticeSource::Metadata
            };
            rows.push(format!(
                "Latest reported {}: {} observation window.",
                name.label(),
                latest.scope.label()
            ));
            rows.push(format!(
                "Admitted entries: {}",
                latest
                    .retained
                    .map_or_else(|| "unknown".into(), |n| n.to_string())
            ));
            rows.push(format!("Latest omitted: {}", latest.counts.omitted.words()));
            rows.push(format!(
                "Latest shortened: {}",
                latest.counts.shortened.words()
            ));
            rows.push(format!(
                "Latest abandoned: {}",
                latest.counts.abandoned.words()
            ));
        }
    }
    rows.extend([
        format!("Limits per window: {MAX_NOTICE_COUNT} entries; {MAX_NOTICE_TEXT_BYTES} retained UTF-8 field bytes; {MAX_NOTICE_MESSAGE_BYTES} bytes per message."),
        "Omitted: rejected after the client retention allowance was exhausted.".into(),
        "Shortened: an admitted message was shortened by the per-message rule.".into(),
        "Abandoned: retained prefix notices were not returned when an operation future was dropped.".into(),
        "Counts describe observed messages, not missing rows, complete warning delivery or a process-memory ceiling.".into(),
        "Acknowledgement applies only to this captured snapshot. New loss remains pending; it is not proof of reading.".into(),
    ]);
    rows
}

pub(crate) fn build(model: &Model, presentation: &Presentation, area: Rect) -> Option<View> {
    let details = model.notice_details.as_ref()?;
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
    let close = key(model, Action::Dismiss);
    let accept = key(model, Action::Activate);
    let navigation = format!(
        "{}/{} scroll",
        page_key(key(model, Action::MovePage(Direction::Up))),
        page_key(key(model, Action::MovePage(Direction::Down)))
    );
    let commands = format!("{accept} acknowledge and close; {close} close");
    let mut rail: Vec<String> = [navigation, commands]
        .iter()
        .flat_map(|line| wrap_to_width(line, width))
        .collect();
    let position_rows = 1usize;
    let usable = area.width >= super::USABLE_COLUMNS
        && area.height >= super::USABLE_ROWS
        && inner.width >= 8
        && rail.len().saturating_add(position_rows) < usize::from(inner.height);
    let rows = content(details)
        .iter()
        .flat_map(|line| wrap_to_width(line, width))
        .collect::<Vec<_>>();
    if !usable {
        rail = wrap_to_width(&format!("{close} close"), width);
    }
    let rail_height = rail
        .len()
        .saturating_add(if usable { position_rows } else { 0 })
        .min(usize::from(inner.height));
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner
            .height
            .saturating_sub(u16::try_from(rail_height).unwrap_or(u16::MAX)),
    );
    let max_scroll = rows.len().saturating_sub(usize::from(body.height).max(1));
    let offset = details.scroll.min(max_scroll);
    if usable {
        rail.push(format!(
            "Rows {}-{}/{}",
            offset + 1,
            offset
                .saturating_add(usize::from(body.height))
                .min(rows.len()),
            rows.len()
        ));
    }
    let rail_area = Rect::new(
        inner.x,
        body.bottom(),
        inner.width,
        u16::try_from(rail_height).unwrap_or(u16::MAX),
    );
    Some(View {
        outer,
        body,
        rail_area,
        rows,
        rail,
        offset,
        max_scroll,
        usable,
    })
}

pub(crate) fn render(model: &Model, presentation: &Presentation, area: Rect, buf: &mut Buffer) {
    let Some(view) = build(model, presentation, area) else {
        return;
    };
    Clear.render(view.outer, buf);
    crate::ui::widgets::overlay_block(" Server notice retention ".into(), presentation)
        .render(view.outer, buf);
    let lines = if view.usable {
        view.rows
            .iter()
            .skip(view.offset)
            .take(usize::from(view.body.height))
            .map(|row| Line::from(row.clone()))
            .collect::<Vec<_>>()
    } else {
        vec![Line::from("Enlarge terminal to acknowledge.")]
    };
    Paragraph::new(lines)
        .style(presentation.theme.style(Token::Text))
        .render(view.body, buf);
    Paragraph::new(
        view.rail
            .iter()
            .cloned()
            .map(Line::from)
            .collect::<Vec<_>>(),
    )
    .style(presentation.theme.style(Token::Info))
    .render(view.rail_area, buf);
}

pub(crate) fn badge(model: &Model, available: usize) -> Option<String> {
    let counts = model.notice_state.pending;
    if !counts.needs_report() || covered(model) {
        return None;
    }
    let parts = [
        (counts.omitted, "omitted", "O"),
        (counts.shortened, "shortened", "S"),
        (counts.abandoned, "abandoned", "A"),
    ];
    let full = format!(
        "Notices: {}",
        parts
            .iter()
            .filter(|(count, _, _)| !count.is_zero())
            .map(|(count, word, _)| format!("{} {word}", count.words()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if display_width(&full) <= available {
        return Some(full);
    }
    let compact = format!(
        "Notices: {}",
        parts
            .iter()
            .filter(|(count, _, _)| !matches!(count, NoticeCount::Exact(0)))
            .map(|(count, _, label)| format!("{label}{}", count.compact()))
            .collect::<Vec<_>>()
            .join(" ")
    );
    (display_width(&compact) <= available).then_some(compact)
}
fn covered(model: &Model) -> bool {
    model.help_open
        || model.notice_details.is_some()
        || model.palette.is_some()
        || model.connection_details
        || model.prefix_pending
        || model.password_prompt.is_some()
        || model.parameter_prompt.is_some()
        || model.name_prompt.is_some()
        || model.pending_run.is_some()
        || model.pending_plan.is_some()
        || model.pending_copy.is_some()
        || model.pending_update.is_some()
        || model.update_prompt.is_some()
        || model.inspector.is_some()
        || model.definition.is_some()
        || model.completion.menu.is_some()
}

#[cfg(test)]
mod tests;
