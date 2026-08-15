//! The result model.
//!
//! Results are bounded in memory by construction: a [`ResultSet`] stops keeping
//! rows once it reaches its cap and records that it did. Truncation is therefore
//! a fact the interface can state, never a silent omission, and the client never
//! re-runs a query to page through it.
//!
//! Values arrive in PostgreSQL's text format, exactly as the server rendered
//! them. Numerics keep their precision, timestamps keep their offset, and types
//! this build has never heard of still display correctly.

use crate::query::value::Cell;
use std::time::Duration;

/// A message the server sent alongside a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Server severity, for example `NOTICE` or `WARNING`.
    pub severity: String,
    /// The message text.
    pub message: String,
    /// SQLSTATE, when the server supplied one.
    pub code: Option<String>,
}

/// Rows returned by one statement, bounded by a cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultSet {
    /// Column names in order.
    pub columns: Vec<String>,
    /// Retained rows. Never longer than the cap.
    pub rows: Vec<Vec<Cell>>,
    /// Total rows the server sent, including any beyond the cap.
    pub rows_seen: u64,
    /// The cap that was applied.
    pub cap: usize,
}

impl ResultSet {
    /// Creates an empty set with a row cap.
    #[must_use]
    pub const fn new(columns: Vec<String>, cap: usize) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            rows_seen: 0,
            cap,
        }
    }

    /// Adds a row, keeping it only while the cap allows.
    ///
    /// Returns whether the row was retained, so a caller streaming to a file can
    /// tell the difference between "not kept" and "not received".
    pub fn push(&mut self, row: Vec<Cell>) -> bool {
        self.rows_seen += 1;
        if self.rows.len() < self.cap {
            self.rows.push(row);
            true
        } else {
            false
        }
    }

    /// Whether rows were dropped because of the cap.
    #[must_use]
    pub fn is_truncated(&self) -> bool {
        self.rows_seen > self.rows.len() as u64
    }

    /// Rows currently held in memory.
    #[must_use]
    pub fn retained(&self) -> usize {
        self.rows.len()
    }

    /// A short description of what is being shown, always stating truncation.
    #[must_use]
    pub fn window_label(&self) -> String {
        if self.is_truncated() {
            format!(
                "showing {} of {} rows (limit {} reached)",
                self.rows.len(),
                self.rows_seen,
                self.cap
            )
        } else if self.rows_seen == 1 {
            "1 row".to_owned()
        } else {
            format!("{} rows", self.rows_seen)
        }
    }
}

/// What one statement produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementResult {
    /// Rows, when the statement returned any.
    pub result_set: Option<ResultSet>,
    /// Row count reported by the server on completion.
    ///
    /// PostgreSQL's simple query protocol reports a count; the full command tag
    /// text is not exposed by the driver, so the interface says "n rows" rather
    /// than inventing an "INSERT 0 n" string. See `docs/support/compatibility.md`.
    pub rows_affected: Option<u64>,
    /// How long the statement took, measured client-side.
    pub elapsed: Duration,
    /// Messages the server raised while running it.
    pub notices: Vec<Notice>,
}

impl StatementResult {
    /// A one-line summary for the status bar and the CLI.
    #[must_use]
    pub fn summary(&self) -> String {
        let millis = self.elapsed.as_millis();
        match &self.result_set {
            Some(set) => format!("{}, {millis} ms", set.window_label()),
            None => match self.rows_affected {
                Some(1) => format!("1 row affected, {millis} ms"),
                Some(n) => format!("{n} rows affected, {millis} ms"),
                None => format!("completed, {millis} ms"),
            },
        }
    }
}

/// How a whole execution ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Every statement succeeded.
    Succeeded,
    /// A statement failed. Later statements did not run.
    Failed,
    /// The user cancelled and the server confirmed.
    Cancelled,
    /// The connection dropped and the outcome of the last statement is unknown.
    ConnectionLost,
}

impl ExecutionStatus {
    /// Wording that reflects the real state, including when it is unknown.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Succeeded => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Query cancelled by server",
            Self::ConnectionLost => "Connection lost - query outcome unknown",
        }
    }
}

/// Identity of a running or finished query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "job-{}", self.0)
    }
}

/// The full outcome of executing a buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    /// The job this outcome belongs to. Stale results are rejected by comparing it.
    pub job: JobId,
    /// One entry per statement that ran, in order.
    pub statements: Vec<StatementResult>,
    /// How the execution ended.
    pub status: ExecutionStatus,
    /// Total elapsed time.
    pub elapsed: Duration,
    /// The failure, when one ended the execution. Results produced before it are
    /// still present in `statements`, because they really happened.
    pub error: Option<crate::diagnostics::Diagnostic>,
}

impl Execution {
    /// Total rows retained across every statement.
    #[must_use]
    pub fn retained_rows(&self) -> usize {
        self.statements
            .iter()
            .filter_map(|s| s.result_set.as_ref())
            .map(ResultSet::retained)
            .sum()
    }

    /// Whether any statement's rows were truncated.
    #[must_use]
    pub fn any_truncated(&self) -> bool {
        self.statements
            .iter()
            .filter_map(|s| s.result_set.as_ref())
            .any(ResultSet::is_truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: &str) -> Vec<Cell> {
        vec![Cell::Text(value.to_owned())]
    }

    #[test]
    fn a_result_set_stops_at_its_cap_but_keeps_counting() {
        let mut set = ResultSet::new(vec!["n".into()], 3);
        for i in 0..10 {
            let retained = set.push(row(&i.to_string()));
            assert_eq!(retained, i < 3, "row {i}");
        }
        assert_eq!(set.retained(), 3, "memory stays bounded");
        assert_eq!(set.rows_seen, 10, "the true count is still known");
        assert!(set.is_truncated());
        assert_eq!(set.window_label(), "showing 3 of 10 rows (limit 3 reached)");
    }

    #[test]
    fn an_untruncated_set_says_so_plainly() {
        let mut set = ResultSet::new(vec!["n".into()], 100);
        set.push(row("1"));
        assert!(!set.is_truncated());
        assert_eq!(set.window_label(), "1 row");
        set.push(row("2"));
        assert_eq!(set.window_label(), "2 rows");
    }

    #[test]
    fn an_empty_set_is_not_truncated() {
        let set = ResultSet::new(vec!["n".into()], 10);
        assert!(!set.is_truncated());
        assert_eq!(set.window_label(), "0 rows");
    }

    #[test]
    fn summaries_distinguish_rows_returned_from_rows_affected() {
        let mut set = ResultSet::new(vec!["n".into()], 10);
        set.push(row("1"));
        let select = StatementResult {
            result_set: Some(set),
            rows_affected: Some(1),
            elapsed: Duration::from_millis(87),
            notices: Vec::new(),
        };
        assert_eq!(select.summary(), "1 row, 87 ms");

        let update = StatementResult {
            result_set: None,
            rows_affected: Some(4),
            elapsed: Duration::from_millis(12),
            notices: Vec::new(),
        };
        assert_eq!(update.summary(), "4 rows affected, 12 ms");

        let ddl = StatementResult {
            result_set: None,
            rows_affected: None,
            elapsed: Duration::from_millis(3),
            notices: Vec::new(),
        };
        assert_eq!(ddl.summary(), "completed, 3 ms");
    }

    #[test]
    fn status_wording_never_claims_more_than_is_known() {
        assert_eq!(
            ExecutionStatus::ConnectionLost.label(),
            "Connection lost - query outcome unknown"
        );
        assert_eq!(
            ExecutionStatus::Cancelled.label(),
            "Query cancelled by server"
        );
        assert_ne!(
            ExecutionStatus::Failed.label(),
            ExecutionStatus::Cancelled.label()
        );
    }

    #[test]
    fn an_execution_aggregates_across_statements() {
        let mut first = ResultSet::new(vec!["a".into()], 2);
        first.push(row("1"));
        first.push(row("2"));
        first.push(row("3"));
        let mut second = ResultSet::new(vec!["b".into()], 10);
        second.push(row("x"));

        let execution = Execution {
            job: JobId(1),
            statements: vec![
                StatementResult {
                    result_set: Some(first),
                    rows_affected: Some(3),
                    elapsed: Duration::from_millis(5),
                    notices: Vec::new(),
                },
                StatementResult {
                    result_set: Some(second),
                    rows_affected: Some(1),
                    elapsed: Duration::from_millis(2),
                    notices: Vec::new(),
                },
            ],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(7),
            error: None,
        };
        assert_eq!(execution.retained_rows(), 3);
        assert!(execution.any_truncated());
        assert_eq!(execution.job.to_string(), "job-1");
    }

    #[test]
    fn a_zero_cap_keeps_nothing_but_still_reports_the_count() {
        let mut set = ResultSet::new(vec!["n".into()], 0);
        assert!(!set.push(row("1")));
        assert_eq!(set.retained(), 0);
        assert_eq!(set.rows_seen, 1);
        assert!(set.is_truncated());
    }
}
