//! Icons, in three tiers.
//!
//! An icon is never the only thing carrying a meaning. Every one of them sits
//! beside the word it decorates, so the interface reads identically when the
//! icons are gone. That is what makes it safe to use them liberally: at the
//! lowest tier they simply disappear and nothing is lost.
//!
//! The tiers are chosen, not guessed. Detecting a Nerd Font is not possible from
//! inside a terminal, and a confident wrong answer produces a screen full of
//! replacement characters, so the Nerd tier is opt-in through configuration.

use ratatui::symbols::border;

/// How much the terminal and its font can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphTier {
    /// Nothing but ASCII. Icons are omitted entirely.
    Ascii,
    /// Box drawing and widely supported symbols.
    Unicode,
    /// A patched Nerd Font, so the private-use icon range is available.
    Nerd,
}

/// A meaning that may be decorated with an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// The product itself.
    Brand,
    /// A database.
    Database,
    /// A role or user.
    Role,
    /// An encrypted connection whose identity was verified.
    LockVerified,
    /// An encrypted connection with no identity check.
    LockPlain,
    /// An unencrypted connection.
    LockOpen,
    /// A production environment.
    Production,
    /// A non-production environment.
    Environment,
    /// Elapsed time.
    Clock,
    /// A result set.
    Rows,
    /// The SQL editor.
    Editor,
    /// Help.
    Help,
    /// Something succeeded.
    Success,
    /// Something needs attention.
    Warning,
    /// Something failed.
    Error,
    /// Neutral information.
    Info,
    /// A SQL NULL.
    Null,
    /// The focused pane.
    Focus,
    /// The session search path.
    SearchPath,
    /// A truncated result.
    Truncated,
    /// A read-only session.
    ReadOnly,
    /// A read-write session.
    ReadWrite,
    /// A key hint.
    Key,
    /// A schema.
    Schema,
    /// A table.
    Table,
    /// A view.
    View,
    /// A materialized view.
    MaterializedView,
    /// A sequence.
    Sequence,
    /// A function.
    Function,
    /// A column.
    Column,
    /// A column that is part of the primary key.
    KeyColumn,
    /// An index.
    Index,
    /// An installed extension.
    Extension,
    /// A closed tree node.
    ChevronClosed,
    /// An open tree node.
    ChevronOpen,
}

/// The icon set for a tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    /// Which tier is in use.
    pub tier: GlyphTier,
}

impl Glyphs {
    /// Builds a glyph set for a tier.
    #[must_use]
    pub const fn new(tier: GlyphTier) -> Self {
        Self { tier }
    }

    /// The icon for a meaning, or an empty string when icons are unavailable.
    ///
    /// Nerd Font code points are from the Font Awesome range that every Nerd
    /// Font patch has carried since the scheme was introduced.
    #[must_use]
    pub const fn icon(&self, icon: Icon) -> &'static str {
        match self.tier {
            GlyphTier::Ascii => "",
            GlyphTier::Unicode => match icon {
                Icon::Brand => "\u{25c8}",        // ◈
                Icon::Database => "\u{2637}",     // ☷
                Icon::Role => "\u{25cb}",         // ○
                Icon::LockVerified => "\u{2713}", // ✓
                Icon::LockPlain => "\u{25cf}",    // ●
                Icon::LockOpen => "\u{25cc}",     // ◌
                Icon::Production => "\u{25b2}",   // ▲
                Icon::Environment => "\u{25b6}",  // ▶
                Icon::Clock => "\u{25f7}",        // ◷
                Icon::Rows => "\u{2261}",         // ≡
                Icon::Editor => "\u{270e}",       // ✎
                Icon::Help => "?",
                Icon::Success => "\u{2713}",          // ✓
                Icon::Warning => "\u{25b3}",          // △
                Icon::Error => "\u{2717}",            // ✗
                Icon::Info => "\u{2022}",             // •
                Icon::Null => "\u{2205}",             // ∅
                Icon::Focus => "\u{258e}",            // ▎
                Icon::SearchPath => "\u{203a}",       // ›
                Icon::Truncated => "\u{2026}",        // …
                Icon::ReadOnly => "\u{25d0}",         // ◐
                Icon::ReadWrite => "\u{25cf}",        // ●
                Icon::Key => "\u{2219}",              // ∙
                Icon::Schema => "\u{25a3}",           // ▣
                Icon::Table => "\u{25a6}",            // ▦
                Icon::View => "\u{25eb}",             // ◫
                Icon::MaterializedView => "\u{25a9}", // ▩
                Icon::Sequence => "\u{21c5}",         // ⇅
                Icon::Function => "\u{0192}",         // ƒ
                Icon::Column => "\u{25aa}",           // ▪
                Icon::KeyColumn => "\u{25c6}",        // ◆
                Icon::Index => "\u{2315}",            // ⌕
                Icon::Extension => "\u{271a}",        // ✚
                Icon::ChevronClosed => "\u{25b8}",    // ▸
                Icon::ChevronOpen => "\u{25be}",      // ▾
            },
            GlyphTier::Nerd => match icon {
                Icon::Brand => "\u{f1c0}",            // database
                Icon::Database => "\u{f1c0}",         // database
                Icon::Role => "\u{f007}",             // user
                Icon::LockVerified => "\u{f023}",     // lock
                Icon::LockPlain => "\u{f023}",        // lock
                Icon::LockOpen => "\u{f09c}",         // unlock
                Icon::Production => "\u{f071}",       // exclamation-triangle
                Icon::Environment => "\u{f233}",      // server
                Icon::Clock => "\u{f017}",            // clock
                Icon::Rows => "\u{f0ce}",             // table
                Icon::Editor => "\u{f040}",           // pencil
                Icon::Help => "\u{f059}",             // question-circle
                Icon::Success => "\u{f00c}",          // check
                Icon::Warning => "\u{f071}",          // exclamation-triangle
                Icon::Error => "\u{f00d}",            // times
                Icon::Info => "\u{f05a}",             // info-circle
                Icon::Null => "\u{f05e}",             // ban
                Icon::Focus => "\u{258e}",            // ▎ block, not an icon
                Icon::SearchPath => "\u{f07c}",       // folder-open
                Icon::Truncated => "\u{f141}",        // ellipsis-h
                Icon::ReadOnly => "\u{f06e}",         // eye
                Icon::ReadWrite => "\u{f044}",        // pencil-square
                Icon::Key => "\u{f11c}",              // keyboard
                Icon::Schema => "\u{f07b}",           // folder
                Icon::Table => "\u{f0ce}",            // table
                Icon::View => "\u{f06e}",             // eye
                Icon::MaterializedView => "\u{f0c5}", // files
                Icon::Sequence => "\u{f0cb}",         // list-ol
                Icon::Function => "\u{f085}",         // cogs
                Icon::Column => "\u{f0db}",           // columns
                Icon::KeyColumn => "\u{f084}",        // key
                Icon::Index => "\u{f002}",            // search
                Icon::Extension => "\u{f12e}",        // puzzle-piece
                Icon::ChevronClosed => "\u{f054}",    // chevron-right
                Icon::ChevronOpen => "\u{f078}",      // chevron-down
            },
        }
    }

    /// An icon followed by a space, or nothing at all.
    ///
    /// Callers concatenate this in front of the word. When icons are off the
    /// text simply starts where the icon would have been.
    #[must_use]
    pub fn prefix(&self, icon: Icon) -> String {
        let glyph = self.icon(icon);
        if glyph.is_empty() {
            String::new()
        } else {
            format!("{glyph} ")
        }
    }

    /// One frame of the activity indicator.
    ///
    /// Always accompanied by a word in the interface, so a terminal that cannot
    /// draw the frame still says what is happening.
    #[must_use]
    pub fn spinner(&self, frame: u64) -> &'static str {
        const BRAILLE: [&str; 10] = [
            "\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}",
            "\u{2827}", "\u{2807}", "\u{280f}",
        ];
        const ASCII: [&str; 4] = ["|", "/", "-", "\\"];
        match self.tier {
            GlyphTier::Ascii => ASCII[(frame % ASCII.len() as u64) as usize],
            GlyphTier::Unicode | GlyphTier::Nerd => {
                BRAILLE[(frame % BRAILLE.len() as u64) as usize]
            }
        }
    }

    /// A progress bar of a fixed width, used for the elapsed-time indicator.
    #[must_use]
    pub fn meter(&self, filled: usize, width: usize) -> String {
        let filled = filled.min(width);
        match self.tier {
            GlyphTier::Ascii => {
                format!("{}{}", "#".repeat(filled), "-".repeat(width - filled))
            }
            GlyphTier::Unicode | GlyphTier::Nerd => {
                format!(
                    "{}{}",
                    "\u{2588}".repeat(filled),
                    "\u{2591}".repeat(width - filled)
                )
            }
        }
    }

    /// The marker showing whether a tree node is open.
    ///
    /// Unlike the other icons this is never empty, including in ASCII: a tree
    /// where open and closed look identical is not a tree.
    #[must_use]
    pub const fn chevron(&self, expanded: bool) -> &'static str {
        match self.tier {
            GlyphTier::Ascii => {
                if expanded {
                    "v"
                } else {
                    ">"
                }
            }
            GlyphTier::Unicode | GlyphTier::Nerd => {
                if expanded {
                    self.icon(Icon::ChevronOpen)
                } else {
                    self.icon(Icon::ChevronClosed)
                }
            }
        }
    }

    /// The indent guide drawn to the left of nested rows.
    #[must_use]
    pub const fn indent_guide(&self) -> &'static str {
        match self.tier {
            GlyphTier::Ascii => "  ",
            GlyphTier::Unicode | GlyphTier::Nerd => "\u{2502} ",
        }
    }

    /// Border characters. The focused pane gets a heavier frame, so focus is
    /// visible with no colour at all.
    #[must_use]
    pub fn borders(&self, focused: bool) -> border::Set<'static> {
        match self.tier {
            GlyphTier::Ascii => border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                vertical_left: "|",
                vertical_right: "|",
                horizontal_top: if focused { "=" } else { "-" },
                horizontal_bottom: if focused { "=" } else { "-" },
            },
            GlyphTier::Unicode | GlyphTier::Nerd => {
                if focused {
                    border::THICK
                } else {
                    border::ROUNDED
                }
            }
        }
    }

    /// The separator between status items.
    #[must_use]
    pub const fn separator(&self) -> &'static str {
        match self.tier {
            GlyphTier::Ascii => " | ",
            GlyphTier::Unicode | GlyphTier::Nerd => " \u{2502} ",
        }
    }

    /// The vertical rule drawn between result columns.
    #[must_use]
    pub const fn column_rule(&self) -> &'static str {
        match self.tier {
            GlyphTier::Ascii => "|",
            GlyphTier::Unicode | GlyphTier::Nerd => "\u{2502}",
        }
    }

    /// Whether this tier can draw anything beyond ASCII.
    #[must_use]
    pub const fn is_ascii(&self) -> bool {
        matches!(self.tier, GlyphTier::Ascii)
    }
}

#[cfg(test)]
mod tests;
