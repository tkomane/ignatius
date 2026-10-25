//! One transient review surface shared by rendering and effect admission.
//!
//! Only the scroll offset belongs in the model. In particular, the generated
//! bound UPDATE below is derived for this call and never cached or debugged.

use crate::app::discovery::KeymapSnapshot;
use crate::app::message::{Action, Direction};
use crate::app::model::Model;
use crate::query::value::{display_width, sanitize_for_display, wrap_to_width};
use crate::ui::layout::{OverlayAnchor, Presentation, overlay_box};
use crate::ui::theme::Token;
use ratatui::layout::Rect;

/// Which existing one-shot confirmation owns the review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Copy,
    Update,
    Production,
    Analyze,
    Parameter,
}

/// The protected review currently owning input, following reducer precedence.
pub(crate) fn active_kind(model: &Model) -> Option<Kind> {
    if model.password_prompt.is_some() {
        return None;
    }
    if model.parameter_prompt.is_some() {
        return Some(Kind::Parameter);
    }
    if model.name_prompt.is_some() || model.connection_details || model.prefix_pending {
        return None;
    }
    if model.pending_run.is_some() {
        return Some(Kind::Production);
    }
    if model.pending_plan.is_some() {
        return Some(Kind::Analyze);
    }
    if model.pending_copy.is_some() {
        return Some(Kind::Copy);
    }
    if model.update_prompt.is_some() {
        return None;
    }
    model.pending_update.as_ref().map(|_| Kind::Update)
}

/// One already escaped and wrapped row. Deliberately has no Debug output.
pub(crate) struct Row {
    pub(crate) text: String,
    pub(crate) token: Token,
}

/// Derived content and geometry, shared by the reducer and renderer.
///
/// This deliberately does not implement Debug: its transient body may contain
/// the intentional, value-visible UPDATE review.
pub(crate) struct View {
    pub(crate) title: &'static str,
    pub(crate) outer: Rect,
    pub(crate) body: Rect,
    pub(crate) input_area: Option<Rect>,
    pub(crate) rail_area: Rect,
    pub(crate) rows: Vec<Row>,
    pub(crate) input: Option<String>,
    pub(crate) rail: Vec<String>,
    pub(crate) refusal: Vec<String>,
    pub(crate) offset: usize,
    pub(crate) max_scroll: usize,
    pub(crate) usable: bool,
    pub(crate) ready: bool,
}

impl View {
    /// The current wrapped rows, without a narrow integer scroll conversion.
    pub(crate) fn window(&self) -> &[Row] {
        let start = self.offset.min(self.rows.len());
        let end = start
            .saturating_add(usize::from(self.body.height))
            .min(self.rows.len());
        &self.rows[start..end]
    }

    /// Existing navigation intents belong to the modal, never an underlying pane.
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

enum Input {
    Masked(usize),
    Name(String),
}

struct Document {
    title: &'static str,
    width: u16,
    former_height: u16,
    rows: Vec<Row>,
    input: Option<Input>,
    accept: &'static str,
    typed_satisfied: bool,
    valid: bool,
}

fn push(rows: &mut Vec<Row>, text: impl AsRef<str>, token: Token) {
    // Intentional source/body line breaks survive. Controls inside metadata
    // have already been escaped before interpolation into a body line.
    for line in text.as_ref().split('\n') {
        rows.push(Row {
            text: sanitize_for_display(line),
            token,
        });
    }
}

fn preview(rows: &mut Vec<Row>, sql: &str, redact: bool) {
    for line in sql.lines().take(4) {
        let line = if redact {
            crate::diagnostics::redact_text(line)
        } else {
            line.to_owned()
        };
        push(
            rows,
            format!("  {}", sanitize_for_display(&line)),
            Token::Muted,
        );
    }
    if sql.lines().nth(4).is_some() {
        push(
            rows,
            "Preview truncated after four source lines.",
            Token::Warning,
        );
    }
}

fn document(model: &Model, kind: Kind) -> Option<Document> {
    let mut rows = Vec::new();
    let target = model.connection.info().map_or_else(
        || {
            if kind == Kind::Copy {
                "this terminal"
            } else {
                "this database"
            }
            .to_owned()
        },
        |info| {
            let target = if matches!(kind, Kind::Copy | Kind::Analyze) {
                crate::diagnostics::redact_text(&info.target)
            } else {
                info.target.clone()
            };
            sanitize_for_display(&target)
        },
    );
    let mut doc = Document {
        title: "Confirm",
        width: 84,
        former_height: 11,
        rows: Vec::new(),
        input: None,
        accept: "accept",
        typed_satisfied: true,
        valid: true,
    };
    match kind {
        Kind::Copy => {
            let pending = model.pending_copy.as_ref()?;
            doc.title = "Confirm copy";
            doc.accept = "send once";
            let column = model
                .visible_result()
                .and_then(|set| set.columns.get(pending.column))
                .map_or_else(
                    || format!("column {}", pending.column.saturating_add(1)),
                    |name| {
                        format!(
                            "column {} ({})",
                            pending.column.saturating_add(1),
                            sanitize_for_display(name)
                        )
                    },
                );
            push(
                &mut rows,
                "Send one selected text value through the terminal?",
                Token::Warning,
            );
            push(
                &mut rows,
                format!(
                    "Cell: row {}, {column}",
                    pending.source_row.saturating_add(1)
                ),
                Token::Text,
            );
            push(
                &mut rows,
                format!(
                    "Size: {} UTF-8 bytes, {} characters",
                    pending.bytes, pending.characters
                ),
                Token::Info,
            );
            push(
                &mut rows,
                format!("Path: {target} -> terminal clipboard sequence"),
                Token::Muted,
            );
            push(
                &mut rows,
                "The terminal, local applications, SSH path, or multiplexer may observe or retain it.",
                Token::Danger,
            );
            push(
                &mut rows,
                "Ignatius writes only; it never reads or clears the clipboard.",
                Token::Muted,
            );
            push(
                &mut rows,
                "Terminal acceptance is unconfirmed: this client can only know its write and flush.",
                Token::Muted,
            );
        }
        Kind::Update => {
            let pending = model.pending_update.as_ref()?;
            doc.title = "Confirm cell update";
            doc.width = 104;
            doc.former_height = 17;
            doc.accept = "run this UPDATE once";
            push(&mut rows, "Review generated UPDATE", Token::Warning);
            push(
                &mut rows,
                format!(
                    "Target: {}.{}; column: {}",
                    sanitize_for_display(&pending.relation.schema),
                    sanitize_for_display(&pending.relation.relation),
                    sanitize_for_display(&pending.plan.target_column)
                ),
                Token::Text,
            );
            push(
                &mut rows,
                format!(
                    "Selected retained source row: {}",
                    pending.candidate.source_row.saturating_add(1)
                ),
                Token::Text,
            );
            let keys = pending
                .plan
                .key_columns
                .iter()
                .map(|name| sanitize_for_display(name))
                .collect::<Vec<_>>()
                .join(", ");
            push(
                &mut rows,
                format!("Primary-key identity: {keys}"),
                Token::Muted,
            );
            push(
                &mut rows,
                "The replacement is literal text data, including empty text; it cannot add SQL syntax.",
                Token::Muted,
            );
            push(&mut rows, "Exact bound statement:", Token::Header);
            match pending.plan.bound_sql() {
                Ok(sql) => push(&mut rows, sql, Token::Text),
                Err(_) => {
                    doc.valid = false;
                    push(
                        &mut rows,
                        "The generated statement could not be rendered for review. Cancel and restage it.",
                        Token::Danger,
                    );
                }
            }
            push(&mut rows, "Nothing has been sent yet.", Token::Info);
            push(
                &mut rows,
                "The prior SELECT is a snapshot and will not be rerun automatically.",
                Token::Muted,
            );
        }
        Kind::Production | Kind::Analyze => {
            let (sql, impact, typed, required) = if kind == Kind::Production {
                let pending = model.pending_run.as_ref()?;
                doc.title = "Confirm";
                doc.width = 78;
                doc.former_height = 12;
                doc.accept = "run it once";
                push(
                    &mut rows,
                    format!("[PROD] {target}"),
                    Token::EnvironmentProduction,
                );
                push(
                    &mut rows,
                    format!("This statement {}.", pending.impact.label()),
                    Token::Danger,
                );
                (
                    &pending.sql,
                    pending.impact,
                    &pending.typed,
                    &pending.required,
                )
            } else {
                let pending = model.pending_plan.as_ref()?;
                doc.title = "Confirm analyzed plan";
                doc.former_height = 14;
                doc.accept = "execute once and measure it";
                push(
                    &mut rows,
                    "EXPLAIN ANALYZE will execute the statement to measure it.",
                    Token::Danger,
                );
                push(
                    &mut rows,
                    "Side effects are possible. This client will not automatically roll it back.",
                    Token::Warning,
                );
                push(&mut rows, format!("Target: {target}"), Token::Muted);
                (
                    &pending.sql,
                    pending.impact,
                    &pending.typed,
                    &pending.required,
                )
            };
            preview(&mut rows, sql, kind == Kind::Analyze);
            if impact.needs_typed_confirmation() {
                let displayed_required = if kind == Kind::Analyze {
                    crate::diagnostics::redact_text(required)
                } else {
                    required.clone()
                };
                let displayed_typed = if kind == Kind::Analyze {
                    crate::diagnostics::redact_text(typed)
                } else {
                    typed.clone()
                };
                push(
                    &mut rows,
                    format!(
                        "Type the database name to continue: {}",
                        sanitize_for_display(&displayed_required)
                    ),
                    Token::Warning,
                );
                doc.input = Some(Input::Name(sanitize_for_display(&displayed_typed)));
                doc.typed_satisfied = typed == required;
            }
            let advisory = if kind == Kind::Production {
                "This reads leading keywords only. Database permissions are the real control."
            } else {
                "Classification is advisory: database permissions remain the real control."
            };
            push(&mut rows, advisory, Token::Muted);
        }
        Kind::Parameter => {
            let prompt = model.parameter_prompt.as_ref()?;
            doc.title = "Parameter";
            doc.input = Some(Input::Masked(prompt.typed_length()));
            push(
                &mut rows,
                format!(
                    "Parameter {} of {}: :{}",
                    prompt.active_index().saturating_add(1),
                    prompt.total(),
                    sanitize_for_display(prompt.active_name().unwrap_or("unknown"))
                ),
                Token::Focus,
            );
            push(
                &mut rows,
                "The answer is literal text data. Quotes, punctuation and newlines stay data; empty text is valid.",
                Token::Muted,
            );
            push(
                &mut rows,
                "The SQL template stays unchanged and prompted values are kept nowhere in history.",
                Token::Muted,
            );
        }
    }
    doc.rows = rows;
    Some(doc)
}

fn key(snapshot: &KeymapSnapshot, actions: &[Action]) -> String {
    actions
        .iter()
        .find_map(|action| {
            snapshot
                .bindings
                .iter()
                .find(|binding| binding.action == *action)
                .map(|binding| sanitize_for_display(&binding.key))
        })
        .unwrap_or_else(|| "unbound".to_owned())
}

fn acceptance_keys(snapshot: &KeymapSnapshot) -> String {
    let mut keys = Vec::new();
    for action in [Action::Activate, Action::RunBuffer] {
        if let Some(binding) = snapshot
            .bindings
            .iter()
            .find(|binding| binding.action == action)
        {
            let label = sanitize_for_display(&binding.key);
            if !keys.contains(&label) {
                keys.push(label);
            }
        }
    }
    if keys.is_empty() {
        "unbound".into()
    } else {
        keys.join("/")
    }
}

fn compact_navigation_key(label: &str) -> String {
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

fn wrap_rows(rows: Vec<Row>, width: usize) -> Vec<Row> {
    rows.into_iter()
        .flat_map(|row| {
            wrap_to_width(&row.text, width)
                .into_iter()
                .map(move |text| Row {
                    text,
                    token: row.token,
                })
        })
        .collect()
}

fn wrapped_rail(navigation: &str, state: &str, width: usize) -> Vec<String> {
    [navigation, state]
        .into_iter()
        .flat_map(|line| wrap_to_width(line, width))
        .collect()
}

fn field(input: Input, width: usize, presentation: &Presentation) -> String {
    let cursor = if presentation.glyphs.is_ascii() {
        "|"
    } else {
        "\u{2588}"
    };
    let (label, value) = match input {
        Input::Masked(count) => {
            let room = width.saturating_sub(8);
            let dot = if presentation.glyphs.is_ascii() {
                "*"
            } else {
                "\u{2022}"
            };
            let value = if count == 0 {
                "(empty)".chars().take(room).collect()
            } else if count > room && room >= 3 {
                format!("{}...", dot.repeat(room - 3))
            } else {
                dot.repeat(count.min(room))
            };
            ("Value: ", value)
        }
        Input::Name(value) => {
            let room = width.saturating_sub(7);
            let truncated = display_width(&value) > room;
            let budget = room.saturating_sub(if truncated { 3 } else { 0 });
            let mut reversed = String::new();
            let mut used = 0usize;
            for character in value.chars().rev() {
                let size = display_width(&character.to_string());
                if used.saturating_add(size) > budget {
                    break;
                }
                reversed.push(character);
                used = used.saturating_add(size);
            }
            let tail = reversed.chars().rev().collect::<String>();
            (
                "Name: ",
                if truncated {
                    format!("...{tail}")
                } else {
                    tail
                },
            )
        }
    };
    format!("{label}{value}{cursor}")
}

/// Builds fresh content, canonical wrapped rows, stable rail space and admission.
pub(crate) fn build(
    model: &Model,
    kind: Kind,
    presentation: &Presentation,
    area: Rect,
) -> Option<View> {
    let doc = document(model, kind)?;
    let snapshot = if model.keymap_snapshot.is_empty() {
        crate::ui::Keymap::new().snapshot()
    } else {
        model.keymap_snapshot.clone()
    };
    let accept = acceptance_keys(&snapshot);
    let cancel = key(&snapshot, &[Action::Dismiss, Action::Cancel]);
    let up = key(
        &snapshot,
        &[Action::MovePage(Direction::Up), Action::Move(Direction::Up)],
    );
    let down = key(
        &snapshot,
        &[
            Action::MovePage(Direction::Down),
            Action::Move(Direction::Down),
        ],
    );
    let first = key(&snapshot, &[Action::MoveLineStart, Action::MoveBufferStart]);
    let last = key(&snapshot, &[Action::MoveLineEnd, Action::MoveBufferEnd]);
    let eligible = format!("{accept} to {}; {cancel} to cancel.", doc.accept);
    let blocked = format!("{accept} blocked: review to end; {cancel} to cancel.");
    let typed = format!("{accept} blocked: type exact name; {cancel} to cancel.");
    let input_rows = usize::from(doc.input.is_some());
    // Width does not depend on height, so use it to measure all rail states
    // before reducing outer clearance to preserve the required interaction rows.
    let preliminary = overlay_box(
        area,
        presentation,
        (doc.width, area.height),
        0,
        OverlayAnchor::Center,
    );
    let width = usize::from(preliminary.width.saturating_sub(2));
    let navigation = if width < 50 {
        format!(
            "{}/{} scroll {}/{} top/end",
            compact_navigation_key(&up),
            compact_navigation_key(&down),
            compact_navigation_key(&first),
            compact_navigation_key(&last),
        )
    } else {
        format!("{up}/{down} scroll; {first}/{last} first/last.")
    };
    let eligible_rail = wrapped_rail(&navigation, &eligible, width);
    let blocked_rail = wrapped_rail(&navigation, &blocked, width);
    let typed_rail = wrapped_rail(&navigation, &typed, width);
    let rail_rows = eligible_rail
        .len()
        .max(blocked_rail.len())
        .max(typed_rail.len());
    let minimum = 2usize
        .saturating_add(2)
        .saturating_add(input_rows)
        .saturating_add(rail_rows);
    let available = overlay_box(
        area,
        presentation,
        (doc.width, area.height),
        u16::try_from(minimum).unwrap_or(u16::MAX),
        OverlayAnchor::Center,
    );
    let rows = wrap_rows(doc.rows, width);
    let desired = rows
        .len()
        .saturating_add(rail_rows)
        .saturating_add(input_rows)
        .saturating_add(2)
        .max(usize::from(doc.former_height));
    let height = available
        .height
        .min(u16::try_from(desired).unwrap_or(u16::MAX));
    let outer = Rect::new(
        available.x,
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        available.width,
        height,
    );
    let inner = Rect::new(
        outer.x.saturating_add(1),
        outer.y.saturating_add(1),
        outer.width.saturating_sub(2),
        outer.height.saturating_sub(2),
    );
    let body_height =
        usize::from(inner.height).saturating_sub(rail_rows.saturating_add(input_rows));
    let usable = area.width >= super::USABLE_COLUMNS
        && area.height >= super::USABLE_ROWS
        && width >= 8
        && body_height >= 2;
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        u16::try_from(body_height).unwrap_or(u16::MAX),
    );
    let max_scroll = rows.len().saturating_sub(body_height);
    let offset = model.review_scroll.min(max_scroll);
    let at_end = offset >= max_scroll;
    let ready = usable && doc.valid && at_end && doc.typed_satisfied;
    let mut rail = if !at_end || !doc.valid {
        blocked_rail
    } else if !doc.typed_satisfied {
        typed_rail
    } else {
        eligible_rail
    };
    rail.resize(rail_rows, String::new());
    let input_area = doc
        .input
        .as_ref()
        .map(|_| Rect::new(inner.x, body.bottom(), inner.width, 1));
    let rail_area = Rect::new(
        inner.x,
        body.bottom()
            .saturating_add(u16::try_from(input_rows).unwrap_or(u16::MAX)),
        inner.width,
        u16::try_from(rail_rows).unwrap_or(u16::MAX),
    );
    let input = doc.input.map(|value| field(value, width, presentation));
    let refusal = wrap_to_width(
        &format!("Enlarge terminal to review. {cancel} to cancel."),
        width,
    );
    Some(View {
        title: doc.title,
        outer,
        body,
        input_area,
        rail_area,
        rows,
        input,
        rail,
        refusal,
        offset,
        max_scroll,
        usable,
        ready,
    })
}

#[cfg(test)]
mod tests;
