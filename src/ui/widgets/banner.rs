//! Identity mark banner widget and boot reveal state.
//!
//! Pinned art, easing, and sweep reveal functions for the startup banner
//! displayed over the connecting wait.

use crate::ui::layout::Presentation;
use crate::ui::theme::Token;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// Total ticks for the reveal animation (12 ticks * 90 ms = 1.08 s, <= 1.4 s cap).
pub const BANNER_TOTAL_TICKS: u8 = 12;

/// Below this width the layout is too small to render the banner.
pub const TOO_SMALL_WIDTH: u16 = crate::ui::USABLE_COLUMNS;

/// Below this height the layout is too small to render the banner.
pub const TOO_SMALL_HEIGHT: u16 = crate::ui::USABLE_ROWS;

/// Wordmark art for Unicode / Nerd-Font tiers (59 columns wide, 6 lines).
pub const UNICODE_WORDMARK_LINES: [&str; 6] = [
    "██╗ ██████╗ ███╗   ██╗ █████╗ ████████╗██╗██╗   ██╗███████╗",
    "██║██╔════╝ ████╗  ██║██╔══██╗╚══██╔══╝██║██║   ██║██╔════╝",
    "██║██║  ███╗██╔██╗ ██║███████║   ██║   ██║██║   ██║███████╗",
    "██║██║   ██║██║╚██╗██║██╔══██║   ██║   ██║██║   ██║╚════██║",
    "██║╚██████╔╝██║ ╚████║██║  ██║   ██║   ██║╚██████╔╝███████║",
    "╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝   ╚═╝   ╚═╝ ╚═════╝ ╚══════╝",
];

/// Underline row for the Unicode / Nerd-Font wordmark.
pub const UNICODE_WORDMARK_UNDERLINE: &str =
    "▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔";

/// Tagline row for the Unicode / Nerd-Font wordmark.
pub const UNICODE_WORDMARK_TAGLINE: &str =
    "       the PostgreSQL client you can point at production";

/// Monogram art for Unicode / Nerd-Font tiers (< 64 columns, 6 lines).
pub const UNICODE_MONOGRAM_LINES: [&str; 6] = [
    "██╗ ██████╗",
    "██║██╔════╝",
    "██║██║ ███╗   ignatius",
    "██║██║  ██║",
    "██║╚█████╔╝",
    "╚═╝ ╚════╝",
];

/// Wordmark art for ASCII tier (48 columns wide, 4 lines).
pub const ASCII_WORDMARK_LINES: [&str; 4] = [
    " ___   ___  _  _    _    _____  ___  _   _  ___",
    "|_ _| / __|| \\| |  /_\\  |_   _||_ _|| | | |/ __|",
    " | | | (_ || .` | / _ \\   | |   | | | |_| |\\__ \\",
    "|___| \\___||_|\\_|/_/ \\_\\  |_|  |___| \\___/ |___/",
];

/// Underline row for the ASCII wordmark.
pub const ASCII_WORDMARK_UNDERLINE: &str = "------------------------------------------------";

/// Tagline row for the ASCII wordmark.
pub const ASCII_WORDMARK_TAGLINE: &str = "the PostgreSQL client you can point at production";

/// Monogram fallback for the ASCII tier.
pub const ASCII_MONOGRAM: &str = "ignatius";

/// Deterministic decode character pool for the Unicode tier.
pub const UNICODE_DECODE_POOL: [char; 4] = ['░', '▒', '▓', '█'];

/// Deterministic decode character pool for the ASCII tier.
pub const ASCII_DECODE_POOL: [char; 8] = ['/', '\\', '|', '_', '-', '=', '+', '*'];

/// Sweep reveal state machine for the startup banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BannerState {
    /// In the process of revealing letterforms across ticks 0..11.
    Revealing { tick: u8 },
    /// Final revealed state with art, underline meter, and tagline shown.
    #[default]
    Final,
}

impl BannerState {
    /// Initialises banner state. If reduced motion is requested, starts at `Final`.
    #[must_use]
    pub const fn new(reduced_motion: bool) -> Self {
        if reduced_motion {
            Self::Final
        } else {
            Self::Revealing { tick: 0 }
        }
    }

    /// Whether the banner has reached its final state.
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Final)
    }

    /// Current reveal tick (0..=11 for Revealing, 12 for Final).
    #[must_use]
    pub const fn tick(self) -> u8 {
        match self {
            Self::Revealing { tick } => tick,
            Self::Final => BANNER_TOTAL_TICKS,
        }
    }

    /// Advances the reveal animation by one tick.
    pub fn advance(&mut self) {
        match self {
            Self::Revealing { tick } => {
                let next = *tick + 1;
                if next >= BANNER_TOTAL_TICKS {
                    *self = Self::Final;
                } else {
                    *tick = next;
                }
            }
            Self::Final => {}
        }
    }

    /// Immediately jumps to the final revealed state.
    pub fn finish(&mut self) {
        *self = Self::Final;
    }
}

/// Computes the sweep front column for a given banner width and state.
///
/// Uses the ease function: ease(t) = 1 - (1 - t)^2 where t = tick / 12.
#[must_use]
pub fn front_col(width: usize, state: BannerState) -> usize {
    match state {
        BannerState::Final => width,
        BannerState::Revealing { tick } => {
            if tick >= BANNER_TOTAL_TICKS {
                width
            } else {
                let t = f64::from(tick) / f64::from(BANNER_TOTAL_TICKS);
                let ease = 1.0 - (1.0 - t).powi(2);
                ((width as f64 * ease).round() as usize).min(width)
            }
        }
    }
}

/// Renders the banner lines according to the presentation tier and active state.
#[must_use]
pub fn render_banner_lines(
    state: BannerState,
    presentation: &Presentation,
    cols: u16,
) -> Vec<Line<'static>> {
    if cols < TOO_SMALL_WIDTH {
        return Vec::new();
    }

    let is_wordmark = cols >= 64;
    let is_ascii = presentation.glyphs.is_ascii();
    let theme = &presentation.theme;

    if is_wordmark {
        if is_ascii {
            render_ascii_wordmark(state, presentation)
        } else {
            render_unicode_wordmark(state, presentation)
        }
    } else if is_ascii {
        let width = ASCII_MONOGRAM.len();
        let ramp = theme.banner_ramp(width);
        let front = front_col(width, state);
        let art_chars: Vec<char> = ASCII_MONOGRAM.chars().collect();
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            let final_ch = art_chars[col];
            let ch = if state.is_final() || col + 3 < front {
                final_ch
            } else if col < front {
                ASCII_DECODE_POOL[(col + usize::from(state.tick())) % ASCII_DECODE_POOL.len()]
            } else {
                ' '
            };
            let style = if ch != ' ' {
                ramp[col]
            } else {
                Style::default()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        vec![Line::from(spans)]
    } else {
        let width = 22;
        let ramp = theme.banner_ramp(width);
        let front = front_col(width, state);
        let mut lines = Vec::with_capacity(UNICODE_MONOGRAM_LINES.len());
        for (row, &art_line) in UNICODE_MONOGRAM_LINES.iter().enumerate() {
            let art_chars: Vec<char> = art_line.chars().collect();
            let mut spans = Vec::with_capacity(art_chars.len());
            for (col, &final_ch) in art_chars.iter().enumerate() {
                let ch = if state.is_final() || col + 3 < front {
                    final_ch
                } else if col < front {
                    if final_ch != ' ' {
                        UNICODE_DECODE_POOL
                            [(col + row + usize::from(state.tick())) % UNICODE_DECODE_POOL.len()]
                    } else {
                        ' '
                    }
                } else {
                    ' '
                };
                let style = if ch != ' ' {
                    ramp[col]
                } else {
                    Style::default()
                };
                spans.push(Span::styled(ch.to_string(), style));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

fn render_unicode_wordmark(state: BannerState, presentation: &Presentation) -> Vec<Line<'static>> {
    const WIDTH: usize = 59;
    let theme = &presentation.theme;
    let ramp = theme.banner_ramp(WIDTH);
    let front = front_col(WIDTH, state);
    let mut lines = Vec::with_capacity(8);

    for (row, &art_line) in UNICODE_WORDMARK_LINES.iter().enumerate() {
        let art_chars: Vec<char> = art_line.chars().collect();
        let mut spans = Vec::with_capacity(WIDTH);
        for col in 0..WIDTH {
            let final_ch = art_chars.get(col).copied().unwrap_or(' ');
            let ch = if state.is_final() || col + 3 < front {
                final_ch
            } else if col < front {
                if final_ch != ' ' {
                    UNICODE_DECODE_POOL
                        [(col + row + usize::from(state.tick())) % UNICODE_DECODE_POOL.len()]
                } else {
                    ' '
                }
            } else {
                ' '
            };
            let style = if ch != ' ' {
                ramp[col]
            } else {
                Style::default()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }

    // Underline row (row 6)
    let ul_len = UNICODE_WORDMARK_UNDERLINE.chars().count();
    let mut spans = Vec::with_capacity(ul_len);
    for (col, &style) in ramp.iter().enumerate().take(ul_len) {
        if col < front {
            spans.push(Span::styled("▔", style));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    lines.push(Line::from(spans));

    // Tagline row (row 7)
    if state.is_final() {
        lines.push(Line::styled(
            UNICODE_WORDMARK_TAGLINE,
            theme.style(Token::Muted),
        ));
    } else {
        lines.push(Line::raw(" ".repeat(UNICODE_WORDMARK_TAGLINE.len())));
    }

    lines
}

fn render_ascii_wordmark(state: BannerState, presentation: &Presentation) -> Vec<Line<'static>> {
    const WIDTH: usize = 48;
    let theme = &presentation.theme;
    let ramp = theme.banner_ramp(WIDTH);
    let front = front_col(WIDTH, state);
    let mut lines = Vec::with_capacity(6);

    for (row, &art_line) in ASCII_WORDMARK_LINES.iter().enumerate() {
        let art_chars: Vec<char> = art_line.chars().collect();
        let line_len = art_chars.len();
        let mut spans = Vec::with_capacity(line_len);
        for col in 0..line_len {
            let final_ch = art_chars[col];
            let ch = if state.is_final() || col + 3 < front {
                final_ch
            } else if col < front {
                if final_ch != ' ' {
                    ASCII_DECODE_POOL
                        [(col + row + usize::from(state.tick())) % ASCII_DECODE_POOL.len()]
                } else {
                    ' '
                }
            } else {
                ' '
            };
            let style = if ch != ' ' {
                ramp[col]
            } else {
                Style::default()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }

    // Underline row (row 4)
    let ul_len = ASCII_WORDMARK_UNDERLINE.chars().count();
    let mut spans = Vec::with_capacity(ul_len);
    for (col, &style) in ramp.iter().enumerate().take(ul_len) {
        if col < front {
            spans.push(Span::styled("-", style));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    lines.push(Line::from(spans));

    // Tagline row (row 5)
    if state.is_final() {
        lines.push(Line::styled(
            ASCII_WORDMARK_TAGLINE,
            theme.style(Token::Muted),
        ));
    } else {
        lines.push(Line::raw(" ".repeat(ASCII_WORDMARK_TAGLINE.len())));
    }

    lines
}

/// Renders the identity mark banner centered within the given area,
/// optionally drawing the connecting step line below it.
pub fn render_banner(
    state: BannerState,
    step_line: Option<&str>,
    presentation: &Presentation,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width < TOO_SMALL_WIDTH || area.height < TOO_SMALL_HEIGHT {
        return;
    }
    // Use the full terminal width when available so a 64+ column terminal
    // with a 1-cell border margin still selects the wordmark tier.
    let selection_cols = buf.area().width.max(area.width);
    let lines = render_banner_lines(state, presentation, selection_cols);
    if lines.is_empty() {
        return;
    }

    let block_height = lines.len() as u16;
    let block_width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;

    let start_x = area.x + area.width.saturating_sub(block_width) / 2;
    let start_y = area.y + area.height.saturating_sub(block_height) / 2;

    let max_w = area.right().saturating_sub(start_x);
    let max_h = area.bottom().saturating_sub(start_y);
    let banner_rect = Rect::new(
        start_x,
        start_y,
        block_width.min(max_w),
        block_height.min(max_h),
    );
    Paragraph::new(lines).render(banner_rect, buf);

    if let Some(step) = step_line {
        let step_y = start_y + block_height;
        if step_y < area.bottom() {
            let step_len = crate::query::value::display_width(step) as u16;
            let step_x = area.x + area.width.saturating_sub(step_len) / 2;
            let step_w = step_len.min(area.right().saturating_sub(step_x));
            let step_rect = Rect::new(step_x, step_y, step_w, 1);
            Paragraph::new(Line::styled(step, presentation.theme.style(Token::Muted)))
                .render(step_rect, buf);
        }
    }
}

#[cfg(test)]
mod tests;
