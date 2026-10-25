//! The result model.
//!
//! Retained row counts are bounded: a [`ResultSet`] stops keeping rows once it
//! reaches its cap and records that it did. Individual values and driver frames
//! still allocate before row retention; this is not a process-memory ceiling. Truncation is therefore
//! a fact the interface can state, never a silent omission, and the client never
//! re-runs a query to page through it.
//!
//! Values arrive in PostgreSQL's text format, exactly as the server rendered
//! them. Numerics keep their precision, timestamps keep their offset, and types
//! this build has never heard of still display correctly.

use crate::query::value::Cell;
use std::time::Duration;

/// Largest notice message retained for display with an execution.
pub const MAX_NOTICE_MESSAGE_BYTES: usize = 64 * 1024;
const NOTICE_TRUNCATION_SUFFIX: &str = " ... [notice truncated]";

/// Maximum entries admitted in one notice observation window.
pub const MAX_NOTICE_COUNT: usize = 256;
/// Maximum retained UTF-8 notice fields in one observation window.
pub const MAX_NOTICE_TEXT_BYTES: usize = 1024 * 1024;

/// Precision of an observed client-side count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeCount {
    /// The exact observed count.
    Exact(u64),
    /// At least this many were observed; accounting saturated.
    AtLeast(u64),
    /// Accounting was unavailable.
    Unknown,
}

impl Default for NoticeCount {
    fn default() -> Self {
        Self::Exact(0)
    }
}

impl NoticeCount {
    /// Adds counts without wrapping or inventing precision.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Exact(a), Self::Exact(b)) => a
                .checked_add(b)
                .map_or(Self::AtLeast(u64::MAX), Self::Exact),
            (Self::Exact(a) | Self::AtLeast(a), Self::Exact(b) | Self::AtLeast(b)) => {
                Self::AtLeast(a.saturating_add(b))
            }
        }
    }
    /// Observations since an exact baseline; uncertain baselines cannot be subtracted.
    #[must_use]
    pub fn since(self, baseline: Self) -> Self {
        match (self, baseline) {
            (Self::Exact(a), Self::Exact(b)) if a >= b => Self::Exact(a - b),
            (Self::AtLeast(a), Self::Exact(b)) if a >= b => Self::AtLeast(a - b),
            _ => Self::Unknown,
        }
    }
    /// Whether the count is known to be zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        matches!(self, Self::Exact(0))
    }
    /// Fixed, value-free wording with truthful precision.
    #[must_use]
    pub fn words(self) -> String {
        match self {
            Self::Exact(n) => n.to_string(),
            Self::AtLeast(n) => format!("at least {n}"),
            Self::Unknown => "an unknown number of".into(),
        }
    }
    /// Compact ASCII count, used only beside an explicit category label.
    #[must_use]
    pub fn compact(self) -> String {
        match self {
            Self::Exact(n) => n.to_string(),
            Self::AtLeast(n) => format!(">={n}"),
            Self::Unknown => "?".into(),
        }
    }
    /// The additive scripted status schema.
    #[must_use]
    pub fn to_json(self) -> serde_json::Value {
        match self {
            Self::Exact(n) => serde_json::json!({"count":n,"precision":"exact"}),
            Self::AtLeast(n) => serde_json::json!({"count":n,"precision":"at_least"}),
            Self::Unknown => serde_json::json!({"count":null,"precision":"unknown"}),
        }
    }
}

/// Client notice loss, independent of SQL outcome and result rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoticeCounts {
    /// Notices rejected by admission.
    pub omitted: NoticeCount,
    /// Admitted messages shortened by the per-message rule.
    pub shortened: NoticeCount,
    /// Admitted prefix notices not returned when a future was abandoned.
    pub abandoned: NoticeCount,
}
impl NoticeCounts {
    /// Unavailable accounting, rather than a fabricated zero.
    pub const UNKNOWN: Self = Self {
        omitted: NoticeCount::Unknown,
        shortened: NoticeCount::Unknown,
        abandoned: NoticeCount::Unknown,
    };
    /// Adds fixed counters without retaining payload.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            omitted: self.omitted.plus(other.omitted),
            shortened: self.shortened.plus(other.shortened),
            abandoned: self.abandoned.plus(other.abandoned),
        }
    }
    /// Counts since a checkpoint with precision preserved.
    #[must_use]
    pub fn since(self, baseline: Self) -> Self {
        Self {
            omitted: self.omitted.since(baseline.omitted),
            shortened: self.shortened.since(baseline.shortened),
            abandoned: self.abandoned.since(baseline.abandoned),
        }
    }
    /// Whether any client loss or uncertain accounting needs disclosure.
    #[must_use]
    pub fn needs_report(self) -> bool {
        !self.omitted.is_zero() || !self.shortened.is_zero() || !self.abandoned.is_zero()
    }
    /// Exact fixed client wording, containing no server text.
    #[must_use]
    pub fn lines(self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.omitted.is_zero() {
            lines.push(format!("Client omitted {} observed server notices after reaching the notice-retention limit.",self.omitted.words()));
        }
        if !self.shortened.is_zero() {
            lines.push(format!(
                "Client shortened {} retained server notice messages.",
                self.shortened.words()
            ));
        }
        if !self.abandoned.is_zero() {
            lines.push(format!("Client did not return {} retained server notices because an operation was abandoned.",self.abandoned.words()));
        }
        lines
    }
}

/// The observation window; it does not establish statement causation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoticeScope {
    /// Ordinary buffered execution.
    Query,
    /// Complete streaming export.
    Export,
    /// Structured plan request.
    Plan,
    /// Idle, metadata, startup or late messages on a connection.
    #[default]
    Connection,
}
impl NoticeScope {
    /// Stable machine-readable name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Export => "export",
            Self::Plan => "plan",
            Self::Connection => "connection",
        }
    }
}

/// Fixed cumulative counters survive payload transfer and dropped outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoticeCheckpoint {
    /// Monotonic until saturation; a saturated revision is not an unchanged proof.
    pub revision: u64,
    /// Committed counters for this physical connection.
    pub totals: NoticeCounts,
}
impl NoticeCheckpoint {
    /// A fail-closed snapshot when the accounting lock is unavailable.
    pub const UNKNOWN: Self = Self {
        revision: u64::MAX,
        totals: NoticeCounts::UNKNOWN,
    };
}

/// Bounded payload and client accounting for one observation window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeReport {
    /// Where messages were observed, without causal attribution.
    pub scope: NoticeScope,
    /// All admitted entries, including transferred statement notices; unavailable is None.
    pub retained: Option<u16>,
    /// Client accounting for this delivered report.
    pub counts: NoticeCounts,
    /// Entries not already owned by successful statement results.
    pub unattributed: Vec<Notice>,
    /// Cumulative committed counters, for duplicate-safe consumer transfer.
    pub checkpoint: NoticeCheckpoint,
}
impl Default for NoticeReport {
    fn default() -> Self {
        Self::empty(NoticeScope::Connection)
    }
}
impl NoticeReport {
    /// Empty known accounting, used for operations that did not establish an inbox.
    #[must_use]
    pub fn empty(scope: NoticeScope) -> Self {
        Self {
            scope,
            retained: Some(0),
            counts: NoticeCounts::default(),
            unattributed: Vec::new(),
            checkpoint: NoticeCheckpoint::default(),
        }
    }
    /// No invented counts when accounting is unavailable.
    #[must_use]
    pub fn unavailable(scope: NoticeScope) -> Self {
        Self {
            scope,
            retained: None,
            counts: NoticeCounts::UNKNOWN,
            unattributed: Vec::new(),
            checkpoint: NoticeCheckpoint::UNKNOWN,
        }
    }
    /// Additive value-free stderr representation.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({"scope":self.scope.label(),"observed_only":true,"retained":self.retained,"omitted":self.counts.omitted.to_json(),"shortened":self.counts.shortened.to_json(),"abandoned":self.counts.abandoned.to_json(),"limits":{"count":MAX_NOTICE_COUNT,"text_bytes":MAX_NOTICE_TEXT_BYTES,"message_bytes":MAX_NOTICE_MESSAGE_BYTES}})
    }
}

/// An outcome whose notice report survives both success and error.
#[derive(Debug)]
pub struct Reported<T> {
    /// Original outcome; accounting never changes database facts.
    pub value: T,
    /// Bounded observed notices and client accounting.
    pub notice_report: NoticeReport,
}

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

/// Retains a notice message without allowing one server message to dominate
/// the execution model. The suffix makes the loss visible.
#[must_use]
pub fn bounded_notice_message(message: &str) -> String {
    if message.len() <= MAX_NOTICE_MESSAGE_BYTES {
        return message.to_owned();
    }

    let end = notice_prefix_end(message);
    let mut bounded = String::with_capacity(end + NOTICE_TRUNCATION_SUFFIX.len());
    bounded.push_str(&message[..end]);
    bounded.push_str(NOTICE_TRUNCATION_SUFFIX);
    bounded
}

/// Retained message byte length without allocating another application copy.
pub(crate) fn bounded_notice_message_len(message: &str) -> usize {
    if message.len() <= MAX_NOTICE_MESSAGE_BYTES {
        return message.len();
    }
    notice_prefix_end(message) + NOTICE_TRUNCATION_SUFFIX.len()
}

fn notice_prefix_end(message: &str) -> usize {
    let mut end = MAX_NOTICE_MESSAGE_BYTES
        .saturating_sub(NOTICE_TRUNCATION_SUFFIX.len())
        .min(message.len());
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Rows returned by one statement, bounded by a cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultSet {
    /// Column names in order.
    pub columns: Vec<String>,
    /// Optional server-described PostgreSQL type names in the same order.
    ///
    /// The simple query value path deliberately remains text-based so extension
    /// values keep their server rendering. A missing entry means the type could
    /// not be described, never that it was inferred from a cell.
    pub column_types: Vec<Option<String>>,
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
    pub fn new(columns: Vec<String>, cap: usize) -> Self {
        Self {
            column_types: vec![None; columns.len()],
            columns,
            rows: Vec::new(),
            rows_seen: 0,
            cap,
        }
    }

    /// Creates a result set with optional type labels from a server description.
    ///
    /// A vector with the wrong length is rejected as unavailable rather than
    /// being padded or truncated, because shifting a label onto another source
    /// column would be a more dangerous result than no label.
    #[must_use]
    pub fn with_column_types(
        columns: Vec<String>,
        column_types: Vec<Option<String>>,
        cap: usize,
    ) -> Self {
        let mut result = Self::new(columns, cap);
        result.set_column_types(column_types);
        result
    }

    /// Replaces type labels when their positions align exactly with the headers.
    /// Returns whether the metadata was accepted.
    pub fn set_column_types(&mut self, column_types: Vec<Option<String>>) -> bool {
        if column_types.len() != self.columns.len() {
            self.column_types = vec![None; self.columns.len()];
            return false;
        }
        self.column_types = column_types;
        true
    }

    /// The server-described type label for one source column, when available.
    #[must_use]
    pub fn column_type(&self, index: usize) -> Option<&str> {
        self.column_types.get(index).and_then(Option::as_deref)
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

/// Whether the session is inside a transaction, and whether it is usable.
///
/// Read from the server after each execution rather than inferred from the
/// statements sent. Inference would be wrong exactly when it matters most: a
/// server-side rollback, a statement inside a function, or an error that ends a
/// transaction without the client seeing a ROLLBACK.
///
/// The protocol carries this on every ReadyForQuery message, but the driver does
/// not expose it, so it is asked for instead. See
/// [`crate::postgres::session`] for how, and why that question is the one that
/// actually answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransactionState {
    /// Not in a transaction. Each statement commits on its own.
    #[default]
    Autocommit,
    /// Inside a transaction that is still usable.
    Open,
    /// Inside a transaction that has failed. Nothing will run until it ends.
    Failed,
    /// The state could not be read.
    Unknown,
}

impl TransactionState {
    /// The label shown in the status bar. Always words.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Autocommit => "Autocommit",
            Self::Open => "In transaction",
            Self::Failed => "Transaction failed",
            Self::Unknown => "Transaction state unknown",
        }
    }

    /// What the user should do next, when there is something to do.
    #[must_use]
    pub const fn recovery(self) -> Option<&'static str> {
        match self {
            Self::Failed => Some("ROLLBACK ends it. Nothing else will run until you do."),
            Self::Open => Some("COMMIT or ROLLBACK when you are done."),
            Self::Autocommit | Self::Unknown => None,
        }
    }

    /// Builds a state from the server's answer to the transaction probe.
    ///
    /// `in_transaction` is the server comparing the transaction's own timestamp
    /// with this statement's: they differ only inside an explicit transaction
    /// block. A failed transaction cannot answer at all, which is how it is
    /// recognised.
    #[must_use]
    pub const fn from_probe(in_transaction: Option<bool>) -> Self {
        match in_transaction {
            Some(true) => Self::Open,
            Some(false) => Self::Autocommit,
            None => Self::Unknown,
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
    /// The transaction state the server reported afterwards.
    pub transaction: TransactionState,
    /// Bounded operation-wide notice accounting and unattributed messages.
    pub notice_report: NoticeReport,
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
mod tests;
