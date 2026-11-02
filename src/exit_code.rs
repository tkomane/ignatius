//! Stable process exit codes.
//!
//! These are a compatibility contract: scripts and CI depend on them, so the
//! numeric values may only change in a major release. They are documented in
//! `docs/support/compatibility.md` and asserted in `tests/cli_contract.rs`.

/// Process exit codes returned by every command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum ExitCode {
    /// The command completed and every statement succeeded.
    Success = 0,
    /// The command line could not be parsed, or arguments conflict.
    Usage = 2,
    /// Configuration is missing, invalid, or could not be migrated.
    Config = 3,
    /// The server could not be reached: DNS, TCP, socket, or timeout.
    Connection = 4,
    /// The server was reached and rejected the credentials.
    Authentication = 5,
    /// TLS could not be established under the requested policy.
    Tls = 6,
    /// The server reported an error for a statement.
    Query = 7,
    /// The user or the server cancelled the running statement.
    Cancelled = 8,
    /// An export started but did not finish; a `.partial` file may remain.
    ExportInterrupted = 9,
    /// An unexpected internal failure. Always a defect worth reporting.
    Internal = 70,
}

impl ExitCode {
    /// Numeric value handed to the operating system.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Stable machine-readable identifier, used in `--json` output.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Usage => "usage",
            Self::Config => "config",
            Self::Connection => "connection",
            Self::Authentication => "authentication",
            Self::Tls => "tls",
            Self::Query => "query",
            Self::Cancelled => "cancelled",
            Self::ExportInterrupted => "export-interrupted",
            Self::Internal => "internal",
        }
    }

    /// Every exit code, in numeric order. Used to document and test the contract.
    #[must_use]
    pub const fn all() -> [Self; 10] {
        [
            Self::Success,
            Self::Usage,
            Self::Config,
            Self::Connection,
            Self::Authentication,
            Self::Tls,
            Self::Query,
            Self::Cancelled,
            Self::ExportInterrupted,
            Self::Internal,
        ]
    }
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(value: ExitCode) -> Self {
        // Values are all within the 0-255 range that process exit codes allow.
        Self::from(u8::try_from(value.code()).unwrap_or(70))
    }
}

#[cfg(test)]
mod tests;
