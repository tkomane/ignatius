use super::*;
use secrecy::ExposeSecret;

const SECRET: &str = "hunter2-not-a-real-password";

fn config() -> ConnectionConfig {
    ConnectionConfig::default()
}

#[test]
fn defaults_apply_when_nothing_is_given() {
    let target = resolve(
        None,
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("resolve");
    assert_eq!(target.host, Host::Tcp("localhost".into()));
    assert_eq!(target.port, DEFAULT_PORT);
    assert_eq!(target.sslmode, SslMode::Prefer, "local default");
    assert_eq!(target.environment, Environment::Unclassified);
    assert!(target.password.is_none());
}

#[test]
fn arguments_beat_connection_string_which_beats_environment() {
    let env = EnvSnapshot::from_pairs(&[
        ("PGHOST", "env-host"),
        ("PGPORT", "1111"),
        ("PGDATABASE", "env-db"),
        ("PGUSER", "env-user"),
    ]);
    let uri = "postgres://uri-user@uri-host:2222/uri-db";

    // Environment alone.
    let target = resolve(None, &ConnectionArgs::default(), &env, &config()).expect("env");
    assert_eq!(target.host, Host::Tcp("env-host".into()));
    assert_eq!(target.port, 1111);
    assert_eq!(target.database, "env-db");
    assert_eq!(target.user, "env-user");

    // Connection string beats environment.
    let target = resolve(Some(uri), &ConnectionArgs::default(), &env, &config()).expect("uri");
    assert_eq!(target.host, Host::Tcp("uri-host".into()));
    assert_eq!(target.port, 2222);
    assert_eq!(target.database, "uri-db");
    assert_eq!(target.user, "uri-user");

    // Arguments beat both.
    let args = ConnectionArgs {
        host: Some("arg-host".into()),
        port: Some(3333),
        dbname: Some("arg-db".into()),
        username: Some("arg-user".into()),
        ..ConnectionArgs::default()
    };
    let target = resolve(Some(uri), &args, &env, &config()).expect("args");
    assert_eq!(target.host, Host::Tcp("arg-host".into()));
    assert_eq!(target.port, 3333);
    assert_eq!(target.database, "arg-db");
    assert_eq!(target.user, "arg-user");
}

#[test]
fn remote_hosts_default_to_full_verification_and_local_ones_do_not() {
    let remote = resolve(
        Some("postgres://app@db.example.net/orders"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("remote");
    assert_eq!(remote.sslmode, SslMode::VerifyFull);
    assert!(remote.notes.iter().any(|n| n.subject == "sslmode"));

    for local in ["localhost", "127.0.0.1", "::1"] {
        let target = resolve(
            Some(&format!("postgres://app@{local}/orders")),
            &ConnectionArgs::default(),
            &EnvSnapshot::default(),
            &config(),
        )
        .expect("local");
        assert_eq!(target.sslmode, SslMode::Prefer, "{local}");
    }
}

#[test]
fn ssl_modes_state_different_guarantees() {
    assert!(SslMode::VerifyFull.verifies_certificate());
    assert!(!SslMode::Require.verifies_certificate());
    assert!(SslMode::Require.requires_tls());
    assert!(!SslMode::Prefer.requires_tls());
    assert_ne!(
        SslMode::Require.guarantee(),
        SslMode::VerifyFull.guarantee()
    );
    assert!(SslMode::parse("nonsense").is_err());
    assert_eq!(
        SslMode::parse("VERIFY-FULL").expect("parse"),
        SslMode::VerifyFull
    );
}

#[test]
fn verify_ca_is_accepted_and_stays_weaker_than_verify_full() {
    let target = resolve(
        Some("postgres://app@db.example.net/orders?sslmode=verify-ca"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("verify-ca is implemented now");
    assert_eq!(target.sslmode, SslMode::VerifyCa);
    assert!(target.sslmode.verifies_certificate());
    assert_ne!(
        SslMode::VerifyCa.guarantee(),
        SslMode::VerifyFull.guarantee(),
        "the two must never be described the same way"
    );
}

#[test]
fn certificate_paths_are_carried_through_from_every_layer() {
    let target = resolve(
        Some("host=db sslrootcert=/tmp/ca.pem sslcert=/tmp/client.crt sslkey=/tmp/client.key"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("resolve");
    assert_eq!(
        target.root_cert,
        Some(std::path::PathBuf::from("/tmp/ca.pem"))
    );
    assert_eq!(
        target.client_cert,
        Some(std::path::PathBuf::from("/tmp/client.crt"))
    );
    assert_eq!(
        target.client_key,
        Some(std::path::PathBuf::from("/tmp/client.key"))
    );

    let env = EnvSnapshot::from_pairs(&[("PGSSLROOTCERT", "/env/ca.pem")]);
    let target =
        resolve(Some("host=db"), &ConnectionArgs::default(), &env, &config()).expect("resolve");
    assert_eq!(
        target.root_cert,
        Some(std::path::PathBuf::from("/env/ca.pem"))
    );
}

#[test]
fn security_parameters_fail_and_other_unknowns_only_warn() {
    let err = resolve(
        Some("postgres://app@db.example.net/orders?gssencmode=require"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect_err("security parameter must not be ignored");
    assert!(err.headline.contains("gssencmode"), "{}", err.headline);

    let target = resolve(
        Some("postgres://app@db.example.net/orders?keepalives_idle=60"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("non-security parameter is a note");
    assert!(
        target.notes.iter().any(|n| n.subject == "keepalives_idle"),
        "{:?}",
        target.notes
    );
}

/// Writes a service file and returns its directory and path.
fn service_file(contents: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("pg_service.conf");
    std::fs::write(&path, contents).expect("write");
    (dir, path.display().to_string())
}

/// Writes an owner-only password file and returns its directory and path.
fn password_file(contents: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("pgpass");
    std::fs::write(&path, contents).expect("write");
    crate::platform::restrict_to_owner(&path).expect("restrict");
    (dir, path.display().to_string())
}

#[test]
fn a_named_service_supplies_the_connection_parameters() {
    let (_dir, path) =
        service_file("[orders-prod]\nhost=db.example.net\nport=6432\ndbname=orders\nuser=app\n");
    let env = EnvSnapshot::from_pairs(&[("PGSERVICEFILE", &path)]);

    let target = resolve(
        Some("service=orders-prod"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect("resolve");
    assert_eq!(target.host, Host::Tcp("db.example.net".into()));
    assert_eq!(target.port, 6432);
    assert_eq!(target.database, "orders");
    assert_eq!(target.user, "app");
    assert!(
        target.notes.iter().any(|n| n.subject == "service"),
        "using a service is worth saying: {:?}",
        target.notes
    );
}

#[test]
fn pgservice_selects_a_service_just_as_the_parameter_does() {
    let (_dir, path) = service_file("[dev]\nhost=localhost\ndbname=orders_dev\n");
    let env = EnvSnapshot::from_pairs(&[("PGSERVICEFILE", &path), ("PGSERVICE", "dev")]);

    let target = resolve(None, &ConnectionArgs::default(), &env, &config()).expect("resolve");
    assert_eq!(target.database, "orders_dev");
}

#[test]
fn a_service_sits_below_the_connection_string_and_above_the_environment() {
    let (_dir, path) = service_file("[s]\nhost=service-host\nport=6432\ndbname=service-db\n");
    let env = EnvSnapshot::from_pairs(&[
        ("PGSERVICEFILE", &path),
        ("PGHOST", "env-host"),
        ("PGPORT", "1111"),
        ("PGDATABASE", "env-db"),
    ]);

    // The connection string wins over the service.
    let target = resolve(
        Some("service=s host=string-host"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect("resolve");
    assert_eq!(target.host, Host::Tcp("string-host".into()));
    // The service wins over the environment.
    assert_eq!(target.port, 6432, "the service beat PGPORT");
    assert_eq!(target.database, "service-db", "the service beat PGDATABASE");

    // And an explicit argument still wins over everything.
    let args = ConnectionArgs {
        host: Some("arg-host".into()),
        ..ConnectionArgs::default()
    };
    let target = resolve(Some("service=s"), &args, &env, &config()).expect("resolve");
    assert_eq!(target.host, Host::Tcp("arg-host".into()));
}

#[test]
fn an_unknown_service_names_the_ones_that_exist() {
    let (_dir, path) = service_file("[prod]\nhost=db\n\n[dev]\nhost=localhost\n");
    let env = EnvSnapshot::from_pairs(&[("PGSERVICEFILE", &path)]);
    let error = resolve(
        Some("service=stage"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect_err("must fail");
    let cause = error.likely_cause.expect("a cause");
    assert!(cause.contains("prod") && cause.contains("dev"), "{cause}");
}

#[test]
fn a_service_carrying_an_unsupported_security_parameter_is_refused() {
    // A service file is shared across a team, so a parameter that would
    // weaken protection must fail here exactly as it does in a URI.
    let (_dir, path) = service_file("[s]\nhost=db\ngssencmode=require\n");
    let env = EnvSnapshot::from_pairs(&[("PGSERVICEFILE", &path)]);
    let error = resolve(
        Some("service=s"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect_err("must refuse");
    assert!(error.headline.contains("gssencmode"), "{}", error.headline);
}

#[test]
fn a_password_file_supplies_the_password_when_nothing_else_does() {
    let (_dir, path) = password_file("db.example.net:5432:orders:app:from-the-file\n");
    let env = EnvSnapshot::from_pairs(&[("PGPASSFILE", &path)]);

    let target = resolve(
        Some("postgres://app@db.example.net:5432/orders"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect("resolve");
    assert_eq!(
        target.password.as_ref().expect("password").expose_secret(),
        "from-the-file"
    );
    assert!(
        !format!("{target:?}").contains("from-the-file"),
        "Debug leaked the password"
    );
}

#[test]
fn an_explicit_password_beats_the_password_file() {
    let (_dir, path) = password_file("*:*:*:*:from-the-file\n");
    let env = EnvSnapshot::from_pairs(&[("PGPASSFILE", &path)]);
    let target = resolve(
        Some(&format!("postgres://app:{SECRET}@db/orders")),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect("resolve");
    assert_eq!(
        target.password.as_ref().expect("password").expose_secret(),
        SECRET
    );
}

#[test]
fn a_password_file_that_matches_nothing_says_so() {
    let (_dir, path) = password_file("other:5432:other:other:nope\n");
    let env = EnvSnapshot::from_pairs(&[("PGPASSFILE", &path)]);
    let target = resolve(
        Some("postgres://app@db.example.net:5432/orders"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect("resolve");
    assert!(target.password.is_none());
    assert!(
        target.notes.iter().any(|n| n.subject == "password file"),
        "a file that did not help must say so: {:?}",
        target.notes
    );
}

#[test]
fn a_synthetic_environment_never_reads_a_real_home_directory() {
    // Tests must not depend on, or be broken by, the developer's own
    // ~/.pgpass or ~/.pg_service.conf.
    let target = resolve(
        Some("postgres://app@db.example.net/orders"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("resolve");
    assert!(target.password.is_none());
    assert!(
        !target.notes.iter().any(|n| n.subject == "password file"),
        "no real file should have been consulted: {:?}",
        target.notes
    );
}

#[test]
fn pgpassword_is_consumed_with_a_note_and_never_displayed() {
    let env = EnvSnapshot::from_pairs(&[("PGPASSWORD", SECRET)]);
    let target = resolve(None, &ConnectionArgs::default(), &env, &config()).expect("resolve");
    assert_eq!(
        target.password.as_ref().expect("password").expose_secret(),
        SECRET
    );
    assert!(target.notes.iter().any(|n| n.subject == "PGPASSWORD"));
    assert!(
        !format!("{target:?}").contains(SECRET),
        "Debug leaked the password"
    );
    assert!(!target.safe_display().contains(SECRET));
    assert!(
        !env.names().join(",").contains(SECRET),
        "names must not carry values"
    );
}

#[test]
fn uri_parsing_handles_userinfo_ipv6_sockets_and_escapes() {
    let parsed = parse_connection_string(&format!(
            "postgres://user%40corp:{SECRET}@db.example.net:6432/orders?sslmode=require&application_name=x"
        ))
        .expect("parse");
    assert_eq!(parsed["user"], "user@corp");
    assert_eq!(parsed["password"], SECRET);
    assert_eq!(parsed["host"], "db.example.net");
    assert_eq!(parsed["port"], "6432");
    assert_eq!(parsed["dbname"], "orders");
    assert_eq!(parsed["sslmode"], "require");

    let parsed = parse_connection_string("postgresql://[::1]:5433/orders").expect("ipv6");
    assert_eq!(parsed["host"], "::1");
    assert_eq!(parsed["port"], "5433");

    let parsed =
        parse_connection_string("postgresql:///orders?host=/var/run/postgresql").expect("socket");
    assert_eq!(parsed["dbname"], "orders");
    assert_eq!(parsed["host"], "/var/run/postgresql");
}

#[test]
fn keyword_value_parsing_handles_quotes_and_escapes() {
    let parsed = parse_connection_string(&format!(
        r"host=db port=5432 dbname='my db' password='{SECRET}' user=app"
    ))
    .expect("parse");
    assert_eq!(parsed["dbname"], "my db");
    assert_eq!(parsed["password"], SECRET);
    assert_eq!(parsed["user"], "app");

    let parsed = parse_connection_string(r"host = db  dbname = orders").expect("spaces");
    assert_eq!(parsed["host"], "db");
    assert_eq!(parsed["dbname"], "orders");
}

#[test]
fn a_bare_word_target_is_a_database_name_like_psql() {
    let target = resolve(
        Some("orders"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("resolve");
    assert_eq!(target.database, "orders");
}

#[test]
fn malformed_connection_strings_fail_with_a_next_action_and_no_secret() {
    let err = parse_connection_string(&format!("host=db password='{SECRET}"))
        .expect_err("unterminated quote");
    assert!(err.next_action.is_some());
    assert!(
        !err.to_json().to_string().contains(SECRET),
        "diagnostic leaked the secret"
    );

    assert!(parse_connection_string("postgres://h/db?x=%ZZ").is_err());
}

#[test]
fn a_socket_host_is_local_and_named_by_its_path() {
    let target = resolve(
        Some("host=/var/run/postgresql dbname=orders"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    );
    if crate::platform::supports_unix_sockets() {
        let target = target.expect("resolve");
        assert_eq!(
            target.host,
            Host::Socket(PathBuf::from("/var/run/postgresql"))
        );
        assert!(target.host.is_local());
        assert_eq!(target.sslmode, SslMode::Prefer);
    } else {
        assert!(target.is_err(), "platforms without sockets must say so");
    }
}

#[test]
fn environment_classification_is_explicit_never_guessed() {
    // A host called "prod" is not production unless the user says so.
    let target = resolve(
        Some("postgres://app@prod-db.example.net/orders"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("resolve");
    assert_eq!(target.environment, Environment::Unclassified);
    assert!(!target.environment.is_production());

    let args = ConnectionArgs {
        environment: Some(Environment::parse("production").expect("parse")),
        ..ConnectionArgs::default()
    };
    let target = resolve(
        Some("postgres://app@harmless.example.net/orders"),
        &args,
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("resolve");
    assert!(target.environment.is_production());
    assert_eq!(target.environment.label(), "PROD");
}

#[test]
fn environment_labels_are_words_not_symbols() {
    for (input, label) in [
        ("local", "LOCAL"),
        ("dev", "DEV"),
        ("staging", "STAGING"),
        ("prod", "PROD"),
        ("sandbox", "SANDBOX"),
    ] {
        assert_eq!(Environment::parse(input).expect("parse").label(), label);
    }
    assert!(Environment::parse("  ").is_err());
}

#[test]
fn an_invalid_port_is_rejected_with_guidance() {
    let err = resolve(
        Some("postgres://app@host:99999/db"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect_err("port out of range");
    assert!(err.next_action.is_some());
}

#[test]
fn target_session_attrs_parses_all_libpq_values_and_rejects_unknowns() {
    for (input, expected, canonical) in [
        ("any", TargetSessionAttrs::Any, "any"),
        (" ANY ", TargetSessionAttrs::Any, "any"),
        ("read-write", TargetSessionAttrs::ReadWrite, "read-write"),
        ("Read-Write", TargetSessionAttrs::ReadWrite, "read-write"),
        ("read-only", TargetSessionAttrs::ReadOnly, "read-only"),
        ("primary", TargetSessionAttrs::Primary, "primary"),
        ("standby", TargetSessionAttrs::Standby, "standby"),
    ] {
        let parsed = TargetSessionAttrs::parse(input).expect("valid target_session_attrs");
        assert_eq!(parsed, expected);
        assert_eq!(parsed.as_str(), canonical);
        assert!(!parsed.description().is_empty());
    }
    let err = TargetSessionAttrs::parse("prefer-standby").expect_err("must reject unknown");
    assert!(err.contains("target_session_attrs"), "{err}");
    assert!(err.contains("read-write"), "{err}");
}

#[test]
fn target_session_attrs_follows_libpq_precedence_and_object_tree_resets_to_any() {
    let (_dir, service_path) =
        service_file("[s]\nhost=db.example.net\ntarget_session_attrs=standby\n");
    let env_only = EnvSnapshot::from_pairs(&[("PGTARGETSESSIONATTRS", "read-only")]);
    let env_with_service = EnvSnapshot::from_pairs(&[
        ("PGSERVICEFILE", &service_path),
        ("PGTARGETSESSIONATTRS", "read-only"),
    ]);

    // Default is Any.
    let target = resolve(
        None,
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("default");
    assert_eq!(target.target_session_attrs, TargetSessionAttrs::Any);

    // Env applies when nothing higher sets it.
    let target = resolve(None, &ConnectionArgs::default(), &env_only, &config()).expect("env");
    assert_eq!(target.target_session_attrs, TargetSessionAttrs::ReadOnly);

    // Service beats env.
    let target = resolve(
        Some("service=s"),
        &ConnectionArgs::default(),
        &env_with_service,
        &config(),
    )
    .expect("service");
    assert_eq!(target.target_session_attrs, TargetSessionAttrs::Standby);

    // URI/kv beats service and env.
    let target = resolve(
        Some("service=s target_session_attrs=primary"),
        &ConnectionArgs::default(),
        &env_with_service,
        &config(),
    )
    .expect("kv");
    assert_eq!(target.target_session_attrs, TargetSessionAttrs::Primary);

    let uri_target = resolve(
        Some("postgres://app@db.example.net/orders?target_session_attrs=read-write"),
        &ConnectionArgs::default(),
        &env_only,
        &config(),
    )
    .expect("uri");
    assert_eq!(
        uri_target.target_session_attrs,
        TargetSessionAttrs::ReadWrite
    );
    assert!(
        !uri_target
            .notes
            .iter()
            .any(|n| n.subject == "target_session_attrs"),
        "supported key must not emit unsupported note: {:?}",
        uri_target.notes
    );

    // CLI argument beats URI, service, and env (even when CLI selects `any`).
    let cli_target = resolve(
        Some("postgres://app@db.example.net/orders?target_session_attrs=read-write"),
        &ConnectionArgs {
            target_session_attrs: Some(TargetSessionAttrs::Any),
            ..ConnectionArgs::default()
        },
        &env_only,
        &config(),
    )
    .expect("cli overrides uri");
    assert_eq!(cli_target.target_session_attrs, TargetSessionAttrs::Any);

    // Object tree is read-only and resets target_session_attrs to Any.
    let tree_target = uri_target.for_object_tree();
    assert!(tree_target.read_only);
    assert_eq!(tree_target.target_session_attrs, TargetSessionAttrs::Any);
}

#[test]
fn invalid_target_session_attrs_in_uri_service_or_env_fails_with_config_diagnostic() {
    let err = resolve(
        Some("postgres://app@db.example.net/orders?target_session_attrs=invalid"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect_err("invalid uri target_session_attrs");
    assert_eq!(err.kind, DiagnosticKind::Config);
    assert!(err.next_action.is_some());

    let env = EnvSnapshot::from_pairs(&[("PGTARGETSESSIONATTRS", "bogus")]);
    let err = resolve(None, &ConnectionArgs::default(), &env, &config())
        .expect_err("invalid env PGTARGETSESSIONATTRS");
    assert_eq!(err.kind, DiagnosticKind::Config);

    let (_dir, service_path) = service_file("[bad]\nhost=db\ntarget_session_attrs=bogus\n");
    let env = EnvSnapshot::from_pairs(&[("PGSERVICEFILE", &service_path)]);
    let err = resolve(
        Some("service=bad"),
        &ConnectionArgs::default(),
        &env,
        &config(),
    )
    .expect_err("invalid service target_session_attrs");
    assert_eq!(err.kind, DiagnosticKind::Config);
}

#[test]
fn parse_and_format_timeout_duration_handle_valid_bounds_and_errors() {
    for (input, expected) in [
        ("0", Duration::ZERO),
        ("0ms", Duration::ZERO),
        ("0s", Duration::ZERO),
        ("0M", Duration::ZERO),
        ("0h", Duration::ZERO),
        (" 30 ", Duration::from_secs(30)),
        ("500ms", Duration::from_millis(500)),
        ("500MS", Duration::from_millis(500)),
        ("15s", Duration::from_secs(15)),
        ("2M", Duration::from_secs(120)),
        ("1H", Duration::from_secs(3_600)),
        ("24h", Duration::from_secs(86_400)),
        ("86400", Duration::from_secs(86_400)),
        ("86400000ms", Duration::from_secs(86_400)),
    ] {
        assert_eq!(
            parse_timeout_duration(input).expect("valid timeout"),
            expected,
            "{input:?}"
        );
    }

    for invalid in [
        "",
        "   ",
        "-1",
        "-500ms",
        "+10s",
        "ms",
        "s",
        "abc",
        "1.5s",
        "10 s",
        "10us",
        "1d",
        "25h",
        "1441m",
        "86401",
        "86400001ms",
        "99999999999999999999999s",
    ] {
        assert!(
            parse_timeout_duration(invalid).is_err(),
            "{invalid:?} must be rejected"
        );
    }

    assert_eq!(format_timeout_duration(Duration::ZERO), "disabled");
    assert_eq!(format_timeout_duration(Duration::from_millis(100)), "100ms");
    assert_eq!(
        format_timeout_duration(Duration::from_millis(1_500)),
        "1500ms"
    );
    assert_eq!(format_timeout_duration(Duration::from_secs(30)), "30s");
    assert_eq!(format_timeout_duration(Duration::from_secs(120)), "2m");
    assert_eq!(format_timeout_duration(Duration::from_secs(3_600)), "1h");
}

#[test]
fn session_timeouts_follow_precedence_and_object_tree_bounds() {
    let (_dir, service_path) =
        service_file("[s]\nhost=db.example.net\nstatement_timeout=20s\nlock_timeout=3s\n");
    let env = EnvSnapshot::from_pairs(&[("PGSERVICEFILE", &service_path)]);

    // Default config leaves both unset.
    let default_target = resolve(
        None,
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("default");
    assert!(default_target.statement_timeout.is_none());
    assert!(default_target.lock_timeout.is_none());

    // Global config defaults apply when > 0.
    let global_cfg = ConnectionConfig {
        statement_timeout_seconds: 45,
        lock_timeout_seconds: 7,
        ..ConnectionConfig::default()
    };
    let from_global = resolve(
        None,
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &global_cfg,
    )
    .expect("global config");
    assert_eq!(from_global.statement_timeout, Some(Duration::from_secs(45)));
    assert_eq!(from_global.lock_timeout, Some(Duration::from_secs(7)));

    // Service beats global config.
    let from_service = resolve(
        Some("service=s"),
        &ConnectionArgs::default(),
        &env,
        &global_cfg,
    )
    .expect("service");
    assert_eq!(
        from_service.statement_timeout,
        Some(Duration::from_secs(20))
    );
    assert_eq!(from_service.lock_timeout, Some(Duration::from_secs(3)));

    // URI/kv beats service and global config, including explicit 0 (disabled).
    let from_uri = resolve(
        Some("postgres://app@db.example.net/orders?statement_timeout=0&lock_timeout=500ms"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &global_cfg,
    )
    .expect("uri");
    assert_eq!(from_uri.statement_timeout, Some(Duration::ZERO));
    assert_eq!(from_uri.lock_timeout, Some(Duration::from_millis(500)));
    assert!(
        !from_uri
            .notes
            .iter()
            .any(|n| n.subject == "statement_timeout" || n.subject == "lock_timeout"),
        "supported timeout keys must not emit unsupported notes: {:?}",
        from_uri.notes
    );

    // Explicit CLI args beat URI, service, and global config.
    let from_cli = resolve(
        Some("service=s statement_timeout=2m lock_timeout=10s"),
        &ConnectionArgs {
            statement_timeout: Some(Duration::from_millis(250)),
            lock_timeout: Some(Duration::ZERO),
            ..ConnectionArgs::default()
        },
        &env,
        &global_cfg,
    )
    .expect("cli");
    assert_eq!(from_cli.statement_timeout, Some(Duration::from_millis(250)));
    assert_eq!(from_cli.lock_timeout, Some(Duration::ZERO));

    // Invalid timeout in URI or service fails with Config diagnostic.
    let err = resolve(
        Some("postgres://app@db.example.net/orders?statement_timeout=-5s"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect_err("invalid uri statement_timeout");
    assert_eq!(err.kind, DiagnosticKind::Config);

    // Object tree enforces bounded metadata timeouts regardless of user target timeouts.
    let tree_target = from_cli.for_object_tree();
    assert_eq!(
        tree_target.statement_timeout,
        Some(DEFAULT_OBJECT_TREE_STATEMENT_TIMEOUT)
    );
    assert_eq!(
        tree_target.lock_timeout,
        Some(DEFAULT_OBJECT_TREE_LOCK_TIMEOUT)
    );
}
