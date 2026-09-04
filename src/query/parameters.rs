//! PostgreSQL-aware named-parameter discovery and literal binding.
//!
//! Named parameters are an Ignatius interaction feature, not PostgreSQL
//! prepared-statement syntax. This module therefore keeps the original SQL
//! template intact and produces one escaped text literal for each occurrence
//! immediately before the existing simple-query path sends it.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use secrecy::{ExposeSecret, SecretString};
use std::fmt;
use std::fmt::Write as _;

/// Maximum number of distinct names one prompt or binding can hold.
pub const MAX_DISTINCT_PARAMETERS: usize = 64;

/// One executable `:name` occurrence in a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedPlaceholder {
    /// The case-sensitive name without its leading colon.
    pub name: String,
    /// UTF-8 byte offset of the leading colon.
    pub start: usize,
    /// UTF-8 byte offset just after the name.
    pub end: usize,
}

/// The safe, value-free result of scanning a SQL template.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParameterTemplate {
    names: Vec<String>,
    occurrences: Vec<NamedPlaceholder>,
}

impl ParameterTemplate {
    /// Distinct names in first-use order.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Every executable occurrence, including repeats.
    #[must_use]
    pub fn occurrences(&self) -> &[NamedPlaceholder] {
        &self.occurrences
    }

    /// Whether this template has no named parameters.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Binds complete secret values into one simple-query SQL string.
    ///
    /// This is intentionally a text-literal operation. It does not infer a
    /// PostgreSQL type and it never allows a value to become SQL syntax.
    pub fn bind(&self, sql: &str, bindings: &ParameterBindings) -> Result<String, ParameterError> {
        bindings.validate()?;
        if bindings.entries.len() != self.names.len() {
            return Err(ParameterError::BindingCount {
                expected: self.names.len(),
                actual: bindings.entries.len(),
            });
        }

        for name in &self.names {
            if !bindings.entries.iter().any(|entry| entry.name == *name) {
                return Err(ParameterError::MissingBinding { name: name.clone() });
            }
        }
        if let Some(entry) = bindings
            .entries
            .iter()
            .find(|entry| !self.names.iter().any(|name| name == &entry.name))
        {
            return Err(ParameterError::UnexpectedBinding {
                name: entry.name.clone(),
            });
        }

        let mut output = String::with_capacity(sql.len());
        let mut cursor = 0usize;
        for occurrence in &self.occurrences {
            if occurrence.start > occurrence.end
                || occurrence.end > sql.len()
                || !sql.is_char_boundary(occurrence.start)
                || !sql.is_char_boundary(occurrence.end)
            {
                return Err(ParameterError::InvalidTemplate);
            }
            output.push_str(&sql[cursor..occurrence.start]);
            let value = bindings
                .entries
                .iter()
                .find(|entry| entry.name == occurrence.name)
                .expect("validated binding names");
            output.push_str(&escape_literal(&occurrence.name, &value.value)?);
            cursor = occurrence.end;
        }
        output.push_str(&sql[cursor..]);
        Ok(output)
    }
}

/// Finds executable named placeholders in SQL.
pub fn discover(sql: &str) -> Result<ParameterTemplate, ParameterError> {
    let bytes = sql.as_bytes();
    let mut names = Vec::new();
    let mut occurrences = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' => i = crate::query::statements::skip_single_quoted(sql, i),
            b'"' => i = crate::query::statements::skip_double_quoted(sql, i),
            b'$' => {
                let after = crate::query::statements::skip_dollar_quoted(sql, i);
                i = if after == i { i + 1 } else { after };
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i = crate::query::statements::skip_line_comment(sql, i);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = crate::query::statements::skip_block_comment(sql, i);
            }
            b':' if bytes.get(i.wrapping_sub(1)) == Some(&b':')
                || bytes.get(i + 1) == Some(&b':') =>
            {
                // Both colons are ignored. This also keeps the second colon
                // from being mistaken for a parameter when the first one was
                // consumed by the branch above.
                i += 1;
            }
            b':' => {
                let Some(&first) = bytes.get(i + 1) else {
                    i += 1;
                    continue;
                };
                if !is_name_start(first) {
                    i += 1;
                    continue;
                }
                let mut end = i + 2;
                while bytes.get(end).is_some_and(|byte| is_name_continue(*byte)) {
                    end += 1;
                }
                let name = &sql[i + 1..end];
                if !names.iter().any(|known| known == name) {
                    if names.len() == MAX_DISTINCT_PARAMETERS {
                        return Err(ParameterError::TooManyNames {
                            limit: MAX_DISTINCT_PARAMETERS,
                        });
                    }
                    names.push(name.to_owned());
                }
                occurrences.push(NamedPlaceholder {
                    name: name.to_owned(),
                    start: i,
                    end,
                });
                i = end;
            }
            _ => i += 1,
        }
    }

    Ok(ParameterTemplate { names, occurrences })
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Secret values matched to a template name.
#[derive(Clone)]
pub struct ParameterBindings {
    entries: Vec<ParameterBinding>,
}

#[derive(Clone)]
struct ParameterBinding {
    name: String,
    value: SecretString,
}

impl PartialEq for ParameterBindings {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl Eq for ParameterBindings {}

impl PartialEq for ParameterBinding {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.value.expose_secret() == other.value.expose_secret()
    }
}

impl Eq for ParameterBinding {}

impl fmt::Debug for ParameterBindings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParameterBindings")
            .field(
                "names",
                &self
                    .entries
                    .iter()
                    .map(|entry| &entry.name)
                    .collect::<Vec<_>>(),
            )
            .field("values", &"<hidden>")
            .finish()
    }
}

impl ParameterBindings {
    /// Creates bindings from names and secret values without exposing them in
    /// diagnostics or debug output.
    pub fn from_secrets(
        names: Vec<String>,
        values: Vec<SecretString>,
    ) -> Result<Self, ParameterError> {
        if names.len() != values.len() {
            return Err(ParameterError::BindingCount {
                expected: names.len(),
                actual: values.len(),
            });
        }
        let mut entries = Vec::with_capacity(names.len());
        for (name, value) in names.into_iter().zip(values) {
            if entries
                .iter()
                .any(|entry: &ParameterBinding| entry.name == name)
            {
                return Err(ParameterError::DuplicateBinding { name });
            }
            entries.push(ParameterBinding { name, value });
        }
        Ok(Self { entries })
    }

    /// Refuses values that cannot cross PostgreSQL's text query boundary.
    pub(crate) fn validate(&self) -> Result<(), ParameterError> {
        for entry in &self.entries {
            if entry.value.expose_secret().contains('\0') {
                return Err(ParameterError::NulValue {
                    name: entry.name.clone(),
                });
            }
        }
        Ok(())
    }

    /// Names held by these bindings. Values are never returned.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    /// Whether there are no bindings.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn escape_literal(name: &str, value: &SecretString) -> Result<String, ParameterError> {
    let raw = value.expose_secret();
    if raw.contains('\0') {
        return Err(ParameterError::NulValue {
            name: name.to_owned(),
        });
    }

    let mut escaped = String::with_capacity(raw.len() + 3);
    escaped.push_str("E'");
    for character in raw.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000C}' => escaped.push_str("\\f"),
            character if character.is_ascii_control() => {
                let _ = write!(escaped, "\\x{:02X}", character as u32);
            }
            character => escaped.push(character),
        }
    }
    escaped.push('\'');
    Ok(escaped)
}

/// Errors produced before a parameterized statement is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterError {
    /// More distinct names were found than the bounded prompt can hold.
    TooManyNames { limit: usize },
    /// The number of names and values did not match.
    BindingCount { expected: usize, actual: usize },
    /// A template name has no corresponding value.
    MissingBinding { name: String },
    /// A binding was supplied for a name outside the template.
    UnexpectedBinding { name: String },
    /// The same name was supplied more than once.
    DuplicateBinding { name: String },
    /// A value cannot cross the text protocol because it contains NUL.
    NulValue { name: String },
    /// Internal span corruption prevented safe binding.
    InvalidTemplate,
}

impl fmt::Display for ParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyNames { limit } => {
                write!(
                    f,
                    "the statement contains more than {limit} distinct named parameters"
                )
            }
            Self::BindingCount { expected, actual } => write!(
                f,
                "parameter binding expected {expected} value(s), but received {actual}"
            ),
            Self::MissingBinding { name } => write!(f, "no value was supplied for :{name}"),
            Self::UnexpectedBinding { name } => {
                write!(f, "a value was supplied for unknown parameter :{name}")
            }
            Self::DuplicateBinding { name } => {
                write!(f, "parameter :{name} was supplied more than once")
            }
            Self::NulValue { name } => {
                write!(f, "the value for :{name} contains NUL and cannot be sent")
            }
            Self::InvalidTemplate => write!(f, "the parameter template has invalid source spans"),
        }
    }
}

impl ParameterError {
    /// Converts the safe error into the shared diagnostic contract.
    #[must_use]
    pub fn diagnostic(&self, attempted: impl Into<String>) -> Diagnostic {
        let next = match self {
            Self::TooManyNames { .. } => {
                "use fewer distinct names, or split the statement into smaller runs"
            }
            Self::BindingCount { .. }
            | Self::MissingBinding { .. }
            | Self::UnexpectedBinding { .. }
            | Self::DuplicateBinding { .. } => "provide exactly one value for each named parameter",
            Self::NulValue { .. } => "remove NUL from the value and try again",
            Self::InvalidTemplate => "reopen the statement and try again",
        };
        Diagnostic::new(DiagnosticKind::Usage, self.to_string(), attempted).next_action(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings(names: &[&str], values: &[&str]) -> ParameterBindings {
        ParameterBindings::from_secrets(
            names.iter().map(|name| (*name).to_owned()).collect(),
            values
                .iter()
                .map(|value| SecretString::from((*value).to_owned()))
                .collect(),
        )
        .expect("valid bindings")
    }

    #[test]
    fn discovers_only_executable_names_in_first_use_order() {
        let sql = r#"SELECT ':ignored', ":also_ignored", $$:body$$,
            :first, value::text, :first, :second
            /* :comment /* :nested */ */ -- :line
            WHERE note = E'escaped \' :literal'"#;
        let template = discover(sql).expect("template");
        assert_eq!(template.names(), ["first", "second"]);
        assert_eq!(template.occurrences().len(), 3);
        assert_eq!(template.occurrences()[0].name, "first");
    }

    #[test]
    fn punctuation_after_a_colon_is_left_for_postgresql() {
        let template = discover("SELECT :ok, :=, :9, ::text, :_").expect("template");
        assert_eq!(template.names(), ["ok", "_"]);
    }

    #[test]
    fn the_distinct_name_limit_is_enforced_without_partial_output() {
        let sql = (0..=MAX_DISTINCT_PARAMETERS)
            .map(|index| format!(":p{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(
            discover(&sql),
            Err(ParameterError::TooManyNames {
                limit: MAX_DISTINCT_PARAMETERS
            })
        );
    }

    #[test]
    fn binding_repeats_one_literal_and_escapes_adversarial_text() {
        let sql = "SELECT :value, :value, :empty";
        let template = discover(sql).expect("template");
        let value = "quote ' slash \\\\ newline\n tab\t unicode ✓ ; -- /* */";
        let bound = template
            .bind(sql, &bindings(&["value", "empty"], &[value, ""]))
            .expect("bound");
        assert_eq!(
            bound,
            "SELECT E'quote \\\' slash \\\\\\\\ newline\\n tab\\t unicode ✓ ; -- /* */', E'quote \\\' slash \\\\\\\\ newline\\n tab\\t unicode ✓ ; -- /* */', E''"
        );
    }

    #[test]
    fn nul_is_refused_without_including_the_value_in_the_error() {
        let sql = "SELECT :secret";
        let template = discover(sql).expect("template");
        let secret = "safe-marker\0never-log";
        let error = template
            .bind(sql, &bindings(&["secret"], &[secret]))
            .expect_err("NUL must be refused");
        assert_eq!(
            error,
            ParameterError::NulValue {
                name: "secret".to_owned()
            }
        );
        assert!(!error.to_string().contains("never-log"));
        assert!(!format!("{error:?}").contains("never-log"));
    }

    #[test]
    fn bindings_debug_contains_names_but_not_values() {
        let bindings = bindings(&["token"], &["synthetic-secret"]);
        let debug = format!("{bindings:?}");
        assert!(debug.contains("token"));
        assert!(debug.contains("<hidden>"));
        assert!(!debug.contains("synthetic-secret"));
    }
}
