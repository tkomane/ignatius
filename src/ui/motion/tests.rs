use super::*;
use crate::config::ThemeChoice;

const fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn every_easing_starts_at_zero_ends_at_one_and_never_goes_backwards() {
    for easing in [Easing::Linear, Easing::OutCubic, Easing::InOutCubic] {
        assert_eq!(easing.apply(0.0), 0.0, "{easing:?}");
        assert_eq!(easing.apply(1.0), 1.0, "{easing:?}");
        let mut previous = 0.0_f32;
        for step in 0..=100 {
            let value = easing.apply(step as f32 / 100.0);
            assert!((0.0..=1.0).contains(&value), "{easing:?} {step}");
            assert!(value >= previous, "{easing:?} went backwards at {step}");
            previous = value;
        }
    }
}

#[test]
fn easing_clamps_out_of_range_progress_and_treats_nan_as_finished() {
    for easing in [Easing::Linear, Easing::OutCubic, Easing::InOutCubic] {
        assert_eq!(easing.apply(-3.0), 0.0);
        assert_eq!(easing.apply(7.0), 1.0);
        assert_eq!(easing.apply(f32::NAN), 1.0);
    }
    assert_eq!(linear(0.25), 0.25);
}

#[test]
fn the_curves_have_their_documented_shape() {
    assert!(
        ease_out_cubic(0.5) > 0.5,
        "out-cubic is ahead at the midpoint"
    );
    assert!((ease_in_out_cubic(0.5) - 0.5).abs() < 1e-6);
    assert!(ease_in_out_cubic(0.25) < 0.25, "in-out starts gently");
    assert!(ease_in_out_cubic(0.75) > 0.75, "in-out ends gently");
}

#[test]
fn the_duration_table_is_indexed_by_kind_and_bounded() {
    for (index, kind) in TransitionKind::ALL.iter().enumerate() {
        assert_eq!(TABLE[index].kind, *kind, "the table order follows the kind");
        assert_eq!(*kind as usize, index);
        assert!(kind.duration() > Duration::ZERO);
        assert!(kind.duration() <= LONGEST, "{kind:?} is longer than 800 ms");
    }
    assert_eq!(TransitionKind::ThemeCurtain.duration(), ms(320));
    assert_eq!(TransitionKind::ThemeCurtain.easing(), Easing::OutCubic);
    let settle = TransitionKind::ResultSettle.duration();
    assert!(settle >= ms(450) && settle <= ms(600), "{settle:?}");
    assert_eq!(TransitionKind::ResultSettle.easing(), Easing::OutCubic);
    assert_eq!(LONGEST, ms(800));
}

#[test]
fn the_motion_cadence_is_about_thirty_frames_a_second() {
    assert_eq!(FRAME_INTERVAL, ms(33));
    assert!(FRAME_INTERVAL < ms(90), "faster than the busy ticker");
}

#[test]
fn a_transition_reports_zero_at_its_start_one_at_its_end_and_settles() {
    for kind in TransitionKind::ALL {
        let start = ms(1_000);
        let transition = Transition::new(kind, start);
        let end = start + kind.duration();
        assert_eq!(transition.duration, kind.duration());
        assert_eq!(transition.progress(start), 0.0, "{kind:?}");
        assert!(!transition.is_settled(start));
        let middle = transition.progress(start + kind.duration() / 2);
        assert!(middle > 0.0 && middle < 1.0, "{kind:?} {middle}");
        assert!(!transition.is_settled(end - ms(1)));
        assert_eq!(transition.progress(end), 1.0);
        assert!(transition.is_settled(end));
        assert_eq!(transition.progress(end + ms(5_000)), 1.0);
        assert!(transition.is_settled(end + ms(5_000)));
        // A time before the start cannot happen on a monotonic clock, but if
        // it did it would draw the first frame, not panic.
        assert_eq!(transition.progress(ms(0)), 0.0);
    }
}

#[test]
fn a_zero_length_transition_is_already_finished() {
    let transition = Transition {
        kind: TransitionKind::ResultSettle,
        start: ms(10),
        duration: Duration::ZERO,
    };
    assert_eq!(transition.linear_progress(ms(10)), 1.0);
    assert!(transition.is_settled(ms(10)));
}

#[test]
fn a_fresh_motion_is_disabled_still_and_reports_nothing() {
    let motion = Motion::default();
    assert!(!motion.is_enabled());
    assert!(!motion.is_active());
    assert_eq!(motion.clock(), Duration::ZERO);
    for kind in TransitionKind::ALL {
        assert_eq!(motion.progress(kind), None);
    }
    assert!(motion.theme_curtain().is_none());
    assert!(motion.result_settle().is_none());
}

#[test]
fn each_kind_starts_at_the_current_clock_and_carries_its_detail() {
    let mut motion = Motion::default();
    motion.advance(ms(500));
    let outgoing = Theme::new(ThemeChoice::Paper, true);
    motion.begin_theme_curtain(outgoing.clone());
    motion.begin_result_settle(JobId(7));
    assert!(motion.is_active());
    for kind in TransitionKind::ALL {
        assert_eq!(motion.progress(kind), Some(0.0), "{kind:?}");
    }
    assert_eq!(motion.theme_curtain(), Some((&outgoing, 0.0)));
    assert_eq!(motion.result_settle(), Some((JobId(7), 0.0)));

    motion.advance(ms(160));
    let curtain = motion.progress(TransitionKind::ThemeCurtain).unwrap();
    assert!(curtain > 0.5, "out-cubic is past half way at half time");
    assert!(motion.progress(TransitionKind::ResultSettle).unwrap() < curtain);
}

#[test]
fn advancing_drops_each_transition_once_it_settles() {
    let mut motion = Motion::default();
    motion.begin_theme_curtain(Theme::new(ThemeChoice::Dark, true));
    motion.begin_result_settle(JobId(1));

    motion.advance(ms(319));
    assert!(motion.theme_curtain().is_some());
    motion.advance(ms(1));
    assert!(motion.theme_curtain().is_none(), "the curtain lasts 320 ms");
    assert!(motion.is_active(), "the result settle is still running");
    motion.advance(TransitionKind::ResultSettle.duration() - ms(320));
    assert!(motion.result_settle().is_none());
    assert!(!motion.is_active(), "everything settled, nothing wakes");
    assert_eq!(motion.clock(), TransitionKind::ResultSettle.duration());
}

#[test]
fn settling_jumps_every_transition_to_its_final_frame() {
    let mut motion = Motion::default();
    motion.begin_theme_curtain(Theme::new(ThemeChoice::Dark, true));
    motion.begin_result_settle(JobId(3));
    motion.settle();
    assert!(!motion.is_active());
    for kind in TransitionKind::ALL {
        assert_eq!(motion.progress(kind), None);
    }
}

#[test]
fn starting_a_kind_again_replaces_it_rather_than_stacking() {
    let mut motion = Motion::default();
    motion.begin_result_settle(JobId(1));
    motion.advance(ms(400));
    motion.begin_result_settle(JobId(2));
    assert_eq!(motion.result_settle(), Some((JobId(2), 0.0)));
    motion.advance(TransitionKind::ResultSettle.duration() - ms(1));
    assert!(
        motion.result_settle().is_some(),
        "measured from the restart"
    );
}

#[test]
fn an_unpruned_but_finished_transition_reads_as_settled() {
    // The clock only moves through `advance`, which prunes, but the read API
    // must never report a finished transition as moving.
    let mut motion = Motion::default();
    motion.begin_theme_curtain(Theme::new(ThemeChoice::Light, true));
    motion.clock = ms(320);
    assert_eq!(motion.progress(TransitionKind::ThemeCurtain), None);
    assert!(motion.theme_curtain().is_none());
}

#[test]
fn the_curtain_runs_from_all_outgoing_to_all_incoming() {
    for width in [1_u16, 7, 80, 213] {
        for column in 0..width {
            assert!(!curtain_shows_incoming(column, width, 0.0));
            assert!(curtain_shows_incoming(column, width, 1.0));
            assert!(curtain_shows_incoming(column, width, f32::NAN));
        }
    }
}

#[test]
fn the_curtain_edge_is_hard_and_moves_left_to_right() {
    let width = 64;
    for step in 0..=64 {
        let progress = step as f32 / 64.0;
        let incoming: Vec<bool> = (0..width)
            .map(|column| curtain_shows_incoming(column, width, progress))
            .collect();
        assert_eq!(
            incoming.iter().filter(|shown| **shown).count(),
            step as usize,
            "one column per step"
        );
        for pair in incoming.windows(2) {
            assert!(pair[0] || !pair[1], "incoming is always to the left");
        }
    }
}
