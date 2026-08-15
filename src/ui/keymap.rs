//! Key bindings.
//!
//! Bindings are data, not a match arm buried in the event loop, so they can be
//! listed in help, shown as hints, checked for conflicts, and later replaced from
//! configuration without touching behaviour.
//!
//! No binding assumes prior knowledge of a modal editor. Every action is
//! reachable with a named key, and every default is listed in
//! `docs/design/keymap.md`.

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
            binding(
                K::F(9),
                none,
                Action::RunStatement,
                "Run the statement at the cursor (if your system has not claimed F9)",
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

    /// Every binding, for help and documentation.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
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
    ('f', Action::StartFilter, "Filter the object tree"),
    ('r', Action::ReloadObjects, "Reload the object tree"),
    ('h', Action::ToggleHelp, "Show or hide help"),
    (
        'x',
        Action::ToggleExpandedRow,
        "Expand the selected row down the screen",
    ),
    ('i', Action::ToggleInspector, "Inspect the selected cell"),
];

/// Resolves the second key of a chord.
#[must_use]
pub fn chord_action(key: char) -> Option<Action> {
    CHORDS
        .iter()
        .find(|(candidate, _, _)| *candidate == key.to_ascii_lowercase())
        .map(|(_, action, _)| action.clone())
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
    match action {
        Action::ToggleSidebar => "Objects",
        Action::OpenPalette => "Palette",
        Action::BeginPrefix => "Chord",
        Action::StartFilter => "Filter",
        Action::ReloadObjects => "Reload",
        Action::Activate => "Open",
        Action::ToggleExpandedRow => "Expand row",
        Action::ToggleInspector => "Inspect",
        Action::RunBuffer => "Run",
        Action::RunStatement => "Run statement",
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
        Action::Redo => "Redo",
        Action::Newline => "New line",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
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
        assert!(hints.len() <= 6, "the hint line must not become clutter");
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
    fn the_advertised_keys_are_chords_not_function_keys() {
        let hints = Keymap::new().hints();
        for (key, label) in &hints {
            assert!(
                !key.starts_with('F') || key == "F1",
                "{label} is advertised as {key}, a function key the system may claim"
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
}
