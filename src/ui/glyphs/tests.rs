use super::*;

const ALL_ICONS: &[Icon] = &[
    Icon::Brand,
    Icon::Database,
    Icon::Role,
    Icon::LockVerified,
    Icon::LockPlain,
    Icon::LockOpen,
    Icon::Production,
    Icon::Environment,
    Icon::Clock,
    Icon::Rows,
    Icon::Editor,
    Icon::Help,
    Icon::Success,
    Icon::Warning,
    Icon::Error,
    Icon::Info,
    Icon::Null,
    Icon::Focus,
    Icon::SearchPath,
    Icon::Truncated,
    Icon::ReadOnly,
    Icon::ReadWrite,
    Icon::Key,
    Icon::Schema,
    Icon::Table,
    Icon::View,
    Icon::MaterializedView,
    Icon::Sequence,
    Icon::Function,
    Icon::Column,
    Icon::KeyColumn,
    Icon::Index,
    Icon::Extension,
    Icon::ChevronClosed,
    Icon::ChevronOpen,
];

#[test]
fn the_ascii_tier_emits_no_icons_at_all() {
    let glyphs = Glyphs::new(GlyphTier::Ascii);
    for icon in ALL_ICONS {
        assert_eq!(glyphs.icon(*icon), "", "{icon:?} leaked into ASCII mode");
        assert_eq!(glyphs.prefix(*icon), "");
    }
    assert!(glyphs.is_ascii());
}

#[test]
fn ascii_output_stays_within_ascii() {
    let glyphs = Glyphs::new(GlyphTier::Ascii);
    let mut text = String::new();
    text.push_str(glyphs.separator());
    text.push_str(glyphs.column_rule());
    text.push_str(&glyphs.meter(3, 10));
    for frame in 0..8 {
        text.push_str(glyphs.spinner(frame));
    }
    let borders = glyphs.borders(true);
    text.push_str(borders.top_left);
    text.push_str(borders.horizontal_top);
    assert!(text.is_ascii(), "non-ASCII reached ASCII mode: {text:?}");
}

#[test]
fn every_icon_is_defined_in_every_decorated_tier() {
    for tier in [GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        for icon in ALL_ICONS {
            assert!(
                !glyphs.icon(*icon).is_empty(),
                "{icon:?} is missing from {tier:?}"
            );
        }
    }
}

#[test]
fn icons_are_single_characters_so_layout_maths_stays_predictable() {
    for tier in [GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        for icon in ALL_ICONS {
            assert_eq!(
                glyphs.icon(*icon).chars().count(),
                1,
                "{icon:?} in {tier:?} is more than one character"
            );
        }
    }
}

#[test]
fn a_chevron_shows_whether_a_node_is_open_even_in_ascii() {
    // The tree is unusable if open and closed look the same, so this is the
    // one place ASCII gets a substitute rather than nothing.
    for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        assert_ne!(
            glyphs.chevron(true),
            glyphs.chevron(false),
            "{tier:?} draws open and closed nodes identically"
        );
        assert!(!glyphs.chevron(true).is_empty());
    }
}

#[test]
fn object_kinds_are_visually_distinct_where_icons_exist() {
    for tier in [GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        let kinds = [
            Icon::Schema,
            Icon::Table,
            Icon::View,
            Icon::MaterializedView,
            Icon::Sequence,
            Icon::Function,
            Icon::Index,
            Icon::Extension,
        ];
        for (i, a) in kinds.iter().enumerate() {
            for b in kinds.iter().skip(i + 1) {
                assert_ne!(
                    glyphs.icon(*a),
                    glyphs.icon(*b),
                    "{tier:?}: {a:?} and {b:?} look the same"
                );
            }
        }
    }
}

#[test]
fn states_that_must_be_told_apart_use_different_icons() {
    // These pairs mean materially different things. An interface that drew
    // them identically would be lying in exactly the way the constitution
    // forbids, even though the words beside them differ.
    for tier in [GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        assert_ne!(glyphs.icon(Icon::LockVerified), glyphs.icon(Icon::LockOpen));
        assert_ne!(glyphs.icon(Icon::Success), glyphs.icon(Icon::Error));
        assert_ne!(
            glyphs.icon(Icon::Production),
            glyphs.icon(Icon::Environment)
        );
        assert_ne!(glyphs.icon(Icon::ReadOnly), glyphs.icon(Icon::ReadWrite));
    }
}

#[test]
fn the_spinner_cycles_and_never_indexes_out_of_range() {
    for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        let mut seen = std::collections::BTreeSet::new();
        for frame in 0..200u64 {
            seen.insert(glyphs.spinner(frame));
        }
        assert!(seen.len() >= 4, "{tier:?} spinner barely moves");
        // A very large frame counter must not panic or wrap oddly.
        assert!(!glyphs.spinner(u64::MAX).is_empty());
    }
}

#[test]
fn the_meter_is_always_exactly_the_requested_width() {
    for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        for filled in 0..=12 {
            let meter = glyphs.meter(filled, 10);
            assert_eq!(
                meter.chars().count(),
                10,
                "{tier:?} meter with {filled} filled was the wrong width"
            );
        }
    }
}

#[test]
fn the_focused_border_differs_from_the_unfocused_one() {
    // Focus must be visible without colour, so the frame itself changes.
    for tier in [GlyphTier::Ascii, GlyphTier::Unicode, GlyphTier::Nerd] {
        let glyphs = Glyphs::new(tier);
        assert_ne!(
            glyphs.borders(true).horizontal_top,
            glyphs.borders(false).horizontal_top,
            "{tier:?} draws focused and unfocused panes identically"
        );
    }
}

#[test]
fn unicode_database_icon_uses_fisheye_fallback() {
    let glyphs = Glyphs::new(GlyphTier::Unicode);
    assert_eq!(glyphs.icon(Icon::Database), "\u{25c9}");
}
