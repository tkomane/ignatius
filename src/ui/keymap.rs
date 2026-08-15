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
        let bindings = vec![
            binding(K::F(5), none, Action::RunBuffer, "Run the whole buffer", true),
            binding(K::F(9), none, Action::RunStatement, "Run the statement at the cursor", true),
            binding(K::Char('c'), ctrl, Action::Cancel, "Cancel the running statement", true),
            binding(K::Char('q'), ctrl, Action::Quit, "Quit", true),
            binding(K::F(1), none, Action::ToggleHelp, "Show or hide help", true),
            binding(K::Tab, none, Action::FocusNext, "Move focus between panes", false),
            binding(K::Esc, none, Action::Dismiss, "Dismiss help or the current error", false),
            binding(K::Char('d'), ctrl, Action::ToggleErrorDetail, "Expand or collapse error detail", false),
            binding(K::Up, none, Action::Move(Direction::Up), "Move up", false),
            binding(K::Down, none, Action::Move(Direction::Down), "Move down", false),
            binding(K::Left, none, Action::Move(Direction::Left), "Move left", false),
            binding(K::Right, none, Action::Move(Direction::Right), "Move right", false),
            binding(K::Backspace, none, Action::Backspace, "Delete the character before the cursor", false),
            binding(K::Enter, none, Action::Newline, "Insert a line break", false),
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
        Action::Backspace => "Delete",
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
        assert!(conflicts[0].contains("Run the whole buffer"), "{}", conflicts[0]);
    }

    #[test]
    fn core_actions_are_reachable_with_named_keys() {
        let keymap = Keymap::new();
        for (code, modifiers, expected) in [
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
        let mut event = press(KeyCode::F(5), KeyModifiers::NONE);
        event.kind = KeyEventKind::Release;
        assert_eq!(keymap.resolve(&event), None);

        event.kind = KeyEventKind::Press;
        assert_eq!(keymap.resolve(&event), Some(Action::RunBuffer));
    }

    #[test]
    fn key_labels_read_the_way_people_write_them() {
        let keymap = Keymap::new();
        let labels: Vec<String> = keymap.bindings().iter().map(Binding::key_label).collect();
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
    fn every_binding_has_a_description_for_help() {
        for binding in Keymap::new().bindings() {
            assert!(!binding.description.is_empty(), "{binding:?}");
        }
    }
}
