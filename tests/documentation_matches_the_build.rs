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
const INTERACTION: &str = include_str!("../docs/design/interaction-principles.md");
const DATA_HANDLING: &str = include_str!("../docs/security/data-handling.md");
const JOURNEYS: &str = include_str!("../docs/product/journeys.md");
const DIAGNOSTICS_DOC: &str = include_str!("../docs/support/diagnostics.md");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");
const EXPERIENCE_ROADMAP: &str = include_str!("../docs/product/experience-roadmap.md");
const PRODUCT_ROADMAP: &str = include_str!("../docs/product/roadmap.md");
const QUICKSTART: &str = include_str!("../specs/023-cell-update/quickstart.md");
const REFRESH_QUICKSTART: &str = include_str!("../specs/024-retained-result-refresh/quickstart.md");

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
        "Ctrl+R",
        "Ctrl+T",
        "Ctrl+Shift+F",
        "Ctrl+C",
        "Ctrl+Q",
        "F1",
        "F6",
        "Ctrl+P",
        "Tab",
        "Esc",
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

#[test]
fn guided_discovery_documents_its_active_key_and_machine_output_boundary() {
    for (document, expected) in [
        (KEYMAP, "active keymap"),
        (KEYMAP, "at most five"),
        (KEYMAP, "command palette"),
        (INTERACTION, "Discovery follows the state"),
        (
            COMPATIBILITY,
            "presentation layer over the existing interactive model",
        ),
        (COMPATIBILITY, "does not change CSV, TSV, JSON"),
        (COMPATIBILITY, "terminal-restoration output"),
    ] {
        assert!(
            document.contains(expected),
            "guided discovery contract is missing {expected:?}"
        );
    }
}

#[test]
fn query_plan_documents_name_safety_metrics_and_recovery() {
    for (document, expected) in [
        (KEYMAP, "`l` | Show the estimated query plan"),
        (
            KEYMAP,
            "`a` | Measure the query plan (executes the statement)",
        ),
        (INTERACTION, "A plan separates belief from observation"),
        (INTERACTION, "Plan exploration is local"),
        (
            COMPATIBILITY,
            "The local display is bounded at 1 MiB and 500 retained nodes",
        ),
        (COMPATIBILITY, "not automatically rolled back"),
        (DIAGNOSTICS, "actual startup and total time"),
        (
            DIAGNOSTICS,
            "The attention marker says whether it uses estimated cost or measured time",
        ),
    ] {
        assert!(
            document.contains(expected),
            "query-plan contract is missing {expected:?}"
        );
    }
}

#[test]
fn clipboard_copy_documents_its_opt_in_write_only_boundary() {
    for (document, expected) in [
        (KEYMAP, "`c` | Copy the selected result value"),
        (KEYMAP, "printable `c`"),
        (INTERACTION, "Moving a value out is intentional"),
        (COMPATIBILITY, "[clipboard] osc52 = true"),
        (COMPATIBILITY, "acceptance is unconfirmed"),
        (DATA_HANDLING, "OSC 52"),
        (DATA_HANDLING, "never reads the destination clipboard"),
    ] {
        assert!(
            document.contains(expected),
            "clipboard-copy contract is missing {expected:?}"
        );
    }
}

#[test]
fn format_buffer_documents_its_local_bounded_and_plain_boundaries() {
    for (document, expected) in [
        (KEYMAP, "Ctrl+Shift+F"),
        (KEYMAP, "| `q` | Format the SQL buffer |"),
        (KEYMAP, "`format-buffer`"),
        (INTERACTION, "Formatting is a local, reversible edit"),
        (COMPATIBILITY, "## SQL buffer formatting"),
        (COMPATIBILITY, "The source bound is 1 MiB of"),
        (COMPATIBILITY, "UTF-8 bytes."),
        (COMPATIBILITY, "stdout remains reserved for result data"),
        (DATA_HANDLING, "## Local SQL formatting"),
        (DATA_HANDLING, "contacts PostgreSQL"),
    ] {
        assert!(
            document.contains(expected),
            "format-buffer contract is missing {expected:?}"
        );
    }
}

#[test]
fn connection_picker_documents_choice_safety_switch_and_plain_boundaries() {
    for (document, expected) in [
        (KEYMAP, "`n` | Choose a connection"),
        (KEYMAP, "`Ctrl+K n` opens it again"),
        (INTERACTION, "Starting is a choice when choices exist"),
        (COMPATIBILITY, "## Interactive connection picker"),
        (COMPATIBILITY, "profile name to the runtime"),
        (COMPATIBILITY, "Late work from the previous target"),
        (DATA_HANDLING, "## Interactive connection picker"),
        (DATA_HANDLING, "only `Option<String>`"),
        (DATA_HANDLING, "password-shaped"),
    ] {
        assert!(
            document.contains(expected),
            "connection-picker contract is missing {expected:?}"
        );
    }
}

#[test]
fn intentional_exports_document_shape_choice_and_insert_safety() {
    for (document, expected) in [
        (KEYMAP, "| `e` | Write the rows on screen to a file |"),
        (INTERACTION, "Saving is an intentional shape choice"),
        (INTERACTION, "filename extension"),
        (COMPATIBILITY, "--format insert"),
        (COMPATIBILITY, "--insert-table TABLE"),
        (
            COMPATIBILITY,
            "Duplicate column labels and NUL-containing text",
        ),
        (COMPATIBILITY, "never guesses from the filename"),
        (DATA_HANDLING, "CLI `--format insert` requires an explicit"),
        (DATA_HANDLING, "never executes it automatically"),
    ] {
        assert!(
            document.contains(expected),
            "intentional-export contract is missing {expected:?}"
        );
    }
}

#[test]
fn prompted_parameters_document_the_prompt_and_secret_boundaries() {
    for (document, expected) in [
        (JOURNEYS, "## When one template needs rerunning"),
        (JOURNEYS, "Parameter n of total"),
        (JOURNEYS, "mapping validation"),
        (COMPATIBILITY, "## Prompted named parameters"),
        (COMPATIBILITY, "Up to 64 distinct"),
        (COMPATIBILITY, "--param-env NAME=VARIABLE"),
        (COMPATIBILITY, "guessed template caret"),
        (DATA_HANDLING, "Prompted named-parameter values"),
        (DATA_HANDLING, "not a command-line argument"),
        (DATA_HANDLING, "simple-query boundary"),
    ] {
        assert!(
            document.contains(expected),
            "prompted-parameter contract is missing {expected:?}"
        );
    }
}

#[test]
fn cell_update_documentation_matches_the_build_and_quickstart() {
    for (document, expected) in [
        (KEYMAP, "Generate a reviewed UPDATE from the selected cell"),
        (KEYMAP, "exact bound statement"),
        (COMPATIBILITY, "## Reviewable cell-to-UPDATE"),
        (COMPATIBILITY, "one `ExecuteParameterized` effect"),
        (COMPATIBILITY, "bounded at 16 KiB"),
        (DIAGNOSTICS, "## Reading a cell-update refusal or review"),
        (DATA_HANDLING, "## Interactive cell updates"),
        (
            JOURNEYS,
            "## When one retained cell needs a deliberate update",
        ),
    ] {
        assert!(
            document.contains(expected),
            "cell-update contract is missing {expected:?}"
        );
    }

    for expected in [
        "cargo test --locked --lib update",
        "cargo test --locked --test documentation_matches_the_build cell_update",
        "cargo test --locked --test postgres_integration cell_update",
        "cargo --locked xtask verify",
        "If PostgreSQL is unavailable",
        "skipped, not\npassed",
        "Unix-socket, terminal, screen-reader, Windows Terminal and Linux",
    ] {
        assert!(
            QUICKSTART.contains(expected),
            "Feature 023 quickstart is missing {expected:?}"
        );
    }
}

#[test]
fn retained_result_refresh_documentation_matches_the_build_and_quickstart() {
    for (document, expected) in [
        (KEYMAP, "| `F6` | Refresh the retained result | Yes |"),
        (KEYMAP, "terminals can encode it as `Ctrl+R`"),
        (KEYMAP, "`refresh-result`"),
        (KEYMAP, "retained source, never the current editor buffer"),
        (INTERACTION, "Refreshing is explicit and source-bound"),
        (
            JOURNEYS,
            "## When a retained result needs a deliberate refresh",
        ),
        (COMPATIBILITY, "## Explicit retained-result refresh"),
        (COMPATIBILITY, "read-classified statement"),
        (DATA_HANDLING, "## Interactive retained-result refresh"),
        (
            DIAGNOSTICS_DOC,
            "## Reading a retained-result refresh outcome",
        ),
        (EXPERIENCE_ROADMAP, "Feature 024 is implemented"),
        (PRODUCT_ROADMAP, "024 Explicit retained-result refresh"),
        (CHANGELOG, "explicit retained-result refresh"),
    ] {
        assert!(
            document.contains(expected),
            "retained-result-refresh contract is missing {expected:?}"
        );
    }

    for expected in [
        "cargo test --locked --lib app::update::tests::refreshing_",
        "cargo test --locked --lib app::discovery::tests::retained_result_refresh",
        "cargo --locked xtask verify",
        "Manual terminal, screen-reader and cross-platform checks",
        "A skipped gate is not a pass.",
    ] {
        assert!(
            REFRESH_QUICKSTART.contains(expected),
            "Feature 024 quickstart is missing {expected:?}"
        );
    }
}
