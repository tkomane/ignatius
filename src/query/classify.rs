//! Advisory classification of what a statement will do.
//!
//! **This is not a security boundary and must never be described as one.**
//! PostgreSQL permissions, transaction control and server-side read-only
//! settings are the real controls. This exists so the interface can say "this
//! looks like it deletes rows" before it runs, which is a usability feature.
//!
//! What it can get wrong, stated plainly so nobody builds a guarantee on it:
//!
//! - A function call can do anything. `SELECT wipe_everything()` is classified
//!   as reading, because it is a `SELECT`.
//! - `WITH ... DELETE` and other statements that start with a harmless keyword
//!   are handled, but the list of shapes is finite and SQL is not.
//! - A statement built at runtime, or one hidden inside `DO $$ ... $$`, is not
//!   inspected beyond its first keyword.
//!
//! The classification is deliberately blunt: it looks at leading keywords only.
//! A cleverer version would be wrong in subtler ways, which is worse.

use crate::query::statements::Statement;

/// What a statement appears to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Impact {
    /// Appears to read only.
    Read,
    /// Appears to change rows.
    Write,
    /// Appears to change or remove a schema object, or grant rights.
    Structural,
    /// Appears to destroy data or objects outright.
    Destructive,
}

impl Impact {
    /// A short label for the interface. Always a word, never a colour alone.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Read => "reads",
            Self::Write => "writes",
            Self::Structural => "changes structure",
            Self::Destructive => "destroys data",
        }
    }

    /// Whether this needs the user to say yes when protection is on.
    #[must_use]
    pub const fn needs_confirmation(self) -> bool {
        !matches!(self, Self::Read)
    }

    /// Whether this is severe enough to be worth typing a word to confirm.
    ///
    /// Reserved for the statements people regret, so that the confirmation stays
    /// meaningful. A prompt that appears constantly is one people learn to
    /// dismiss without reading, which is worse than no prompt.
    #[must_use]
    pub const fn needs_typed_confirmation(self) -> bool {
        matches!(self, Self::Destructive)
    }
}

/// Classifies one statement by its leading keywords.
#[must_use]
pub fn classify(statement: &Statement) -> Impact {
    let words = leading_words(&statement.text);
    let Some(first) = words.first().map(String::as_str) else {
        return Impact::Read;
    };

    // A CTE can hide a write in its tail, and that is common enough to handle.
    if first == "with" {
        let text = crate::query::statements::strip_comments(&statement.text).to_lowercase();
        for keyword in ["delete", "update", "insert", "merge"] {
            if contains_word(&text, keyword) {
                return if keyword == "delete" {
                    Impact::Destructive
                } else {
                    Impact::Write
                };
            }
        }
        return Impact::Read;
    }

    // EXPLAIN only plans, unless ANALYZE makes it execute the statement it
    // describes: `EXPLAIN ANALYZE DELETE ...` deletes. Then the statement's
    // own impact applies, so the production confirmation cannot be stepped
    // around by putting EXPLAIN ANALYZE in front of a write.
    if first == "explain" {
        return explain_analyzed_target(&statement.text).map_or(Impact::Read, |target| {
            classify(&Statement {
                text: target,
                ..statement.clone()
            })
        });
    }

    match first {
        "drop" | "truncate" => Impact::Destructive,
        "delete" => Impact::Destructive,
        "insert" | "update" | "merge" | "copy" | "upsert" => Impact::Write,
        "alter" | "create" | "grant" | "revoke" | "comment" | "reindex" | "cluster" | "refresh"
        | "vacuum" | "analyze" => Impact::Structural,
        "select" | "show" | "explain" | "table" | "values" | "fetch" | "close" | "declare"
        | "prepare" | "deallocate" | "discard" | "listen" | "unlisten" | "reset" | "set"
        | "begin" | "start" | "commit" | "rollback" | "savepoint" | "release" | "end"
        | "checkpoint" | "do" | "call" => {
            // `do` and `call` can do anything, and are marked as writes for that
            // reason: their bodies are not inspected.
            if matches!(first, "do" | "call") {
                Impact::Write
            } else {
                Impact::Read
            }
        }
        // An unrecognised statement is treated as a write. Assuming the safer
        // reading of something we do not understand is the whole point.
        _ => Impact::Write,
    }
}

/// The strongest impact across a set of statements.
#[must_use]
pub fn classify_all(statements: &[Statement]) -> Impact {
    statements
        .iter()
        .map(classify)
        .max()
        .unwrap_or(Impact::Read)
}

/// The leading words of a statement, with comments removed first.
///
/// Removing them first is the point: stopping at a comment would classify
/// `-- note\nDROP TABLE orders` as a read, which is exactly the mistake this
/// module exists to prevent.
fn leading_words(text: &str) -> Vec<String> {
    let without_comments = crate::query::statements::strip_comments(text);
    let mut words = Vec::new();
    for raw in without_comments.split_whitespace() {
        let word: String = raw
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !word.is_empty() {
            words.push(word.to_lowercase());
        }
        if words.len() >= 4 {
            break;
        }
    }
    words
}

/// The statement an `EXPLAIN` will execute, when its options include ANALYZE.
///
/// Both forms PostgreSQL accepts are read: the legacy words
/// (`EXPLAIN ANALYZE VERBOSE ...`) and the parenthesised option list
/// (`EXPLAIN (ANALYZE, BUFFERS) ...`), where `ANALYZE false`, `off` or `0`
/// turns execution back off. Plain `EXPLAIN` returns `None`: it only plans.
fn explain_analyzed_target(text: &str) -> Option<String> {
    let cleaned = crate::query::statements::strip_comments(text);
    let rest = cleaned.trim_start();
    // The caller has already seen `explain` as the first word.
    let rest = rest.get("explain".len()..)?.trim_start();

    if let Some(options) = rest.strip_prefix('(') {
        let close = options.find(')')?;
        let (list, tail) = options.split_at(close);
        let analyze = list.split(',').any(|option| {
            let mut parts = option.split_whitespace();
            let name = parts.next().unwrap_or_default().to_lowercase();
            let value = parts.next().map(str::to_lowercase);
            matches!(name.as_str(), "analyze" | "analyse")
                && !matches!(value.as_deref(), Some("false" | "off" | "0"))
        });
        return analyze.then(|| tail[1..].trim_start().to_owned());
    }

    let mut analyze = false;
    let mut remainder = rest;
    loop {
        let end = remainder
            .find(char::is_whitespace)
            .unwrap_or(remainder.len());
        match remainder[..end].to_lowercase().as_str() {
            "analyze" | "analyse" => analyze = true,
            "verbose" => {}
            _ => break,
        }
        remainder = remainder[end..].trim_start();
    }
    analyze.then(|| remainder.to_owned())
}

/// Whether a keyword appears as a whole word.
fn contains_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| token == word)
}

#[cfg(test)]
mod tests;
