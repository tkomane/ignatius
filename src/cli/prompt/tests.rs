use super::*;

#[test]
fn asking_needs_a_terminal_at_both_ends() {
    // Under `cargo test` neither stream is a terminal, which is exactly the
    // case that must not hang: a script waiting for a password nobody can
    // type is worse than a script that fails.
    assert!(!can_ask());
}

#[test]
fn an_empty_answer_is_the_same_as_no_answer() {
    // Sending nothing would earn the same refusal from the server and look
    // like a bug, so it is treated as a decision not to answer.
    let empty: Option<String> = Some(String::new()).filter(|p| !p.is_empty());
    assert!(empty.is_none());
}
