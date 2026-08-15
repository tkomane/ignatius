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

/// Whether a keyword appears as a whole word.
fn contains_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|token| token == word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::statements::split;

    fn impact(sql: &str) -> Impact {
        let statements = split(sql);
        assert_eq!(statements.len(), 1, "the fixture should be one statement");
        classify(&statements[0])
    }

    #[test]
    fn reads_are_recognised_as_reads() {
        for sql in [
            "SELECT 1",
            "select * from orders",
            "  SELECT 1",
            "EXPLAIN SELECT 1",
            "SHOW search_path",
            "TABLE orders",
            "VALUES (1)",
        ] {
            assert_eq!(impact(sql), Impact::Read, "{sql}");
        }
    }

    #[test]
    fn writes_are_recognised_as_writes() {
        for sql in [
            "INSERT INTO orders VALUES (1)",
            "update orders set total = 0",
            "MERGE INTO orders USING x ON true",
            "COPY orders FROM stdin",
        ] {
            assert_eq!(impact(sql), Impact::Write, "{sql}");
        }
    }

    #[test]
    fn the_statements_people_regret_are_marked_destructive() {
        for sql in [
            "DROP TABLE orders",
            "drop schema public cascade",
            "TRUNCATE orders",
            "DELETE FROM orders",
            "DELETE FROM orders WHERE id = 1",
        ] {
            assert_eq!(impact(sql), Impact::Destructive, "{sql}");
            assert!(classify(&split(sql)[0]).needs_typed_confirmation());
        }
    }

    #[test]
    fn structural_changes_are_their_own_class() {
        for sql in [
            "ALTER TABLE orders ADD COLUMN x int",
            "CREATE TABLE t (id int)",
            "GRANT SELECT ON orders TO app",
            "REVOKE SELECT ON orders FROM app",
        ] {
            assert_eq!(impact(sql), Impact::Structural, "{sql}");
            assert!(classify(&split(sql)[0]).needs_confirmation());
            assert!(
                !classify(&split(sql)[0]).needs_typed_confirmation(),
                "typing a word for every CREATE would train people to type it"
            );
        }
    }

    #[test]
    fn a_write_hidden_in_a_common_table_expression_is_still_found() {
        assert_eq!(
            impact("WITH moved AS (DELETE FROM orders RETURNING *) SELECT * FROM moved"),
            Impact::Destructive
        );
        assert_eq!(
            impact("WITH x AS (INSERT INTO orders VALUES (1) RETURNING *) SELECT * FROM x"),
            Impact::Write
        );
        assert_eq!(
            impact("WITH x AS (SELECT 1) SELECT * FROM x"),
            Impact::Read,
            "a read-only CTE stays a read"
        );
    }

    #[test]
    fn anything_unrecognised_is_treated_as_a_write() {
        // Assuming the safer reading of something we do not understand is the
        // entire point of this module.
        assert_eq!(impact("FROBNICATE THE DATABASE"), Impact::Write);
        assert_eq!(impact("DO $$ BEGIN PERFORM 1; END $$"), Impact::Write);
        assert_eq!(impact("CALL do_something()"), Impact::Write);
    }

    #[test]
    fn comments_cannot_hide_what_a_statement_does() {
        // A comment in front of a destructive statement is the obvious way to
        // slip one past a classifier that stops at the first comment.
        assert_eq!(
            impact("-- a note about this\nDROP TABLE orders"),
            Impact::Destructive
        );
        assert_eq!(
            impact("/* a note */ DROP TABLE orders"),
            Impact::Destructive
        );
        assert_eq!(
            impact("/* multi\n   line */ TRUNCATE orders"),
            Impact::Destructive
        );
        assert_eq!(impact("/* a note */ SELECT 1"), Impact::Read);
        assert_eq!(
            impact("-- SELECT 1 is what this is not\nDELETE FROM orders"),
            Impact::Destructive,
            "a comment that mentions a read must not make it one"
        );
    }

    #[test]
    fn the_strongest_impact_across_a_buffer_is_what_counts() {
        let statements = split("SELECT 1; UPDATE orders SET total = 0; SELECT 2;");
        assert_eq!(classify_all(&statements), Impact::Write);

        let statements = split("SELECT 1; DROP TABLE orders;");
        assert_eq!(classify_all(&statements), Impact::Destructive);

        assert_eq!(classify_all(&split("SELECT 1;")), Impact::Read);
        assert_eq!(
            classify_all(&[]),
            Impact::Read,
            "nothing to run reads nothing"
        );
    }

    #[test]
    fn a_function_call_that_hides_a_write_is_a_documented_false_negative() {
        // This is the limit of the approach, and it is written down rather than
        // pretended away. A SELECT that calls a function which deletes rows
        // reads, as far as this can tell.
        assert_eq!(impact("SELECT wipe_everything()"), Impact::Read);
    }

    #[test]
    fn every_impact_has_a_label_and_they_are_ordered_by_severity() {
        assert!(Impact::Read < Impact::Write);
        assert!(Impact::Write < Impact::Structural);
        assert!(Impact::Structural < Impact::Destructive);
        for impact in [
            Impact::Read,
            Impact::Write,
            Impact::Structural,
            Impact::Destructive,
        ] {
            assert!(!impact.label().is_empty());
        }
        assert!(!Impact::Read.needs_confirmation());
    }
}
