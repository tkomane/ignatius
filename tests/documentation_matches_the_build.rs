//! The documentation, checked against the build it describes.
//!
//! Several documents are contracts rather than prose: the keymap table, the list
//! of bindable actions, the fields a connection profile understands, and the
//! exit codes. Each of them exists in the code as well, and every one of them
//! has drifted at least once in this project's short life - a key added without
//! a row, a status described that a surface does not have.
//!
//! So they are compared here. The code is the authority and the document is the
//! promise; when they disagree, this fails and says which line to write.
//!
//! What this cannot check is whether a sentence is true. It checks that nothing
//! the build offers is missing from what the user is told, which is the half
//! that rots silently.

use ignatius::ExitCode;
use ignatius::config::schema::PROFILE_FIELDS;
use ignatius::ui::keymap::{CHORDS, CONFIGURABLE, Keymap};

const KEYMAP: &str = include_str!("../docs/design/keymap.md");
const DIAGNOSTICS: &str = include_str!("../docs/support/diagnostics.md");
const COMPATIBILITY: &str = include_str!("../docs/support/compatibility.md");

#[test]
fn every_chord_the_build_offers_is_in_the_keymap_document() {
    for (key, _, description) in CHORDS {
        assert!(
            KEYMAP.contains(&format!("| `{key}` |")),
            "the chord `Ctrl+K {key}` ({description}) is not in docs/design/keymap.md"
        );
        assert!(
            KEYMAP.contains(description),
            "the chord `Ctrl+K {key}` is listed with different words than the build uses: \
             {description:?}"
        );
    }
}

#[test]
fn every_chord_the_document_promises_is_one_the_build_has() {
    // The direction that matters more, and the one that was missing when this
    // file was written: the first version of it checked only that the document
    // listed everything the build had, and passed while the document promised
    // five chords the build did not bind at all.
    let table = KEYMAP
        .split("## Chords")
        .nth(1)
        .expect("the chord table is in docs/design/keymap.md");
    let table = table.split("\n## ").next().unwrap_or(table);

    for line in table.lines() {
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let Some((key, _)) = rest.split_once('`') else {
            continue;
        };
        let Some(key) = key.chars().next().filter(|_| key.chars().count() == 1) else {
            continue;
        };
        assert!(
            CHORDS.iter().any(|(bound, _, _)| *bound == key),
            "docs/design/keymap.md promises `Ctrl+K {key}` and this build does not bind it"
        );
    }
}

#[test]
fn every_bindable_action_is_named_in_the_keymap_document() {
    // The names are the configuration file's vocabulary. One missing from the
    // document is a feature nobody can find.
    for (name, _) in CONFIGURABLE {
        assert!(
            KEYMAP.contains(&format!("`{name}`")),
            "the bindable action `{name}` is not in docs/design/keymap.md"
        );
    }
}

#[test]
fn every_advertised_key_is_one_the_build_actually_binds() {
    // The other direction: a row promising a key that does nothing is worse
    // than a key nobody knows about.
    let keymap = Keymap::new();
    let labels: Vec<String> = keymap
        .bindings()
        .iter()
        .map(ignatius::ui::keymap::Binding::key_label)
        .collect();

    for advertised in [
        "Ctrl+R", "Ctrl+T", "Ctrl+C", "Ctrl+Q", "F1", "Ctrl+P", "Tab", "Esc",
    ] {
        assert!(
            KEYMAP.contains(&format!("| `{advertised}` |")),
            "{advertised} is bound but not documented"
        );
        assert!(
            labels.iter().any(|label| label == advertised),
            "{advertised} is documented but not bound"
        );
    }
}

#[test]
fn every_exit_code_is_documented_with_its_number() {
    for code in ExitCode::all() {
        assert!(
            DIAGNOSTICS.contains(&format!("| {} |", code.code())),
            "exit code {} ({}) is not in docs/support/diagnostics.md",
            code.code(),
            code.slug()
        );
    }

    // And nothing is documented that the build cannot produce.
    for line in DIAGNOSTICS.lines() {
        let Some(rest) = line.strip_prefix("| ") else {
            continue;
        };
        let Some((number, _)) = rest.split_once(" |") else {
            continue;
        };
        let Ok(number) = number.trim().parse::<i32>() else {
            continue;
        };
        assert!(
            ExitCode::all().iter().any(|code| code.code() == number),
            "docs/support/diagnostics.md documents exit code {number}, which this build \
             cannot produce"
        );
    }
}

#[test]
fn every_field_a_profile_understands_is_documented() {
    for field in PROFILE_FIELDS {
        assert!(
            COMPATIBILITY.contains(&format!("`{field}`")),
            "a profile understands `{field}` and docs/support/compatibility.md does not say so"
        );
    }
}
