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

    // The database alone, which is what the target shortens to first.
    let mut database_spans = match &model.connection {
        ConnectionState::Connected(info) => Some(vec![
            Span::styled(
                presentation.icon(Icon::Database),
                theme.on_stripe(Token::Focus),
            ),
            Span::styled(
                sanitize_for_display(&info.database),
                theme.on_stripe(Token::Header),
            ),
        ])
        .filter(|database| spans_width(database) < spans_width(&target_spans)),
        _ => None,
    };

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
        // A quiet classification is bounded, so a long custom name can never
        // push a safety capsule off the row.
        let label = quiet_environment_label(&environment, presentation);
        left.push(Part::quiet_label(label, theme.on_stripe(env_token)));
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

    if matches!(
        model.metadata_link,
        crate::app::model::MetadataLink::Unavailable(_)
    ) {
        left.push(Part::yielding(
            1,
            vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{}[objects: {}]",
                        presentation.icon(Icon::Info),
                        model.metadata_link.label()
                    ),
                    theme.on_stripe(Token::Info),
                ),
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

    // Active watch loop capsule. A running loop repeats a statement on its
    // own, so it is a side effect in progress: it outlasts the full target and
    // its stop hint goes with it as one unit.
    if let Some(watch) = &model.watch {
        right.push((
            Part::yielding(
                WATCH_RANK,
                vec![
                    Span::styled(
                        format!(" [WATCH: {}s #{}] ", watch.interval_secs, watch.iteration),
                        capsule_style(presentation, Token::Focus),
                    ),
                    Span::raw(" "),
                    Span::styled("(\\watch stop)", theme.on_stripe(Token::Muted)),
                ],
            ),
            " ",
        ));
    }

    // Running / completed elapsed indicator. A completed time is among the
    // first things to go on a short row; a live clock goes after the provider
    // badge, and both go before the target shortens, because the footer and
    // the Results pane carry the same clock.
    if let Some(d) = model.running_for {
        let text = if !presentation.reduced_motion {
            format!(
                "{} {}",
                presentation.glyphs.spinner(model.frame),
                super::live_seconds(d)
            )
        } else {
            super::live_seconds(d)
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
            // The Results summary's own formatter, so the one duration reads
            // the same in both places.
            let text = crate::query::result::elapsed_label(d);
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

    // When the row is short, things give way in a fixed order: decoration
    // first (ranks below the target's), then the target shortens to its
    // database and then further, then the product badge goes, then a quiet
    // environment label shortens, and a running watch loop goes last. The
    // environment, `[READ ONLY]` and transaction capsules never yield: a
    // safety marker must be on screen in every layout that draws a header.
    let total_width = usize::from(area.width);
    let unicode = !presentation.glyphs.is_ascii();
    let mut brand_spans = Some(brand_spans);
    let fits = |brand: &Option<Vec<Span<'static>>>,
                target: &[Span<'static>],
                left: &[Part],
                right: &[(Part, &'static str)]| {
        header_width(brand, target, left, right) <= total_width
    };
    let drop_up_to = |rank: u8, left: &mut Vec<Part>, right: &mut Vec<(Part, &'static str)>| {
        let lowest = left
            .iter()
            .filter_map(|part| part.yields)
            .chain(right.iter().filter_map(|(part, _)| part.yields))
            .filter(|yields| *yields <= rank)
            .min();
        let Some(lowest) = lowest else {
            return false;
        };
        if let Some(index) = left.iter().position(|part| part.yields == Some(lowest)) {
            left.remove(index);
        } else if let Some(index) = right
            .iter()
            .position(|(part, _)| part.yields == Some(lowest))
        {
            right.remove(index);
        }
        true
    };
    while !fits(&brand_spans, &target_spans, &left, &right)
        && drop_up_to(TARGET_RANK - 1, &mut left, &mut right)
    {}
    if !fits(&brand_spans, &target_spans, &left, &right)
        && let Some(database) = database_spans.take()
    {
        target_spans = database;
    }
    if !fits(&brand_spans, &target_spans, &left, &right) {
        let room = total_width.saturating_sub(header_width(&brand_spans, &[], &left, &right));
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
    if !fits(&brand_spans, &target_spans, &left, &right)
        && let Some(index) = left.iter().position(|part| part.quiet.is_some())
    {
        let others = header_width(&brand_spans, &target_spans, &left, &right)
            - spans_width(&left[index].spans);
        let room = total_width.saturating_sub(others);
        if !left[index].shrink_quiet(room, unicode) {
            left.remove(index);
        }
    }
    while !fits(&brand_spans, &target_spans, &left, &right)
        && drop_up_to(u8::MAX, &mut left, &mut right)
    {}

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

/// Yield ranks below this go before the target shortens.
const TARGET_RANK: u8 = 6;

/// A running watch loop yields after the target, the badge and a quiet label.
const WATCH_RANK: u8 = 9;

/// The most cells a quiet environment label may take inside its brackets.
const QUIET_LABEL_CELLS: usize = 14;

/// A run of header spans and how readily it gives way on a short row.
struct Part {
    spans: Vec<Span<'static>>,
    /// Lower numbers yield first; `None` never yields, because it is a safety
    /// marker.
    yields: Option<u8>,
    /// For a quiet environment label: its full words and style, so it can be
    /// shortened rather than clipped.
    quiet: Option<(String, Style)>,
}

impl Part {
    const fn safety(spans: Vec<Span<'static>>) -> Self {
        Self {
            spans,
            yields: None,
            quiet: None,
        }
    }

    const fn yielding(rank: u8, spans: Vec<Span<'static>>) -> Self {
        Self {
            spans,
            yields: Some(rank),
            quiet: None,
        }
    }

    fn quiet_label(label: String, style: Style) -> Self {
        Self {
            spans: vec![Span::raw("  "), Span::styled(format!("[{label}]"), style)],
            yields: None,
            quiet: Some((label, style)),
        }
    }

    /// Shortens a quiet label to fit `room` cells, keeping at least a few
    /// characters of it. Returns false when even that does not fit.
    fn shrink_quiet(&mut self, room: usize, unicode: bool) -> bool {
        let Some((label, style)) = &self.quiet else {
            return false;
        };
        // Two cells of spacing and two brackets surround the words.
        let words = room.saturating_sub(4);
        if words < 4 {
            return false;
        }
        let short = truncate_to_width(label, words, unicode);
        self.spans = vec![Span::raw("  "), Span::styled(format!("[{short}]"), *style)];
        true
    }
}

/// A quiet environment label, sanitised and bounded to a readable prefix.
fn quiet_environment_label(environment: &Environment, presentation: &Presentation) -> String {
    truncate_to_width(
        &sanitize_for_display(&environment.label()),
        QUIET_LABEL_CELLS,
        !presentation.glyphs.is_ascii(),
    )
}

/// The header's drawn width for a set of parts: both groups and the gap.
fn header_width(
    brand: &Option<Vec<Span<'static>>>,
    target: &[Span<'static>],
    left: &[Part],
    right: &[(Part, &'static str)],
) -> usize {
    let left_width = brand.as_deref().map_or(0, spans_width)
        + spans_width(target)
        + left
            .iter()
            .map(|part| spans_width(&part.spans))
            .sum::<usize>();
    let right_width = right_group_width(right);
    left_width + usize::from(right_width > 0) * 2 + right_width
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
                "{}{}",
                presentation.icon(Icon::Clock),
                super::live_seconds(elapsed)
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
    // The safety capsules lead, so nothing to their left can push them off a
    // narrow row. Quieter facts follow, each drawn whole or shortened with a
    // visible ellipsis, never clipped mid-word.
    let unicode = !presentation.glyphs.is_ascii();
    let mut spans = Vec::new();
    let capsule_environment =
        environment.is_production() || matches!(environment, Environment::Staging);
    if capsule_environment {
        let token = if environment.is_production() {
            Token::EnvironmentProduction
        } else {
            Token::Warning
        };
        spans.push(Span::styled(
            format!(" [{}] ", environment.label()),
            capsule_style(presentation, token),
        ));
        spans.push(Span::raw(" "));
    }
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

    let width = usize::from(area.width);
    let mut used = spans_width(&spans);
    let quiet_style = theme.style(Token::EnvironmentNonProduction);
    let quiet_label =
        (!capsule_environment).then(|| quiet_environment_label(&environment, presentation));
    if let Some(label) = quiet_label {
        let whole = format!("[{label}] ");
        if used + display_width(&whole) <= width {
            used += display_width(&whole);
            spans.push(Span::styled(whole, quiet_style));
        } else if width.saturating_sub(used) >= 7 {
            let short = truncate_to_width(&label, width - used - 3, unicode);
            let short = format!("[{short}] ");
            used += display_width(&short);
            spans.push(Span::styled(short, quiet_style));
        }
    }
    let room = width.saturating_sub(used);
    if display_width(&connection) <= room {
        used += display_width(&connection);
        spans.push(Span::styled(connection, theme.style(Token::Text)));
        let state = connection_state_spans(model, presentation);
        let focus = Span::styled(
            format!(
                " {}[{}]",
                presentation.icon(Icon::Focus),
                model.focus.label()
            ),
            theme.style(Token::Focus),
        );
        if used + spans_width(&state) <= width {
            used += spans_width(&state);
            spans.extend(state);
            if used + display_width(&focus.content) <= width {
                spans.push(focus);
            }
        }
    } else if room >= 6 {
        spans.push(Span::styled(
            truncate_to_width(&connection, room, unicode),
            theme.style(Token::Text),
        ));
    }
    Paragraph::new(Line::from(spans))
        .style(theme.surface(Token::SurfaceAlt))
        .render(area, buf);
}

#[cfg(test)]
mod tests;
