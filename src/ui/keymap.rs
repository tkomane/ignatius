//! Key bindings.
//!
//! Bindings are data, not a match arm buried in the event loop, so they can be
//! listed in help, shown as hints, checked for conflicts, and later replaced from
//! configuration without touching behaviour.
//!
//! No binding assumes prior knowledge of a modal editor. Every action is
//! reachable with a named key, and every default is listed in
//! `docs/design/keymap.md`.
//!
//! Bindings can be replaced from configuration. Three things are errors rather
//! than things to ignore: an action name this build does not know, a key it
//! cannot parse, and two actions on one key. A file whose whole purpose is to
//! say what the keyboard does must not contain a line that quietly does
//! nothing.

use crate::app::discovery::{KeyBindingSnapshot, KeyChordSnapshot, KeymapSnapshot};
use crate::app::message::{Action, Direction};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// One key binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The key that triggers it.
    pub code: KeyCode,
    /// Modifiers that must be held.
    pub modifiers: KeyModifiers,
    /// What it does.
    pub action: Action,
    /// How it is described in help and hints.
    pub description: &'static str,
    /// Whether it appears in the footer hint line.
    pub is_hint: bool,
}

impl Binding {
    /// The key as a person would write it, for example `Ctrl+C`.
    #[must_use]
    pub fn key_label(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl".to_owned());
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt".to_owned());
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift".to_owned());
        }
        parts.push(match self.code {
            KeyCode::F(n) => format!("F{n}"),
            KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
            KeyCode::Esc => "Esc".to_owned(),
            KeyCode::Tab => "Tab".to_owned(),
            KeyCode::Enter => "Enter".to_owned(),
            KeyCode::Backspace => "Backspace".to_owned(),
            KeyCode::Up => "Up".to_owned(),
            KeyCode::Down => "Down".to_owned(),
            KeyCode::Left => "Left".to_owned(),
            KeyCode::Right => "Right".to_owned(),
            other => format!("{other:?}"),
        });
        parts.join("+")
    }
}

/// Actions that can be bound from configuration, by the name used there.
///
/// One table, used to parse the file, to report an unknown name with the list
/// of real ones, and to generate the documentation. Movement keys are not here:
/// they mean different things in each pane and rebinding them individually
/// would produce an interface nobody could describe.
pub const CONFIGURABLE: &[(&str, Action)] = &[
    ("run-buffer", Action::RunBuffer),
    ("run-statement", Action::RunStatement),
    ("refresh-result", Action::RefreshResult),
    ("format-buffer", Action::FormatBuffer),
    ("cancel", Action::Cancel),
    ("quit", Action::Quit),
    ("toggle-help", Action::ToggleHelp),
    ("focus-next", Action::FocusNext),
    ("toggle-error-detail", Action::ToggleErrorDetail),
    ("dismiss", Action::Dismiss),
    ("toggle-sidebar", Action::ToggleSidebar),
    ("shrink-pane", Action::ShrinkPane),
    ("grow-pane", Action::GrowPane),
    ("zoom-pane", Action::ZoomPane),
    ("open-palette", Action::OpenPalette),
    ("open-result-grid", Action::OpenResultControls),
    ("narrow-column", Action::NarrowColumn),
    ("widen-column", Action::WidenColumn),
    ("begin-prefix", Action::BeginPrefix),
    ("start-filter", Action::StartFilter),
    ("reload-objects", Action::ReloadObjects),
    ("complete", Action::Complete),
    ("show-definition", Action::ShowDefinition),
    ("show-dependencies", Action::ShowDependencies),
    ("save-query", Action::SaveQuery),
    ("export-rows", Action::ExportRows),
    ("open-query", Action::OpenQuery),
    ("open-history", Action::OpenHistory),
    ("toggle-history-recording", Action::ToggleHistoryRecording),
    ("toggle-expanded-row", Action::ToggleExpandedRow),
    ("toggle-inspector", Action::ToggleInspector),
    ("undo", Action::Undo),
    ("redo", Action::Redo),
    ("delete-forward", Action::DeleteForward),
    ("delete-word-left", Action::DeleteWordLeft),
    ("move-line-start", Action::MoveLineStart),
    ("move-line-end", Action::MoveLineEnd),
    ("move-buffer-start", Action::MoveBufferStart),
    ("move-buffer-end", Action::MoveBufferEnd),
];

/// The action a configuration name means.
#[must_use]
pub fn action_named(name: &str) -> Option<Action> {
    CONFIGURABLE
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, action)| action.clone())
}

/// Parses a key as written in configuration, for example `ctrl+r` or `f5`.
///
/// Deliberately small and case-insensitive. Everything it accepts is listed in
/// `docs/design/keymap.md`, and everything it does not accept is an error that
/// names what it saw.
pub fn parse_key(text: &str) -> Result<(KeyCode, KeyModifiers), String> {
    let mut modifiers = KeyModifiers::NONE;
    let mut code = None;
    for part in text.split('+') {
        let part = part.trim().to_ascii_lowercase();
        match part.as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" | "option" | "meta" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "" => return Err(format!("{text:?} has an empty part")),
            other => {
                if code.is_some() {
                    return Err(format!("{text:?} names more than one key"));
                }
                code = Some(parse_code(other)?);
            }
        }
    }
    code.map(|code| (code, modifiers))
        .ok_or_else(|| format!("{text:?} names modifiers but no key"))
}

fn parse_code(text: &str) -> Result<KeyCode, String> {
    Ok(match text {
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "enter" | "return" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "page-up" => KeyCode::PageUp,
        "pagedown" | "page-down" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "space" => KeyCode::Char(' '),
        other => {
            if let Some(number) = other.strip_prefix('f')
                && let Ok(n) = number.parse::<u8>()
                && (1..=24).contains(&n)
            {
                return Ok(KeyCode::F(n));
            }
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => KeyCode::Char(ch),
                _ => return Err(format!("{other:?} is not a key this build knows")),
            }
        }
    })
}

/// The set of active bindings.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::new()
    }
}

impl Keymap {
    /// The built-in bindings.
    #[must_use]
    pub fn new() -> Self {
        use KeyCode as K;
        let ctrl = KeyModifiers::CONTROL;
        let none = KeyModifiers::NONE;
        let alt = KeyModifiers::ALT;
        let bindings = vec![
            // Run is bound three ways on purpose. Ctrl+R is the one advertised,
            // because function keys are routinely claimed by the operating
            // system or another application before a terminal program ever sees
            // them. Ctrl+Enter is only distinguishable in terminals that speak
            // the Kitty keyboard protocol; elsewhere it arrives as plain Enter
            // and this binding simply never fires. F5 stays for muscle memory.
            binding(
                K::Char('r'),
                ctrl,
                Action::RunBuffer,
                "Run the whole buffer",
                true,
            ),
            binding(
                K::Enter,
                ctrl,
                Action::RunBuffer,
                "Run the whole buffer (terminals with the Kitty keyboard protocol)",
                false,
            ),
            binding(
                K::F(5),
                none,
                Action::RunBuffer,
                "Run the whole buffer (if your system has not claimed F5)",
                false,
            ),
            binding(
                K::Char('t'),
                ctrl,
                Action::RunStatement,
                "Run the statement at the cursor",
                true,
            ),
            // Traditional terminals can encode Ctrl+Shift+R as the same
            // control byte as Ctrl+R. F6 is the advertised portable direct
            // key, while the shifted-control alias remains useful where the
            // terminal preserves enhanced keyboard modifiers.
            binding(
                K::F(6),
                none,
                Action::RefreshResult,
                "Refresh the retained result",
                true,
            ),
            binding(
                K::Char('r'),
                ctrl | KeyModifiers::SHIFT,
                Action::RefreshResult,
                "Refresh the retained result (enhanced terminal modifiers)",
                false,
            ),
            binding(
                K::F(9),
                none,
                Action::RunStatement,
                "Run the statement at the cursor (if your system has not claimed F9)",
                false,
            ),
            binding(
                K::Char('f'),
                ctrl | KeyModifiers::SHIFT,
                Action::FormatBuffer,
                "Format the SQL buffer",
                false,
            ),
            binding(
                K::Char('c'),
                ctrl,
                Action::Cancel,
                "Cancel the running statement",
                true,
            ),
            binding(K::Char('q'), ctrl, Action::Quit, "Quit", true),
            binding(
                K::Char('p'),
                ctrl,
                Action::OpenPalette,
                "Open the command palette",
                true,
            ),
            binding(
                K::Char(' '),
                ctrl,
                Action::Complete,
                "Show schema completion",
                false,
            ),
            binding(
                K::Char('b'),
                ctrl,
                Action::ToggleSidebar,
                "Show or hide the object tree",
                false,
            ),
            binding(
                K::Char('k'),
                ctrl,
                Action::BeginPrefix,
                "Start a chord and list what follows it",
                false,
            ),
            binding(K::F(1), none, Action::ToggleHelp, "Show or hide help", true),
            binding(
                K::Char('g'),
                ctrl,
                Action::ToggleHelp,
                "Show or hide help, when F1 is unavailable",
                false,
            ),
            binding(
                K::Tab,
                none,
                Action::FocusNext,
                "Move focus between panes",
                false,
            ),
            binding(
                K::Esc,
                none,
                Action::Dismiss,
                "Dismiss help or the current error",
                false,
            ),
            binding(
                K::Char('d'),
                ctrl,
                Action::ToggleErrorDetail,
                "Expand or collapse error detail",
                false,
            ),
            binding(K::Up, none, Action::Move(Direction::Up), "Move up", false),
            binding(
                K::Down,
                none,
                Action::Move(Direction::Down),
                "Move down",
                false,
            ),
            binding(
                K::Left,
                none,
                Action::Move(Direction::Left),
                "Move left",
                false,
            ),
            binding(
                K::Right,
                none,
                Action::Move(Direction::Right),
                "Move right",
                false,
            ),
            binding(
                K::Backspace,
                none,
                Action::Backspace,
                "Delete the character before the cursor",
                false,
            ),
            binding(
                K::Enter,
                none,
                Action::Activate,
                "Insert a line break, open a node, or confirm",
                false,
            ),
            // Editing keys. Two modifiers are bound for word movement because
            // terminals disagree: Windows Terminal and most Linux emulators send
            // Ctrl, while macOS terminals conventionally send Alt for the same
            // gesture. Binding both means the key works where the user is.
            binding(
                K::Left,
                ctrl,
                Action::MoveWord(Direction::Left),
                "Move a word left",
                false,
            ),
            binding(
                K::Right,
                ctrl,
                Action::MoveWord(Direction::Right),
                "Move a word right",
                false,
            ),
            binding(
                K::Left,
                alt,
                Action::MoveWord(Direction::Left),
                "Move a word left (macOS terminals)",
                false,
            ),
            binding(
                K::Right,
                alt,
                Action::MoveWord(Direction::Right),
                "Move a word right (macOS terminals)",
                false,
            ),
            binding(
                K::Home,
                none,
                Action::MoveLineStart,
                "Move to the start of the line",
                false,
            ),
            binding(
                K::End,
                none,
                Action::MoveLineEnd,
                "Move to the end of the line",
                false,
            ),
            binding(
                K::Home,
                ctrl,
                Action::MoveBufferStart,
                "Move to the start of the buffer",
                false,
            ),
            binding(
                K::End,
                ctrl,
                Action::MoveBufferEnd,
                "Move to the end of the buffer",
                false,
            ),
            binding(
                K::PageUp,
                none,
                Action::MovePage(Direction::Up),
                "Move up a screenful",
                false,
            ),
            binding(
                K::PageDown,
                none,
                Action::MovePage(Direction::Down),
                "Move down a screenful",
                false,
            ),
            binding(
                K::Delete,
                none,
                Action::DeleteForward,
                "Delete the character after the cursor",
                false,
            ),
            binding(
                K::Backspace,
                alt,
                Action::DeleteWordLeft,
                "Delete the word before the cursor",
                false,
            ),
            binding(
                K::Char('w'),
                ctrl,
                Action::DeleteWordLeft,
                "Delete the word before the cursor",
                false,
            ),
            binding(K::Char('z'), ctrl, Action::Undo, "Undo", false),
            binding(K::Char('y'), ctrl, Action::Redo, "Redo", false),
        ];
        Self { bindings }
    }

    /// The built-in bindings with the user's replacements applied.
    ///
    /// A named action loses its default keys entirely when the file binds it, so
    /// what the file says is what the keyboard does. Anything wrong with the
    /// file is an error naming the line, never a binding that quietly does
    /// nothing.
    pub fn from_config(
        keys: &std::collections::BTreeMap<String, crate::config::schema::KeySpec>,
    ) -> Result<Self, crate::diagnostics::Diagnostic> {
        use crate::diagnostics::{Diagnostic, DiagnosticKind};
        let mut keymap = Self::new();

        for (name, spec) in keys {
            let Some(action) = action_named(name) else {
                let known: Vec<&str> = CONFIGURABLE.iter().map(|(name, _)| *name).collect();
                return Err(Diagnostic::new(
                    DiagnosticKind::Config,
                    format!("keys.{name} is not an action this build knows"),
                    "reading key bindings from the configuration file",
                )
                .likely_cause(format!("the actions are: {}", known.join(", ")))
                .next_action("correct the name, or remove the line"));
            };

            // The defaults for this action go, so the file is the whole answer
            // for it rather than an addition nobody can predict.
            keymap.bindings.retain(|binding| binding.action != action);

            for key in spec.keys() {
                let (code, modifiers) = parse_key(key).map_err(|reason| {
                    Diagnostic::new(
                        DiagnosticKind::Config,
                        format!("keys.{name} is not a key this build can read"),
                        "reading key bindings from the configuration file",
                    )
                    .likely_cause(reason)
                    .next_action(
                        "write it as ctrl+r, alt+left, f5, esc, tab, enter, home, pageup, or a \
                         single character",
                    )
                })?;
                keymap.bindings.push(Binding {
                    code,
                    modifiers,
                    action: action.clone(),
                    description: CONFIGURABLE
                        .iter()
                        .find(|(known, _)| known == name)
                        .map_or("configured binding", |_| description_of(&action)),
                    is_hint: false,
                });
            }
        }

        let conflicts = keymap.conflicts();
        if !conflicts.is_empty() {
            return Err(Diagnostic::new(
                DiagnosticKind::Config,
                "two actions are bound to the same key",
                "reading key bindings from the configuration file",
            )
            .likely_cause(conflicts.join("; "))
            .next_action("bind one of them to a different key, or remove it"));
        }

        Ok(keymap)
    }

    /// Every binding, for help and documentation.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// Copies the active human-readable keys across the application boundary.
    ///
    /// The reducer and palette do not need crossterm event values. They do need
    /// the exact configured labels, including a configured chord prefix, so a
    /// discovery surface can never fall back to a stale default in production.
    #[must_use]
    pub fn snapshot(&self) -> KeymapSnapshot {
        KeymapSnapshot {
            bindings: self
                .bindings
                .iter()
                .map(|binding| KeyBindingSnapshot {
                    action: binding.action.clone(),
                    key: binding.key_label(),
                    description: binding.description.to_owned(),
                })
                .collect(),
            chords: CHORDS
                .iter()
                .map(|(key, action, description)| KeyChordSnapshot {
                    key: *key,
                    action: action.clone(),
                    description: (*description).to_owned(),
                })
                .collect(),
            prefix: self
                .bindings
                .iter()
                .find(|binding| binding.action == Action::BeginPrefix)
                .map(Binding::key_label),
        }
    }

    /// Returns the key and short label for an action in the active keymap.
    ///
    /// Movement is one discoverable action even though it has four bindings;
    /// grouping them keeps the footer useful without hiding any direction.
    #[must_use]
    pub fn contextual_hint(&self, action: &Action) -> Option<(String, &'static str)> {
        if matches!(action, Action::Move(_)) {
            let keys: Vec<String> = [
                Action::Move(Direction::Up),
                Action::Move(Direction::Down),
                Action::Move(Direction::Left),
                Action::Move(Direction::Right),
            ]
            .iter()
            .filter_map(|candidate| {
                self.bindings
                    .iter()
                    .find(|binding| binding.action == *candidate)
                    .map(Binding::key_label)
            })
            .collect();
            if keys.is_empty() {
                return None;
            }
            return Some((keys.join("/"), "Move"));
        }
        if let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| binding.action == *action)
        {
            return Some((binding.key_label(), short_label(action)));
        }
        let (key, _, _) = CHORDS.iter().find(|(_, chord, _)| chord == action)?;
        let prefix = self
            .bindings
            .iter()
            .find(|binding| binding.action == Action::BeginPrefix)
            .map(Binding::key_label)
            .unwrap_or_else(|| "Ctrl+K".to_owned());
        Some((format!("{prefix} {key}"), short_label(action)))
    }

    /// Bindings shown in the footer, in order.
    #[must_use]
    pub fn hints(&self) -> Vec<(String, &'static str)> {
        self.bindings
            .iter()
            .filter(|b| b.is_hint)
            .map(|b| (b.key_label(), short_label(&b.action)))
            .collect()
    }

    /// Translates a key event into an action.
    ///
    /// Unbound printable keys become editor input, which is why typing works
    /// without a binding for every character.
    #[must_use]
    pub fn resolve(&self, event: &KeyEvent) -> Option<Action> {
        // Key release events are delivered by terminals that speak the Kitty
        // protocol. Acting on both would run every statement twice.
        if event.kind == KeyEventKind::Release {
            return None;
        }
        for binding in &self.bindings {
            if binding.code == event.code && binding.modifiers == event.modifiers {
                return Some(binding.action.clone());
            }
        }
        match event.code {
            KeyCode::Char(ch)
                if event.modifiers.is_empty() || event.modifiers == KeyModifiers::SHIFT =>
            {
                Some(Action::Insert(ch))
            }
            _ => None,
        }
    }

    /// Reports bindings that would shadow each other.
    ///
    /// Returned as data rather than rejected outright, so configuration can be
    /// validated with a clear message instead of failing to start.
    #[must_use]
    pub fn conflicts(&self) -> Vec<String> {
        let mut conflicts = Vec::new();
        for (i, a) in self.bindings.iter().enumerate() {
            for b in self.bindings.iter().skip(i + 1) {
                if a.code == b.code && a.modifiers == b.modifiers {
                    conflicts.push(format!(
                        "{} is bound to both \"{}\" and \"{}\"",
                        a.key_label(),
                        a.description,
                        b.description
                    ));
                }
            }
        }
        conflicts
    }
}

/// The second key of a chord, and what it does.
///
/// Kept as data so the popup that lists them cannot drift from what they do.
pub const CHORDS: &[(char, Action, &str)] = &[
    (
        ',',
        Action::ShrinkPane,
        "Shrink the sidebar or editor share",
    ),
    ('.', Action::GrowPane, "Grow the sidebar or editor share"),
    (
        'z',
        Action::ZoomPane,
        "Zoom the focused pane or restore the layout",
    ),
    ('b', Action::ToggleSidebar, "Show or hide the object tree"),
    ('p', Action::OpenPalette, "Open the command palette"),
    ('c', Action::CopyValue, "Copy the selected result value"),
    (
        'u',
        Action::GenerateCellUpdate,
        "Generate a reviewed UPDATE from the selected cell",
    ),
    ('q', Action::FormatBuffer, "Format the SQL buffer"),
    ('n', Action::OpenConnectionPicker, "Choose a connection"),
    ('g', Action::OpenResultControls, "Open result grid controls"),
    (
        '[',
        Action::NarrowColumn,
        "Narrow the selected result column by 2 cells",
    ),
    (
        ']',
        Action::WidenColumn,
        "Widen the selected result column by 2 cells",
    ),
    ('l', Action::ExplainPlan, "Show the estimated query plan"),
    (
        'a',
        Action::AnalyzePlan,
        "Measure the query plan (executes the statement)",
    ),
    ('f', Action::StartFilter, "Filter the object tree"),
    ('r', Action::ReloadObjects, "Reload the object tree"),
    ('h', Action::ToggleHelp, "Show or hide help"),
    (
        'x',
        Action::ToggleExpandedRow,
        "Expand the selected row down the screen",
    ),
    ('i', Action::ToggleInspector, "Inspect the selected cell"),
    (
        's',
        Action::OpenHistory,
        "Search the statements that have run",
    ),
    (
        'v',
        Action::ToggleHistoryRecording,
        "Pause or resume recording statements",
    ),
    (
        'd',
        Action::ShowDefinition,
        "Show the selected object's definition",
    ),
    (
        'y',
        Action::ShowDependencies,
        "Show what depends on the selected object",
    ),
    ('w', Action::SaveQuery, "Save the buffer as a named query"),
    ('o', Action::OpenQuery, "Open a saved query"),
    (
        'e',
        Action::ExportRows,
        "Write the rows on screen to a file",
    ),
];

/// Resolves the second key of a chord.
#[must_use]
pub fn chord_action(key: char) -> Option<Action> {
    CHORDS
        .iter()
        .find(|(candidate, _, _)| *candidate == key.to_ascii_lowercase())
        .map(|(_, action, _)| action.clone())
}

/// The built-in description of an action, so a configured binding still reads
/// correctly in help.
fn description_of(action: &Action) -> &'static str {
    Keymap::new()
        .bindings
        .iter()
        .find(|binding| &binding.action == action)
        .map_or_else(
            || {
                CHORDS
                    .iter()
                    .find(|(_, chord, _)| chord == action)
                    .map_or("configured binding", |(_, _, description)| *description)
            },
            |binding| binding.description,
        )
}

fn binding(
    code: KeyCode,
    modifiers: KeyModifiers,
    action: Action,
    description: &'static str,
    is_hint: bool,
) -> Binding {
    Binding {
        code,
        modifiers,
        action,
        description,
        is_hint,
    }
}

const fn short_label(action: &Action) -> &'static str {
    // Exhaustive, so a new action stops the build here. When it does: add it to
    // `Action::all()` as well, or the test that asks whether every action can be
    // reached will not be asking about the new one.
    match action {
        Action::ToggleSidebar => "Objects",
        Action::OpenPalette => "Palette",
        Action::OpenResultControls => "Grid controls",
        Action::NarrowColumn => "Narrow column",
        Action::WidenColumn => "Widen column",
        Action::BeginPrefix => "Chord",
        Action::StartFilter => "Filter",
        Action::ReloadObjects => "Reload",
        Action::Complete => "Complete",
        Action::Activate => "Open",
        Action::ToggleExpandedRow => "Expand row",
        Action::ToggleInspector => "Inspect",
        Action::RunBuffer => "Run",
        Action::RunStatement => "Run statement",
        Action::ExplainPlan => "Plan",
        Action::AnalyzePlan => "Analyze plan",
        Action::CopyValue => "Copy",
        Action::GenerateCellUpdate => "Update",
        Action::RefreshResult => "Refresh",
        Action::FormatBuffer => "Format",
        Action::OpenConnectionPicker => "Connections",
        Action::Cancel => "Cancel",
        Action::Quit => "Quit",
        Action::ToggleHelp => "Help",
        Action::FocusNext => "Focus",
        Action::ShrinkPane => "Shrink pane",
        Action::GrowPane => "Grow pane",
        Action::ZoomPane => "Zoom/restore",
        Action::Dismiss => "Dismiss",
        Action::ToggleErrorDetail => "Detail",
        Action::Move(_) => "Move",
        Action::Insert(_) => "Type",
        Action::Backspace | Action::DeleteForward | Action::DeleteWordLeft => "Delete",
        Action::MoveWord(_)
        | Action::MoveLineStart
        | Action::MoveLineEnd
        | Action::MoveBufferStart
        | Action::MoveBufferEnd
        | Action::MovePage(_) => "Move",
        Action::Undo => "Undo",
        Action::OpenHistory => "History",
        Action::ShowDefinition => "Definition",
        Action::ShowDependencies => "Dependencies",
        Action::SaveQuery => "Save",
        Action::ExportRows => "Write",
        Action::OpenQuery => "Open",
        Action::ToggleHistoryRecording => "Private",
        Action::Redo => "Redo",
    }
}

#[cfg(test)]
mod tests;
