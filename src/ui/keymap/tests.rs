use super::*;

#[test]
fn pane_chords_and_configured_replacements_preserve_plain_text() {
    for (key, name, action) in [
        (',', "shrink-pane", Action::ShrinkPane),
        ('.', "grow-pane", Action::GrowPane),
        ('z', "zoom-pane", Action::ZoomPane),
    ] {
        assert_eq!(chord_action(key), Some(action.clone()));
        assert_eq!(action_named(name), Some(action.clone()));
        let keymap = Keymap::new();
        assert_eq!(
            keymap.resolve(&press(KeyCode::Char(key), KeyModifiers::NONE)),
            Some(Action::Insert(key))
        );
        let keys = std::collections::BTreeMap::from([(
            name.to_owned(),
            crate::config::schema::KeySpec::One("alt+f12".into()),
        )]);
        let configured = Keymap::from_config(&keys).unwrap();
        assert_eq!(
            configured.resolve(&press(KeyCode::F(12), KeyModifiers::ALT)),
            Some(action.clone())
        );
        assert_eq!(configured.contextual_hint(&action).unwrap().0, "Alt+F12");
    }
}

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
    let error = Keymap::from_config(&configured(&[("quit", KeySpec::One("hyper+q".to_owned()))]))
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
fn column_resize_has_named_chords_without_stealing_printable_brackets() {
    assert_eq!(
        action_named("narrow-column"),
        Some(Action::NarrowColumn),
        "contracts/grid.md names the narrow action"
    );
    assert_eq!(
        action_named("widen-column"),
        Some(Action::WidenColumn),
        "contracts/grid.md names the widen action"
    );
    assert_eq!(chord_action('['), Some(Action::NarrowColumn));
    assert_eq!(chord_action(']'), Some(Action::WidenColumn));

    let keymap = Keymap::new();
    assert_eq!(
        keymap.contextual_hint(&Action::NarrowColumn),
        Some(("Ctrl+K [".to_owned(), "Narrow column"))
    );
    assert_eq!(
        keymap.contextual_hint(&Action::WidenColumn),
        Some(("Ctrl+K ]".to_owned(), "Widen column"))
    );
    // Plain brackets remain editor text; the chord owns only the prefixed form.
    assert_eq!(
        keymap.resolve(&press(KeyCode::Char('['), KeyModifiers::NONE)),
        Some(Action::Insert('['))
    );
    assert_eq!(
        keymap.resolve(&press(KeyCode::Char(']'), KeyModifiers::NONE)),
        Some(Action::Insert(']'))
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
    assert!(chord_action('~').is_none(), "an unbound key ends the chord");
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
