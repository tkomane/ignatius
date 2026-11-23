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
    // own impact applies. Anything the parser cannot read with confidence is
    // treated as a write, as for any unrecognised statement.
    if first == "explain" {
        return match explain_target(&statement.text) {
            ExplainTarget::PlanOnly => Impact::Read,
            ExplainTarget::Executes(target) => classify(&Statement {
                text: target,
                ..statement.clone()
            }),
            ExplainTarget::Unclear => Impact::Write,
        };
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

/// What an `EXPLAIN` will do with the statement it describes.
enum ExplainTarget {
    /// Plain `EXPLAIN`: the statement is planned, never executed.
    PlanOnly,
    /// `EXPLAIN ANALYZE`: this statement is executed.
    Executes(String),
    /// Syntax the parser cannot read with confidence.
    Unclear,
}

/// Options that only change how a plan is shown, never whether the statement
/// runs. Anything else in the option list, except an ANALYZE switched off, is
/// read as executing, so an unknown or oddly quoted option fails safe.
const PLAN_ONLY_OPTIONS: [&str; 11] = [
    "verbose",
    "costs",
    "settings",
    "generic_plan",
    "buffers",
    "serialize",
    "wal",
    "timing",
    "summary",
    "memory",
    "format",
];

/// Reads both forms PostgreSQL accepts: the legacy words
/// (`EXPLAIN ANALYZE VERBOSE ...`) and the parenthesised option list
/// (`EXPLAIN (ANALYZE, BUFFERS) ...`), where `ANALYZE false`, `off`, `no` or
/// `0` turns execution back off. Comments count as whitespace, as they do for
/// the server, so `EXPLAIN ANALYZE/* note */DELETE` is still read correctly.
fn explain_target(text: &str) -> ExplainTarget {
    let spaced = comments_as_spaces(text);
    let Some(rest) = spaced.trim_start().get("explain".len()..) else {
        return ExplainTarget::Unclear;
    };
    let rest = rest.trim_start();

    if let Some(options) = rest.strip_prefix('(') {
        let Some(close) = options.find(')') else {
            return ExplainTarget::Unclear;
        };
        let (list, tail) = options.split_at(close);
        let mut executes = false;
        for option in list.split(',') {
            let option = option.trim();
            let name_len = option
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(option.len());
            let name = option[..name_len].to_ascii_lowercase();
            let value = option[name_len..]
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_ascii_lowercase();
            if matches!(name.as_str(), "analyze" | "analyse") {
                if !matches!(value.as_str(), "false" | "off" | "no" | "0") {
                    executes = true;
                }
            } else if name.is_empty() || !PLAN_ONLY_OPTIONS.contains(&name.as_str()) {
                // A quoted, unicode-escaped or unknown option name.
                executes = true;
            }
        }
        return if executes {
            target_of(&tail[1..])
        } else {
            ExplainTarget::PlanOnly
        };
    }

    let mut analyze = false;
    let mut remainder = rest;
    loop {
        let end = remainder
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(remainder.len());
        match remainder[..end].to_ascii_lowercase().as_str() {
            "analyze" | "analyse" => analyze = true,
            "verbose" => {}
            _ => break,
        }
        remainder = remainder[end..].trim_start();
    }
    if analyze {
        target_of(remainder)
    } else {
        ExplainTarget::PlanOnly
    }
}

/// The executed statement, unless it is another EXPLAIN, which PostgreSQL
/// rejects and which would otherwise make classification recurse.
fn target_of(remainder: &str) -> ExplainTarget {
    // A parenthesised query, `EXPLAIN ANALYZE (SELECT ...)`, is judged by
    // what is inside the parentheses.
    let target = remainder.trim_start_matches(|c: char| c == '(' || c.is_whitespace());
    if target
        .get(.."explain".len())
        .is_some_and(|word| word.eq_ignore_ascii_case("explain"))
    {
        ExplainTarget::Unclear
    } else {
        ExplainTarget::Executes(target.to_owned())
    }
}

/// The text with every comment replaced by one space, as the server's lexer
/// treats it. Line comments keep their newline; block comments nest.
fn comments_as_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('-', Some('-')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
                out.push(' ');
                out.push('\n');
            }
            ('/', Some('*')) => {
                chars.next();
                let mut depth = 1usize;
                let mut previous = '\0';
                while depth > 0 {
                    let Some(next) = chars.next() else { break };
                    if previous == '/' && next == '*' {
                        depth += 1;
                        previous = '\0';
                    } else if previous == '*' && next == '/' {
                        depth -= 1;
                        previous = '\0';
                    } else {
                        previous = next;
                    }
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whether a keyword appears as a whole word.
fn contains_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| token == word)
}

#[cfg(test)]
mod tests;
