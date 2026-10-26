//! Bounded post-connect notice admission and operation ownership.

use crate::query::result::{
    MAX_NOTICE_COUNT, MAX_NOTICE_MESSAGE_BYTES, MAX_NOTICE_TEXT_BYTES, Notice, NoticeCheckpoint,
    NoticeCount, NoticeCounts, NoticeReport, NoticeScope, bounded_notice_message,
    bounded_notice_message_len,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

struct Entry {
    notice: Notice,
    shortened: bool,
}

#[derive(Default)]
struct Bucket {
    entries: Vec<Entry>,
    spent_count: u16,
    spent_bytes: usize,
    counts: NoticeCounts,
    exhausted: bool,
}

impl Bucket {
    fn bytes(notice: &Notice) -> usize {
        notice.severity.len() + notice.message.len() + notice.code.as_deref().map_or(0, str::len)
    }

    fn privatize(&mut self) {
        for entry in &mut self.entries {
            entry.notice = super::session::private_notice(entry.notice.code.as_deref());
            entry.shortened = false;
        }
        // At a new operation boundary, charge the fixed representation only.
        self.spent_bytes = self
            .entries
            .iter()
            .map(|entry| Self::bytes(&entry.notice))
            .sum();
        self.counts.shortened = NoticeCount::Exact(0);
    }

    fn retain(&mut self, private: bool, severity: &str, message: &str, code: &str) {
        let private_message = super::session::PRIVATE_NOTICE_MESSAGE;
        let (severity_bytes, message_bytes, code_bytes) = if private {
            (
                6,
                private_message.len(),
                super::error::safe_sqlstate(code).map_or(0, str::len),
            )
        } else {
            (
                severity.len(),
                bounded_notice_message_len(message),
                code.len(),
            )
        };
        let bytes = severity_bytes
            .checked_add(message_bytes)
            .and_then(|n| n.checked_add(code_bytes));
        if self.exhausted
            || usize::from(self.spent_count) >= MAX_NOTICE_COUNT
            || bytes
                .and_then(|n| self.spent_bytes.checked_add(n))
                .is_none_or(|n| n > MAX_NOTICE_TEXT_BYTES)
        {
            self.exhausted = true;
            self.counts.omitted = self.counts.omitted.plus(NoticeCount::Exact(1));
            return;
        }
        let notice = if private {
            super::session::private_notice(Some(code))
        } else {
            Notice {
                severity: severity.to_owned(),
                message: bounded_notice_message(message),
                code: Some(code.to_owned()),
            }
        };
        if !private && message.len() > MAX_NOTICE_MESSAGE_BYTES {
            self.counts.shortened = self.counts.shortened.plus(NoticeCount::Exact(1));
        }
        self.spent_bytes += bytes.expect("admission checked fields");
        self.spent_count += 1;
        self.entries.push(Entry {
            notice,
            shortened: !private && message.len() > MAX_NOTICE_MESSAGE_BYTES,
        });
    }
}

struct Active {
    token: u64,
    scope: NoticeScope,
    prelude: Vec<Entry>,
    bucket: Bucket,
    drained: u16,
    drained_shortened: u16,
}

#[derive(Default)]
struct State {
    idle: Bucket,
    active: Option<Active>,
    next_token: u64,
    checkpoint: NoticeCheckpoint,
    // Already committed by an abandoned future, still owed to the next report.
    undelivered: NoticeCounts,
}

impl State {
    fn commit(&mut self, counts: NoticeCounts) {
        if counts.needs_report() {
            self.checkpoint.totals = self.checkpoint.totals.plus(counts);
        }
        // Every actual committed window has an order, including zero-loss
        // windows. Empty idle polling never calls commit.
        self.checkpoint.revision = self.checkpoint.revision.saturating_add(1);
    }
}

/// One bounded inbox; failures never become fabricated zero accounting.
#[derive(Default)]
pub(super) struct Notices {
    state: Mutex<State>,
}

impl std::fmt::Debug for Notices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notices").finish_non_exhaustive()
    }
}

impl Notices {
    pub(super) fn retain(&self, privacy: &AtomicBool, severity: &str, message: &str, code: &str) {
        if let Ok(mut state) = self.state.lock() {
            let private = privacy.load(Ordering::SeqCst);
            match state.active.as_mut() {
                Some(active) => active.bucket.retain(private, severity, message, code),
                None => state.idle.retain(private, severity, message, code),
            }
        }
    }

    pub(super) fn activate_privacy(&self, privacy: &AtomicBool) {
        if let Ok(mut state) = self.state.lock() {
            privacy.store(true, Ordering::SeqCst);
            // Public parameterized entry calls this before operation adoption.
            // No active payload is transferred before its private rewrite.
            state.idle.privatize();
            if let Some(active) = state.active.as_mut() {
                for entry in &mut active.prelude {
                    entry.notice = super::session::private_notice(entry.notice.code.as_deref());
                    entry.shortened = false;
                }
                let spent = active.bucket.spent_bytes;
                active.bucket.privatize();
                // Defensive late activation must not replenish active spending.
                active.bucket.spent_bytes = spent.max(active.bucket.spent_bytes);
            }
        } else {
            privacy.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn begin(&self, scope: NoticeScope) -> Operation<'_> {
        let token = if let Ok(mut state) = self.state.lock() {
            assert!(
                state.active.is_none(),
                "user SQL gate owns notice operation"
            );
            state.next_token = state.next_token.saturating_add(1);
            let token = state.next_token;
            let mut bucket = std::mem::take(&mut state.idle);
            let prelude = std::mem::take(&mut bucket.entries);
            state.active = Some(Active {
                token,
                scope,
                prelude,
                bucket,
                drained: 0,
                drained_shortened: 0,
            });
            Some(token)
        } else {
            None
        };
        Operation {
            notices: self,
            token,
            scope,
            finished: false,
        }
    }

    #[cfg(test)]
    pub(super) fn state_for_tests(&self) -> Vec<Notice> {
        self.state
            .lock()
            .unwrap()
            .idle
            .entries
            .iter()
            .map(|entry| entry.notice.clone())
            .collect()
    }

    pub(super) fn retirement_counts(&self) -> NoticeCounts {
        let Ok(state) = self.state.lock() else {
            return NoticeCounts::UNKNOWN;
        };
        let pending = state
            .active
            .as_ref()
            .map_or(state.idle.counts, |active| active.bucket.counts);
        // undelivered already belongs to checkpoint totals; never add it twice.
        state.checkpoint.totals.plus(pending)
    }

    pub(super) fn checkpoint(&self) -> NoticeCheckpoint {
        self.state
            .lock()
            .map_or(NoticeCheckpoint::UNKNOWN, |state| state.checkpoint)
    }

    pub(super) fn take_idle(&self) -> Option<NoticeReport> {
        let Ok(mut state) = self.state.lock() else {
            return Some(NoticeReport::unavailable(NoticeScope::Connection));
        };
        if state.active.is_some() {
            return None;
        }
        if state.idle.spent_count == 0
            && !state.idle.counts.needs_report()
            && !state.undelivered.needs_report()
        {
            return None;
        }
        let bucket = std::mem::take(&mut state.idle);
        state.commit(bucket.counts);
        let counts = bucket.counts.plus(std::mem::take(&mut state.undelivered));
        Some(NoticeReport {
            scope: NoticeScope::Connection,
            retained: Some(bucket.spent_count),
            counts,
            unattributed: bucket
                .entries
                .into_iter()
                .map(|entry| entry.notice)
                .collect(),
            checkpoint: state.checkpoint,
        })
    }
}

/// Finishing or dropping ownership never performs database I/O.
pub(super) struct Operation<'a> {
    notices: &'a Notices,
    token: Option<u64>,
    scope: NoticeScope,
    finished: bool,
}

impl Operation<'_> {
    pub(super) fn take_statement(&self) -> Vec<Notice> {
        let Ok(mut state) = self.notices.state.lock() else {
            return Vec::new();
        };
        let Some(active) = state
            .active
            .as_mut()
            .filter(|active| Some(active.token) == self.token)
        else {
            return Vec::new();
        };
        let entries = std::mem::take(&mut active.bucket.entries);
        active.drained += u16::try_from(entries.len()).expect("bounded notice entries");
        active.drained_shortened +=
            u16::try_from(entries.iter().filter(|entry| entry.shortened).count())
                .expect("bounded shortened entries");
        entries.into_iter().map(|entry| entry.notice).collect()
    }

    /// Preserve pre-query/probe/describe observations without attributing them
    /// to the successful query-phase payload. No allowance is replenished.
    pub(super) fn hold_unattributed(&self) {
        let Ok(mut state) = self.notices.state.lock() else {
            return;
        };
        if let Some(active) = state
            .active
            .as_mut()
            .filter(|active| Some(active.token) == self.token)
        {
            active.prelude.append(&mut active.bucket.entries);
        }
    }

    pub(super) fn finish(mut self) -> NoticeReport {
        self.finished = true;
        let Ok(mut state) = self.notices.state.lock() else {
            return NoticeReport::unavailable(self.scope);
        };
        let Some(mut active) = state
            .active
            .take()
            .filter(|active| Some(active.token) == self.token)
        else {
            return NoticeReport::unavailable(self.scope);
        };
        state.commit(active.bucket.counts);
        let counts = active
            .bucket
            .counts
            .plus(std::mem::take(&mut state.undelivered));
        active.prelude.append(&mut active.bucket.entries);
        NoticeReport {
            scope: active.scope,
            retained: Some(active.bucket.spent_count),
            counts,
            unattributed: active
                .prelude
                .into_iter()
                .map(|entry| entry.notice)
                .collect(),
            checkpoint: state.checkpoint,
        }
    }
}

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let Ok(mut state) = self.notices.state.lock() else {
            return;
        };
        if !state
            .active
            .as_ref()
            .is_some_and(|active| Some(active.token) == self.token)
        {
            return;
        }
        let mut active = state.active.take().expect("verified active ownership");
        active.bucket.counts.abandoned = active
            .bucket
            .counts
            .abandoned
            .plus(NoticeCount::Exact(u64::from(active.drained)));
        // Pending shortening remains entry provenance until actual transfer;
        // a first privacy rewrite can suppress it without undoing public totals.
        active.bucket.counts.shortened = NoticeCount::Exact(u64::from(active.drained_shortened));
        state.commit(active.bucket.counts);
        state.undelivered = state.undelivered.plus(active.bucket.counts);
        active.prelude.append(&mut active.bucket.entries);
        // The abandoned operation ended. Pending payload starts one bounded
        // idle window; already-drained payload is accounted, never recreated.
        let spent_bytes = active
            .prelude
            .iter()
            .map(|entry| Bucket::bytes(&entry.notice))
            .sum();
        let shortened = u64::try_from(
            active
                .prelude
                .iter()
                .filter(|entry| entry.shortened)
                .count(),
        )
        .expect("bounded pending shortening");
        state.idle = Bucket {
            spent_count: u16::try_from(active.prelude.len()).expect("bounded pending notices"),
            spent_bytes,
            entries: active.prelude,
            counts: NoticeCounts {
                shortened: NoticeCount::Exact(shortened),
                ..NoticeCounts::default()
            },
            ..Bucket::default()
        };
    }
}

#[cfg(test)]
mod tests;
