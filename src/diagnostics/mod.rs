//! Redaction, layered diagnostics, opt-in logging, and the doctor command.

pub mod diagnostic;
pub mod doctor;
pub mod logging;
pub mod redaction;

pub use diagnostic::{
    Diagnostic, DiagnosticKind, ObjectContext, SqlPosition, TechnicalField, render_position_marker,
};
pub use redaction::{REDACTED, redact_arguments, redact_connection_string, redact_text};
