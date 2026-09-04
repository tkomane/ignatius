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
    ("open-palette", Action::OpenPalette),
    ("open-result-grid", Action::OpenResultControls),
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
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    use crate::config::schema::KeySpec;
    use std::collections::BTreeMap;

    fn configured(pairs: &[(&str, KeySpec)]) -> BTreeMap<String, KeySpec> {
        pairs
            .iter()
            .map(|(name, spec)| ((*name).to_owned(), spec.clone()))
            .collect()
    }

    #[test]
    fn a_key_is_read_the_way_people_write_it() {
        for (text, expected) in [
            ("ctrl+r", (KeyCode::Char('r'), KeyModifiers::CONTROL)),
            ("Ctrl+R", (KeyCode::Char('r'), KeyModifiers::CONTROL)),
            ("alt+left", (KeyCode::Left, KeyModifiers::ALT)),
            ("f5", (KeyCode::F(5), KeyModifiers::NONE)),
            ("F12", (KeyCode::F(12), KeyModifiers::NONE)),
            ("esc", (KeyCode::Esc, KeyModifiers::NONE)),
            ("page-up", (KeyCode::PageUp, KeyModifiers::NONE)),
            ("space", (KeyCode::Char(' '), KeyModifiers::NONE)),
            (
                "ctrl+shift+d",
                (
                    KeyCode::Char('d'),
                    KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                ),
            ),
        ] {
            assert_eq!(parse_key(text).expect(text), expected, "{text}");
        }

        // And what it cannot read, it says it cannot read.
        for text in ["", "ctrl+", "ctrl+r+t", "hyper+r", "f99", "notakey"] {
            assert!(parse_key(text).is_err(), "{text:?} should not parse");
        }
    }

    #[test]
    fn a_configured_binding_replaces_the_default_for_that_action() {
        let keymap = Keymap::from_config(&configured(&[(
            "run-buffer",
            KeySpec::One("f2".to_owned()),
        )]))
        .expect("a valid file");

        assert_eq!(
            keymap.resolve(&press(KeyCode::F(2), KeyModifiers::NONE)),
            Some(Action::RunBuffer)
        );
        assert_ne!(
            keymap.resolve(&press(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            Some(Action::RunBuffer),
            "the default goes, so the file is the whole answer for that action"
        );
        // The other defaults are untouched.
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
    }

    #[test]
    fn several_keys_can_be_bound_to_one_action() {
        let keymap = Keymap::from_config(&configured(&[(
            "quit",
            KeySpec::Many(vec!["ctrl+x".to_owned(), "f10".to_owned()]),
        )]))
        .expect("a valid file");
        for key in [
            press(KeyCode::Char('x'), KeyModifiers::CONTROL),
            press(KeyCode::F(10), KeyModifiers::NONE),
        ] {
            assert_eq!(keymap.resolve(&key), Some(Action::Quit));
        }
    }

    #[test]
    fn a_line_that_would_quietly_do_nothing_is_an_error_instead() {
        // An action this build does not know. The message lists the ones it does,
        // because a typo is the likeliest cause and guessing is not a fix.
        let error = Keymap::from_config(&configured(&[(
            "run-everythng",
            KeySpec::One("f2".to_owned()),
        )]))
        .expect_err("must refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Config);
        assert!(
            error
                .likely_cause
                .unwrap_or_default()
                .contains("run-buffer"),
            "the real names are listed"
        );

        // A key it cannot read.
        let error =
            Keymap::from_config(&configured(&[("quit", KeySpec::One("hyper+q".to_owned()))]))
                .expect_err("must refuse");
        assert!(error.next_action.unwrap_or_default().contains("ctrl+r"));

        // And two actions on one key, which is the one that would be maddening
        // rather than merely useless.
        let error = Keymap::from_config(&configured(&[(
            "toggle-help",
            KeySpec::One("ctrl+q".to_owned()),
        )]))
        .expect_err("must refuse");
        let cause = error.likely_cause.unwrap_or_default();
        assert!(cause.contains("Ctrl+Q"), "{cause}");
    }

    #[test]
    fn an_empty_keys_table_is_exactly_the_defaults() {
        let configured = Keymap::from_config(&BTreeMap::new()).expect("valid");
        assert_eq!(configured.bindings(), Keymap::new().bindings());
    }

    #[test]
    fn every_configurable_name_is_unique_and_reaches_a_real_action() {
        let mut names: Vec<&str> = CONFIGURABLE.iter().map(|(name, _)| *name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a name means one thing");

        for (name, action) in CONFIGURABLE {
            assert_eq!(action_named(name).as_ref(), Some(action));
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{name} should be kebab-case"
            );
            // Every configurable action can be described, so help still reads
            // correctly after the file replaces a binding.
            assert!(!description_of(action).is_empty());
        }
        assert!(action_named("nonsense").is_none());
    }

    #[test]
    fn every_action_this_build_has_can_be_reached() {
        // The test that would have caught five features shipping with no way in.
        // Everything else about the keymap iterates the tables, so a table
        // missing an entry looks complete from inside it. This iterates the
        // actions instead, which is the list that cannot agree with the mistake.
        let keymap = Keymap::new();
        for action in Action::all() {
            let bound = keymap
                .bindings()
                .iter()
                .any(|binding| binding.action == action);
            let chorded = CHORDS.iter().any(|(_, chord, _)| *chord == action);
            let typed = matches!(action, Action::Insert(_));
            assert!(
                bound || chorded || typed,
                "{action:?} has no key, no chord and no other way in"
            );
        }
    }

    #[test]
    fn every_action_worth_looking_up_is_in_the_palette_or_bound_to_a_key() {
        // The palette is built from the bindings and the chords, so an action
        // in neither is invisible there too. Typing and movement are excluded:
        // nobody looks up "move left" by name.
        let keymap = Keymap::new();
        for action in Action::all() {
            if matches!(
                action,
                Action::Insert(_)
                    | Action::Move(_)
                    | Action::MoveWord(_)
                    | Action::MovePage(_)
                    | Action::Backspace
                    | Action::Activate
                    | Action::DeleteForward
                    | Action::DeleteWordLeft
                    | Action::MoveLineStart
                    | Action::MoveLineEnd
                    | Action::MoveBufferStart
                    | Action::MoveBufferEnd
            ) {
                continue;
            }
            let listed = keymap
                .bindings()
                .iter()
                .any(|binding| binding.action == action)
                || CHORDS.iter().any(|(_, chord, _)| *chord == action);
            assert!(
                listed,
                "{action:?} cannot be found by anyone looking for it"
            );
        }
    }

    #[test]
    fn every_action_has_a_short_label_for_the_footer() {
        for action in Action::all() {
            assert!(
                !short_label(&action).is_empty(),
                "{action:?} has no label, so a hint for it would be blank"
            );
        }
    }

    #[test]
    fn the_default_keymap_has_no_conflicts() {
        let conflicts = Keymap::new().conflicts();
        assert!(conflicts.is_empty(), "{conflicts:#?}");
    }

    #[test]
    fn conflicts_are_reported_with_both_descriptions() {
        let mut keymap = Keymap::new();
        keymap.bindings.push(binding(
            KeyCode::F(5),
            KeyModifiers::NONE,
            Action::Quit,
            "Quit unexpectedly",
            false,
        ));
        let conflicts = keymap.conflicts();
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("F5"), "{}", conflicts[0]);
        assert!(
            conflicts[0].contains("Run the whole buffer"),
            "{}",
            conflicts[0]
        );
    }

    #[test]
    fn core_actions_are_reachable_with_named_keys() {
        let keymap = Keymap::new();
        for (code, modifiers, expected) in [
            (KeyCode::Char('r'), KeyModifiers::CONTROL, Action::RunBuffer),
            (
                KeyCode::Char('t'),
                KeyModifiers::CONTROL,
                Action::RunStatement,
            ),
            (
                KeyCode::Char('r'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                Action::RefreshResult,
            ),
            (KeyCode::F(6), KeyModifiers::NONE, Action::RefreshResult),
            (KeyCode::F(5), KeyModifiers::NONE, Action::RunBuffer),
            (KeyCode::F(9), KeyModifiers::NONE, Action::RunStatement),
            (KeyCode::Char('c'), KeyModifiers::CONTROL, Action::Cancel),
            (KeyCode::Char('q'), KeyModifiers::CONTROL, Action::Quit),
            (KeyCode::F(1), KeyModifiers::NONE, Action::ToggleHelp),
            (KeyCode::Tab, KeyModifiers::NONE, Action::FocusNext),
            (KeyCode::Esc, KeyModifiers::NONE, Action::Dismiss),
        ] {
            assert_eq!(
                keymap.resolve(&press(code, modifiers)),
                Some(expected.clone()),
                "{code:?} with {modifiers:?}"
            );
        }
    }

    #[test]
    fn unbound_printable_keys_type_into_the_editor() {
        let keymap = Keymap::new();
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('s'), KeyModifiers::NONE)),
            Some(Action::Insert('s'))
        );
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('r'), KeyModifiers::NONE)),
            Some(Action::Insert('r'))
        );
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('S'), KeyModifiers::SHIFT)),
            Some(Action::Insert('S'))
        );
        // A bound chord is not swallowed as text.
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
    }

    #[test]
    fn key_release_events_are_ignored_so_actions_do_not_fire_twice() {
        let keymap = Keymap::new();
        let mut event = press(KeyCode::Char('r'), KeyModifiers::CONTROL);
        event.kind = KeyEventKind::Release;
        assert_eq!(keymap.resolve(&event), None);

        event.kind = KeyEventKind::Press;
        assert_eq!(keymap.resolve(&event), Some(Action::RunBuffer));
    }

    #[test]
    fn key_labels_read_the_way_people_write_them() {
        let keymap = Keymap::new();
        let labels: Vec<String> = keymap.bindings().iter().map(Binding::key_label).collect();
        assert!(labels.contains(&"Ctrl+R".to_owned()), "{labels:?}");
        assert!(labels.contains(&"F5".to_owned()), "{labels:?}");
        assert!(labels.contains(&"Ctrl+C".to_owned()), "{labels:?}");
        assert!(labels.contains(&"Esc".to_owned()), "{labels:?}");
    }

    #[test]
    fn footer_hints_cover_the_actions_a_new_user_needs_first() {
        let hints = Keymap::new().hints();
        let text: Vec<&str> = hints.iter().map(|(_, label)| *label).collect();
        for needed in ["Run", "Cancel", "Quit", "Help"] {
            assert!(text.contains(&needed), "{needed} missing from {text:?}");
        }
        assert!(hints.len() <= 7, "the hint line must not become clutter");
    }

    #[test]
    fn running_is_reachable_without_a_function_key() {
        // Function keys are routinely claimed by the operating system before a
        // terminal program sees them, so every run action has a chord as well.
        let keymap = Keymap::new();
        for (code, modifiers) in [
            (KeyCode::Char('r'), KeyModifiers::CONTROL),
            (KeyCode::Enter, KeyModifiers::CONTROL),
            (KeyCode::F(5), KeyModifiers::NONE),
        ] {
            assert_eq!(
                keymap.resolve(&press(code, modifiers)),
                Some(Action::RunBuffer),
                "{code:?} should run the buffer"
            );
        }
        // A plain Enter is context-dependent: the reducer turns it into a line
        // break in the editor, opening a node in the tree, or confirming in the
        // palette. What matters here is that it never runs the buffer.
        assert_eq!(
            keymap.resolve(&press(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::Activate)
        );
        // Help has a chord too, for systems that claim F1.
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            Some(Action::ToggleHelp)
        );
    }

    #[test]
    fn the_advertised_keys_use_safe_direct_forms() {
        let hints = Keymap::new().hints();
        for (key, label) in &hints {
            assert!(
                !key.starts_with('F') || matches!(key.as_str(), "F1" | "F6"),
                "{label} is advertised as {key}, an unsupported function key"
            );
        }
    }

    #[test]
    fn navigation_commands_have_chords_of_their_own() {
        let keymap = Keymap::new();
        for (code, modifiers, expected) in [
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                Action::OpenPalette,
            ),
            (
                KeyCode::Char('b'),
                KeyModifiers::CONTROL,
                Action::ToggleSidebar,
            ),
            (
                KeyCode::Char('k'),
                KeyModifiers::CONTROL,
                Action::BeginPrefix,
            ),
        ] {
            assert_eq!(keymap.resolve(&press(code, modifiers)), Some(expected));
        }
    }

    #[test]
    fn plan_commands_have_named_chords_without_stealing_editor_letters() {
        assert_eq!(chord_action('l'), Some(Action::ExplainPlan));
        assert_eq!(chord_action('a'), Some(Action::AnalyzePlan));
        let keymap = Keymap::new();
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('l'), KeyModifiers::NONE)),
            Some(Action::Insert('l'))
        );
        assert_eq!(
            keymap.contextual_hint(&Action::ExplainPlan),
            Some(("Ctrl+K l".to_owned(), "Plan"))
        );
    }

    #[test]
    fn copy_has_a_named_chord_without_stealing_printable_c() {
        assert_eq!(chord_action('c'), Some(Action::CopyValue));
        assert_eq!(chord_action('C'), Some(Action::CopyValue));
        let keymap = Keymap::new();
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(Action::Insert('c'))
        );
        assert_eq!(
            keymap.contextual_hint(&Action::CopyValue),
            Some(("Ctrl+K c".to_owned(), "Copy"))
        );
        assert!(
            !CONFIGURABLE
                .iter()
                .any(|(_, action)| *action == Action::CopyValue),
            "copy remains a safe chord rather than a printable editor binding"
        );
    }

    #[test]
    fn format_has_a_direct_binding_and_safe_chord_without_stealing_printable_f() {
        let keymap = Keymap::new();
        let modifiers = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('f'), modifiers)),
            Some(Action::FormatBuffer)
        );
        assert_eq!(chord_action('q'), Some(Action::FormatBuffer));
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('f'), KeyModifiers::NONE)),
            Some(Action::Insert('f'))
        );
        assert_eq!(
            keymap.contextual_hint(&Action::FormatBuffer),
            Some(("Ctrl+Shift+F".to_owned(), "Format"))
        );
    }

    #[test]
    fn result_grid_controls_are_named_and_reachable_without_stealing_printable_keys() {
        assert_eq!(
            action_named("open-result-grid"),
            Some(Action::OpenResultControls)
        );
        assert_eq!(
            chord_action('g'),
            Some(Action::OpenResultControls),
            "Ctrl+K g is the contextual grid entry point"
        );
        assert_eq!(
            chord_action('G'),
            Some(Action::OpenResultControls),
            "the chord popup accepts the shifted printable key too"
        );

        let keymap = Keymap::new();
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('g'), KeyModifiers::NONE)),
            Some(Action::Insert('g')),
            "plain g remains available for editor text"
        );
    }

    #[test]
    fn connection_picker_has_a_named_chord_without_stealing_printable_n() {
        assert_eq!(
            chord_action('n'),
            Some(Action::OpenConnectionPicker),
            "Ctrl+K n is the connection choice entry point"
        );
        assert_eq!(
            chord_action('N'),
            Some(Action::OpenConnectionPicker),
            "the chord popup accepts the shifted printable key too"
        );

        let keymap = Keymap::new();
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char('n'), KeyModifiers::NONE)),
            Some(Action::Insert('n')),
            "plain n remains available for editor text"
        );
        assert_eq!(
            keymap.contextual_hint(&Action::OpenConnectionPicker),
            Some(("Ctrl+K n".to_owned(), "Connections"))
        );
    }

    #[test]
    fn every_chord_is_listed_and_resolves_to_what_it_says() {
        for (key, action, description) in CHORDS {
            assert!(!description.is_empty(), "{key} has no description");
            assert_eq!(
                chord_action(*key).as_ref(),
                Some(action),
                "{key} resolves to something other than its description"
            );
            // The popup lists these, so an uppercase press must work too.
            assert_eq!(
                chord_action(key.to_ascii_uppercase()).as_ref(),
                Some(action)
            );
        }
        assert!(chord_action('z').is_none(), "an unbound key ends the chord");
    }

    #[test]
    fn chord_keys_do_not_collide_with_each_other() {
        for (index, (key, _, _)) in CHORDS.iter().enumerate() {
            for (other, _, _) in CHORDS.iter().skip(index + 1) {
                assert_ne!(key, other, "chord key {key} is bound twice");
            }
        }
    }

    #[test]
    fn every_binding_has_a_description_for_help() {
        for binding in Keymap::new().bindings() {
            assert!(!binding.description.is_empty(), "{binding:?}");
        }
    }

    #[test]
    fn discovery_snapshot_and_hints_use_configured_keys() {
        let keymap = Keymap::from_config(&configured(&[
            ("run-buffer", KeySpec::One("f2".to_owned())),
            ("begin-prefix", KeySpec::One("f4".to_owned())),
            ("refresh-result", KeySpec::One("f6".to_owned())),
        ]))
        .expect("valid configured keymap");

        let snapshot = keymap.snapshot();
        assert_eq!(snapshot.prefix.as_deref(), Some("F4"));
        assert_eq!(
            snapshot
                .bindings
                .iter()
                .find(|binding| binding.action == Action::RunBuffer)
                .map(|binding| binding.key.as_str()),
            Some("F2")
        );
        assert_eq!(
            keymap.contextual_hint(&Action::RunBuffer),
            Some(("F2".to_owned(), "Run"))
        );
        assert_eq!(
            keymap.resolve(&press(KeyCode::F(6), KeyModifiers::NONE)),
            Some(Action::RefreshResult)
        );
        assert_eq!(
            keymap.contextual_hint(&Action::RefreshResult),
            Some(("F6".to_owned(), "Refresh"))
        );
        assert_eq!(
            keymap.contextual_hint(&Action::OpenResultControls),
            Some(("F4 g".to_owned(), "Grid controls"))
        );
    }
}
