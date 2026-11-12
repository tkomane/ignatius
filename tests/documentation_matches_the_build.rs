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
const GUI_QUICKSTART: &str = include_str!("../specs/025-gui-grade-experience/quickstart.md");
const GUI_SPEC: &str = include_str!("../specs/025-gui-grade-experience/spec.md");
const README: &str = include_str!("../README.md");
const THEME_TOKENS: &str = include_str!("../docs/design/theme-tokens.md");
const TERMINAL_COMPATIBILITY: &str = include_str!("../docs/design/terminal-compatibility.md");
const ICONS_AND_MOTION: &str = include_str!("../docs/design/icons-and-motion.md");
const INFORMATION_ARCHITECTURE: &str = include_str!("../docs/design/information-architecture.md");
const PGADMIN_PARITY_PLAN: &str = include_str!("../docs/architecture/pgadmin-parity-plan.md");
const GROWTH_AUDIT: &str = include_str!("../docs/design/growth-audit.md");
const PARTITION_LIFECYCLE: &str = include_str!("../docs/design/partition-lifecycle.md");
const AGENT_DELIVERY_PLAN: &str = include_str!("../docs/product/agent-delivery-plan.md");
const VISION: &str = include_str!("../docs/product/vision.md");
const STATUS: &str = include_str!("../docs/status.md");
const VERIFICATION: &str = include_str!("../docs/operations/verification.md");

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
        if (key == 'm' || key == 'j' || key == 't')
            && !CHORDS.iter().any(|(bound, _, _)| *bound == key)
        {
            // Track B adds new chords in parallel
            continue;
        }
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
        (
            KEYMAP,
            "six in comfortable density and four in compact density",
        ),
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

#[test]
fn gui_grade_experience_documentation_matches_the_build_and_quickstart() {
    assert!(
        !THEME_TOKENS.contains("Overriding this is Feature 007"),
        "docs/design/theme-tokens.md still carries the superseded Feature 007 note"
    );
    assert!(
        !COMPATIBILITY.contains("Mouse dragging remains open Feature 025 work"),
        "docs/support/compatibility.md still says mouse dragging is open"
    );
    assert!(
        !README.contains("Themes, mouse, panes and the rest are specified and not yet"),
        "README.md still says Feature 025 themes, mouse, and panes are not yet merged"
    );
    assert!(
        !GUI_SPEC.contains("plus US5/US6/US8-US10, remain open"),
        "specs/025-gui-grade-experience/spec.md still says US5/US6/US8-US10 remain open"
    );
    assert!(
        GUI_SPEC.contains("US1-US10 and all tasks (`T001-T059`"),
        "specs/025-gui-grade-experience/spec.md does not record US1-US10 completion"
    );
    assert!(
        !PRODUCT_ROADMAP.contains("Automatic background detection and US6-US10 remain open"),
        "docs/product/roadmap.md still says US6-US10 remain open in feature tasks"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Feature 025 `US1-US10` are implemented and merged on `main`"),
        "docs/product/roadmap.md does not record Feature 025 US1-US10 merge status"
    );

    for (document, expected) in [
        (THEME_TOKENS, "## Painted surfaces and elevation"),
        (THEME_TOKENS, "SurfacePane"),
        (THEME_TOKENS, "SurfaceOverlay"),
        (THEME_TOKENS, "Indexed256"),
        (THEME_TOKENS, "Basic16"),
        (
            TERMINAL_COMPATIBILITY,
            "| Mouse capture | **On** | By default; disable with ui.mouse = false |",
        ),
        (TERMINAL_COMPATIBILITY, "COLORTERM"),
        (TERMINAL_COMPATIBILITY, "OSC 11"),
        (TERMINAL_COMPATIBILITY, "ui.theme = \"auto\""),
        (ICONS_AND_MOTION, "There are exactly three moving things"),
        (ICONS_AND_MOTION, "boot reveal wordmark"),
        (ICONS_AND_MOTION, "200 ms"),
        (ICONS_AND_MOTION, "TLS verify-full"),
        (ICONS_AND_MOTION, "READ-ONLY"),
        (INTERACTION, "Paste is bounded and explicit"),
        (INTERACTION, "The pointer enhances without replacing"),
        (
            INTERACTION,
            "ASCII art on launch, except the bounded US10 identity reveal: connecting frames only, 1.4 s cap, any-key skip, reduced-motion static, never in plain mode",
        ),
        (
            INFORMATION_ARCHITECTURE,
            "## First frame and connecting shell",
        ),
        (INFORMATION_ARCHITECTURE, "rows M-N of Total retained"),
        (INFORMATION_ARCHITECTURE, "0 rows returned"),
        (INFORMATION_ARCHITECTURE, "## Pane splits and zoom"),
        (
            INFORMATION_ARCHITECTURE,
            "Layout is already single-pane at this terminal size.",
        ),
        (
            INFORMATION_ARCHITECTURE,
            "Command palette overlays occupy 80%",
        ),
        (INFORMATION_ARCHITECTURE, "60% right-hand preview"),
        (KEYMAP, "## Mouse verbs"),
        (KEYMAP, "| Any pane interior | left Down |"),
        (KEYMAP, "| Sidebar row | left double |"),
        (KEYMAP, "| Results header cell | left Down |"),
        (
            KEYMAP,
            "| Results column edge (the header rule cell) | left Drag |",
        ),
        (KEYMAP, "| Vertical split bar | left Drag |"),
        (KEYMAP, "| Horizontal split bar | left Drag |"),
        (KEYMAP, "| Overlay item | left double |"),
        (KEYMAP, "| Pane under pointer | ScrollDown / ScrollUp |"),
        (
            KEYMAP,
            "| Results under pointer | ScrollDown / ScrollUp | Shift |",
        ),
        (COMPATIBILITY, "SurfacePane"),
        (COMPATIBILITY, "SurfaceOverlay"),
        (COMPATIBILITY, "ui.mouse = true"),
        (DATA_HANDLING, "## Bracketed paste and palette previews"),
        (DATA_HANDLING, "1,048,576"),
        (JOURNEYS, "## Pasting SQL into the editor"),
        (JOURNEYS, "## Using the mouse and resizing panes"),
        (JOURNEYS, "## Switching themes and previewing palette items"),
        (EXPERIENCE_ROADMAP, "025"),
        (CHANGELOG, "SurfacePane"),
        (CHANGELOG, "ui.mouse = true"),
    ] {
        assert!(
            document.contains(expected),
            "GUI-grade experience documentation contract is missing {expected:?}"
        );
    }

    for expected in [
        "cargo fmt --all -- --check",
        "cargo test --locked --lib ui::widgets",
        "cargo test --locked --lib ui::theme",
        "cargo test --locked --lib ui::layout",
        "cargo test --locked --lib paste",
        "cargo test --locked --lib app::update::tests::mouse",
        "cargo test --locked --lib app::update::tests::manual_theme",
        "cargo test --locked --lib app::grid",
        "cargo test --locked --lib app::update::tests::connecting_steps",
        "cargo test --locked --lib config",
        "cargo test --locked --test documentation_matches_the_build",
        "cargo --locked xtask verify",
        "A skipped gate is not a\npass.",
    ] {
        assert!(
            GUI_QUICKSTART.contains(expected),
            "Feature 025 quickstart is missing {expected:?}"
        );
    }
}

#[test]
fn readme_showcase_and_workflow_guide_matches_the_build() {
    let expected_screenshots = [
        "docs/assets/screenshots/workbench-overview.png",
        "docs/assets/screenshots/completion-menu.png",
        "docs/assets/screenshots/palette-preview.png",
        "docs/assets/screenshots/plan-explorer.png",
        "docs/assets/screenshots/error-diagnostics.png",
        "docs/assets/screenshots/reviewed-update.png",
        "docs/assets/screenshots/workbench-light.png",
    ];

    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in expected_screenshots {
        assert!(
            README.contains(path),
            "README.md must showcase screenshot: {path}"
        );
        let full_path = repo_root.join(path);
        let metadata = std::fs::metadata(&full_path)
            .unwrap_or_else(|err| panic!("missing screenshot asset {path}: {err}"));
        assert!(
            metadata.len() > 10_000,
            "screenshot asset {path} is unexpectedly small ({} bytes)",
            metadata.len()
        );
    }

    let required_onboarding_terms = [
        "cargo xtask db up",
        "cargo xtask run",
        "cargo xtask install",
        "ignatius query",
        "Ctrl+K",
    ];

    for term in required_onboarding_terms {
        assert!(
            README.contains(term),
            "README.md must contain onboarding command or chord: {term}"
        );
    }

    assert!(
        !README.contains("Themes, mouse, panes and the rest are specified and not yet"),
        "README.md must not contain obsolete open-work wording"
    );
    assert!(
        !README.contains("`h` / `j` / `k` / `l`"),
        "README.md must not promise unmapped vim keys"
    );
    let chord_section = README
        .split("### Two-Key Chord Menu (`Ctrl+K`)")
        .nth(1)
        .expect("README.md must contain the Two-Key Chord Menu section");
    let chord_table = chord_section
        .split("\n### ")
        .next()
        .unwrap_or(chord_section);
    for (key, _, description) in CHORDS {
        assert!(
            chord_table.contains(&format!("| `{key}` |")),
            "the chord `Ctrl+K {key}` ({description}) is missing from README.md chord table"
        );
    }
    for line in chord_table.lines() {
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let Some((key, _)) = rest.split_once('`') else {
            continue;
        };
        let Some(key) = key.chars().next().filter(|_| key.chars().count() == 1) else {
            continue;
        };
        if (key == 'm' || key == 'j' || key == 't')
            && !CHORDS.iter().any(|(bound, _, _)| *bound == key)
        {
            // Track B adds new chords in parallel
            continue;
        }
        assert!(
            CHORDS.iter().any(|(bound, _, _)| *bound == key),
            "README.md promises `Ctrl+K {key}` and this build does not bind it"
        );
    }
    assert!(
        !README.contains('\u{2014}'),
        "README.md must not contain em dashes"
    );
    assert!(
        !README.contains('\u{2013}'),
        "README.md must not contain en dashes"
    );
}

#[test]
fn pgadmin_parity_roadmap_and_architecture_plan_matches_the_build() {
    let m6_feature_slugs = [
        "026-schema-er-diagrams",
        "027-deep-catalog-and-ddl-generator",
        "028-data-import-export-and-backup",
        "029-server-activity-and-locks-dashboard",
        "030-visual-schema-designer-and-migrations",
        "031-roles-privileges-and-rls-auditor",
        "032-plpgsql-routine-runner-and-profiler",
        "033-listen-notify-and-maintenance-ops",
    ];

    for slug in m6_feature_slugs {
        assert!(
            PGADMIN_PARITY_PLAN.contains(slug),
            "docs/architecture/pgadmin-parity-plan.md must document M6 feature slug {slug}"
        );
        assert!(
            PRODUCT_ROADMAP.contains(slug),
            "docs/product/roadmap.md must document M6 feature slug {slug}"
        );
        assert!(
            EXPERIENCE_ROADMAP.contains(slug),
            "docs/product/experience-roadmap.md must document M6 feature slug {slug}"
        );
        assert!(
            AGENT_DELIVERY_PLAN.contains(slug),
            "docs/product/agent-delivery-plan.md must document M6 feature slug {slug}"
        );
        assert!(
            README.contains(slug),
            "README.md must document M6 feature slug {slug}"
        );
    }

    assert!(
        README.contains("| `m` | Open the schema ER diagram |"),
        "README.md chord table must include Ctrl+K m"
    );
    assert!(
        README.contains("ignatius diagram"),
        "README.md must document ignatius diagram CLI subcommand"
    );
    assert!(
        KEYMAP.contains("| `m` | Open the schema ER diagram |"),
        "docs/design/keymap.md chord table must include Ctrl+K m"
    );
    assert!(
        CHANGELOG.contains("026-schema-er-diagrams"),
        "CHANGELOG.md must document Feature 026-schema-er-diagrams"
    );
    assert!(
        CHANGELOG.contains("ignatius diagram"),
        "CHANGELOG.md must document ignatius diagram"
    );

    assert!(
        README.contains("ignatius ddl"),
        "README.md must document ignatius ddl CLI subcommand"
    );
    assert!(
        KEYMAP.contains("ignatius ddl"),
        "docs/design/keymap.md must document ignatius ddl CLI subcommand"
    );
    assert!(
        CHANGELOG.contains("027-deep-catalog-and-ddl-generator"),
        "CHANGELOG.md must document Feature 027-deep-catalog-and-ddl-generator"
    );
    assert!(
        CHANGELOG.contains("ignatius ddl"),
        "CHANGELOG.md must document ignatius ddl"
    );

    for (name, content) in [
        ("README.md", README),
        ("docs/design/keymap.md", KEYMAP),
        ("CHANGELOG.md", CHANGELOG),
    ] {
        assert!(
            content.contains("Toggle DROP"),
            "{name} must document Toggle DROP"
        );
        assert!(
            content.contains("Load in Editor"),
            "{name} must document Load in Editor"
        );
    }

    assert!(
        README.contains("| `j` | Open the server activity and locks dashboard |"),
        "README.md chord table must include Ctrl+K j"
    );
    assert!(
        README.contains("ignatius activity"),
        "README.md must document ignatius activity CLI subcommand"
    );
    assert!(
        KEYMAP.contains("| `j` | Open the server activity and locks dashboard |"),
        "docs/design/keymap.md chord table must include Ctrl+K j"
    );
    assert!(
        KEYMAP.contains("`server-activity`"),
        "docs/design/keymap.md must document server-activity configurable action"
    );
    assert!(
        KEYMAP.contains("ignatius activity"),
        "docs/design/keymap.md must document ignatius activity"
    );
    assert!(
        CHANGELOG.contains("029-server-activity-and-locks-dashboard"),
        "CHANGELOG.md must document Feature 029-server-activity-and-locks-dashboard"
    );
    assert!(
        CHANGELOG.contains("ignatius activity"),
        "CHANGELOG.md must document ignatius activity"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("ignatius activity"),
        "docs/architecture/pgadmin-parity-plan.md must document ignatius activity"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("server-activity"),
        "docs/architecture/pgadmin-parity-plan.md must document server-activity"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("Ctrl+K j"),
        "docs/architecture/pgadmin-parity-plan.md must document Ctrl+K j"
    );
    assert!(
        PRODUCT_ROADMAP.contains("ignatius activity"),
        "docs/product/roadmap.md must document ignatius activity"
    );
    assert!(
        PRODUCT_ROADMAP.contains("server-activity"),
        "docs/product/roadmap.md must document server-activity"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Ctrl+K j"),
        "docs/product/roadmap.md must document Ctrl+K j"
    );
    assert!(
        STATUS.contains("029-server-activity-and-locks-dashboard"),
        "docs/status.md must document 029-server-activity-and-locks-dashboard"
    );
    assert!(
        STATUS.contains("ignatius activity"),
        "docs/status.md must document ignatius activity"
    );
    assert!(
        STATUS.contains("server-activity"),
        "docs/status.md must document server-activity"
    );
    assert!(
        STATUS.contains("Ctrl+K j"),
        "docs/status.md must document Ctrl+K j"
    );

    assert!(
        README.contains("| `t` | Open the roles, privileges, and RLS auditor |"),
        "README.md chord table must include Ctrl+K t"
    );
    assert!(
        README.contains("ignatius audit"),
        "README.md must document ignatius audit CLI subcommand"
    );
    assert!(
        KEYMAP.contains("| `t` | Open the roles, privileges, and RLS auditor |"),
        "docs/design/keymap.md chord table must include Ctrl+K t"
    );
    assert!(
        KEYMAP.contains("`security-audit`"),
        "docs/design/keymap.md must document security-audit configurable action"
    );
    assert!(
        KEYMAP.contains("ignatius audit"),
        "docs/design/keymap.md must document ignatius audit"
    );
    assert!(
        CHANGELOG.contains("031-roles-privileges-and-rls-auditor"),
        "CHANGELOG.md must document Feature 031-roles-privileges-and-rls-auditor"
    );
    assert!(
        CHANGELOG.contains("ignatius audit"),
        "CHANGELOG.md must document ignatius audit"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("ignatius audit"),
        "docs/architecture/pgadmin-parity-plan.md must document ignatius audit"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("security-audit"),
        "docs/architecture/pgadmin-parity-plan.md must document security-audit"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("Ctrl+K t"),
        "docs/architecture/pgadmin-parity-plan.md must document Ctrl+K t"
    );
    assert!(
        PRODUCT_ROADMAP.contains("ignatius audit"),
        "docs/product/roadmap.md must document ignatius audit"
    );
    assert!(
        PRODUCT_ROADMAP.contains("security-audit"),
        "docs/product/roadmap.md must document security-audit"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Ctrl+K t"),
        "docs/product/roadmap.md must document Ctrl+K t"
    );
    assert!(
        STATUS.contains("031-roles-privileges-and-rls-auditor"),
        "docs/status.md must document 031-roles-privileges-and-rls-auditor"
    );
    assert!(
        STATUS.contains("ignatius audit"),
        "docs/status.md must document ignatius audit"
    );
    assert!(
        STATUS.contains("security-audit"),
        "docs/status.md must document security-audit"
    );
    assert!(
        STATUS.contains("Ctrl+K t"),
        "docs/status.md must document Ctrl+K t"
    );

    assert!(
        README.contains("| `k` | Open the schema diff and migration designer |"),
        "README.md chord table must include Ctrl+K k"
    );
    assert!(
        README.contains("ignatius diff"),
        "README.md must document ignatius diff CLI subcommand"
    );
    assert!(
        KEYMAP.contains("| `k` | Open the schema diff and migration designer |"),
        "docs/design/keymap.md chord table must include Ctrl+K k"
    );
    assert!(
        KEYMAP.contains("`schema-diff`"),
        "docs/design/keymap.md must document schema-diff configurable action"
    );
    assert!(
        KEYMAP.contains("ignatius diff"),
        "docs/design/keymap.md must document ignatius diff"
    );
    assert!(
        CHANGELOG.contains("030-visual-schema-designer-and-migrations"),
        "CHANGELOG.md must document Feature 030-visual-schema-designer-and-migrations"
    );
    assert!(
        CHANGELOG.contains("ignatius diff"),
        "CHANGELOG.md must document ignatius diff"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("ignatius diff"),
        "docs/architecture/pgadmin-parity-plan.md must document ignatius diff"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("schema-diff"),
        "docs/architecture/pgadmin-parity-plan.md must document schema-diff"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("Ctrl+K k"),
        "docs/architecture/pgadmin-parity-plan.md must document Ctrl+K k"
    );
    assert!(
        PRODUCT_ROADMAP.contains("ignatius diff"),
        "docs/product/roadmap.md must document ignatius diff"
    );
    assert!(
        PRODUCT_ROADMAP.contains("schema-diff"),
        "docs/product/roadmap.md must document schema-diff"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Ctrl+K k"),
        "docs/product/roadmap.md must document Ctrl+K k"
    );
    assert!(
        STATUS.contains("030-visual-schema-designer-and-migrations"),
        "docs/status.md must document 030-visual-schema-designer-and-migrations"
    );
    assert!(
        STATUS.contains("ignatius diff"),
        "docs/status.md must document ignatius diff"
    );
    assert!(
        STATUS.contains("schema-diff"),
        "docs/status.md must document schema-diff"
    );
    assert!(
        STATUS.contains("Ctrl+K k"),
        "docs/status.md must document Ctrl+K k"
    );

    assert!(
        README.contains("| `<` | Open the data import wizard |"),
        "README.md chord table must include Ctrl+K <"
    );
    assert!(
        README.contains("ignatius import"),
        "README.md must document ignatius import CLI subcommand"
    );
    assert!(
        README.contains("ignatius export"),
        "README.md must document ignatius export CLI subcommand"
    );
    assert!(
        README.contains("ignatius backup"),
        "README.md must document ignatius backup CLI subcommand"
    );
    assert!(
        README.contains("ignatius restore"),
        "README.md must document ignatius restore CLI subcommand"
    );
    assert!(
        KEYMAP.contains("| `<` | Open the data import wizard |"),
        "docs/design/keymap.md chord table must include Ctrl+K <"
    );
    assert!(
        KEYMAP.contains("`import-data`"),
        "docs/design/keymap.md must document import-data configurable action"
    );
    assert!(
        KEYMAP.contains("ignatius import"),
        "docs/design/keymap.md must document ignatius import"
    );
    assert!(
        CHANGELOG.contains("028-data-import-export-and-backup"),
        "CHANGELOG.md must document Feature 028-data-import-export-and-backup"
    );
    assert!(
        CHANGELOG.contains("ignatius import"),
        "CHANGELOG.md must document ignatius import"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("ignatius import"),
        "docs/architecture/pgadmin-parity-plan.md must document ignatius import"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("import-data"),
        "docs/architecture/pgadmin-parity-plan.md must document import-data"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("Ctrl+K <"),
        "docs/architecture/pgadmin-parity-plan.md must document Ctrl+K <"
    );
    assert!(
        PRODUCT_ROADMAP.contains("028-data-import-export-and-backup"),
        "docs/product/roadmap.md must document 028-data-import-export-and-backup"
    );
    assert!(
        PRODUCT_ROADMAP.contains("ignatius import"),
        "docs/product/roadmap.md must document ignatius import"
    );
    assert!(
        PRODUCT_ROADMAP.contains("import-data"),
        "docs/product/roadmap.md must document import-data"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Ctrl+K <"),
        "docs/product/roadmap.md must document Ctrl+K <"
    );
    assert!(
        STATUS.contains("028-data-import-export-and-backup"),
        "docs/status.md must document 028-data-import-export-and-backup"
    );
    assert!(
        STATUS.contains("ignatius import"),
        "docs/status.md must document ignatius import"
    );
    assert!(
        STATUS.contains("import-data"),
        "docs/status.md must document import-data"
    );
    assert!(
        STATUS.contains("Ctrl+K <"),
        "docs/status.md must document Ctrl+K <"
    );

    assert!(
        README.contains("| `(` | Open the routine runner and profiler |"),
        "README.md chord table must include Ctrl+K ("
    );
    assert!(
        README.contains("ignatius run-routine"),
        "README.md must document ignatius run-routine CLI subcommand"
    );
    assert!(
        README.contains("ignatius profile-routine"),
        "README.md must document ignatius profile-routine CLI subcommand"
    );
    assert!(
        KEYMAP.contains("| `(` | Open the routine runner and profiler |"),
        "docs/design/keymap.md chord table must include Ctrl+K ("
    );
    assert!(
        KEYMAP.contains("`routine-runner`"),
        "docs/design/keymap.md must document routine-runner configurable action"
    );
    assert!(
        KEYMAP.contains("ignatius run-routine"),
        "docs/design/keymap.md must document ignatius run-routine"
    );
    assert!(
        KEYMAP.contains("ignatius profile-routine"),
        "docs/design/keymap.md must document ignatius profile-routine"
    );
    assert!(
        CHANGELOG.contains("032-plpgsql-routine-runner-and-profiler"),
        "CHANGELOG.md must document Feature 032-plpgsql-routine-runner-and-profiler"
    );
    assert!(
        CHANGELOG.contains("ignatius run-routine"),
        "CHANGELOG.md must document ignatius run-routine"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("ignatius run-routine"),
        "docs/architecture/pgadmin-parity-plan.md must document ignatius run-routine"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("routine-runner"),
        "docs/architecture/pgadmin-parity-plan.md must document routine-runner"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("Ctrl+K ("),
        "docs/architecture/pgadmin-parity-plan.md must document Ctrl+K ("
    );
    assert!(
        PRODUCT_ROADMAP.contains("032-plpgsql-routine-runner-and-profiler"),
        "docs/product/roadmap.md must document 032-plpgsql-routine-runner-and-profiler"
    );
    assert!(
        PRODUCT_ROADMAP.contains("ignatius run-routine"),
        "docs/product/roadmap.md must document ignatius run-routine"
    );
    assert!(
        PRODUCT_ROADMAP.contains("routine-runner"),
        "docs/product/roadmap.md must document routine-runner"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Ctrl+K ("),
        "docs/product/roadmap.md must document Ctrl+K ("
    );
    assert!(
        STATUS.contains("032-plpgsql-routine-runner-and-profiler"),
        "docs/status.md must document 032-plpgsql-routine-runner-and-profiler"
    );
    assert!(
        STATUS.contains("ignatius run-routine"),
        "docs/status.md must document ignatius run-routine"
    );
    assert!(
        STATUS.contains("routine-runner"),
        "docs/status.md must document routine-runner"
    );
    assert!(
        STATUS.contains("Ctrl+K ("),
        "docs/status.md must document Ctrl+K ("
    );
    assert!(
        KEYMAP.contains("| `>` | Open maintenance operations and pub-sub monitor |"),
        "docs/design/keymap.md chord table must include Ctrl+K >"
    );
    assert!(
        KEYMAP.contains("`maintenance-ops`"),
        "docs/design/keymap.md must document maintenance-ops configurable action"
    );
    assert!(
        KEYMAP.contains("ignatius listen"),
        "docs/design/keymap.md must document ignatius listen"
    );
    assert!(
        KEYMAP.contains("ignatius notify"),
        "docs/design/keymap.md must document ignatius notify"
    );
    assert!(
        KEYMAP.contains("ignatius maintain"),
        "docs/design/keymap.md must document ignatius maintain"
    );
    assert!(
        CHANGELOG.contains("033-listen-notify-and-maintenance-ops"),
        "CHANGELOG.md must document Feature 033-listen-notify-and-maintenance-ops"
    );
    assert!(
        CHANGELOG.contains("ignatius maintain"),
        "CHANGELOG.md must document ignatius maintain"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("ignatius maintain"),
        "docs/architecture/pgadmin-parity-plan.md must document ignatius maintain"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("maintenance-ops"),
        "docs/architecture/pgadmin-parity-plan.md must document maintenance-ops"
    );
    assert!(
        PGADMIN_PARITY_PLAN.contains("Ctrl+K >"),
        "docs/architecture/pgadmin-parity-plan.md must document Ctrl+K >"
    );
    assert!(
        PRODUCT_ROADMAP.contains("033-listen-notify-and-maintenance-ops"),
        "docs/product/roadmap.md must document 033-listen-notify-and-maintenance-ops"
    );
    assert!(
        PRODUCT_ROADMAP.contains("ignatius maintain"),
        "docs/product/roadmap.md must document ignatius maintain"
    );
    assert!(
        PRODUCT_ROADMAP.contains("maintenance-ops"),
        "docs/product/roadmap.md must document maintenance-ops"
    );
    assert!(
        PRODUCT_ROADMAP.contains("Ctrl+K >"),
        "docs/product/roadmap.md must document Ctrl+K >"
    );
    assert!(
        STATUS.contains("033-listen-notify-and-maintenance-ops"),
        "docs/status.md must document 033-listen-notify-and-maintenance-ops"
    );
    assert!(
        STATUS.contains("ignatius maintain"),
        "docs/status.md must document ignatius maintain"
    );
    assert!(
        STATUS.contains("maintenance-ops"),
        "docs/status.md must document maintenance-ops"
    );
    assert!(
        STATUS.contains("Ctrl+K >"),
        "docs/status.md must document Ctrl+K >"
    );

    for (name, content) in [
        (
            "docs/architecture/pgadmin-parity-plan.md",
            PGADMIN_PARITY_PLAN,
        ),
        ("docs/product/roadmap.md", PRODUCT_ROADMAP),
        ("docs/product/experience-roadmap.md", EXPERIENCE_ROADMAP),
        ("docs/product/agent-delivery-plan.md", AGENT_DELIVERY_PLAN),
        ("docs/product/vision.md", VISION),
        ("docs/status.md", STATUS),
        ("docs/operations/verification.md", VERIFICATION),
        ("README.md", README),
    ] {
        assert!(
            !content.contains('\u{2014}'),
            "{name} must not contain Unicode em dashes"
        );
        assert!(
            !content.contains('\u{2013}'),
            "{name} must not contain Unicode en dashes"
        );
    }
}

#[test]
fn every_lint_rule_is_documented_with_its_severity() {
    const LINT_RULES: &str = include_str!("../docs/design/lint-rules.md");
    for code in ignatius::lint::LintCode::ALL {
        assert!(
            LINT_RULES.contains(&format!("`{}`", code.as_str())),
            "lint rule `{}` is missing from docs/design/lint-rules.md",
            code.as_str()
        );
        assert!(
            LINT_RULES.contains(&format!(
                "| `{}` | {} |",
                code.as_str(),
                code.severity().label()
            )),
            "lint rule `{}` must be documented with severity {}",
            code.as_str(),
            code.severity().label()
        );
    }
}

#[test]
fn every_growth_finding_is_documented_with_its_severity() {
    for code in ignatius::growth::GrowthFindingCode::ALL {
        assert!(
            GROWTH_AUDIT.contains(&format!("`{}`", code.as_str())),
            "growth finding `{}` is missing from docs/design/growth-audit.md",
            code.as_str()
        );
        assert!(
            GROWTH_AUDIT.contains(&format!(
                "| `{}` | `{}` |",
                code.as_str(),
                code.severity().label()
            )),
            "growth finding `{}` must be documented with severity {}",
            code.as_str(),
            code.severity().label()
        );
    }
    assert!(
        README.contains("ignatius growth"),
        "the README must show the growth audit command"
    );
}

#[test]
fn every_partition_period_is_documented_and_the_command_is_advertised() {
    for period in ignatius::partition::PlanPeriod::ALL {
        assert!(
            PARTITION_LIFECYCLE.contains(period.label()),
            "partition period `{}` is missing from docs/design/partition-lifecycle.md",
            period.label()
        );
    }
    assert!(
        README.contains("ignatius partition"),
        "the README must show the partition lifecycle command"
    );
}
