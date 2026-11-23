use crate::app::model::{ConnectingStep, ConnectionState, Model, TimingDisplay};
use crate::connection::Environment;
use crate::query::result::TransactionState;
use crate::query::value::{display_width, sanitize_for_display, truncate_to_width};
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

    // 1. Product badge (elevated keycap over uniform SurfaceAlt rail)
    let brand = presentation.icon(Icon::Brand);
    let name = crate::branding::PRODUCT_NAME;
    let brand_spans = vec![
        Span::raw(" "),
        Span::styled(format!(" {brand}{name}: "), theme.keycap(Token::Header)),
        Span::raw(" "),
    ];

    // 2. Connection state / target
    let mut target_spans: Vec<Span<'static>> = Vec::new();
    match &model.connection {
        ConnectionState::Connected(info) => {
            target_spans.push(Span::styled(
                presentation.icon(Icon::Database),
                theme.on_stripe(Token::Focus),
            ));
            let clean_target = sanitize_for_display(&info.target);
            if let Some(at_idx) = clean_target.find('@')
                && let Some(slash_idx) = clean_target.rfind('/')
                && at_idx < slash_idx
            {
                let user = &clean_target[..at_idx];
                let host_port = &clean_target[at_idx..=slash_idx];
                let db = &clean_target[slash_idx + 1..];
                target_spans.push(Span::styled(user.to_owned(), theme.on_stripe(Token::Info)));
                target_spans.push(Span::styled(
                    host_port.to_owned(),
                    theme.on_stripe(Token::Muted),
                ));
                target_spans.push(Span::styled(db.to_owned(), theme.on_stripe(Token::Header)));
            } else {
                target_spans.push(Span::styled(clean_target, theme.on_stripe(Token::Text)));
            }
        }
        _ => {
            target_spans.push(Span::styled(
                format!(
                    "{}{}",
                    presentation.icon(Icon::Database),
                    model.connection.label()
                ),
                theme.on_stripe(Token::Text),
            ));
        }
    }

    let mut left = Vec::new();

    // Failed connection step if any
    if let Some(step) = failed_step(model) {
        left.push(Part::yielding(
            5,
            vec![
                Span::raw("  "),
                Span::styled(
                    format!("failed during {}", step.label()),
                    theme.on_stripe(Token::Danger),
                ),
            ],
        ));
    }

    // 3. Active provider badge if any
    if let Some(provider) = &model.credential_provider {
        left.push(Part::yielding(
            3,
            vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{}[auth: {}]",
                        presentation.icon(Icon::Info),
                        sanitize_for_display(provider)
                    ),
                    theme.on_stripe(Token::Info),
                ),
            ],
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

    if environment.is_production() || matches!(environment, Environment::Staging) {
        left.push(Part::safety(vec![
            Span::raw(" "),
            Span::styled(
                format!(" [{}] ", environment.label()),
                capsule_style(presentation, env_token),
            ),
        ]));
    } else {
        left.push(Part::safety(vec![
            Span::raw("  "),
            Span::styled(
                format!("[{}]", environment.label()),
                theme.on_stripe(env_token),
            ),
        ]));
    }

    // Read-only capsule
    if let Some(info) = model.connection.info()
        && info.read_only
    {
        left.push(Part::safety(vec![
            Span::raw(" "),
            Span::styled(" [READ ONLY] ", capsule_style(presentation, Token::Info)),
        ]));
    }

    // History and metadata link notices
    let notice = if !model.records_history() {
        Some(format!("{}[history off]", presentation.icon(Icon::Info)))
    } else if matches!(
        model.metadata_link,
        crate::app::model::MetadataLink::Unavailable(_)
    ) {
        Some(format!(
            "{}[objects: {}]",
            presentation.icon(Icon::Info),
            model.metadata_link.label()
        ))
    } else if model.history_note.is_some() {
        Some(format!("{}[not in history]", presentation.icon(Icon::Info)))
    } else {
        None
    };
    if let Some(notice) = notice {
        left.push(Part::yielding(
            1,
            vec![
                Span::raw("  "),
                Span::styled(notice, theme.on_stripe(Token::Info)),
            ],
        ));
    }

    // Right group: active session context, each part with the joiner it
    // takes when another part precedes it.
    let mut right: Vec<(Part, &'static str)> = Vec::new();

    // Non-idle transaction capsule
    match model.transaction {
        TransactionState::Open => right.push((
            Part::safety(vec![Span::styled(
                " [TX ACTIVE] ",
                capsule_style(presentation, Token::TransactionActive),
            )]),
            "",
        )),
        TransactionState::Failed => right.push((
            Part::safety(vec![Span::styled(
                " [TX FAILED] ",
                capsule_style(presentation, Token::TransactionFailed),
            )]),
            "",
        )),
        _ => {}
    }

    // Active watch loop capsule
    if let Some(watch) = &model.watch {
        right.push((
            Part::yielding(
                4,
                vec![Span::styled(
                    format!(" [WATCH: {}s #{}] ", watch.interval_secs, watch.iteration),
                    capsule_style(presentation, Token::Focus),
                )],
            ),
            " ",
        ));
        right.push((
            Part::yielding(
                2,
                vec![Span::styled(
                    "(\\watch stop)",
                    theme.on_stripe(Token::Muted),
                )],
            ),
            " ",
        ));
    }

    // Running / completed elapsed indicator. A live clock is a truthful wait
    // indicator and yields late; a completed time yields early.
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
        right.push((
            Part::yielding(4, vec![Span::styled(text, theme.on_stripe(Token::Info))]),
            "  ",
        ));
    } else if let Some(d) = model.last_elapsed {
        let show = match model.timing_display {
            TimingDisplay::Off => false,
            TimingDisplay::Threshold200ms => d >= Duration::from_millis(200),
            TimingDisplay::Always => true,
        };
        if show {
            // The same unit spacing as the Results summary (`240 ms`), so the
            // one duration reads the same in both places.
            let text = if d >= Duration::from_secs(1) {
                format!("{:.2} s", d.as_secs_f64())
            } else if d.as_millis() == 0 {
                "<1 ms".to_owned()
            } else {
                format!("{} ms", d.as_millis())
            };
            right.push((
                Part::yielding(2, vec![Span::styled(text, theme.on_stripe(Token::Muted))]),
                "  ",
            ));
        }
    }

    // Motion badge
    if presentation.reduced_motion {
        right.push((
            Part::yielding(
                1,
                vec![Span::styled("[MUTED]", theme.on_stripe(Token::Muted))],
            ),
            "  ",
        ));
    }

    // When the row is short, decoration yields first, in order, then the
    // target shortens and finally the product badge goes. The environment,
    // `[READ ONLY]` and transaction capsules never yield: a safety marker
    // must be on screen in every layout that draws a header.
    let total_width = usize::from(area.width);
    let unicode = !presentation.glyphs.is_ascii();
    let mut brand_spans = Some(brand_spans);
    let fits = |brand: &Option<Vec<Span<'static>>>,
                target: &[Span<'static>],
                left: &[Part],
                right: &[(Part, &'static str)]| {
        let left_width = brand.as_deref().map_or(0, spans_width)
            + spans_width(target)
            + left
                .iter()
                .map(|part| spans_width(&part.spans))
                .sum::<usize>();
        let right_width = right_group_width(right);
        left_width + usize::from(right_width > 0) * 2 + right_width <= total_width
    };
    while !fits(&brand_spans, &target_spans, &left, &right) {
        let lowest = left
            .iter()
            .filter_map(|part| part.yields)
            .chain(right.iter().filter_map(|(part, _)| part.yields))
            .min();
        let Some(lowest) = lowest else { break };
        if let Some(index) = left.iter().position(|part| part.yields == Some(lowest)) {
            left.remove(index);
        } else if let Some(index) = right
            .iter()
            .position(|(part, _)| part.yields == Some(lowest))
        {
            right.remove(index);
        }
    }
    if !fits(&brand_spans, &target_spans, &left, &right) {
        let others = brand_spans.as_deref().map_or(0, spans_width)
            + left
                .iter()
                .map(|part| spans_width(&part.spans))
                .sum::<usize>()
            + right_group_width(&right)
            + usize::from(!right.is_empty()) * 2;
        let room = total_width.saturating_sub(others);
        // A target shorter than a few cells says nothing, so it goes whole.
        target_spans = if room >= 6 {
            fit_spans(target_spans, room, unicode)
        } else {
            Vec::new()
        };
    }
    if !fits(&brand_spans, &target_spans, &left, &right) {
        brand_spans = None;
    }

    let mut left_spans: Vec<Span<'static>> = brand_spans.unwrap_or_default();
    left_spans.extend(target_spans);
    for part in left {
        left_spans.extend(part.spans);
    }
    let mut right_spans: Vec<Span<'static>> = Vec::new();
    for (part, joiner) in right {
        if !right_spans.is_empty() && !joiner.is_empty() {
            right_spans.push(Span::raw(joiner));
        }
        right_spans.extend(part.spans);
    }

    let right_starts_padded = right_spans
        .first()
        .is_some_and(|s| s.content.starts_with(' '));
    if !right_spans.is_empty() {
        right_spans.push(Span::raw(" "));
    }

    let left_width: usize = left_spans.iter().map(|s| display_width(&s.content)).sum();
    let right_width: usize = right_spans.iter().map(|s| display_width(&s.content)).sum();
    let raw_separator = presentation.glyphs.separator();
    let separator = if right_starts_padded {
        raw_separator.trim_end()
    } else {
        raw_separator
    };
    let separator_width = display_width(separator);

    if !right_spans.is_empty() && left_width + 2 + right_width <= total_width {
        let use_separator = left_width + separator_width + right_width <= total_width;
        let (final_right, right_total_width) = if use_separator {
            let mut with_sep = Vec::with_capacity(right_spans.len() + 1);
            with_sep.push(Span::styled(separator, theme.on_stripe(Token::Border)));
            with_sep.extend(right_spans);
            (with_sep, (separator_width + right_width) as u16)
        } else {
            (right_spans, right_width as u16)
        };
        let left_area = Rect::new(area.x, area.y, area.width - right_total_width, 1);
        let right_x = area.x + area.width - right_total_width;
        let right_area = Rect::new(right_x, area.y, right_total_width, 1);
        Paragraph::new(Line::from(left_spans))
            .style(theme.surface(Token::SurfaceAlt))
            .render(left_area, buf);
        Paragraph::new(Line::from(final_right))
            .style(theme.surface(Token::SurfaceAlt))
            .render(right_area, buf);
    } else if !right_spans.is_empty() && left_width + 2 < total_width {
        let mut combined = left_spans;
        if left_width + separator_width < total_width {
            combined.push(Span::styled(separator, theme.on_stripe(Token::Border)));
        } else {
            combined.push(Span::raw("  "));
        }
        combined.extend(right_spans);
        Paragraph::new(Line::from(combined))
            .style(theme.surface(Token::SurfaceAlt))
            .render(area, buf);
    } else {
        Paragraph::new(Line::from(left_spans))
            .style(theme.surface(Token::SurfaceAlt))
            .render(area, buf);
    }
}

/// A run of header spans and how readily it gives way on a short row.
struct Part {
    spans: Vec<Span<'static>>,
    /// Lower numbers yield first; `None` never yields, because it is a safety
    /// marker.
    yields: Option<u8>,
}

impl Part {
    const fn safety(spans: Vec<Span<'static>>) -> Self {
        Self {
            spans,
            yields: None,
        }
    }

    const fn yielding(rank: u8, spans: Vec<Span<'static>>) -> Self {
        Self {
            spans,
            yields: Some(rank),
        }
    }
}

fn spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|span| display_width(&span.content)).sum()
}

/// The right group's width as it is drawn: joiners between parts and one
/// trailing cell of padding.
fn right_group_width(right: &[(Part, &'static str)]) -> usize {
    if right.is_empty() {
        return 0;
    }
    right
        .iter()
        .enumerate()
        .map(|(index, (part, joiner))| {
            spans_width(&part.spans) + if index > 0 { display_width(joiner) } else { 0 }
        })
        .sum::<usize>()
        + 1
}

/// Shortens styled spans to `width` cells, ending in the truncation marker in
/// the style of the span it interrupts.
fn fit_spans(spans: Vec<Span<'static>>, width: usize, unicode: bool) -> Vec<Span<'static>> {
    if spans_width(&spans) <= width {
        return spans;
    }
    let marker = if unicode {
        crate::query::value::TRUNCATION_MARKER.to_string()
    } else {
        crate::query::value::TRUNCATION_MARKER_ASCII.to_owned()
    };
    let mut budget = width.saturating_sub(display_width(&marker));
    let mut out = Vec::new();
    for span in spans {
        let span_width = display_width(&span.content);
        if span_width <= budget {
            budget -= span_width;
            out.push(span);
            continue;
        }
        let kept = truncate_to_width(&span.content, budget + display_width(&marker), unicode);
        out.push(Span::styled(kept, span.style));
        return out;
    }
    out
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
    // The safety markers lead, so a narrow row clips the database name and
    // the focus word before it could clip `[READ ONLY]` or a transaction.
    let mut spans = vec![if environment.is_production() {
        Span::styled(
            label,
            capsule_style(presentation, Token::EnvironmentProduction),
        )
    } else {
        Span::styled(label, theme.style(Token::EnvironmentNonProduction))
    }];
    if model.connection.info().is_some_and(|info| info.read_only) {
        spans.push(Span::styled(
            " [READ ONLY] ",
            capsule_style(presentation, Token::Info),
        ));
        spans.push(Span::raw(" "));
    }
    match model.transaction {
        TransactionState::Open => {
            spans.push(Span::styled(
                " [TX ACTIVE] ",
                capsule_style(presentation, Token::TransactionActive),
            ));
            spans.push(Span::raw(" "));
        }
        TransactionState::Failed => {
            spans.push(Span::styled(
                " [TX FAILED] ",
                capsule_style(presentation, Token::TransactionFailed),
            ));
            spans.push(Span::raw(" "));
        }
        _ => {}
    }
    spans.push(Span::styled(connection, theme.style(Token::Text)));
    spans.extend(connection_state_spans(model, presentation));
    spans.push(Span::styled(
        format!(
            " {}[{}]",
            presentation.icon(Icon::Focus),
            model.focus.label()
        ),
        theme.style(Token::Focus),
    ));
    Paragraph::new(Line::from(spans))
        .style(theme.surface(Token::SurfaceAlt))
        .render(area, buf);
}

#[cfg(test)]
mod tests;
