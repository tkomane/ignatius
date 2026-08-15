//! Terminal lifecycle, theme tokens, layout and keymaps.
//!
//! Nothing in this module opens a connection, reads configuration from disk, or
//! writes a file. Widgets render state and emit intent; the application layer
//! turns intent into effects.

pub mod keymap;
pub mod layout;
pub mod terminal;
pub mod theme;

pub use keymap::{Binding, Keymap};
pub use layout::{LayoutMode, Presentation, layout_mode, render};
pub use terminal::{TerminalGuard, TerminalOptions};
pub use theme::{Rgb, Theme, Token};

/// Narrowest terminal the full layout is designed for.
pub const MIN_COLUMNS: u16 = 80;

/// Shortest terminal the full layout is designed for.
pub const MIN_ROWS: u16 = 24;

/// Below this size the interface shows a single readable message instead of a
/// clipped layout.
pub const USABLE_COLUMNS: u16 = 40;

/// Companion to [`USABLE_COLUMNS`].
pub const USABLE_ROWS: u16 = 8;
