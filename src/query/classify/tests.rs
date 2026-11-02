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
