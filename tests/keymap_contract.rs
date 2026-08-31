//! Contract for the editor-facing keymap surface in Feature 005.
//!
//! This integration test keeps the advertised editing keys observable without
//! coupling the assertions to the keymap's internal representation.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ignatius::app::{Action, Direction};
use ignatius::ui::Keymap;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn editor_navigation_and_recovery_keys_resolve_to_their_documented_actions() {
    let keymap = Keymap::new();
    for (code, expected) in [
        (KeyCode::Home, Action::MoveLineStart),
        (KeyCode::End, Action::MoveLineEnd),
        (KeyCode::PageUp, Action::MovePage(Direction::Up)),
        (KeyCode::PageDown, Action::MovePage(Direction::Down)),
        (KeyCode::Delete, Action::DeleteForward),
        (KeyCode::Char('z'), Action::Undo),
        (KeyCode::Char('y'), Action::Redo),
    ] {
        let modifiers = match &expected {
            Action::Undo | Action::Redo => KeyModifiers::CONTROL,
            _ => KeyModifiers::NONE,
        };
        assert_eq!(
            keymap.resolve(&press(code, modifiers)),
            Some(expected),
            "{code:?} with {modifiers:?} must resolve to its editor action"
        );
    }
}

#[test]
fn both_terminal_word_movement_modifiers_resolve_left_and_right() {
    let keymap = Keymap::new();
    for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
        assert_eq!(
            keymap.resolve(&press(KeyCode::Left, modifiers)),
            Some(Action::MoveWord(Direction::Left)),
            "left word movement must work with {modifiers:?}"
        );
        assert_eq!(
            keymap.resolve(&press(KeyCode::Right, modifiers)),
            Some(Action::MoveWord(Direction::Right)),
            "right word movement must work with {modifiers:?}"
        );
    }
}
