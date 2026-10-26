use super::*;
use crate::app::model::{ParameterPrompt, PendingRun};
use crate::config::Density;

fn minimum_model(kind: Kind, density: Density) -> Model {
    use crate::app::{Message, update};
    let mut model = Model::new(100);
    model.presentation = model.presentation.clone().with_density(density);
    model.connection =
        crate::app::model::ConnectionState::Connected(Box::new(crate::postgres::SessionInfo {
            target: "audit@localhost:55432/orders".into(),
            database: "orders".into(),
            user: "audit".into(),
            server_version: "18.6".into(),
            backend_pid: "123".into(),
            search_path: "public".into(),
            read_only: false,
            tls: crate::postgres::TlsState::Disabled,
            environment: if kind == Kind::Production {
                crate::connection::Environment::Production
            } else {
                crate::connection::Environment::Local
            },
        }));
    model.editor.set_text(if kind == Kind::Parameter {
        "SELECT :note;"
    } else {
        "TRUNCATE orders;"
    });
    assert!(update(&mut model, Message::Action(Action::RunBuffer)).is_empty());
    for character in (if kind == Kind::Parameter {
        "synthetic"
    } else {
        "orders"
    })
    .chars()
    {
        assert!(update(&mut model, Message::Action(Action::Insert(character))).is_empty());
    }
    update(&mut model, Message::Resized(40, 8));
    model
}

fn painted(model: &Model, keymap: &crate::ui::Keymap) -> ratatui::buffer::Buffer {
    let area = Rect::new(0, 0, model.size.0, model.size.1);
    let mut buffer = ratatui::buffer::Buffer::empty(area);
    crate::ui::layout::render(model, keymap, area, &mut buffer);
    buffer
}

fn text_in(buffer: &ratatui::buffer::Buffer, area: Rect) -> String {
    let mut selected = ratatui::buffer::Buffer::empty(area);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            selected[(x, y)] = buffer[(x, y)].clone();
        }
    }
    crate::ui::layout::buffer_to_string(&selected)
}

fn minimum_input_and_review(kind: Kind) {
    use crate::app::{Message, update};
    for density in [Density::Comfortable, Density::Compact] {
        let mut model = minimum_model(kind, density);
        let initial = view(&model, kind, 40, 8);
        let buffer = painted(&model, &crate::ui::Keymap::new());
        assert!(
            initial.usable && initial.body.height >= 2,
            "{kind:?} {density:?}\n{}",
            crate::ui::layout::buffer_to_string(&buffer)
        );
        assert!(!initial.ready);
        let field = text_in(&buffer, initial.input_area.unwrap());
        if kind == Kind::Parameter {
            assert!(
                field.contains("Value:") && (field.contains('*') || field.contains('\u{2022}'))
            );
            assert!(!field.contains("synthetic"));
        } else {
            assert!(field.contains("Name: orders"));
        }
        let rail = text_in(&buffer, initial.rail_area);
        for row in &initial.rail {
            assert!(rail.contains(row.trim_end()));
        }
        assert!(initial.rail.concat().contains("Enter/Ctrl+R blocked"));
        assert!(initial.rail.concat().contains("Esc to cancel"));
        for action in [Action::Activate, Action::RunBuffer] {
            assert!(update(&mut model, Message::Action(action)).is_empty());
        }
        let complete = initial
            .rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<String>();
        if kind == Kind::Parameter {
            assert!(complete.contains("Parameter 1 of 1: :note"));
            assert!(
                complete.contains("literal text data") && complete.contains("nowhere in history")
            );
        } else {
            assert!(
                complete.contains("[PROD]")
                    && complete.contains("database name to continue: orders")
            );
            assert!(complete.contains("Database permissions are the real control"));
        }
        let mut visited = std::collections::BTreeSet::new();
        loop {
            let current = view(&model, kind, 40, 8);
            let current_buffer = painted(&model, &crate::ui::Keymap::new());
            let body = text_in(&current_buffer, current.body);
            for (index, row) in current.window().iter().enumerate() {
                assert!(body.contains(row.text.trim_end()));
                visited.insert(current.offset + index);
            }
            assert_eq!(current.body, initial.body);
            assert_eq!(current.input_area, initial.input_area);
            assert_eq!(current.rail_area, initial.rail_area);
            if current.ready {
                assert_eq!(visited.len(), current.rows.len());
                let final_rail = text_in(&current_buffer, current.rail_area);
                for row in &current.rail {
                    assert!(final_rail.contains(row.trim_end()));
                }
                assert!(current.rail.concat().contains("Esc to cancel"));
                break;
            }
            assert!(
                update(
                    &mut model,
                    Message::Action(Action::MovePage(Direction::Down))
                )
                .is_empty()
            );
        }
        let effects = update(&mut model, Message::Action(Action::Activate));
        assert!(matches!(
            (kind, effects.as_slice()),
            (
                Kind::Parameter,
                [crate::app::Effect::ExecuteParameterized { .. }]
            ) | (Kind::Production, [crate::app::Effect::Execute { .. }])
        ));
    }
}

#[test]
fn minimum_parameter_has_fixed_mask_and_two_review_rows_before_acceptance() {
    minimum_input_and_review(Kind::Parameter);
}

#[test]
fn minimum_typed_production_has_fixed_name_and_two_review_rows_before_acceptance() {
    minimum_input_and_review(Kind::Production);
}

#[test]
fn minimum_navigation_abbreviates_real_page_keys_and_preserves_other_remaps() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    for density in [Density::Comfortable, Density::Compact] {
        for (first, last, expected, first_event, last_event) in [
            (
                "alt+pageup",
                "ctrl+pagedown",
                "Alt+PgUp/Ctrl+PgDn top/end",
                KeyEvent::new(KeyCode::PageUp, KeyModifiers::ALT),
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL),
            ),
            (
                "f7",
                "f8",
                "F7/F8 top/end",
                KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE),
                KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE),
            ),
        ] {
            let configured = std::collections::BTreeMap::from([
                (
                    "move-line-start".into(),
                    crate::config::schema::KeySpec::One(first.into()),
                ),
                (
                    "move-line-end".into(),
                    crate::config::schema::KeySpec::One(last.into()),
                ),
            ]);
            let keymap = crate::ui::Keymap::from_config(&configured).unwrap();
            assert_eq!(keymap.resolve(&first_event), Some(Action::MoveLineStart));
            assert_eq!(keymap.resolve(&last_event), Some(Action::MoveLineEnd));
            let mut model = minimum_model(Kind::Parameter, density);
            model.keymap_snapshot = keymap.snapshot();
            model.size = (52, 12);
            let initial = view(&model, Kind::Parameter, 52, 12);
            assert!(initial.usable && !initial.ready);
            let rail = initial.rail.concat();
            assert!(rail.contains("PgUp/PgDn scroll"));
            assert!(rail.contains(expected), "{rail}");
            crate::app::update(
                &mut model,
                crate::app::Message::Action(keymap.resolve(&last_event).unwrap()),
            );
            let final_view = view(&model, Kind::Parameter, 52, 12);
            assert!(final_view.ready);
            assert_eq!(initial.body, final_view.body);
            assert_eq!(initial.rail_area, final_view.rail_area);
            let buffer = painted(&model, &keymap);
            let actual = text_in(&buffer, final_view.rail_area);
            for row in &final_view.rail {
                assert!(actual.contains(row.trim_end()));
            }
        }
    }
}

fn production(sql: &str, required: Option<&str>) -> Model {
    let mut model = Model::new(100);
    model.pending_run = Some(PendingRun {
        sql: sql.into(),
        impact: if required.is_some() {
            crate::query::Impact::Destructive
        } else {
            crate::query::Impact::Write
        },
        typed: String::new(),
        required: required.unwrap_or_default().into(),
        source: None,
    });
    model
}

#[test]
fn active_review_owner_follows_modal_precedence() {
    let mut model = production("UPDATE orders SET note = 'x'", None);
    assert_eq!(active_kind(&model), Some(Kind::Production));
    model.pending_plan = Some(crate::app::model::PendingPlan {
        sql: "SELECT 1;".into(),
        impact: crate::query::Impact::Read,
        typed: String::new(),
        required: String::new(),
    });
    assert_eq!(active_kind(&model), Some(Kind::Production));
    model.prefix_pending = true;
    assert_eq!(active_kind(&model), None);
    model.parameter_prompt = Some(ParameterPrompt::new(
        "SELECT :note".into(),
        None,
        vec!["note".into()],
    ));
    assert_eq!(active_kind(&model), Some(Kind::Parameter));
    model.password_prompt = Some(crate::app::model::PasswordPrompt::default());
    assert_eq!(active_kind(&model), None);
    model.password_prompt = None;
    model.parameter_prompt = None;
    model.prefix_pending = false;
    model.connection_details = true;
    assert_eq!(active_kind(&model), None);
    model.connection_details = false;
    model.pending_run = None;
    assert_eq!(active_kind(&model), Some(Kind::Analyze));
}

fn view(model: &Model, kind: Kind, width: u16, height: u16) -> View {
    build(
        model,
        kind,
        &model.presentation,
        Rect::new(0, 0, width, height),
    )
    .unwrap()
}

#[test]
fn stable_rail_height_survives_end_and_typed_name_eligibility() {
    let mut model = production(&format!("UPDATE {}", "x".repeat(2000)), Some("orders"));
    let first = view(&model, Kind::Production, 60, 18);
    assert!(first.usable && !first.ready);
    model.review_scroll = first.navigate(&Action::MoveLineEnd).unwrap();
    let typed_blocked = view(&model, Kind::Production, 60, 18);
    assert!(!typed_blocked.ready);
    model.pending_run.as_mut().unwrap().typed = "orders".into();
    let eligible = view(&model, Kind::Production, 60, 18);
    assert!(eligible.ready);
    assert_eq!(first.body, typed_blocked.body);
    assert_eq!(first.body, eligible.body);
    assert_eq!(first.rail_area, eligible.rail_area);
    assert_eq!(first.rail.len(), eligible.rail.len());
    assert!(eligible.rail.concat().contains("Enter/Ctrl+R to run"));
}

#[test]
fn exact_input_mask_is_bounded_without_materializing_the_answer_length() {
    for width in [16, 32, 80, 102] {
        for count in [0, 1, 10, usize::MAX] {
            let text = field(Input::Masked(count), width, &Model::new(1).presentation);
            assert!(display_width(&text) <= width, "{width} {count}: {text}");
            assert!(text.ends_with('|') || text.ends_with('\u{2588}'));
        }
    }
}

#[test]
fn body_wrapping_preserves_explicit_lines_and_escapes_controls() {
    let required = "db-界e\u{301}\x1b\u{202e}-tail";
    let mut model = production("SELECT 'first';\nSELECT 'second';", Some(required));
    model.review_scroll = usize::MAX;
    let rendered = view(&model, Kind::Production, 40, 16);
    let complete = rendered
        .rows
        .iter()
        .map(|row| row.text.as_str())
        .collect::<String>();
    assert!(complete.contains(&sanitize_for_display(required)));
    assert!(complete.contains("SELECT 'first';"));
    assert!(complete.contains("SELECT 'second';"));
    assert!(!complete.contains('\x1b') && !complete.contains('\u{202e}'));
    assert!(
        rendered
            .rows
            .iter()
            .all(|row| display_width(&row.text) <= usize::from(rendered.body.width))
    );
}

#[test]
fn only_the_deliberate_four_source_line_preview_is_truncated() {
    let model = production(
        "SELECT 1;\nSELECT 2;\nSELECT 3;\nSELECT 4;\nSELECT 5;",
        None,
    );
    let rendered = view(&model, Kind::Production, 120, 30);
    let body = rendered
        .rows
        .iter()
        .map(|row| row.text.as_str())
        .collect::<String>();
    assert!(body.contains("SELECT 4;"));
    assert!(!body.contains("SELECT 5;"));
    assert!(body.contains("Preview truncated after four source lines."));
    assert!(rendered.ready);
}

#[test]
fn current_key_snapshot_controls_identical_rail_metrics_in_both_densities() {
    for density in [Density::Comfortable, Density::Compact] {
        let mut model = production("UPDATE orders SET note = 'x'", None);
        model.presentation = model.presentation.clone().with_density(density);
        model.keymap_snapshot = crate::ui::Keymap::new().snapshot();
        for binding in &mut model.keymap_snapshot.bindings {
            if binding.action == Action::RunBuffer {
                binding.key = "Ctrl+Shift+F12".into();
            }
        }
        let rendered = view(&model, Kind::Production, 80, 24);
        let rail = rendered.rail.concat();
        assert!(rail.contains("Enter/Ctrl+Shift+F12 to run"));
        assert!(!rail.contains("Enter to run"));
        assert!(rendered.ready);
        assert!(
            rendered
                .rail
                .iter()
                .all(|line| display_width(line) <= usize::from(rendered.rail_area.width))
        );
        assert!(rendered.body.height >= 2);
        assert!(rendered.rail_area.bottom() <= rendered.outer.bottom().saturating_sub(1));
    }
}

#[test]
fn zero_or_insufficient_geometry_never_admits_and_always_has_bounded_ranges() {
    let mut model = Model::new(100);
    model.parameter_prompt = Some(ParameterPrompt::new(
        "SELECT :note".into(),
        None,
        vec!["note".into()],
    ));
    for density in [Density::Comfortable, Density::Compact] {
        model.presentation = model.presentation.clone().with_density(density);
        for width in [0, 1, 20, 39, 40, 80] {
            for height in [0, 1, 4, 7, 8, 12, 24] {
                let rendered = view(&model, Kind::Parameter, width, height);
                if width < 40 || height < 8 || rendered.body.height < 2 {
                    assert!(!rendered.ready);
                }
                assert!(rendered.outer.width <= width);
                assert!(rendered.outer.height <= height);
                let _ = rendered.window();
                let _ = rendered.navigate(&Action::MovePage(Direction::Down));
            }
        }
    }
}

#[test]
fn very_long_review_tail_remains_reachable_past_u16_offsets() {
    let model = production(
        &format!("UPDATE {}synthetic-sql-tail", "x".repeat(2_400_000)),
        None,
    );
    let first = view(&model, Kind::Production, 40, 12);
    assert!(first.max_scroll > usize::from(u16::MAX));
    let mut last_model = model.clone();
    last_model.review_scroll = first.navigate(&Action::MoveBufferEnd).unwrap();
    let last = view(&last_model, Kind::Production, 40, 12);
    assert!(last.ready);
    assert!(
        last.window()
            .iter()
            .any(|row| row.text.contains("control."))
    );
    assert_eq!(last.navigate(&Action::MoveBufferStart), Some(0));
}
