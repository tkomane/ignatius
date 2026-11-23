//! Short, bounded transitions that make a state change easier to see.
//!
//! Everything here is pure. A transition never decides what the screen says;
//! the model has already reached its new state by the time one starts, and a
//! transition only chooses how that state is painted for a fraction of a
//! second. Seven rules hold every transition in this module:
//!
//! 1. **State first.** The new state is in the model before the transition
//!    starts, so every word, focus marker and safety badge is in the first
//!    frame.
//! 2. **Bounded.** Each kind has one fixed duration in [`TABLE`], and none is
//!    longer than [`LONGEST`].
//! 3. **Input wins.** Any key press settles every transition before the key is
//!    handled.
//! 4. **Reduced motion is stillness.** Nothing starts under reduced motion,
//!    ASCII glyphs, colour off or a 16-colour terminal.
//! 5. **Depth-honest colour.** Truecolour may interpolate, 256 colours step at
//!    the midpoint, and lower depths show the final state immediately.
//! 6. **Idle sleeps.** A settled client has no active transition, so the
//!    runtime wakes nothing.
//! 7. **Safety surfaces never move.** The header and footer rails change in a
//!    single frame, and while a confirmation, review or prompt is open nothing
//!    moves at all.
//!
//! The clock is a plain [`Duration`] that only a runtime message advances. The
//! reducer reads no clock, so any frame of any transition is reproducible in a
//! test by choosing the motion time.

use crate::query::result::JobId;
use crate::ui::theme::Theme;
use std::time::Duration;

/// How often the runtime delivers a motion frame while a transition is active.
///
/// About thirty frames a second. This cadence is separate from the busy
/// ticker, so spinner speed, the boot reveal and double-click timing are not
/// affected by it.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// The longest any transition may last.
pub const LONGEST: Duration = Duration::from_millis(800);

/// A shaping curve from linear progress to painted progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    /// Progress as measured.
    Linear,
    /// Starts quickly and comes to rest gently.
    OutCubic,
    /// Starts and ends gently, quickest in the middle.
    InOutCubic,
}

impl Easing {
    /// Applies the curve to linear progress, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn apply(self, progress: f32) -> f32 {
        let t = unit(progress);
        match self {
            Self::Linear => t,
            Self::OutCubic => ease_out_cubic(t),
            Self::InOutCubic => ease_in_out_cubic(t),
        }
    }
}

/// Progress as measured, clamped to `0.0..=1.0`.
#[must_use]
pub fn linear(progress: f32) -> f32 {
    unit(progress)
}

/// A cubic curve that starts quickly and comes to rest gently.
#[must_use]
pub fn ease_out_cubic(progress: f32) -> f32 {
    let rest = 1.0 - unit(progress);
    1.0 - rest * rest * rest
}

/// A cubic curve that starts and ends gently.
#[must_use]
pub fn ease_in_out_cubic(progress: f32) -> f32 {
    let t = unit(progress);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let rest = -2.0 * t + 2.0;
        1.0 - rest * rest * rest / 2.0
    }
}

/// Clamps progress to `0.0..=1.0`. A value that is not a number is treated as
/// finished, because the settled frame is always a safe frame to show.
fn unit(progress: f32) -> f32 {
    if progress.is_nan() {
        1.0
    } else {
        progress.clamp(0.0, 1.0)
    }
}

/// The kinds of transition the interface knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum TransitionKind {
    /// A new theme sweeps across the pane body from left to right.
    ThemeCurtain,
    /// The status word of a complete, successful result fades from the
    /// success colour to its resting colour.
    ResultSettle,
}

impl TransitionKind {
    /// Every kind, in [`TABLE`] order.
    pub const ALL: [Self; 2] = [Self::ThemeCurtain, Self::ResultSettle];

    /// How long this kind lasts, from the one duration table.
    #[must_use]
    pub const fn duration(self) -> Duration {
        TABLE[self as usize].duration
    }

    /// The curve this kind is painted with, from the one duration table.
    #[must_use]
    pub const fn easing(self) -> Easing {
        TABLE[self as usize].easing
    }
}

/// One row of the duration table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Which kind this row describes.
    pub kind: TransitionKind,
    /// How long it lasts.
    pub duration: Duration,
    /// How its progress is shaped.
    pub easing: Easing,
}

/// The one duration table. Every transition's length and curve live here and
/// nowhere else, indexed by [`TransitionKind`].
pub const TABLE: [Timing; 2] = [
    Timing {
        kind: TransitionKind::ThemeCurtain,
        duration: Duration::from_millis(320),
        easing: Easing::OutCubic,
    },
    Timing {
        kind: TransitionKind::ResultSettle,
        duration: Duration::from_millis(520),
        easing: Easing::OutCubic,
    },
];

/// One transition, measured on the motion clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    /// What is moving.
    pub kind: TransitionKind,
    /// The motion time it started at.
    pub start: Duration,
    /// How long it lasts.
    pub duration: Duration,
}

impl Transition {
    /// A transition of `kind` starting at motion time `start`, with the
    /// duration from the table.
    #[must_use]
    pub const fn new(kind: TransitionKind, start: Duration) -> Self {
        Self {
            kind,
            start,
            duration: kind.duration(),
        }
    }

    /// Progress as measured at motion time `now`, from `0.0` to `1.0`.
    #[must_use]
    pub fn linear_progress(&self, now: Duration) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_sub(self.start);
        unit(elapsed.as_secs_f32() / self.duration.as_secs_f32())
    }

    /// Eased progress at motion time `now`, from `0.0` to `1.0`.
    #[must_use]
    pub fn progress(&self, now: Duration) -> f32 {
        self.kind.easing().apply(self.linear_progress(now))
    }

    /// Whether the transition has reached its final frame at `now`.
    #[must_use]
    pub fn is_settled(&self, now: Duration) -> bool {
        now >= self.start.saturating_add(self.duration)
    }
}

/// The motion clock and every active transition.
///
/// Each kind has one slot, so starting a kind again replaces it rather than
/// stacking a second copy. A freshly built value is still and stays still
/// until the interactive runtime enables it: models built by tests and the
/// showcase therefore always render settled.
#[derive(Debug, Clone, Default)]
pub struct Motion {
    enabled: bool,
    clock: Duration,
    curtain: Option<(Transition, Theme)>,
    result: Option<(Transition, JobId)>,
}

impl Motion {
    /// Allows transitions to start. Only the interactive runtime does this.
    pub const fn enable(&mut self) {
        self.enabled = true;
    }

    /// Whether transitions may start at all.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The current motion time.
    #[must_use]
    pub const fn clock(&self) -> Duration {
        self.clock
    }

    /// Whether any transition is active, which is what keeps motion frames
    /// coming.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.curtain.is_some() || self.result.is_some()
    }

    /// Eased progress of `kind`, or `None` when it is not active.
    ///
    /// `None` means "draw the settled frame". A widget must draw exactly the
    /// same thing for `None` as it would with no motion at all.
    #[must_use]
    pub fn progress(&self, kind: TransitionKind) -> Option<f32> {
        let transition = match kind {
            TransitionKind::ThemeCurtain => self.curtain.as_ref().map(|(t, _)| *t),
            TransitionKind::ResultSettle => self.result.as_ref().map(|(t, _)| *t),
        }?;
        (!transition.is_settled(self.clock)).then(|| transition.progress(self.clock))
    }

    /// The outgoing theme and eased progress while the theme curtain runs.
    #[must_use]
    pub fn theme_curtain(&self) -> Option<(&Theme, f32)> {
        let progress = self.progress(TransitionKind::ThemeCurtain)?;
        self.curtain.as_ref().map(|(_, theme)| (theme, progress))
    }

    /// The job whose successful result is settling, and eased progress.
    ///
    /// A widget tints only the result of this job, so a settle can never
    /// colour a different result than the one that finished.
    #[must_use]
    pub fn result_settle(&self) -> Option<(JobId, f32)> {
        let progress = self.progress(TransitionKind::ResultSettle)?;
        self.result.map(|(_, job)| (job, progress))
    }

    /// Advances the motion clock and drops every transition that has settled.
    pub fn advance(&mut self, elapsed: Duration) {
        self.clock = self.clock.saturating_add(elapsed);
        let now = self.clock;
        if self
            .curtain
            .as_ref()
            .is_some_and(|(t, _)| t.is_settled(now))
        {
            self.curtain = None;
        }
        if self.result.as_ref().is_some_and(|(t, _)| t.is_settled(now)) {
            self.result = None;
        }
    }

    /// Jumps every transition to its final frame.
    pub fn settle(&mut self) {
        self.curtain = None;
        self.result = None;
    }

    /// Starts the theme curtain away from `outgoing`.
    pub fn begin_theme_curtain(&mut self, outgoing: Theme) {
        self.curtain = Some((self.start(TransitionKind::ThemeCurtain), outgoing));
    }

    /// Starts the settle of `job`'s complete, successful result.
    pub fn begin_result_settle(&mut self, job: JobId) {
        self.result = Some((self.start(TransitionKind::ResultSettle), job));
    }

    const fn start(&self, kind: TransitionKind) -> Transition {
        Transition::new(kind, self.clock)
    }
}

/// Whether `column` of a `width`-column body already shows the incoming theme
/// at curtain progress `progress`.
///
/// The edge is hard: every cell is wholly in one theme or the other, because a
/// blend between a dark and a light palette passes through colours where text
/// has no contrast at all. Progress `0.0` is entirely outgoing and `1.0`
/// entirely incoming.
#[must_use]
pub fn curtain_shows_incoming(column: u16, width: u16, progress: f32) -> bool {
    f32::from(column) < unit(progress) * f32::from(width)
}

#[cfg(test)]
mod tests;
