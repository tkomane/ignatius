use super::*;

#[test]
fn theme_refusal_tag_is_typed_and_keeps_the_existing_json_envelope() {
    let value = toml::Value::String("auto".into());
    let refusal = crate::ui::theme::lower_theme_inputs(Some(&value), None).unwrap_err();
    let line = refusal.line().to_owned();
    let diagnostic = Diagnostic::from_theme_refusal(refusal);
    assert_eq!(diagnostic.kind, DiagnosticKind::Config);
    assert_eq!(diagnostic.exit_code(), ExitCode::Config);
    assert_eq!(diagnostic.theme_refusal_line(), Some(line.as_str()));
    let json = diagnostic.to_json();
    assert_eq!(json["kind"], ExitCode::Config.slug());
    assert_eq!(json["headline"], line);
    assert!(!json.as_object().unwrap().contains_key("theme_refusal"));
    assert!(!diagnostic.theme_refusal_line().unwrap().contains('\n'));
}

#[test]
fn configuration_headline_prefix_cannot_enable_theme_presentation() {
    let diagnostic = Diagnostic::new(
        DiagnosticKind::Config,
        "configuration error: ui.theme is ordinary text",
        "parsing configuration",
    );
    assert!(diagnostic.theme_refusal_line().is_none());
    assert!(
        diagnostic
            .render_plain(false)
            .contains("  While: parsing configuration\n")
    );
    assert_eq!(diagnostic.to_json()["exit_code"], 3);
}

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
    let d = Diagnostic::new(
        DiagnosticKind::Query,
        "relation does not exist",
        "running statement 1",
    )
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
fn object_context_keeps_server_fields_structured_and_redacted() {
    let secret = "hunter2-not-a-real-password";
    let object = ObjectContext::from_server_fields(
        Some("public"),
        Some("orders"),
        Some("customer_id"),
        Some(&format!("postgres://u:{secret}@h/c")),
    )
    .expect("at least one object field");
    let diagnostic = Diagnostic::new(DiagnosticKind::Query, "constraint failed", "running")
        .in_statement(2)
        .at_position(19)
        .object_context(object);

    let plain = diagnostic.render_plain(true);
    assert!(
        plain.contains("Position: statement 2, character 19"),
        "{plain}"
    );
    assert!(plain.contains("Object: schema public, table orders, column customer_id"));
    assert!(!plain.contains(secret), "{plain}");
    let json = diagnostic.to_json();
    assert_eq!(json["statement_number"], 2);
    assert_eq!(json["position"], 19);
    assert_eq!(json["object"]["schema"], "public");
    assert!(!json.to_string().contains(secret));
}

#[test]
fn absent_object_fields_are_not_invented() {
    let object = ObjectContext::from_server_fields(None, Some("orders"), None, None)
        .expect("table is enough");
    assert_eq!(object.schema, None);
    assert_eq!(object.table.as_deref(), Some("orders"));
    assert_eq!(object.column, None);
    assert_eq!(object.constraint, None);
}

#[test]
fn position_marker_points_at_the_right_line_and_column() {
    let sql = "SELECT 1\nFROM no_such_table\nWHERE x = 1";
    let marker = render_position_marker(sql, SqlPosition { character: 15 })
        .expect("position inside the statement");
    let lines: Vec<&str> = marker.lines().collect();
    assert_eq!(lines[0], "   2 | FROM no_such_table");
    let caret_column = lines[1].find('^').expect("caret rendered");
    assert_eq!(
        caret_column,
        "   2 | ".len() + 5,
        "caret under `no_such_table`"
    );
}

#[test]
fn position_marker_handles_out_of_range_and_multibyte() {
    assert!(render_position_marker("SELECT 1", SqlPosition { character: 0 }).is_none());
    assert!(render_position_marker("SELECT 1", SqlPosition { character: 999 }).is_none());
    let marker =
        render_position_marker("SELECT 'héllo wörld', oops", SqlPosition { character: 23 })
            .expect("multibyte position resolves");
    // The caret is placed by character count, so multibyte text does not shift it.
    assert!(marker.contains('^'));
}
