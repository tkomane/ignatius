use super::{
    hairline_rule, highlighted_sql_spans, input_cursor, overlay_block, overlay_block_line,
};
use crate::app::model::Model;
use crate::app::palette::theme_family_label;
use crate::config::ThemeChoice;
use crate::query::value::{display_width, sanitize_for_display, truncate_to_width};
use crate::ui::glyphs::Icon;
use crate::ui::layout::Presentation;
use crate::ui::layout::{OverlayAnchor, overlay_box};
use crate::ui::theme::{ColorDepth, Theme, Token};
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

fn push_preview_hairline(
    preview_lines: &mut Vec<Line<'static>>,
    preview_w: u16,
    presentation: &Presentation,
    theme: &Theme,
) {
    if !preview_lines.is_empty() && preview_w >= 8 {
        let rule_w = usize::from(preview_w).min(44);
        preview_lines.push(Line::from(Span::styled(
            hairline_rule(presentation).repeat(rule_w),
            theme.style(Token::Border),
        )));
    }
}

const fn purpose_icon(purpose: crate::app::palette::Purpose) -> Icon {
    use crate::app::palette::Purpose;
    match purpose {
        Purpose::Commands => Icon::Key,
        Purpose::GoTo => Icon::SearchPath,
        Purpose::History => Icon::Clock,
        Purpose::Dependencies => Icon::Schema,
        Purpose::SavedQueries => Icon::Editor,
        Purpose::ResultControls | Purpose::ResultColumns | Purpose::ExportFormats => Icon::Rows,
        Purpose::Connections => Icon::Database,
    }
}

fn palette_title_line(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
) -> Line<'static> {
    let theme = &presentation.theme;
    let raw = palette.purpose.title().trim();
    let (primary, instructions) = raw.split_once("  ").unwrap_or((raw, ""));
    let icon = presentation.icon(purpose_icon(palette.purpose));
    let mut spans = vec![Span::styled(
        format!("{icon}{primary}"),
        theme.style(Token::Focus).add_modifier(Modifier::BOLD),
    )];
    if !instructions.is_empty() {
        spans.push(Span::styled(
            format!("  {instructions}"),
            theme.style(Token::Muted),
        ));
    }
    Line::from(spans)
}

fn render_spotlight_input(
    palette: &crate::app::palette::Palette,
    match_count: usize,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let prompt = format!(" {} ", presentation.glyphs.search_prompt());
    let count_label = match_count_label(match_count);
    let count_width = display_width(&count_label);
    let prompt_width = display_width(&prompt);
    let right_pad = usize::from(area.width >= 20);
    let query_budget =
        usize::from(area.width).saturating_sub(prompt_width + 1 + count_width + 2 + right_pad);
    let clean_query = sanitize_for_display(&palette.query);
    let shown_query = if display_width(&clean_query) <= query_budget {
        clean_query
    } else {
        truncate_to_width(&clean_query, query_budget, unicode)
    };
    // An empty query says what typing does, in the quietest ink, and only
    // when it fits beside the count.
    let placeholder = (shown_query.is_empty() && display_width(PLACEHOLDER) < query_budget)
        .then_some(PLACEHOLDER);
    let left_width = prompt_width
        + display_width(&shown_query)
        + 1
        + placeholder.map_or(0, |text| display_width(text) + 1);
    let mut spans = vec![
        Span::styled(
            prompt,
            theme.on_stripe(Token::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled(shown_query, theme.on_stripe(Token::Text)),
        Span::styled(input_cursor(presentation), theme.on_stripe(Token::Focus)),
    ];
    if let Some(text) = placeholder {
        spans.push(Span::styled(
            format!(" {text}"),
            theme.on_stripe(Token::Muted),
        ));
    }
    let total_width = usize::from(area.width);
    if left_width + 2 + count_width + right_pad <= total_width {
        let gap = total_width - left_width - count_width - right_pad;
        spans.push(Span::styled(" ".repeat(gap), theme.stripe()));
        spans.push(Span::styled(count_label, theme.on_stripe(Token::Muted)));
        if right_pad == 1 {
            spans.push(Span::styled(" ", theme.stripe()));
        }
    } else if left_width + 2 + count_width <= total_width {
        spans.push(Span::styled(
            format!("  {count_label}"),
            theme.on_stripe(Token::Muted),
        ));
    }
    Paragraph::new(Line::from(spans))
        .style(theme.stripe())
        .render(area, buf);
}

/// What an empty palette query says.
const PLACEHOLDER: &str = "Type to filter";

/// The match count in words, singular when there is exactly one.
fn match_count_label(count: usize) -> String {
    if count == 1 {
        "1 match".to_owned()
    } else {
        format!("{count} matches")
    }
}

fn matched_label_spans(label: &str, query: &str, theme: &Theme) -> Vec<Span<'static>> {
    let trimmed = query.trim();
    let base_style = theme.style(Token::Text);
    if trimmed.is_empty() || label.is_empty() {
        return vec![Span::styled(label.to_owned(), base_style)];
    }

    let chars: Vec<(usize, usize, char)> = label
        .char_indices()
        .map(|(start, ch)| {
            (
                start,
                start + ch.len_utf8(),
                ch.to_lowercase().next().unwrap_or(ch),
            )
        })
        .collect();
    let needle: Vec<char> = trimmed
        .chars()
        .map(|ch| ch.to_lowercase().next().unwrap_or(ch))
        .collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return vec![Span::styled(label.to_owned(), base_style)];
    }

    let mut matched = vec![false; chars.len()];
    let mut found_contiguous = false;
    let mut idx = 0;
    while idx + needle.len() <= chars.len() {
        if (0..needle.len()).all(|k| chars[idx + k].2 == needle[k]) {
            matched[idx..idx + needle.len()].fill(true);
            found_contiguous = true;
            idx += needle.len();
        } else {
            idx += 1;
        }
    }

    if !found_contiguous {
        let raw: Vec<char> = label.chars().collect();
        let position_score = |index: usize| -> i32 {
            let boundary = index == 0
                || matches!(raw.get(index - 1), Some('_' | '.' | ' ' | '-' | '/'))
                || (raw[index].is_uppercase()
                    && raw.get(index - 1).is_some_and(char::is_ascii_lowercase));
            10 + if boundary { 14 } else { 0 }
        };
        const UNMATCHED: i32 = i32::MIN / 4;
        let n = needle.len();
        let m = chars.len();
        let mut dp = vec![vec![UNMATCHED; m]; n];
        let mut prev = vec![vec![None; m]; n];
        for (i, needle_ch) in needle.iter().enumerate() {
            let mut best_gap = UNMATCHED;
            let mut best_gap_idx = None;
            for j in 0..m {
                if i > 0 && j >= 2 && dp[i - 1][j - 2] > best_gap {
                    best_gap = dp[i - 1][j - 2];
                    best_gap_idx = Some(j - 2);
                }
                if chars[j].2 != *needle_ch {
                    continue;
                }
                if i == 0 {
                    dp[0][j] = position_score(j);
                } else {
                    let consec = if j > 0 && dp[i - 1][j - 1] > UNMATCHED {
                        dp[i - 1][j - 1] + 18
                    } else {
                        UNMATCHED
                    };
                    if consec >= best_gap && consec > UNMATCHED {
                        dp[i][j] = consec + position_score(j);
                        prev[i][j] = Some(j - 1);
                    } else if best_gap > UNMATCHED {
                        dp[i][j] = best_gap + position_score(j);
                        prev[i][j] = best_gap_idx;
                    }
                }
            }
        }
        if let Some((mut j, _)) = dp[n - 1]
            .iter()
            .enumerate()
            .max_by_key(|&(_, &score)| score)
            .filter(|&(_, &score)| score > UNMATCHED)
        {
            for i in (0..n).rev() {
                matched[j] = true;
                if i > 0 {
                    let Some(p) = prev[i][j] else {
                        break;
                    };
                    j = p;
                }
            }
        }
    }

    let highlight_style = theme
        .style(Token::Focus)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut spans = Vec::new();
    let mut run_start = 0usize;
    while run_start < chars.len() {
        let is_match = matched[run_start];
        let mut run_end = run_start + 1;
        while run_end < chars.len() && matched[run_end] == is_match {
            run_end += 1;
        }
        let byte_start = chars[run_start].0;
        let byte_end = chars[run_end - 1].1;
        spans.push(Span::styled(
            label[byte_start..byte_end].to_owned(),
            if is_match {
                highlight_style
            } else {
                base_style
            },
        ));
        run_start = run_end;
    }
    spans
}

fn push_wrapped_stripe_sql_lines(
    preview_lines: &mut Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    preview_w: usize,
    theme: &Theme,
) {
    let stripe = theme.stripe();
    if preview_w == 0 {
        preview_lines.push(Line::from(spans).style(stripe));
        return;
    }

    let mut cells: Vec<(char, Style, usize)> = Vec::new();
    for span in spans {
        for ch in span.content.chars() {
            let mut buf = [0u8; 4];
            let w = display_width(ch.encode_utf8(&mut buf));
            cells.push((ch, span.style, w));
        }
    }
    if cells.is_empty() {
        preview_lines.push(Line::from(Span::styled(" ".repeat(preview_w), stripe)).style(stripe));
        return;
    }

    let emit_slice = |out: &mut Vec<Line<'static>>, slice: &[(char, Style, usize)]| {
        let mut line_spans = Vec::new();
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        let mut used = 0usize;
        for &(ch, style, w) in slice {
            if run_style != Some(style) {
                if let Some(prev) = run_style {
                    line_spans.push(Span::styled(std::mem::take(&mut run), prev));
                }
                run_style = Some(style);
            }
            run.push(ch);
            used += w;
        }
        if let Some(prev) = run_style {
            line_spans.push(Span::styled(run, prev));
        }
        if used < preview_w {
            line_spans.push(Span::styled(" ".repeat(preview_w - used), stripe));
        }
        out.push(Line::from(line_spans).style(stripe));
    };

    let mut start = 0usize;
    while start < cells.len() {
        let mut width_acc = 0usize;
        let mut end = start;
        let mut last_space = None;
        let mut seen_non_space = false;
        while end < cells.len() {
            let (ch, _, w) = cells[end];
            if width_acc + w > preview_w && end > start {
                break;
            }
            if ch.is_whitespace() {
                if seen_non_space {
                    last_space = Some(end);
                }
            } else {
                seen_non_space = true;
            }
            width_acc += w;
            end += 1;
        }
        if end < cells.len()
            && let Some(space_idx) = last_space
            && space_idx > start
        {
            emit_slice(preview_lines, &cells[start..space_idx]);
            start = space_idx + 1;
        } else {
            emit_slice(preview_lines, &cells[start..end]);
            start = end;
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "match line rendering needs entry, query, selection, layout widths, glyph tier and theme"
)]
fn build_palette_match_line(
    entry: &crate::app::palette::PaletteEntry,
    query: &str,
    selected: bool,
    show_detail: bool,
    text_budget: usize,
    row_width_target: usize,
    unicode: bool,
    theme: &Theme,
) -> Line<'static> {
    let selection = if selected {
        theme.style(Token::Selection)
    } else {
        Style::default()
    };
    let group_raw = format!(" {:<8}", entry.group);
    let group_str = truncate_to_width(&group_raw, text_budget, unicode);
    let mut used = display_width(&group_str);
    let mut spans = vec![Span::styled(
        group_str,
        theme.style(Token::Muted).patch(selection),
    )];

    let label_budget = text_budget.saturating_sub(used);
    if label_budget > 0 {
        let label_str =
            truncate_to_width(&sanitize_for_display(&entry.label), label_budget, unicode);
        used += display_width(&label_str);
        if selected {
            spans.push(Span::styled(label_str, theme.style(Token::Selection)));
        } else {
            spans.extend(matched_label_spans(&label_str, query, theme));
        }
    }

    if show_detail && !entry.detail.is_empty() {
        let detail_budget = text_budget.saturating_sub(used + 2);
        if detail_budget >= 3 {
            let detail_str =
                truncate_to_width(&sanitize_for_display(&entry.detail), detail_budget, unicode);
            used += 2 + display_width(&detail_str);
            spans.push(Span::styled(
                format!("  {detail_str}"),
                theme.style(Token::Muted).patch(selection),
            ));
        }
    }

    if selected && used < row_width_target {
        spans.push(Span::styled(
            " ".repeat(row_width_target - used),
            theme.style(Token::Selection),
        ));
    }
    let mut line = Line::from(spans);
    if selected {
        line = line.style(theme.style(Token::Selection));
    }
    line
}

pub(crate) fn render_palette(
    palette: &crate::app::palette::Palette,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &presentation.theme;
    let unicode = !presentation.glyphs.is_ascii();
    let palette_area = palette_box(palette, presentation, area);
    ratatui::widgets::Clear.render(palette_area, buf);

    let block = overlay_block_line(palette_title_line(palette, presentation), presentation);
    block.render(palette_area, buf);

    let inner = Rect {
        x: palette_area.x.saturating_add(1),
        y: palette_area.y.saturating_add(1),
        width: palette_area.width.saturating_sub(2),
        height: palette_area.height.saturating_sub(2),
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }

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

    render_spotlight_input(
        palette,
        matches.len(),
        presentation,
        Rect::new(inner.x, inner.y, inner.width, 1),
        buf,
    );

    if !show_preview {
        let body_area = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
        let mut lines: Vec<Line<'static>> = note_lines
            .iter()
            .map(|note| {
                Line::from(Span::styled(
                    format!(" {}", sanitize_for_display(note)),
                    theme.style(Token::Muted),
                ))
            })
            .collect();

        let visible = visible_rows(palette, presentation, area);
        let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
        let target_width = usize::from(inner.width);
        let text_budget = target_width.saturating_sub(1).max(1);
        for (index, entry) in matches.iter().enumerate().skip(offset).take(visible) {
            lines.push(build_palette_match_line(
                entry,
                &palette.query,
                index == palette.selected,
                true,
                text_budget,
                target_width,
                unicode,
                theme,
            ));
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
            .style(theme.surface(Token::SurfaceOverlay))
            .render(body_area, buf);
        return;
    }

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
    let preview_x = divider_x + 2;
    let preview_w = inner.width.saturating_sub(list_width + 2);

    let visible = visible_rows(palette, presentation, area);
    let offset = palette.selected.saturating_sub(visible.saturating_sub(1));
    let divider_glyph = presentation.glyphs.column_rule();
    let divider_style = theme
        .surface(Token::SurfaceOverlay)
        .patch(theme.style(Token::Border));
    for y in content_y..content_y + content_height {
        buf[(divider_x, y)]
            .set_symbol(divider_glyph)
            .set_style(divider_style);
    }
    let track = usize::from(content_height);
    if matches.len() > visible && track > 0 {
        let thumb = ((visible * track) / matches.len()).clamp(1, track);
        let max_offset = matches.len().saturating_sub(visible).max(1);
        let position = (offset.min(max_offset) * track.saturating_sub(thumb)) / max_offset;
        let thumb_glyph = if unicode { "\u{2503}" } else { "#" };
        let thumb_style = theme
            .surface(Token::SurfaceOverlay)
            .patch(theme.style(Token::Focus));
        for row in position..(position + thumb).min(track) {
            buf[(divider_x, content_y + u16::try_from(row).unwrap_or(0))]
                .set_symbol(thumb_glyph)
                .set_style(thumb_style);
        }
    }

    let mut match_lines = Vec::new();
    let target_width = usize::from(list_width);
    let text_budget = target_width.saturating_sub(1).max(1);
    for (index, entry) in matches.iter().enumerate().skip(offset).take(visible) {
        match_lines.push(build_palette_match_line(
            entry,
            &palette.query,
            index == palette.selected,
            false,
            text_budget,
            target_width,
            unicode,
            theme,
        ));
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

    if let Some(choice) = palette
        .selected_entry()
        .filter(|_| !matches.is_empty())
        .and_then(|entry| entry.theme_choice())
    {
        let preview_area = Rect::new(preview_x, content_y, preview_w, content_height);
        render_theme_preview(choice, presentation, preview_area, buf);
    } else if let Some(preview_raw) = palette.selected_preview() {
        let mut in_sql_block = false;
        let mut preview_lines: Vec<Line<'static>> = Vec::with_capacity(preview_raw.len());
        for raw_line in preview_raw {
            if raw_line == "SQL:" {
                in_sql_block = true;
                push_preview_hairline(&mut preview_lines, preview_w, presentation, theme);
                preview_lines.push(Line::from(Span::styled(
                    raw_line,
                    theme.style(Token::Header),
                )));
                continue;
            }
            if in_sql_block && !raw_line.starts_with("  ") {
                in_sql_block = false;
            }

            if in_sql_block {
                push_wrapped_stripe_sql_lines(
                    &mut preview_lines,
                    highlighted_sql_spans(&raw_line, theme, true),
                    usize::from(preview_w),
                    theme,
                );
            } else if let Some((key, val)) = raw_line.split_once(": ") {
                if key == "SQL" {
                    push_preview_hairline(&mut preview_lines, preview_w, presentation, theme);
                    let mut line_spans = vec![Span::styled(
                        format!("{key}: "),
                        theme.on_stripe(Token::Header),
                    )];
                    line_spans.extend(highlighted_sql_spans(val, theme, true));
                    push_wrapped_stripe_sql_lines(
                        &mut preview_lines,
                        line_spans,
                        usize::from(preview_w),
                        theme,
                    );
                } else {
                    let value_token = match key {
                        "Query" => Token::Focus,
                        "Origin" | "Tags" => Token::Info,
                        _ => Token::Text,
                    };
                    preview_lines.push(Line::from(vec![
                        Span::styled(format!("{key}: "), theme.style(Token::Header)),
                        Span::styled(val.to_owned(), theme.style(value_token)),
                    ]));
                }
            } else if raw_line.ends_with(':') {
                preview_lines.push(Line::from(Span::styled(
                    raw_line,
                    theme.style(Token::Header),
                )));
            } else if raw_line == "No matching entry selected." {
                preview_lines.push(Line::from(Span::styled(
                    raw_line,
                    theme.style(Token::Muted),
                )));
            } else {
                preview_lines.push(Line::from(Span::styled(raw_line, theme.style(Token::Text))));
            }
        }
        let preview_area = Rect::new(preview_x, content_y, preview_w, content_height);
        Paragraph::new(preview_lines)
            .style(theme.surface(Token::SurfaceOverlay))
            .wrap(Wrap { trim: false })
            .render(preview_area, buf);
    }
}

fn pad_line_to_width(mut spans: Vec<Span<'static>>, width: usize, fill: Style) -> Line<'static> {
    let used: usize = spans.iter().map(|s| display_width(&s.content)).sum();
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), fill));
    }
    Line::from(spans).style(fill)
}

fn push_wrapped_surface_sql_lines(
    preview_lines: &mut Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    preview_w: usize,
    surface_style: Style,
) {
    if preview_w == 0 {
        preview_lines.push(Line::from(spans).style(surface_style));
        return;
    }

    let mut cells: Vec<(char, Style, usize)> = Vec::new();
    for span in spans {
        let patched = span.style.patch(surface_style);
        for ch in span.content.chars() {
            let mut buf = [0u8; 4];
            let w = display_width(ch.encode_utf8(&mut buf));
            cells.push((ch, patched, w));
        }
    }
    if cells.is_empty() {
        preview_lines.push(
            Line::from(Span::styled(" ".repeat(preview_w), surface_style)).style(surface_style),
        );
        return;
    }

    let emit_slice = |out: &mut Vec<Line<'static>>, slice: &[(char, Style, usize)]| {
        let mut line_spans = Vec::new();
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        let mut used = 0usize;
        for &(ch, style, w) in slice {
            if run_style != Some(style) {
                if let Some(prev) = run_style {
                    line_spans.push(Span::styled(std::mem::take(&mut run), prev));
                }
                run_style = Some(style);
            }
            run.push(ch);
            used += w;
        }
        if let Some(prev) = run_style {
            line_spans.push(Span::styled(run, prev));
        }
        if used < preview_w {
            line_spans.push(Span::styled(" ".repeat(preview_w - used), surface_style));
        }
        out.push(Line::from(line_spans).style(surface_style));
    };

    let mut start = 0usize;
    while start < cells.len() {
        let mut width_acc = 0usize;
        let mut end = start;
        let mut last_space = None;
        let mut seen_non_space = false;
        while end < cells.len() {
            let (ch, _, w) = cells[end];
            if width_acc + w > preview_w && end > start {
                break;
            }
            if ch.is_whitespace() {
                if seen_non_space {
                    last_space = Some(end);
                }
            } else {
                seen_non_space = true;
            }
            width_acc += w;
            end += 1;
        }
        if end < cells.len()
            && let Some(space_idx) = last_space
            && space_idx > start
        {
            emit_slice(preview_lines, &cells[start..space_idx]);
            start = space_idx + 1;
        } else {
            emit_slice(preview_lines, &cells[start..end]);
            start = end;
        }
    }
}

fn render_theme_preview(
    choice: ThemeChoice,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let active_theme = &presentation.theme;
    let preview_theme = Theme::from_choice(choice, active_theme.color, active_theme.depth);
    let family = theme_family_label(choice);
    let plain_fallback = !preview_theme.color
        || preview_theme.depth == ColorDepth::None
        || presentation.glyphs.is_ascii();

    if plain_fallback {
        let lines = vec![
            Line::from(vec![
                Span::styled("Theme: ", active_theme.style(Token::Header)),
                Span::styled(choice.label(), active_theme.style(Token::Focus)),
                Span::styled(format!(" ({family})"), active_theme.style(Token::Muted)),
            ]),
            Line::from(Span::styled(
                hairline_rule(presentation).repeat(usize::from(area.width).min(44)),
                active_theme.style(Token::Border),
            )),
            Line::from(Span::styled(
                "Header: Ignatius: demo  [PROD]",
                active_theme.style(Token::Text),
            )),
            Line::from(Span::styled(
                "SQL: SELECT id, total FROM orders WHERE status = 'paid';",
                active_theme.style(Token::Text),
            )),
            Line::from(Span::styled(
                "Row 1: paid | 14500",
                active_theme.style(Token::Text),
            )),
            Line::from(Span::styled(
                "Row 2: [null] | 7350",
                active_theme.style(Token::Muted),
            )),
            Line::from(Span::styled(
                "Swatches: Focus Info Success Warning Danger SyntaxKeyword",
                active_theme.style(Token::Muted),
            )),
        ];
        Paragraph::new(lines)
            .style(active_theme.surface(Token::SurfaceOverlay))
            .wrap(Wrap { trim: false })
            .render(area, buf);
        return;
    }

    let w = usize::from(area.width);
    let overlay_surface = preview_theme.surface(Token::SurfaceOverlay);
    let pane_surface = preview_theme.surface(Token::SurfacePane);
    let stripe_surface = preview_theme.stripe();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(10);

    // 1. Theme name + luminance family pill (`Dark` / `Light` / `High contrast`).
    lines.push(pad_line_to_width(
        vec![
            Span::styled("Theme: ", preview_theme.style(Token::Muted)),
            Span::styled(
                choice.label(),
                preview_theme
                    .style(Token::Header)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" {family} "),
                preview_theme.recessed_pill(Token::Info),
            ),
        ],
        w,
        overlay_surface,
    ));

    // 2. Hairline divider.
    lines.push(pad_line_to_width(
        vec![Span::styled(
            hairline_rule(presentation).repeat(w.min(44)),
            preview_theme.style(Token::Border),
        )],
        w,
        overlay_surface,
    ));

    // 3. Mini header strip (`Ignatius: demo  [PROD]`).
    lines.push(pad_line_to_width(
        vec![
            Span::styled(" Ignatius: ", preview_theme.keycap(Token::Header)),
            Span::styled("demo ", preview_theme.on_stripe(Token::Header)),
            Span::styled(
                " [PROD] ",
                preview_theme.capsule(Token::EnvironmentProduction),
            ),
        ],
        w,
        stripe_surface,
    ));

    // 4. Mini highlighted SQL snippet on SurfacePane.
    let sql = "SELECT id, total FROM orders WHERE status = 'paid';";
    let mut sql_spans = vec![Span::styled(" ", pane_surface)];
    sql_spans.extend(highlighted_sql_spans(sql, &preview_theme, false));
    push_wrapped_surface_sql_lines(&mut lines, sql_spans, w, pane_surface);

    // 5. Mini 2-row result preview (`paid | 14500`, `[null] | 7350`).
    lines.push(pad_line_to_width(
        vec![
            Span::styled(
                " paid ",
                preview_theme.style(Token::Text).patch(pane_surface),
            ),
            Span::styled("|", preview_theme.style(Token::Border).patch(pane_surface)),
            Span::styled(
                " 14500",
                preview_theme.style(Token::SyntaxNumber).patch(pane_surface),
            ),
        ],
        w,
        pane_surface,
    ));
    lines.push(pad_line_to_width(
        vec![
            Span::styled(" ", stripe_surface),
            Span::styled("[null]", preview_theme.on_stripe(Token::NullValue)),
            Span::styled(" ", stripe_surface),
            Span::styled("|", preview_theme.on_stripe(Token::Border)),
            Span::styled(" 7350", preview_theme.on_stripe(Token::SyntaxNumber)),
        ],
        w,
        stripe_surface,
    ));

    // 6. Hairline + 6-token semantic colour swatch bar.
    lines.push(pad_line_to_width(
        vec![Span::styled(
            hairline_rule(presentation).repeat(w.min(44)),
            preview_theme.style(Token::Border),
        )],
        w,
        overlay_surface,
    ));
    let mut swatch_spans = vec![Span::styled(
        "Swatches: ",
        preview_theme.style(Token::Muted),
    )];
    for token in [
        Token::Focus,
        Token::Info,
        Token::Success,
        Token::Warning,
        Token::Danger,
        Token::SyntaxKeyword,
    ] {
        swatch_spans.push(Span::styled(
            "\u{2588}\u{2588}",
            preview_theme.style(token).patch(overlay_surface),
        ));
        swatch_spans.push(Span::styled(" ", overlay_surface));
    }
    lines.push(pad_line_to_width(swatch_spans, w, overlay_surface));
    if lines.len() < usize::from(area.height) {
        lines.push(pad_line_to_width(
            vec![Span::styled(
                truncate_to_width("Focus  Info  Success  Warning  Danger  Keyword", w, true),
                preview_theme.style(Token::Muted),
            )],
            w,
            overlay_surface,
        ));
    }

    Paragraph::new(lines)
        .style(overlay_surface)
        .render(area, buf);
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

    let box_area = overlay_box(area, presentation, (96, 32), 8, OverlayAnchor::Center);
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
    if let Some(info) = model.connection.info() {
        if let Some(timeout) = info.statement_timeout {
            push_detail(
                &mut lines,
                "Statement timeout",
                crate::connection::format_timeout_duration(timeout),
                theme.style(Token::Text),
            );
        }
        if let Some(timeout) = info.lock_timeout {
            push_detail(
                &mut lines,
                "Lock timeout",
                crate::connection::format_timeout_duration(timeout),
                theme.style(Token::Text),
            );
        }
        if let Some(timeout) = info.idle_in_transaction_session_timeout {
            push_detail(
                &mut lines,
                "Idle-in-tx timeout",
                crate::connection::format_timeout_duration(timeout),
                theme.style(Token::Text),
            );
        }
    }

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
