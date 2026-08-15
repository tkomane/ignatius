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
}

impl Theme {
    /// Builds a theme for a palette choice and colour availability.
    #[must_use]
    pub const fn new(choice: ThemeChoice, color: bool) -> Self {
        Self { choice, color }
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
    /// selection remain visible in a monochrome terminal.
    #[must_use]
    pub fn style(&self, token: Token) -> Style {
        if !self.color {
            return match token {
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
            };
        }
        let style = Style::default().fg(self.rgb(token).into());
        match token {
            Token::Selection => Style::default()
                .bg(self.rgb(Token::Selection).into())
                .fg(self.rgb(Token::Surface).into()),
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

const fn dark(token: Token) -> Rgb {
    match token {
        Token::Surface => Rgb(0x12, 0x15, 0x1a),
        Token::SurfaceAlt => Rgb(0x1a, 0x1f, 0x26),
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
        Token::Surface => Rgb(0xfc, 0xfc, 0xfd),
        Token::SurfaceAlt => Rgb(0xef, 0xf1, 0xf4),
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
        Token::Surface | Token::SurfaceAlt => Rgb(0x00, 0x00, 0x00),
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
        if !self.color {
            return Style::default().add_modifier(Modifier::REVERSED);
        }
        Style::default()
            .bg(self.rgb(token).into())
            .fg(self.rgb(Token::Surface).into())
            .add_modifier(Modifier::BOLD)
    }

    /// The background of an alternating result row.
    ///
    /// Striping is a reading aid across wide rows. With colour off it is absent
    /// rather than faked, because a modifier applied to every second row would
    /// be noise rather than help.
    #[must_use]
    pub fn stripe(&self) -> Style {
        if !self.color {
            return Style::default();
        }
        Style::default().bg(self.rgb(Token::SurfaceAlt).into())
    }

    /// Text drawn on a striped row.
    #[must_use]
    pub fn on_stripe(&self, token: Token) -> Style {
        if !self.color {
            return self.style(token);
        }
        self.style(token).bg(self.rgb(Token::SurfaceAlt).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READABLE: f64 = 4.5;
    const LARGE_OR_SECONDARY: f64 = 3.0;
    const HIGH_CONTRAST_MINIMUM: f64 = 7.0;

    fn themes() -> [Theme; 3] {
        [
            Theme::new(ThemeChoice::Dark, true),
            Theme::new(ThemeChoice::Light, true),
            Theme::new(ThemeChoice::HighContrast, true),
        ]
    }

    #[test]
    fn every_theme_defines_every_token() {
        for theme in themes() {
            for token in Token::ALL {
                let rgb = theme.rgb(*token);
                // A token resolving to the surface colour would be invisible.
                if !matches!(token, Token::Surface | Token::SurfaceAlt) {
                    assert_ne!(
                        rgb,
                        theme.background(),
                        "{:?}/{token:?} is invisible against its own background",
                        theme.choice
                    );
                }
            }
        }
    }

    #[test]
    fn primary_text_is_readable_on_its_surface() {
        for theme in themes() {
            let ratio = theme.rgb(Token::Text).contrast(theme.background());
            assert!(
                ratio >= READABLE,
                "{:?}: text contrast {ratio:.2} is below {READABLE}",
                theme.choice
            );
        }
    }

    #[test]
    fn semantic_states_stay_legible_on_their_surface() {
        for theme in themes() {
            for token in [
                Token::Muted,
                Token::Success,
                Token::Warning,
                Token::Danger,
                Token::Info,
                Token::EnvironmentProduction,
                Token::TransactionFailed,
                Token::NullValue,
                Token::Header,
            ] {
                let ratio = theme.rgb(token).contrast(theme.background());
                assert!(
                    ratio >= LARGE_OR_SECONDARY,
                    "{:?}/{token:?} contrast {ratio:.2} is below {LARGE_OR_SECONDARY}",
                    theme.choice
                );
            }
        }
    }

    #[test]
    fn text_stays_readable_on_a_striped_row() {
        // Striping alternates the background, so every contrast guarantee has to
        // hold against the alternate surface as well as the main one.
        for theme in themes() {
            let stripe = theme.rgb(Token::SurfaceAlt);
            assert!(
                theme.rgb(Token::Text).contrast(stripe) >= READABLE,
                "{:?}: text on a striped row is {:.2}",
                theme.choice,
                theme.rgb(Token::Text).contrast(stripe)
            );
            for token in [
                Token::Muted,
                Token::NullValue,
                Token::Danger,
                Token::Success,
            ] {
                assert!(
                    theme.rgb(token).contrast(stripe) >= LARGE_OR_SECONDARY,
                    "{:?}/{token:?} on a striped row is {:.2}",
                    theme.choice,
                    theme.rgb(token).contrast(stripe)
                );
            }
        }
    }

    #[test]
    fn a_capsule_is_legible_and_falls_back_to_reversed_text() {
        for theme in themes() {
            let style = theme.capsule(Token::EnvironmentProduction);
            assert!(
                style.bg.is_some(),
                "{:?}: a capsule needs a fill",
                theme.choice
            );
            assert!(style.fg.is_some());
            // The pair is token against surface, which the palette tests already
            // hold to a threshold; assert it here so the capsule cannot drift.
            let ratio = theme
                .rgb(Token::EnvironmentProduction)
                .contrast(theme.rgb(Token::Surface));
            assert!(
                ratio >= LARGE_OR_SECONDARY,
                "{:?}: capsule contrast {ratio:.2}",
                theme.choice
            );
        }
        let plain = Theme::new(ThemeChoice::Dark, false);
        assert!(plain.capsule(Token::EnvironmentProduction).bg.is_none());
        assert!(
            plain
                .capsule(Token::EnvironmentProduction)
                .add_modifier
                .contains(Modifier::REVERSED),
            "without colour a capsule must still stand out"
        );
        assert!(
            plain.stripe().bg.is_none(),
            "striping is dropped, not faked"
        );
    }

    #[test]
    fn the_high_contrast_theme_actually_earns_its_name() {
        let theme = Theme::new(ThemeChoice::HighContrast, true);
        for token in Token::ALL {
            if matches!(token, Token::Surface | Token::SurfaceAlt) {
                continue;
            }
            let ratio = theme.rgb(*token).contrast(theme.background());
            assert!(
                ratio >= HIGH_CONTRAST_MINIMUM,
                "{token:?} contrast {ratio:.2} is below {HIGH_CONTRAST_MINIMUM}"
            );
        }
    }

    #[test]
    fn the_light_theme_is_light_and_the_dark_theme_is_dark() {
        assert!(
            Theme::new(ThemeChoice::Light, true)
                .background()
                .luminance()
                > 0.7
        );
        assert!(Theme::new(ThemeChoice::Dark, true).background().luminance() < 0.05);
    }

    #[test]
    fn disabling_colour_emits_no_colour_but_keeps_emphasis() {
        let theme = Theme::new(ThemeChoice::Dark, false);
        for token in Token::ALL {
            let style = theme.style(*token);
            assert!(style.fg.is_none(), "{token:?} emitted a foreground colour");
            assert!(style.bg.is_none(), "{token:?} emitted a background colour");
        }
        assert!(
            theme
                .style(Token::Focus)
                .add_modifier
                .contains(Modifier::BOLD),
            "focus must stay visible without colour"
        );
        assert!(
            theme
                .style(Token::Selection)
                .add_modifier
                .contains(Modifier::REVERSED),
            "selection must stay visible without colour"
        );
    }

    #[test]
    fn contrast_maths_matches_known_values() {
        // Black on white is the WCAG maximum of 21:1.
        let ratio = Rgb(0, 0, 0).contrast(Rgb(255, 255, 255));
        assert!((ratio - 21.0).abs() < 0.01, "{ratio}");
        // A colour against itself is 1:1.
        assert!((Rgb(18, 21, 26).contrast(Rgb(18, 21, 26)) - 1.0).abs() < f64::EPSILON);
    }
}
