//! Semantic theme tokens.
//!
//! Widgets ask for meaning - `Token::Danger`, `Token::EnvironmentProduction` -
//! never for a colour. Swapping a theme therefore cannot change what a widget
//! means, and a token that a theme forgets is a compile error rather than an
//! invisible element.
//!
//! Colour is always supplementary. Every state that a token colours is also
//! stated in words somewhere on screen, so the interface survives `NO_COLOR`, a
//! monochrome terminal, and colour-blind vision.

use crate::config::ThemeChoice;
use ratatui::style::{Color, Modifier, Style};

/// A semantic role that the interface needs to express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    /// Ordinary background.
    Surface,
    /// Background of a raised or striped area.
    SurfaceAlt,
    /// Background of a pane interior, one elevation above the base.
    SurfacePane,
    /// Background of an overlay, one elevation above a pane.
    SurfaceOverlay,
    /// Primary readable text.
    Text,
    /// Secondary text: hints, units, inactive labels.
    Muted,
    /// Panel borders and rules.
    Border,
    /// The focused pane's border and title.
    Focus,
    /// A completed, successful outcome.
    Success,
    /// Something worth attention that is not an error.
    Warning,
    /// An error or a destructive action.
    Danger,
    /// Neutral informational emphasis.
    Info,
    /// A connection explicitly classified as production.
    EnvironmentProduction,
    /// A connection classified as anything other than production.
    EnvironmentNonProduction,
    /// An open transaction.
    TransactionActive,
    /// A transaction that has failed and needs a rollback.
    TransactionFailed,
    /// A SQL NULL, which must be distinguishable from an empty string.
    NullValue,
    /// The selected row or cell.
    Selection,
    /// Column headers.
    Header,
    /// A SQL keyword in the editor.
    SyntaxKeyword,
    /// A string or dollar-quoted literal in the editor.
    SyntaxLiteral,
    /// A numeric literal in the editor.
    SyntaxNumber,
    /// A comment in the editor.
    SyntaxComment,
    /// A quoted identifier or parameter placeholder in the editor.
    SyntaxIdentifier,
}

impl Token {
    /// Every token. Used to prove each theme defines all of them.
    pub const ALL: &'static [Self] = &[
        Self::Surface,
        Self::SurfaceAlt,
        Self::SurfacePane,
        Self::SurfaceOverlay,
        Self::Text,
        Self::Muted,
        Self::Border,
        Self::Focus,
        Self::Success,
        Self::Warning,
        Self::Danger,
        Self::Info,
        Self::EnvironmentProduction,
        Self::EnvironmentNonProduction,
        Self::TransactionActive,
        Self::TransactionFailed,
        Self::NullValue,
        Self::Selection,
        Self::Header,
        Self::SyntaxKeyword,
        Self::SyntaxLiteral,
        Self::SyntaxNumber,
        Self::SyntaxComment,
        Self::SyntaxIdentifier,
    ];
}

/// An RGB colour, kept as components so contrast can be checked in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl From<Rgb> for Color {
    fn from(value: Rgb) -> Self {
        Self::Rgb(value.0, value.1, value.2)
    }
}

impl Rgb {
    /// WCAG relative luminance.
    #[must_use]
    pub fn luminance(self) -> f64 {
        fn channel(value: u8) -> f64 {
            let v = f64::from(value) / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.0) + 0.7152 * channel(self.1) + 0.0722 * channel(self.2)
    }

    /// WCAG contrast ratio against another colour, from 1.0 to 21.0.
    #[must_use]
    pub fn contrast(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (light, dark) = if a > b { (a, b) } else { (b, a) };
        (light + 0.05) / (dark + 0.05)
    }
}

/// A resolved theme: a palette plus whether colour may be emitted at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Which built-in palette this is.
    pub choice: ThemeChoice,
    /// When false, tokens resolve to modifiers only and no colour is emitted.
    pub color: bool,
    /// How many colours the terminal can actually show.
    pub depth: ColorDepth,
}

/// How many colours the terminal can actually show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorDepth {
    /// 24-bit colour, emitted exactly as chosen.
    #[default]
    TrueColor,
    /// The 256-colour indexed palette, reached through the quantizer.
    Indexed256,
    /// The 16 basic ANSI colours, reached through the fixed family table.
    Basic16,
    /// No colour of any kind.
    None,
}

impl ColorDepth {
    /// Whether the depth allows a painted surface, as opposed to plain cells
    /// whose background belongs to the terminal.
    #[must_use]
    pub const fn paints_surfaces(self) -> bool {
        matches!(self, Self::TrueColor | Self::Indexed256)
    }
}

/// Where the active colour depth came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthSource {
    /// No override and no environment hint, so the documented fallback chose.
    #[default]
    Default,
    /// The `--color-depth` flag.
    Flag,
    /// The `ui.color-depth` configuration key.
    Configuration,
    /// The `COLORTERM` environment variable.
    Colorterm,
    /// The `TERM` environment variable.
    Term,
    /// `NO_COLOR`, `TERM=dumb`, `--plain` or colour resolved off.
    ForcedOff,
}

impl DepthSource {
    /// The exact label `doctor` reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Flag => "flag",
            Self::Configuration => "configuration",
            Self::Colorterm => "COLORTERM",
            Self::Term => "TERM",
            Self::ForcedOff => "forced off",
        }
    }
}

impl Theme {
    /// Builds a theme for a palette choice and colour availability.
    ///
    /// The depth defaults to full truecolour; the runtime narrows it with
    /// [`Theme::with_depth`] once detection has run.
    #[must_use]
    pub const fn new(choice: ThemeChoice, color: bool) -> Self {
        Self {
            choice,
            color,
            depth: ColorDepth::TrueColor,
        }
    }

    /// Sets the colour depth this theme emits at.
    #[must_use]
    pub const fn with_depth(mut self, depth: ColorDepth) -> Self {
        self.depth = depth;
        self
    }

    /// The colour emitted for a token at the active depth, or `None` when
    /// colour must not be emitted at all.
    ///
    /// This is the single depth-aware conversion point: every style method
    /// goes through it rather than converting `Rgb` directly.
    #[must_use]
    pub fn color(&self, token: Token) -> Option<Color> {
        if !self.color || self.depth == ColorDepth::None {
            return None;
        }
        Some(self.foreground(token))
    }

    /// The colour for a token at the active depth, assuming colour is allowed.
    fn foreground(&self, token: Token) -> Color {
        match self.depth {
            // `None` is filtered by `color`, but converting as truecolour
            // keeps this total rather than panicking on an internal mistake.
            ColorDepth::TrueColor | ColorDepth::None => self.rgb(token).into(),
            ColorDepth::Indexed256 => Color::Indexed(quantize_256(self.rgb(token))),
            ColorDepth::Basic16 => Color::Indexed(basic_16(token, self)),
        }
    }

    /// A fill for one of the painted elevation surfaces.
    ///
    /// At 16-colour depth and below this is empty, so a caller can apply it
    /// unconditionally and get the documented unpainted-surface behaviour.
    #[must_use]
    pub fn surface(&self, token: Token) -> Style {
        if self.depth.paints_surfaces()
            && let Some(colour) = self.color(token)
        {
            return Style::default().bg(colour);
        }
        Style::default()
    }

    /// The palette colour for a token, ignoring whether colour is enabled.
    #[must_use]
    pub const fn rgb(&self, token: Token) -> Rgb {
        match self.choice {
            ThemeChoice::Dark => dark(token),
            ThemeChoice::Light => light(token),
            ThemeChoice::HighContrast => high_contrast(token),
        }
    }

    /// Background colour of the interface.
    #[must_use]
    pub const fn background(&self) -> Rgb {
        self.rgb(Token::Surface)
    }

    /// The style to apply for a token.
    ///
    /// With colour disabled, emphasis is carried by modifiers so focus and
    /// selection remain visible in a monochrome terminal. At 16-colour depth
    /// the foreground comes from the fixed family table and no background is
    /// painted, so elevation follows the colour-off rules.
    #[must_use]
    pub fn style(&self, token: Token) -> Style {
        if !self.color || self.depth == ColorDepth::None {
            return modifier_only(token);
        }
        if self.depth == ColorDepth::Basic16 {
            return match token {
                // A filled selection background is an elevation-style paint:
                // at 16 colours selection falls back to reversed text.
                Token::Selection => modifier_only(token),
                _ => modifier_only(token).fg(self.foreground(token)),
            };
        }
        let style = Style::default().fg(self.foreground(token));
        match token {
            Token::Selection => Style::default()
                .bg(self.foreground(Token::Selection))
                .fg(self.foreground(Token::Surface)),
            Token::Header | Token::Focus | Token::EnvironmentProduction => {
                style.add_modifier(Modifier::BOLD)
            }
            Token::Muted | Token::NullValue | Token::SyntaxComment => {
                style.add_modifier(Modifier::DIM)
            }
            _ => style,
        }
    }
}

/// The colour-off emphasis rules, shared by `ColorDepth::None` and `Basic16`.
fn modifier_only(token: Token) -> Style {
    match token {
        Token::Focus | Token::Header => Style::default().add_modifier(Modifier::BOLD),
        Token::Selection => Style::default().add_modifier(Modifier::REVERSED),
        Token::Danger | Token::EnvironmentProduction | Token::TransactionFailed => {
            Style::default().add_modifier(Modifier::BOLD)
        }
        Token::Muted | Token::NullValue | Token::SyntaxComment => {
            Style::default().add_modifier(Modifier::DIM)
        }
        // Keywords stay emphasised without colour. The rest of the
        // syntax tokens are plain text, because a buffer where every
        // second word is bold is harder to read, not easier.
        Token::SyntaxKeyword => Style::default().add_modifier(Modifier::BOLD),
        _ => Style::default(),
    }
}

const fn dark(token: Token) -> Rgb {
    match token {
        Token::Surface => Rgb(0x18, 0x18, 0x25),
        Token::SurfaceAlt => Rgb(0x1a, 0x1f, 0x26),
        Token::SurfacePane => Rgb(0x1e, 0x1e, 0x2e),
        Token::SurfaceOverlay => Rgb(0x31, 0x32, 0x44),
        Token::Text => Rgb(0xe6, 0xe9, 0xef),
        Token::Muted => Rgb(0x9a, 0xa4, 0xb2),
        Token::Border => Rgb(0x39, 0x42, 0x4f),
        Token::Focus => Rgb(0x82, 0xaa, 0xff),
        Token::Success => Rgb(0x77, 0xd4, 0xa3),
        Token::Warning => Rgb(0xe6, 0xb4, 0x50),
        Token::Danger | Token::TransactionFailed => Rgb(0xff, 0x8f, 0x8f),
        Token::Info => Rgb(0x8a, 0xd5, 0xf0),
        Token::EnvironmentProduction => Rgb(0xff, 0x9e, 0x9e),
        Token::EnvironmentNonProduction => Rgb(0x9a, 0xa4, 0xb2),
        Token::TransactionActive => Rgb(0xe6, 0xb4, 0x50),
        Token::NullValue => Rgb(0x8b, 0x93, 0xa1),
        Token::Selection => Rgb(0x9a, 0xb8, 0xe8),
        Token::Header => Rgb(0xc8, 0xd3, 0xe3),
        Token::SyntaxKeyword => Rgb(0xc9, 0x9c, 0xf0),
        Token::SyntaxLiteral => Rgb(0x9e, 0xd6, 0x7e),
        Token::SyntaxNumber => Rgb(0xf0, 0xc0, 0x74),
        Token::SyntaxComment => Rgb(0x8b, 0x95, 0xa6),
        Token::SyntaxIdentifier => Rgb(0x7d, 0xcf, 0xff),
    }
}

const fn light(token: Token) -> Rgb {
    // Designed for a light terminal rather than inverted from the dark palette:
    // hues are darkened and desaturated so they stay legible on near-white.
    match token {
        Token::Surface => Rgb(0xe6, 0xe9, 0xef),
        Token::SurfaceAlt => Rgb(0xef, 0xf1, 0xf4),
        Token::SurfacePane => Rgb(0xef, 0xf1, 0xf5),
        Token::SurfaceOverlay => Rgb(0xdc, 0xe0, 0xe8),
        Token::Text => Rgb(0x1a, 0x1e, 0x26),
        Token::Muted => Rgb(0x55, 0x5e, 0x6b),
        Token::Border => Rgb(0xc4, 0xca, 0xd3),
        Token::Focus => Rgb(0x1e, 0x4f, 0xbd),
        Token::Success => Rgb(0x0e, 0x63, 0x3e),
        Token::Warning => Rgb(0x7a, 0x4d, 0x00),
        Token::Danger | Token::TransactionFailed => Rgb(0xa8, 0x18, 0x12),
        Token::Info => Rgb(0x0a, 0x55, 0x74),
        Token::EnvironmentProduction => Rgb(0x8f, 0x0f, 0x1a),
        Token::EnvironmentNonProduction => Rgb(0x55, 0x5e, 0x6b),
        Token::TransactionActive => Rgb(0x7a, 0x4d, 0x00),
        Token::NullValue => Rgb(0x60, 0x6a, 0x78),
        Token::Selection => Rgb(0x33, 0x5b, 0xa8),
        Token::Header => Rgb(0x2b, 0x32, 0x3d),
        Token::SyntaxKeyword => Rgb(0x6b, 0x21, 0xa8),
        Token::SyntaxLiteral => Rgb(0x0e, 0x63, 0x3e),
        Token::SyntaxNumber => Rgb(0x7a, 0x4d, 0x00),
        Token::SyntaxComment => Rgb(0x5d, 0x66, 0x73),
        Token::SyntaxIdentifier => Rgb(0x0a, 0x55, 0x74),
    }
}

const fn high_contrast(token: Token) -> Rgb {
    match token {
        Token::Surface | Token::SurfaceAlt | Token::SurfacePane => Rgb(0x00, 0x00, 0x00),
        Token::SurfaceOverlay => Rgb(0x10, 0x10, 0x10),
        Token::Text | Token::Border | Token::Header => Rgb(0xff, 0xff, 0xff),
        Token::Muted | Token::NullValue | Token::EnvironmentNonProduction => Rgb(0xd0, 0xd0, 0xd0),
        Token::Focus | Token::Warning | Token::TransactionActive => Rgb(0xff, 0xff, 0x00),
        Token::Success => Rgb(0x5c, 0xff, 0xa8),
        Token::Danger | Token::TransactionFailed | Token::EnvironmentProduction => {
            Rgb(0xff, 0x8c, 0x8c)
        }
        Token::Info => Rgb(0x7d, 0xf0, 0xff),
        Token::Selection => Rgb(0xff, 0xff, 0xff),
        Token::SyntaxKeyword => Rgb(0xb5, 0xd0, 0xff),
        Token::SyntaxLiteral => Rgb(0x5c, 0xff, 0xa8),
        Token::SyntaxNumber => Rgb(0xff, 0xff, 0x00),
        Token::SyntaxComment => Rgb(0xd0, 0xd0, 0xd0),
        Token::SyntaxIdentifier => Rgb(0x7d, 0xf0, 0xff),
    }
}

impl Theme {
    /// A filled capsule: the token as a background, the surface as the text.
    ///
    /// Used for the markers that must be impossible to miss, such as the
    /// environment classification. Contrast is the same pair as the token's own
    /// foreground contrast, which the palette tests already enforce.
    #[must_use]
    pub fn capsule(&self, token: Token) -> Style {
        if !self.depth.paints_surfaces() {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        match self.color(token) {
            Some(fill) => Style::default()
                .bg(fill)
                .fg(self.foreground(Token::Surface))
                .add_modifier(Modifier::BOLD),
            None => Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    /// The background of an alternating result row.
    ///
    /// Striping is a reading aid across wide rows. With colour off it is absent
    /// rather than faked, because a modifier applied to every second row would
    /// be noise rather than help. At 16-colour depth it is absent for the same
    /// reason as the elevation surfaces.
    #[must_use]
    pub fn stripe(&self) -> Style {
        self.surface(Token::SurfaceAlt)
    }

    /// Text drawn on a striped row.
    #[must_use]
    pub fn on_stripe(&self, token: Token) -> Style {
        let mut style = self.style(token);
        if let Some(background) = self.surface(Token::SurfaceAlt).bg {
            style = style.bg(background);
        }
        style
    }
}

/// The fixed ANSI family each token belongs to, per `contracts/presentation.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Grey,
    Muted,
}

impl Family {
    const fn normal(self) -> u8 {
        match self {
            Self::Red => 1,
            Self::Green => 2,
            Self::Yellow => 3,
            Self::Blue => 4,
            Self::Magenta => 5,
            Self::Cyan => 6,
            Self::Grey => 7,
            Self::Muted => 8,
        }
    }

    const fn bright(self) -> u8 {
        match self {
            Self::Red => 9,
            Self::Green => 10,
            Self::Yellow => 11,
            Self::Blue => 12,
            Self::Magenta => 13,
            Self::Cyan => 14,
            Self::Grey => 15,
            // The muted family has no bright member.
            Self::Muted => 8,
        }
    }

    const fn has_bright(self) -> bool {
        !matches!(self, Self::Muted)
    }
}

/// The contract's fixed family for a token.
///
/// Syntax tokens take the family their hue belongs to; the surface tokens are
/// backgrounds and are never foregrounded, so they take the muted index rather
/// than ever resolving to a bright value if a caller misuses them.
fn family(token: Token) -> Family {
    match token {
        Token::Danger | Token::EnvironmentProduction | Token::TransactionFailed => Family::Red,
        Token::Success | Token::SyntaxLiteral => Family::Green,
        Token::Warning | Token::TransactionActive | Token::SyntaxNumber => Family::Yellow,
        Token::Focus | Token::Selection => Family::Blue,
        Token::SyntaxKeyword => Family::Magenta,
        Token::Info | Token::SyntaxIdentifier => Family::Cyan,
        Token::Text | Token::Header | Token::Border => Family::Grey,
        Token::Muted
        | Token::EnvironmentNonProduction
        | Token::NullValue
        | Token::SyntaxComment
        | Token::Surface
        | Token::SurfaceAlt
        | Token::SurfacePane
        | Token::SurfaceOverlay => Family::Muted,
    }
}

/// The fixed 16-colour lookup for a token: its contract family, brightened
/// when the token's truecolour relative luminance is at least 0.5.
fn basic_16(token: Token, theme: &Theme) -> u8 {
    let family = family(token);
    if family.has_bright() && theme.rgb(token).luminance() >= 0.5 {
        family.bright()
    } else {
        family.normal()
    }
}

/// Maps a truecolour to the nearest xterm-256 index.
///
/// Candidates are the 6x6x6 colour cube (indices 16-231, component levels
/// `[0, 95, 135, 175, 215, 255]`) and the grey ramp (indices 232-255, values
/// `8, 18, .. 238`). The result is the candidate with the smallest squared RGB
/// distance; ties resolve to the lower index. Indices 0-15 are never produced
/// because they are terminal-redefinable and unpredictable.
#[must_use]
pub fn quantize_256(rgb: Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let mut best_index = 16u8;
    let mut best_distance = u32::MAX;
    for (r, level_r) in LEVELS.iter().enumerate() {
        for (g, level_g) in LEVELS.iter().enumerate() {
            for (b, level_b) in LEVELS.iter().enumerate() {
                let distance = distance_squared(rgb, Rgb(*level_r, *level_g, *level_b));
                if distance < best_distance {
                    best_distance = distance;
                    best_index = 16 + (r as u8) * 36 + (g as u8) * 6 + (b as u8);
                }
            }
        }
    }
    for step in 0..24u8 {
        let value = 8 + 10 * step;
        let distance = distance_squared(rgb, Rgb(value, value, value));
        if distance < best_distance {
            best_distance = distance;
            best_index = 232 + step;
        }
    }
    best_index
}

/// Squared Euclidean distance between two RGB colours.
fn distance_squared(a: Rgb, b: Rgb) -> u32 {
    let dr = i32::from(a.0) - i32::from(b.0);
    let dg = i32::from(a.1) - i32::from(b.1);
    let db = i32::from(a.2) - i32::from(b.2);
    (dr * dr + dg * dg + db * db) as u32
}

#[cfg(test)]
mod tests;
