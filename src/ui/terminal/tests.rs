use super::*;

fn sequences(options: TerminalOptions) -> (String, String) {
    let mut entered = Vec::new();
    enter(&mut entered, options).expect("enter");
    let mut left = Vec::new();
    leave(&mut left, options).expect("leave");
    (
        String::from_utf8(entered).expect("utf8"),
        String::from_utf8(left).expect("utf8"),
    )
}

#[test]
fn defaults_take_the_alternate_screen_and_leave_the_mouse_alone() {
    let options = TerminalOptions::default();
    assert!(options.alternate_screen);
    assert!(options.bracketed_paste);
    assert!(!options.mouse, "mouse capture must be opt-in");

    let (entered, left) = sequences(options);
    assert!(entered.contains("\x1b[?1049h"), "alternate screen entered");
    assert!(entered.contains("\x1b[?2004h"), "bracketed paste enabled");
    assert!(entered.contains("\x1b[?25l"), "cursor hidden");
    assert!(
        !entered.contains("\x1b[?1000h"),
        "mouse not captured by default"
    );

    assert!(left.contains("\x1b[?25h"), "cursor shown again");
    assert!(left.contains("\x1b[?2004l"), "bracketed paste disabled");
    assert!(left.contains("\x1b[?1049l"), "alternate screen left");
}

#[test]
fn every_mode_that_is_enabled_is_disabled_again() {
    let options = TerminalOptions {
        mouse: true,
        alternate_screen: true,
        bracketed_paste: true,
    };
    let (entered, left) = sequences(options);
    let mut modes = vec![
        ("\x1b[?1049h", "\x1b[?1049l", "alternate screen"),
        ("\x1b[?2004h", "\x1b[?2004l", "bracketed paste"),
        ("\x1b[?25l", "\x1b[?25h", "cursor visibility"),
    ];
    // Mouse capture is the one mode Crossterm does not express as an escape
    // sequence on Windows: it goes through the console API instead, so no
    // bytes reach this writer. The call is still made and still undone,
    // which is what matters; only its observability differs.
    if cfg!(unix) {
        modes.push(("\x1b[?1000h", "\x1b[?1000l", "mouse capture"));
    }
    for (on, off, what) in modes {
        assert!(entered.contains(on), "{what} was never enabled");
        assert!(left.contains(off), "{what} was left enabled");
    }
}

#[test]
fn restore_order_is_the_reverse_of_acquisition() {
    let options = TerminalOptions {
        mouse: true,
        ..TerminalOptions::default()
    };
    let (_, left) = sequences(options);
    let cursor = left.find("\x1b[?25h").expect("cursor restored");
    let paste = left.find("\x1b[?2004l").expect("paste restored");
    let screen = left.find("\x1b[?1049l").expect("screen left");

    // Mouse capture is released through the console API on Windows and so
    // has no sequence to order; where it does emit one, it belongs between
    // the cursor and the paste mode.
    #[cfg(unix)]
    {
        let mouse = left.find("\x1b[?1000l").expect("mouse released");
        assert!(cursor < mouse && mouse < paste, "mouse released in order");
    }

    assert!(
        cursor < paste && paste < screen,
        "the alternate screen must be left last so the restored state applies to the user's screen"
    );
}

#[test]
fn disabled_options_emit_nothing_for_those_modes() {
    let options = TerminalOptions {
        mouse: false,
        alternate_screen: false,
        bracketed_paste: false,
    };
    let (entered, left) = sequences(options);
    assert!(!entered.contains("\x1b[?1049h"));
    assert!(!left.contains("\x1b[?1049l"));
    assert!(!entered.contains("\x1b[?2004h"));
    assert_eq!(entered, "\x1b[?25l", "only the cursor is touched");
}

#[test]
fn no_color_and_dumb_terminals_win_over_configuration() {
    use crate::config::ColorMode;
    let base = TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: None,
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
    };

    assert!(should_use_color(ColorMode::Auto, &base));
    assert!(should_use_color(ColorMode::Always, &base));
    assert!(!should_use_color(ColorMode::Never, &base));

    let no_color = TerminalFacts {
        no_color: true,
        ..base.clone()
    };
    assert!(
        !should_use_color(ColorMode::Always, &no_color),
        "NO_COLOR is not negotiable"
    );

    let dumb = TerminalFacts {
        term: Some("dumb".into()),
        ..base.clone()
    };
    assert!(!should_use_color(ColorMode::Always, &dumb));

    let piped = TerminalFacts {
        is_terminal: false,
        ..base
    };
    assert!(
        !should_use_color(ColorMode::Auto, &piped),
        "piped output stays plain"
    );
    assert!(
        should_use_color(ColorMode::Always, &piped),
        "explicit --color=always is honoured"
    );
}

/// A capable truecolor terminal with no explicit depth choice. Detection tests
/// are pure over this fact set and never read the process environment.
fn detection_facts() -> TerminalFacts {
    TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: None,
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
    }
}

#[test]
fn forced_off_routes_win_over_an_explicit_choice() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let base = detection_facts();

    let no_color = TerminalFacts {
        no_color: true,
        ..base.clone()
    };
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            true,
            &no_color
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "NO_COLOR defeats an explicit override"
    );

    let dumb = TerminalFacts {
        term: Some("dumb".into()),
        colorterm: Some("truecolor".into()),
        ..base.clone()
    };
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            true,
            &dumb
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "TERM=dumb defeats an explicit override"
    );

    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            false,
            &base
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "colour resolved off defeats an explicit override"
    );
}

#[test]
fn an_explicit_choice_wins_over_detection_and_reports_its_source() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let detected = TerminalFacts {
        colorterm: Some("truecolor".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };

    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::Basic16,
            Some(DepthSource::Flag),
            true,
            &detected
        ),
        (ColorDepth::Basic16, DepthSource::Flag)
    );
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::Indexed256,
            Some(DepthSource::Configuration),
            true,
            &detected
        ),
        (ColorDepth::Indexed256, DepthSource::Configuration)
    );
    assert_eq!(
        resolve_color_depth(
            ColorDepthChoice::TrueColor,
            Some(DepthSource::Flag),
            true,
            &detected
        ),
        (ColorDepth::TrueColor, DepthSource::Flag)
    );
}

#[test]
fn auto_reads_colorterm_case_sensitively() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let truecolor = TerminalFacts {
        colorterm: Some("truecolor".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &truecolor),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );

    let twenty_four_bit = TerminalFacts {
        colorterm: Some("24bit".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &twenty_four_bit),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );

    // The convention is case sensitive: "TRUECOLOR" is not a hint, so the
    // lower-tier TERM fallback decides instead.
    let shouted = TerminalFacts {
        colorterm: Some("TRUECOLOR".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &shouted),
        (ColorDepth::Indexed256, DepthSource::Term)
    );
}

#[test]
fn auto_reads_term_and_falls_back_to_sixteen() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let indexed = TerminalFacts {
        colorterm: None,
        term: Some("tmux-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &indexed),
        (ColorDepth::Indexed256, DepthSource::Term)
    );

    let basic = TerminalFacts {
        colorterm: None,
        term: Some("xterm".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &basic),
        (ColorDepth::Basic16, DepthSource::Default)
    );
}

#[test]
fn colorterm_wins_over_term() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};
    let both = TerminalFacts {
        colorterm: Some("truecolor".into()),
        term: Some("xterm-256color".into()),
        ..detection_facts()
    };
    assert_eq!(
        resolve_color_depth(ColorDepthChoice::Auto, None, true, &both),
        (ColorDepth::TrueColor, DepthSource::Colorterm)
    );
}
