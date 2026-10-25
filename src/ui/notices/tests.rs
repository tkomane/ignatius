use super::*;
use crate::app::model::NoticeDetails;

#[test]
fn reader_geometry_at_minimum_has_body_and_actual_ack_close_rail() {
    for density in [
        crate::config::Density::Comfortable,
        crate::config::Density::Compact,
    ] {
        let mut model = Model::new(10);
        model.presentation = model.presentation.clone().with_density(density);
        model.notice_details = Some(NoticeDetails {
            snapshot: model.notice_state.snapshot(),
            scroll: 0,
        });
        let view = build(&model, &model.presentation, Rect::new(0, 0, 40, 8)).unwrap();
        assert!(view.usable);
        assert!(view.body.height >= 2);
        assert!(view.rail.join(" ").contains("Enter acknowledge and close"));
        assert!(view.rail.join("").contains("Esc close"));
        for (width, height) in [(0, 0), (39, 8), (40, 7), (20, 4)] {
            assert!(
                !build(&model, &model.presentation, Rect::new(0, 0, width, height))
                    .unwrap()
                    .usable
            );
        }
    }
}

#[test]
fn reader_remapped_ack_and_close_keys_reach_actual_cells_without_defaults() {
    use crate::config::schema::KeySpec;
    let keys =
        std::collections::BTreeMap::from([("dismiss".into(), KeySpec::One("alt+f10".into()))]);
    let keymap = crate::ui::Keymap::from_config(&keys).unwrap();
    let mut model = Model::new(10);
    model.keymap_snapshot = keymap.snapshot();
    // Activate is fixed Enter in current config. A synthetic snapshot verifies
    // that the renderer obeys its supplied authority without adding a config route.
    model
        .keymap_snapshot
        .bindings
        .iter_mut()
        .find(|binding| binding.action == Action::Activate)
        .unwrap()
        .key = "Alt+F9".into();
    model.notice_details = Some(NoticeDetails {
        snapshot: model.notice_state.snapshot(),
        scroll: 0,
    });
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);
    render(&model, &model.presentation, area, &mut buf);
    let text = crate::ui::layout::buffer_to_string(&buf);
    assert!(text.contains("Alt+F9 acknowledge and close"));
    assert!(text.contains("Alt+F10 close"));
    assert!(!text.contains("Enter acknowledge"));
}

#[test]
fn reader_every_fixed_row_is_reachable_in_real_ascii_and_styled_buffers() {
    use crate::config::{Density, ThemeChoice};
    use crate::ui::{GlyphTier, Glyphs, Theme};
    for density in [Density::Comfortable, Density::Compact] {
        for (theme, color, tier) in [
            (ThemeChoice::Dark, true, GlyphTier::Unicode),
            (ThemeChoice::Light, true, GlyphTier::Unicode),
            (ThemeChoice::HighContrast, true, GlyphTier::Unicode),
            (ThemeChoice::Dark, false, GlyphTier::Ascii),
        ] {
            let mut model = Model::new(10);
            model.size = (40, 8);
            model.presentation =
                Presentation::new(Theme::new(theme, color), Glyphs::new(tier), true)
                    .with_density(density);
            model.notice_state.pending = crate::query::result::NoticeCounts {
                omitted: NoticeCount::Exact(4),
                shortened: NoticeCount::AtLeast(1),
                abandoned: NoticeCount::Unknown,
            };
            model.notice_details = Some(NoticeDetails {
                snapshot: model.notice_state.snapshot(),
                scroll: 0,
            });
            let area = Rect::new(0, 0, 40, 8);
            let rows = build(&model, &model.presentation, area).unwrap().rows;
            let mut observed = Vec::new();
            loop {
                let view = build(&model, &model.presentation, area).unwrap();
                assert!(view.usable);
                let mut buffer = Buffer::empty(area);
                render(&model, &model.presentation, area, &mut buffer);
                let text = crate::ui::layout::buffer_to_string(&buffer);
                for row in view
                    .rows
                    .iter()
                    .skip(view.offset)
                    .take(usize::from(view.body.height))
                {
                    assert!(
                        text.contains(row.trim_end()),
                        "actual cells contain full wrapped row"
                    );
                    observed.push(row.clone());
                }
                assert!(view.rail.join(" ").contains("Enter acknowledge and close"));
                if tier == GlyphTier::Ascii {
                    assert!(text.is_ascii());
                }
                if view.offset == view.max_scroll {
                    break;
                }
                let next = view.navigate(&Action::MovePage(Direction::Down)).unwrap();
                assert!(next > view.offset);
                model.notice_details.as_mut().unwrap().scroll = next;
            }
            assert!(
                rows.iter().all(|row| observed.contains(row)),
                "every fixed row is available, without reading proof"
            );
        }
    }
}

#[test]
fn reader_reopened_after_ack_keeps_latest_loss_summary_while_pending_is_zero() {
    use crate::app::model::{NoticeSessionId, NoticeSource, NoticeSummary};
    use crate::query::result::{NoticeCheckpoint, NoticeCounts, NoticeReport, NoticeScope};
    let mut model = Model::new(10);
    model.size = (40, 8);
    let id = NoticeSessionId::new();
    let counts = NoticeCounts {
        omitted: NoticeCount::Exact(4),
        shortened: NoticeCount::AtLeast(2),
        abandoned: NoticeCount::Unknown,
    };
    let report = NoticeReport {
        scope: NoticeScope::Query,
        retained: Some(256),
        counts,
        checkpoint: NoticeCheckpoint {
            revision: 1,
            totals: counts,
        },
        ..NoticeReport::default()
    };
    model
        .notice_state
        .install(NoticeSource::User, 1, id.clone());
    model.notice_state.observe(
        NoticeSource::User,
        1,
        &id,
        report.checkpoint,
        Some(NoticeSummary::from(&report)),
    );
    model.notice_details = Some(NoticeDetails {
        snapshot: model.notice_state.snapshot(),
        scroll: 0,
    });
    crate::app::update(&mut model, crate::app::Message::Action(Action::Activate));
    assert!(model.notice_details.is_none());
    assert!(!model.notice_state.pending.needs_report());
    model.notice_details = Some(NoticeDetails {
        snapshot: model.notice_state.snapshot(),
        scroll: 0,
    });
    let mut painted = String::new();
    let mut page = 0;
    loop {
        let area = Rect::new(0, 0, 40, 8);
        let view = build(&model, &model.presentation, area).unwrap();
        let mut buffer = Buffer::empty(area);
        render(&model, &model.presentation, area, &mut buffer);
        let text = crate::ui::layout::buffer_to_string(&buffer);
        painted.push_str(&text);
        if let Ok(directory) = std::env::var("IGNATIUS_NOTICE_CAPTURE_DIR") {
            let directory = std::path::Path::new(&directory);
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(
                directory.join(format!("notice-reader-after-ack-40x8-page-{page:02}.txt")),
                &text,
            )
            .unwrap();
        }
        if view.offset == view.max_scroll {
            break;
        }
        model.notice_details.as_mut().unwrap().scroll =
            view.navigate(&Action::MovePage(Direction::Down)).unwrap();
        page += 1;
    }
    assert!(painted.contains("Latest omitted: 4"));
    assert!(painted.contains("Latest shortened: at least 2"));
    assert!(painted.contains("Latest abandoned: an unknown"));
    assert!(!model.notice_state.pending.needs_report());
}
