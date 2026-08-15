//! Layered diagnostics.
//!
//! A failure is never reduced to "query failed". Every diagnostic carries a short
//! headline, what was being attempted, the most likely actionable cause when one is
//! known, a safe next action, and an expandable technical section holding the
//! server's own fields. The same structure feeds the CLI (stderr), the TUI error
//! panel, and `--json` output, so the three cannot drift apart.
//!
//! Every string is passed through [`crate::diagnostics::redaction`] on construction.

use crate::ExitCode;
use crate::diagnostics::redaction::redact_text;
use std::fmt;

/// The class of failure. Determines the process exit code and the headline tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// Bad or conflicting command-line arguments.
    Usage,
    /// Configuration missing, unreadable, invalid, or unmigratable.
    Config,
    /// Could not reach the server.
    Connection,
    /// Reached the server; credentials were rejected.
    Authentication,
    /// TLS could not be established under the requested policy.
    Tls,
    /// The server reported an error for a statement.
    Query,
    /// The statement was cancelled.
    Cancelled,
    /// An export began but did not finish.
    ExportInterrupted,
    /// An unexpected internal failure.
    Internal,
}

impl DiagnosticKind {
    /// Process exit code for this class of failure.
    #[must_use]
    pub const fn exit_code(self) -> ExitCode {
        match self {
            Self::Usage => ExitCode::Usage,
            Self::Config => ExitCode::Config,
            Self::Connection => ExitCode::Connection,
            Self::Authentication => ExitCode::Authentication,
            Self::Tls => ExitCode::Tls,
            Self::Query => ExitCode::Query,
            Self::Cancelled => ExitCode::Cancelled,
            Self::ExportInterrupted => ExitCode::ExportInterrupted,
            Self::Internal => ExitCode::Internal,
        }
    }

    /// Short text label. Never rely on colour alone to convey severity.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Usage => "Usage error",
            Self::Config => "Configuration error",
            Self::Connection => "Connection failed",
            Self::Authentication => "Authentication failed",
            Self::Tls => "TLS failed",
            Self::Query => "Query error",
            Self::Cancelled => "Cancelled",
            Self::ExportInterrupted => "Export incomplete",
            Self::Internal => "Internal error",
        }
    }
}

/// One labelled technical field, shown in the expandable section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalField {
    /// Field name, for example `SQLSTATE` or `Detail`.
    pub label: String,
    /// Field value, already redacted.
    pub value: String,
}

/// A position inside the submitted SQL that the server pointed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlPosition {
    /// One-based character position as reported by PostgreSQL.
    pub character: u32,
}

/// A failure described in layers, from human headline to server detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Class of failure; determines the exit code.
    pub kind: DiagnosticKind,
    /// Short human headline, one line, no jargon.
    pub headline: String,
    /// What the client was trying to do when this happened.
    pub attempted: String,
    /// The most likely actionable cause, when one can honestly be named.
    pub likely_cause: Option<String>,
    /// A safe next action the user can take.
    pub next_action: Option<String>,
    /// Server and adapter detail, shown on request.
    pub technical: Vec<TechnicalField>,
    /// Position in the submitted SQL, when the server reported one.
    pub position: Option<SqlPosition>,
}

impl Diagnostic {
    /// Creates a diagnostic. Headline and attempted-action are always redacted.
    #[must_use]
    pub fn new(
        kind: DiagnosticKind,
        headline: impl Into<String>,
        attempted: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            headline: redact_text(&headline.into()),
            attempted: redact_text(&attempted.into()),
            likely_cause: None,
            next_action: None,
            technical: Vec::new(),
            position: None,
        }
    }

    /// Adds the most likely actionable cause.
    #[must_use]
    pub fn likely_cause(mut self, cause: impl Into<String>) -> Self {
        self.likely_cause = Some(redact_text(&cause.into()));
        self
    }

    /// Adds a safe next action.
    #[must_use]
    pub fn next_action(mut self, action: impl Into<String>) -> Self {
        self.next_action = Some(redact_text(&action.into()));
        self
    }

    /// Adds one technical field.
    #[must_use]
    pub fn technical(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.technical.push(TechnicalField {
            label: label.into(),
            value: redact_text(&value.into()),
        });
        self
    }

    /// Adds an optional technical field, skipped when absent.
    #[must_use]
    pub fn technical_opt(self, label: &str, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(v) => self.technical(label, v),
            None => self,
        }
    }

    /// Records the SQL position the server reported.
    #[must_use]
    pub const fn at_position(mut self, character: u32) -> Self {
        self.position = Some(SqlPosition { character });
        self
    }

    /// Exit code this diagnostic maps to.
    #[must_use]
    pub const fn exit_code(&self) -> ExitCode {
        self.kind.exit_code()
    }

    /// Renders the human-facing form written to stderr.
    ///
    /// `verbose` controls whether the technical section is expanded; the headline,
    /// attempted action, cause and next action are always shown because they are
    /// what makes the failure actionable.
    #[must_use]
    pub fn render_plain(&self, verbose: bool) -> String {
        let mut out = String::new();
        out.push_str(&format!("{}: {}\n", self.kind.label(), self.headline));
        out.push_str(&format!("  While: {}\n", self.attempted));
        if let Some(cause) = &self.likely_cause {
            out.push_str(&format!("  Likely cause: {cause}\n"));
        }
        if let Some(action) = &self.next_action {
            out.push_str(&format!("  Next: {action}\n"));
        }
        if verbose && !self.technical.is_empty() {
            out.push_str("  Details:\n");
            for field in &self.technical {
                out.push_str(&format!("    {}: {}\n", field.label, field.value));
            }
        } else if !self.technical.is_empty() {
            out.push_str(&format!(
                "  Details: {} field(s) available, re-run with --verbose\n",
                self.technical.len()
            ));
        }
        out
    }

    /// Renders the machine-readable form used by `--json`.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind.exit_code().slug(),
            "label": self.kind.label(),
            "headline": self.headline,
            "attempted": self.attempted,
            "likely_cause": self.likely_cause,
            "next_action": self.next_action,
            "position": self.position.map(|p| p.character),
            "technical": self.technical.iter()
                .map(|f| serde_json::json!({ "label": f.label, "value": f.value }))
                .collect::<Vec<_>>(),
            "exit_code": self.kind.exit_code().code(),
        })
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.label(), self.headline)
    }
}

impl std::error::Error for Diagnostic {}

/// Renders the offending line of SQL with a caret under the reported position.
///
/// PostgreSQL reports a one-based position in characters, not bytes. Returns
/// `None` when the position falls outside the statement, which happens when the
/// server rewrote the query internally.
#[must_use]
pub fn render_position_marker(sql: &str, position: SqlPosition) -> Option<String> {
    let target = usize::try_from(position.character).ok()?;
    if target == 0 {
        return None;
    }
    let mut consumed = 0usize;
    for (line_number, line) in sql.lines().enumerate() {
        let line_chars = line.chars().count();
        // +1 for the newline that `lines()` removed.
        if target <= consumed + line_chars + 1 {
            let column = target - consumed;
            let caret_offset = line
                .chars()
                .take(column.saturating_sub(1))
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum::<usize>();
            let gutter = format!("{:>4} | ", line_number + 1);
            let mut out = String::new();
            out.push_str(&gutter);
            out.push_str(&line.replace('\t', "    "));
            out.push('\n');
            out.push_str(&" ".repeat(gutter.len() + caret_offset));
            out.push('^');
            return Some(out);
        }
        consumed += line_chars + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_maps_to_a_distinct_non_zero_exit_code() {
        let kinds = [
            DiagnosticKind::Usage,
            DiagnosticKind::Config,
            DiagnosticKind::Connection,
            DiagnosticKind::Authentication,
            DiagnosticKind::Tls,
            DiagnosticKind::Query,
            DiagnosticKind::Cancelled,
            DiagnosticKind::ExportInterrupted,
            DiagnosticKind::Internal,
        ];
        for (i, a) in kinds.iter().enumerate() {
            assert_ne!(a.exit_code(), ExitCode::Success);
            for b in kinds.iter().skip(i + 1) {
                assert_ne!(a.exit_code(), b.exit_code());
            }
        }
    }

    #[test]
    fn construction_redacts_every_string_field() {
        let secret = "hunter2-not-a-real-password";
        let d = Diagnostic::new(
            DiagnosticKind::Connection,
            format!("could not reach postgres://u:{secret}@h/db"),
            format!("connecting with password={secret}"),
        )
        .likely_cause(format!("password={secret} was rejected"))
        .next_action(format!("retry postgres://u:{secret}@h/db"))
        .technical("Connection", format!("postgres://u:{secret}@h/db"));

        let rendered = d.render_plain(true);
        assert!(!rendered.contains(secret), "{rendered}");
        assert!(!d.to_json().to_string().contains(secret));
    }

    #[test]
    fn plain_rendering_states_severity_in_words_not_colour() {
        let d = Diagnostic::new(DiagnosticKind::Tls, "certificate has expired", "opening a TLS connection")
            .likely_cause("the server certificate expired on 2026-08-01")
            .next_action("renew the certificate, or connect with sslmode=require to accept encryption without identity checks");
        let out = d.render_plain(false);
        assert!(out.starts_with("TLS failed: certificate has expired"));
        assert!(out.contains("  While: opening a TLS connection"));
        assert!(out.contains("  Likely cause: "));
        assert!(out.contains("  Next: "));
    }

    #[test]
    fn technical_section_is_summarised_until_expanded() {
        let d = Diagnostic::new(DiagnosticKind::Query, "relation does not exist", "running statement 1")
            .technical("SQLSTATE", "42P01")
            .technical("Detail", "no such table");
        let brief = d.render_plain(false);
        assert!(brief.contains("2 field(s) available, re-run with --verbose"));
        assert!(!brief.contains("42P01"));
        let full = d.render_plain(true);
        assert!(full.contains("SQLSTATE: 42P01"));
        assert!(full.contains("Detail: no such table"));
    }

    #[test]
    fn position_marker_points_at_the_right_line_and_column() {
        let sql = "SELECT 1\nFROM no_such_table\nWHERE x = 1";
        let marker = render_position_marker(sql, SqlPosition { character: 15 })
            .expect("position inside the statement");
        let lines: Vec<&str> = marker.lines().collect();
        assert_eq!(lines[0], "   2 | FROM no_such_table");
        let caret_column = lines[1].find('^').expect("caret rendered");
        assert_eq!(caret_column, "   2 | ".len() + 5, "caret under `no_such_table`");
    }

    #[test]
    fn position_marker_handles_out_of_range_and_multibyte() {
        assert!(render_position_marker("SELECT 1", SqlPosition { character: 0 }).is_none());
        assert!(render_position_marker("SELECT 1", SqlPosition { character: 999 }).is_none());
        let marker = render_position_marker("SELECT 'héllo wörld', oops", SqlPosition { character: 23 })
            .expect("multibyte position resolves");
        // The caret is placed by character count, so multibyte text does not shift it.
        assert!(marker.contains('^'));
    }
}
