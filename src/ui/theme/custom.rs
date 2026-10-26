//! Pure manual theme grammar, validation and immutable palette catalog.

use super::{ActivePalette, ColorDepth, Rgb, Theme, Token};
use crate::config::ThemeChoice;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};
use std::fmt;

/// Explicit manual identity, independent of the base of a custom palette.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeSelection {
    /// Default dark palette.
    #[default]
    Dark,
    /// Light palette.
    Light,
    /// High-contrast palette.
    HighContrast,
    /// The validated custom definition.
    Custom,
}

impl ThemeSelection {
    /// Identity of one of the three built-in palettes.
    #[must_use]
    pub const fn from_builtin(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::Dark => Self::Dark,
            ThemeChoice::Light => Self::Light,
            ThemeChoice::HighContrast => Self::HighContrast,
        }
    }

    const fn builtin(self) -> Option<ThemeChoice> {
        match self {
            Self::Dark => Some(ThemeChoice::Dark),
            Self::Light => Some(ThemeChoice::Light),
            Self::HighContrast => Some(ThemeChoice::HighContrast),
            Self::Custom => None,
        }
    }
}

impl<'de> Deserialize<'de> for ThemeSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        lower_selection(Some(&value)).map_err(serde::de::Error::custom)
    }
}

/// Typed definition only; the resolved palette is never serialized here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeDefinition {
    base: ThemeChoice,
    overrides: [Option<Rgb>; Token::COUNT],
}

impl ThemeDefinition {
    /// Defines an inherited palette. Full contrast is checked by the resolver.
    #[must_use]
    pub const fn new(base: ThemeChoice) -> Self {
        Self {
            base,
            overrides: [None; Token::COUNT],
        }
    }

    /// Adds or replaces a typed RGB override, pending full contrast validation.
    #[must_use]
    pub const fn with_override(mut self, token: Token, rgb: Rgb) -> Self {
        self.overrides[token.index()] = Some(rgb);
        self
    }

    /// The built-in base.
    #[must_use]
    pub const fn base(&self) -> ThemeChoice {
        self.base
    }

    /// Only explicitly supplied overrides, not inherited/cache values.
    #[must_use]
    pub const fn override_for(&self, token: Token) -> Option<Rgb> {
        self.overrides[token.index()]
    }
}

impl Serialize for ThemeDefinition {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("extends", &self.base)?;
        for token in Token::ALL {
            if let Some(rgb) = self.override_for(*token) {
                map.serialize_entry(token.key(), &rgb.hex())?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ThemeDefinition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        let (_, definition) =
            lower_theme_inputs(None, Some(&value)).map_err(serde::de::Error::custom)?;
        definition.ok_or_else(|| serde::de::Error::custom("theme definition is absent"))
    }
}

/// Safe completed human refusal. Original pending values are never retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeRefusal {
    line: String,
}

impl ThemeRefusal {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            line: format!("configuration error: {}", detail.into()),
        }
    }

    fn missing() -> Self {
        Self::new("ui.theme = \"custom\" requires a [theme] table.")
    }

    fn contrast(token: Token, background: Token, ratio: f64, threshold: f64) -> Self {
        let displayed = (ratio * 10.0).floor() / 10.0;
        Self::new(format!(
            "[theme] {} on {} is {displayed:.1}:1; this pair must reach {threshold:.1}:1.",
            token.key(),
            background.key()
        ))
    }

    /// Exact safe physical line, without its terminating newline.
    #[must_use]
    pub fn line(&self) -> &str {
        &self.line
    }
}

impl fmt::Display for ThemeRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.line())
    }
}
impl std::error::Error for ThemeRefusal {}

fn reflected(value: &str) -> String {
    let redacted = crate::diagnostics::redaction::redact_text(value);
    let mut result = String::new();
    for ch in redacted.chars() {
        let unit = match ch {
            '"' => "\\\"".into(),
            '\\' => "\\\\".into(),
            '\n' => "\\n".into(),
            '\r' => "\\r".into(),
            '\t' => "\\t".into(),
            c if !c.is_ascii() || c.is_control() => format!("\\u{{{:04X}}}", c as u32),
            c => c.to_string(),
        };
        if result.len() + unit.len() > 80 {
            result.push_str("... [truncated]");
            break;
        }
        result.push_str(&unit);
    }
    result
}

fn lower_selection(value: Option<&toml::Value>) -> Result<ThemeSelection, ThemeRefusal> {
    let Some(value) = value else {
        return Ok(ThemeSelection::Dark);
    };
    let Some(value) = value.as_str() else {
        return Err(ThemeRefusal::new(
            "ui.theme must be a string. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        ));
    };
    match value {
        "dark" => Ok(ThemeSelection::Dark),
        "light" => Ok(ThemeSelection::Light),
        "high-contrast" => Ok(ThemeSelection::HighContrast),
        "custom" => Ok(ThemeSelection::Custom),
        "auto" => Err(ThemeRefusal::new(
            "ui.theme = \"auto\" is not available in this build. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
        )),
        other => Err(ThemeRefusal::new(format!(
            "ui.theme = \"{}\" is not a theme. Use \"dark\", \"light\", \"high-contrast\" or \"custom\".",
            reflected(other)
        ))),
    }
}

/// Lowers only deferred theme values. Callers retain their ordinary error order.
/// The returned typed definition still needs full resolution before admission.
pub fn lower_theme_inputs(
    selection: Option<&toml::Value>,
    definition: Option<&toml::Value>,
) -> Result<(ThemeSelection, Option<ThemeDefinition>), ThemeRefusal> {
    let selection = lower_selection(selection)?;
    if selection == ThemeSelection::Custom && definition.is_none() {
        return Err(ThemeRefusal::missing());
    }
    let Some(value) = definition else {
        return Ok((selection, None));
    };
    let Some(table) = value.as_table() else {
        return Err(ThemeRefusal::new("theme must be a table."));
    };
    let base = match table.get("extends") {
        None => ThemeChoice::Dark,
        Some(value) => {
            let Some(value) = value.as_str() else {
                return Err(ThemeRefusal::new(
                    "[theme] extends must be a string. Use \"dark\", \"light\" or \"high-contrast\".",
                ));
            };
            match value {
                "dark" => ThemeChoice::Dark,
                "light" => ThemeChoice::Light,
                "high-contrast" => ThemeChoice::HighContrast,
                other => {
                    return Err(ThemeRefusal::new(format!(
                        "[theme] extends = \"{}\" is not a built-in theme. Use \"dark\", \"light\" or \"high-contrast\".",
                        reflected(other)
                    )));
                }
            }
        }
    };
    if let Some(key) = table
        .keys()
        .filter(|key| key.as_str() != "extends" && Token::from_key(key).is_none())
        .min()
    {
        return Err(ThemeRefusal::new(format!(
            "[theme] has no token named \"{}\". Token names are listed in docs/design/theme-tokens.md.",
            reflected(key)
        )));
    }
    let mut result = ThemeDefinition::new(base);
    for token in Token::ALL {
        let Some(value) = table.get(token.key()) else {
            continue;
        };
        let Some(value) = value.as_str() else {
            return Err(ThemeRefusal::new(format!(
                "[theme] {} must be a colour string. Use \"#RRGGBB\" hex, for example \"#F38BA8\".",
                token.key()
            )));
        };
        let Some(rgb) = Rgb::from_hex(value) else {
            return Err(ThemeRefusal::new(format!(
                "[theme] {} = \"{}\" is not a colour. Use \"#RRGGBB\" hex, for example \"#F38BA8\".",
                token.key(),
                reflected(value)
            )));
        };
        result = result.with_override(*token, rgb);
    }
    Ok((selection, Some(result)))
}

/// Fixed immutable RGB data that can only be obtained through full validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedCustomPalette {
    pub(super) base: ThemeChoice,
    pub(super) colors: [Rgb; Token::COUNT],
}

/// A validated session catalog and startup selection, with no raw input cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedThemes {
    selected: ActivePalette,
    custom: Option<ValidatedCustomPalette>,
}

impl Default for ValidatedThemes {
    fn default() -> Self {
        Self {
            selected: ActivePalette::Builtin(ThemeChoice::Dark),
            custom: None,
        }
    }
}

/// A programmatic active/catalog mismatch, suitable for an internal diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeCycleError;
impl fmt::Display for ThemeCycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("active custom theme does not match the validated theme catalog")
    }
}
impl std::error::Error for ThemeCycleError {}

impl ValidatedThemes {
    /// Resolves every inherited/overridden custom value, even when inactive.
    pub fn resolve(
        selection: ThemeSelection,
        definition: Option<&ThemeDefinition>,
    ) -> Result<Self, ThemeRefusal> {
        if selection == ThemeSelection::Custom && definition.is_none() {
            return Err(ThemeRefusal::missing());
        }
        let custom = definition
            .map(|definition| {
                let base = Theme::new(definition.base, true);
                let mut colors = [Rgb(0, 0, 0); Token::COUNT];
                for token in Token::ALL {
                    colors[token.index()] = definition
                        .override_for(*token)
                        .unwrap_or_else(|| base.rgb(*token));
                }
                validate_palette(&colors, definition.base)?;
                Ok(ValidatedCustomPalette {
                    base: definition.base,
                    colors,
                })
            })
            .transpose()?;
        let selected = if let Some(base) = selection.builtin() {
            ActivePalette::Builtin(base)
        } else {
            ActivePalette::Custom(custom.clone().ok_or_else(ThemeRefusal::missing)?)
        };
        Ok(Self { selected, custom })
    }

    /// The initial validated palette, preserving the caller's emission gates.
    #[must_use]
    pub fn selected_theme(&self, color: bool, depth: ColorDepth) -> Theme {
        Theme {
            active: self.selected.clone(),
            color,
            depth,
        }
    }

    /// Cycles only matching data and preserves depth and colour availability.
    pub fn cycle(&self, current: &Theme) -> Result<Theme, ThemeCycleError> {
        let active = match &current.active {
            ActivePalette::Builtin(ThemeChoice::Dark) => ActivePalette::Builtin(ThemeChoice::Light),
            ActivePalette::Builtin(ThemeChoice::Light) => {
                ActivePalette::Builtin(ThemeChoice::HighContrast)
            }
            ActivePalette::Builtin(ThemeChoice::HighContrast) => match &self.custom {
                Some(custom) => ActivePalette::Custom(custom.clone()),
                None => ActivePalette::Builtin(ThemeChoice::Dark),
            },
            ActivePalette::Custom(custom) => {
                if self.custom.as_ref() != Some(custom) {
                    return Err(ThemeCycleError);
                }
                ActivePalette::Builtin(ThemeChoice::Dark)
            }
        };
        Ok(Theme {
            active,
            color: current.color,
            depth: current.depth,
        })
    }
}

fn validate_palette(colors: &[Rgb; Token::COUNT], base: ThemeChoice) -> Result<(), ThemeRefusal> {
    let rgb = |token: Token| colors[token.index()];
    let high = base == ThemeChoice::HighContrast;
    let backgrounds = [
        Token::Surface,
        Token::SurfaceAlt,
        Token::SurfacePane,
        Token::SurfaceOverlay,
    ];
    for token in Token::ALL {
        if backgrounds.contains(token) {
            continue;
        }
        if !high && *token == Token::Border {
            if rgb(*token) == rgb(Token::Surface) {
                return Err(ThemeRefusal::new(
                    "[theme] border must differ from surface.",
                ));
            }
            continue;
        }
        let threshold = if high {
            7.0
        } else {
            match token {
                Token::Text
                | Token::Muted
                | Token::Header
                | Token::NullValue
                | Token::SyntaxKeyword
                | Token::SyntaxLiteral
                | Token::SyntaxNumber
                | Token::SyntaxComment
                | Token::SyntaxIdentifier => 4.5,
                Token::Selection => continue,
                _ => 3.0,
            }
        };
        for background in backgrounds {
            let ratio = rgb(*token).contrast(rgb(background));
            if ratio < threshold {
                return Err(ThemeRefusal::contrast(*token, background, ratio, threshold));
            }
        }
    }
    for fill in [Token::Selection, Token::EnvironmentProduction] {
        let threshold = if high { 7.0 } else { 4.5 };
        let ratio = rgb(Token::Surface).contrast(rgb(fill));
        if ratio < threshold {
            return Err(ThemeRefusal::contrast(
                Token::Surface,
                fill,
                ratio,
                threshold,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
