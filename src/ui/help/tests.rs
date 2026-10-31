use super::*;

#[test]
fn every_wrapped_help_row_reaches_actual_cells_with_remaps_in_both_densities() {
    use ratatui::buffer::Buffer;
    let configured = std::collections::BTreeMap::from([
        (
            "run-buffer".into(),
            crate::config::schema::KeySpec::One("ctrl+alt+shift+f12".into()),
        ),
        (
            "begin-prefix".into(),
            crate::config::schema::KeySpec::One("alt+k".into()),
        ),
        (
            "run-statement".into(),
            crate::config::schema::KeySpec::One("ctrl+名".into()),
        ),
    ]);
    let keymap = crate::ui::Keymap::from_config(&configured).unwrap();
    for density in [
        crate::config::Density::Comfortable,
        crate::config::Density::Compact,
    ] {
        for (width, height) in [(40, 8), (100, 30), (120, 30)] {
            let mut model = Model::new(100);
            model.help_open = true;
            model.keymap_snapshot = keymap.snapshot();
            model.size = (width, height);
            model.presentation = model.presentation.clone().with_density(density);
            let area = Rect::new(0, 0, width, height);
            let first = build(&model, &model.presentation, area);
            assert!(first.body.height > 0);
            let without_space = |text: String| {
                text.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
            };
            let described: String = first
                .rows
                .iter()
                .filter(|line| line.spans.len() == 2)
                .map(|line| line.spans[1].content.as_ref())
                .collect();
            let expected: String = model
                .keymap_snapshot
                .bindings
                .iter()
                .map(|binding| binding.description.as_str())
                .chain(
                    model
                        .keymap_snapshot
                        .chords
                        .iter()
                        .map(|chord| chord.description.as_str()),
                )
                .collect();
            assert_eq!(without_space(described), without_space(expected));
            let total = first.rows.len();
            let mut visited = vec![false; total];
            for _ in 0..total {
                let view = build(&model, &model.presentation, area);
                let mut buffer = Buffer::empty(area);
                crate::ui::layout::render(&model, &keymap, area, &mut buffer);
                for (index, expected) in view.window().iter().enumerate() {
                    let y = view.body.y + u16::try_from(index).unwrap();
                    let actual: String = (view.body.x..view.body.right())
                        .map(|x| buffer[(x, y)].symbol())
                        .collect();
                    // Wide glyph continuation cells are padding in the buffer.
                    assert_eq!(without_space(actual), without_space(expected.to_string()));
                    assert!(expected.width() <= usize::from(view.body.width));
                    visited[view.offset + index] = true;
                }
                if view.offset == view.max_scroll {
                    break;
                }
                crate::app::update(
                    &mut model,
                    crate::app::Message::Action(Action::MovePage(Direction::Down)),
                );
            }
            assert!(visited.into_iter().all(|seen| seen));
        }
    }
}

#[test]
fn default_help_keeps_a_body_and_complete_rail_at_forty_by_eight() {
    for density in [
        crate::config::Density::Comfortable,
        crate::config::Density::Compact,
    ] {
        let mut model = Model::new(100);
        model.presentation = model.presentation.clone().with_density(density);
        let view = build(&model, &model.presentation, Rect::new(0, 0, 40, 8));
        assert!(view.body.height >= 1);
        assert!(view.rail.len() <= usize::from(view.rail_area.height));
        assert!(
            view.rail
                .iter()
                .all(|line| display_width(line) <= usize::from(view.rail_area.width))
        );
        assert!(view.rail.concat().contains("Esc to close"));
    }
}

#[test]
fn undersized_help_prioritises_the_actual_close_key() {
    let model = Model::new(100);
    let view = build(&model, &model.presentation, Rect::new(0, 0, 20, 4));
    assert!(view.body.height > 0);
    assert_eq!(view.rail, ["Esc to close"]);
    assert!(view.rail.len() <= usize::from(view.rail_area.height));
    for width in [0, 1, 2, 8, 39] {
        for height in [0, 1, 2, 7] {
            let view = build(&model, &model.presentation, Rect::new(0, 0, width, height));
            assert!(view.outer.right() <= width && view.outer.bottom() <= height);
            let _ = view.window();
        }
    }
}

#[test]
fn long_valid_control_remaps_keep_the_help_body_at_the_minimum() {
    let mut keys = std::collections::BTreeMap::new();
    for (action, label) in [
        ("dismiss", "ctrl+alt+shift+backspace"),
        ("open-palette", "ctrl+alt+shift+pagedown"),
        ("move-line-start", "ctrl+alt+shift+pageup"),
        ("move-line-end", "ctrl+alt+shift+delete"),
    ] {
        keys.insert(
            action.into(),
            crate::config::schema::KeySpec::One(label.into()),
        );
    }
    let keymap = crate::ui::Keymap::from_config(&keys).unwrap();
    for density in [
        crate::config::Density::Comfortable,
        crate::config::Density::Compact,
    ] {
        let mut model = Model::new(100);
        model.keymap_snapshot = keymap.snapshot();
        model.presentation = model.presentation.clone().with_density(density);
        let view = build(&model, &model.presentation, Rect::new(0, 0, 40, 8));
        assert!(view.rows.len() > 50, "every real binding remains reachable");
        assert!(view.body.height > 0);
        assert!(view.rail.len() <= usize::from(view.rail_area.height));
    }
}

#[test]
fn help_intro_mentions_shift_and_mouse_config() {
    let model = Model::new(100);
    let view = build(&model, &model.presentation, Rect::new(0, 0, 100, 30));
    let text = view
        .rows
        .iter()
        .map(|row| row.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Shift"),
        "help must mention Shift for terminal selection: {text}"
    );
    assert!(
        text.contains("ui.mouse = false"),
        "help must mention setting ui.mouse = false: {text}"
    );
}
