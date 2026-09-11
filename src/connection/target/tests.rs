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
        Some("postgres://app@db.example.net/orders?target_session_attrs=read-write"),
        &ConnectionArgs::default(),
        &EnvSnapshot::default(),
        &config(),
    )
    .expect("non-security parameter is a note");
    assert!(
        target
            .notes
            .iter()
            .any(|n| n.subject == "target_session_attrs"),
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
