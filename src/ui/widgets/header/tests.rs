//! Focused tests for the header's connecting and failure wording.
//!
//! The header owns the facts that change rarely but matter constantly. These
//! tests pin the connecting step vocabulary, the elapsed time it adds once the
//! runtime reports one, and the failed stage a failure must name. Every
//! assertion is on words, because the meaning has to survive colour off and the
//! ASCII glyph tier.

use super::{render_compact_header, render_header};
use crate::app::model::{ConnectingStep, ConnectionState, Model};
use crate::config::ThemeChoice;
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::ui::glyphs::{GlyphTier, Glyphs};
use crate::ui::keymap::Keymap;
use crate::ui::layout::{Presentation, buffer_to_string};
use crate::ui::theme::{ColorDepth, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::time::Duration;

fn presentation(tier: GlyphTier, depth: ColorDepth) -> Presentation {
    Presentation::new(
        Theme::new(ThemeChoice::Dark, depth != ColorDepth::None).with_depth(depth),
        Glyphs::new(tier),
        false,
    )
}

/// A model mid-connection, with the runtime having reported one stage.
fn connecting(step: ConnectingStep) -> Model {
    let mut model = Model::new(100);
    model.connection = ConnectionState::Connecting {
        step,
        target: "app@localhost:5432/orders".into(),
    };
    model.connecting_step = Some(step);
    model
}

fn header_text(model: &Model, presentation: &Presentation, width: u16) -> String {
    let area = Rect::new(0, 0, width, 1);
    let mut buf = Buffer::empty(area);
    render_header(model, presentation, area, &mut buf);
    buffer_to_string(&buf)
}

fn diagnostic() -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Connection,
        "certificate verify failed",
        "opening the connection",
    )
}

#[test]
fn a_connecting_header_names_the_step_the_target_and_the_elapsed_time() {
    let mut model = connecting(ConnectingStep::ServerHandshake);
    model.running_for = Some(Duration::from_millis(1500));
    let text = header_text(
        &model,
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        120,
    );
    assert!(text.contains("server handshake"), "{text}");
    assert!(text.contains("app@localhost:5432/orders"), "{text}");
    assert!(
        text.contains("1.5s"),
        "the elapsed connecting time is shown: {text}"
    );
}

#[test]
fn the_connecting_header_names_exactly_one_current_step() {
    let model = connecting(ConnectingStep::TlsHandshake);
    let text = header_text(
        &model,
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        120,
    );
    assert!(text.contains("TLS handshake"), "{text}");
    for absent in [
        "resolving profile",
        "acquiring credential",
        "server handshake",
        "loading catalogue",
    ] {
        assert!(
            !text.contains(absent),
            "a stage the runtime did not report appeared: {absent}\n{text}"
        );
    }
}

#[test]
fn a_connecting_header_omits_the_elapsed_time_until_one_is_reported() {
    let model = connecting(ConnectingStep::LoadingCatalogue);
    let text = header_text(
        &model,
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        120,
    );
    assert!(text.contains("loading catalogue"), "{text}");
    assert!(
        !text.contains("0.0s"),
        "no elapsed time was invented before the runtime reported one: {text}"
    );
}

#[test]
fn a_failed_connection_names_the_stage_it_failed_in() {
    let mut model = connecting(ConnectingStep::TlsHandshake);
    model.connection = ConnectionState::Failed(Box::new(diagnostic()));
    let text = header_text(
        &model,
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        120,
    );
    assert!(text.contains("failed during TLS handshake"), "{text}");
    assert!(
        !text.contains("complete") && !text.contains("completed"),
        "no stage may be called complete: {text}"
    );
    for absent in [
        "resolving profile",
        "acquiring credential",
        "server handshake",
        "loading catalogue",
    ] {
        assert!(
            !text.contains(absent),
            "another stage appeared: {absent}\n{text}"
        );
    }
}

#[test]
fn a_failed_connection_shows_the_step_beside_the_unchanged_diagnostic() {
    let diagnostic = diagnostic();
    let mut model = connecting(ConnectingStep::TlsHandshake);
    model.error = Some(diagnostic.clone());
    model.connection = ConnectionState::Failed(Box::new(diagnostic));
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        100,
        30,
    );
    assert!(text.contains("failed during TLS handshake"), "{text}");
    assert!(
        text.contains("certificate verify failed"),
        "the diagnostic wording changed: {text}"
    );
    assert!(text.contains("While: opening the connection"), "{text}");
}

#[test]
fn the_connecting_header_keeps_its_words_at_every_tier_and_depth() {
    let model = connecting(ConnectingStep::AcquiringCredential);
    for (label, presentation) in [
        (
            "truecolor",
            presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        ),
        (
            "256",
            presentation(GlyphTier::Unicode, ColorDepth::Indexed256),
        ),
        ("16", presentation(GlyphTier::Ascii, ColorDepth::Basic16)),
        ("none", presentation(GlyphTier::Ascii, ColorDepth::None)),
    ] {
        let text = header_text(&model, &presentation, 120);
        assert!(text.contains("acquiring credential"), "{label}: {text}");
        assert!(
            text.contains("app@localhost:5432/orders"),
            "{label}: {text}"
        );
        if presentation.glyphs.is_ascii() {
            assert!(
                text.is_ascii(),
                "{label}: non-ASCII reached ASCII mode: {text:?}"
            );
        }
    }
}

#[test]
fn the_compact_header_still_names_the_connecting_step() {
    let model = connecting(ConnectingStep::ServerHandshake);
    let area = Rect::new(0, 0, 100, 1);
    let mut buf = Buffer::empty(area);
    render_compact_header(
        &model,
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        area,
        &mut buf,
    );
    let text = buffer_to_string(&buf);
    assert!(text.contains("server handshake"), "{text}");
    assert!(text.contains("app@localhost:5432/orders"), "{text}");
    assert!(
        !text.contains("not connected"),
        "the compact header contradicted the connecting state: {text}"
    );
}

#[test]
fn every_pre_connection_frame_paints_the_whole_shell() {
    let mut model = connecting(ConnectingStep::ResolvingProfile);
    model.running_for = Some(Duration::from_millis(400));
    let text = crate::ui::layout::render_to_string(
        &model,
        &Keymap::new(),
        &presentation(GlyphTier::Unicode, ColorDepth::TrueColor),
        120,
        30,
    );
    assert!(text.contains("resolving profile"), "{text}");
    assert!(
        text.contains("0.4s"),
        "the connecting elapsed time is painted in the shell: {text}"
    );
    assert!(
        text.contains("Objects"),
        "the object pane frame is painted: {text}"
    );
    assert!(
        text.contains("Editor"),
        "the editor pane frame is painted: {text}"
    );
    assert!(
        text.contains("Results"),
        "the results pane frame is painted: {text}"
    );
    assert!(text.contains("Ready"), "the footer is painted: {text}");
    assert!(
        text.contains("Quit"),
        "a visible way to quit is offered while connecting: {text}"
    );
}

#[test]
fn the_elapsed_clock_survives_reduced_depth_and_ascii() {
    let mut model = connecting(ConnectingStep::ServerHandshake);
    model.running_for = Some(Duration::from_millis(1500));
    for (tier, depth) in [
        (GlyphTier::Ascii, ColorDepth::Basic16),
        (GlyphTier::Ascii, ColorDepth::None),
    ] {
        let text = header_text(&model, &presentation(tier, depth), 120);
        assert!(
            text.contains("server handshake"),
            "{tier:?}/{depth:?}: {text}"
        );
        assert!(text.contains("1.5s"), "{tier:?}/{depth:?}: {text}");
    }
}
