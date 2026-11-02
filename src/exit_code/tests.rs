use super::ExitCode;

#[test]
fn numeric_values_are_the_documented_contract() {
    // Changing any of these is a breaking change for scripts. If this test
    // fails, the change needs a major version bump and a release note.
    assert_eq!(ExitCode::Success.code(), 0);
    assert_eq!(ExitCode::Usage.code(), 2);
    assert_eq!(ExitCode::Config.code(), 3);
    assert_eq!(ExitCode::Connection.code(), 4);
    assert_eq!(ExitCode::Authentication.code(), 5);
    assert_eq!(ExitCode::Tls.code(), 6);
    assert_eq!(ExitCode::Query.code(), 7);
    assert_eq!(ExitCode::Cancelled.code(), 8);
    assert_eq!(ExitCode::ExportInterrupted.code(), 9);
    assert_eq!(ExitCode::Internal.code(), 70);
}

#[test]
fn codes_and_slugs_are_unique() {
    let all = ExitCode::all();
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a.code(), b.code(), "duplicate exit code");
            assert_ne!(a.slug(), b.slug(), "duplicate exit slug");
        }
    }
}

#[test]
fn exit_codes_fit_in_a_process_status() {
    for code in ExitCode::all() {
        assert!((0..=255).contains(&code.code()));
    }
}
