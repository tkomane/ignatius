//! Sidecar unit tests for the pure cluster config, HBA, and infrastructure model.

use super::*;

fn sample_setting(name: &str, setting: &str, source: &str, pending_restart: bool) -> SettingEntry {
    SettingEntry {
        name: name.to_owned(),
        setting: setting.to_owned(),
        unit: None,
        category: "Resource Usage / Memory".to_owned(),
        context: if pending_restart {
            "postmaster".to_owned()
        } else {
            "user".to_owned()
        },
        source: source.to_owned(),
        boot_val: Some("4096".to_owned()),
        reset_val: Some(setting.to_owned()),
        pending_restart,
        is_non_default: source != "default",
    }
}

fn sample_extension(
    name: &str,
    default_version: Option<&str>,
    installed_version: Option<&str>,
) -> AvailableExtension {
    let upgrade_available = AvailableExtension::has_upgrade(default_version, installed_version);
    AvailableExtension {
        name: name.to_owned(),
        default_version: default_version.map(str::to_owned),
        installed_version: installed_version.map(str::to_owned),
        comment: Some(format!("Extension {name}")),
        upgrade_available,
    }
}

fn sample_report() -> ClusterReport {
    ClusterReport {
        settings: vec![
            sample_setting("work_mem", "64MB", "configuration file", false),
            sample_setting("shared_buffers", "256MB", "configuration file", true),
            sample_setting("enable_seqscan", "on", "default", false),
        ],
        hba_rules: vec![
            HbaRule::new(
                Some(1),
                "local".to_owned(),
                vec!["all".to_owned()],
                vec!["postgres".to_owned()],
                None,
                "peer".to_owned(),
                None,
            ),
            HbaRule::new(
                Some(2),
                "host".to_owned(),
                vec!["all".to_owned()],
                vec!["all".to_owned()],
                Some("127.0.0.1/255.255.255.255".to_owned()),
                "md5".to_owned(),
                None,
            ),
            HbaRule::new(
                Some(3),
                "host".to_owned(),
                vec!["all".to_owned()],
                vec!["all".to_owned()],
                Some("0.0.0.0/0.0.0.0".to_owned()),
                "trust".to_owned(),
                None,
            ),
        ],
        extensions: vec![
            sample_extension("pg_stat_statements", Some("1.11"), Some("1.10")),
            sample_extension("plpgsql", Some("1.0"), Some("1.0")),
            sample_extension("pg_trgm", Some("1.6"), None),
        ],
        tablespaces: vec![
            TablespaceEntry {
                name: "pg_default".to_owned(),
                owner: "postgres".to_owned(),
                location: None,
                size_bytes: Some(1024 * 1024 * 16),
                options: vec![],
            },
            TablespaceEntry {
                name: "fast_nvme".to_owned(),
                owner: "postgres".to_owned(),
                location: Some("/var/lib/postgresql/fast_nvme".to_owned()),
                size_bytes: Some(1024 * 1024 * 512),
                options: vec!["random_page_cost=1.1".to_owned()],
            },
        ],
        fdws: vec![FdwEntry {
            name: "postgres_fdw".to_owned(),
            owner: "postgres".to_owned(),
            handler: Some("postgres_fdw_handler".to_owned()),
            validator: Some("postgres_fdw_validator".to_owned()),
            servers: vec![ForeignServerEntry {
                name: "analytics_srv".to_owned(),
                owner: "postgres".to_owned(),
                server_type: Some("postgres".to_owned()),
                server_version: Some("16.0".to_owned()),
                options: vec![
                    "host=analytics.internal".to_owned(),
                    "port=5432".to_owned(),
                    "dbname=warehouse".to_owned(),
                ],
            }],
        }],
        ..ClusterReport::empty()
    }
    .bounded()
}

#[test]
fn section_parsing_resolves_canonical_and_alias_names() {
    assert_eq!(
        ClusterSection::from_str_loose("all"),
        Some(ClusterSection::All)
    );
    assert_eq!(
        ClusterSection::from_str_loose(" SETTINGS "),
        Some(ClusterSection::Settings)
    );
    assert_eq!(
        ClusterSection::from_str_loose("config"),
        Some(ClusterSection::Settings)
    );
    assert_eq!(
        ClusterSection::from_str_loose("hba"),
        Some(ClusterSection::Hba)
    );
    assert_eq!(
        ClusterSection::from_str_loose("ext"),
        Some(ClusterSection::Extensions)
    );
    assert_eq!(
        ClusterSection::from_str_loose("ts"),
        Some(ClusterSection::Tablespaces)
    );
    assert_eq!(
        ClusterSection::from_str_loose("fdw"),
        Some(ClusterSection::Fdw)
    );
    assert_eq!(ClusterSection::from_str_loose("unknown"), None);
    assert_eq!(ClusterSection::ACCEPTED.len(), 6);
}

#[test]
fn hba_risk_classification_covers_all_severity_tiers() {
    let parse_err = HbaRule::new(
        Some(10),
        "host".to_owned(),
        vec!["all".to_owned()],
        vec!["all".to_owned()],
        Some("10.0.0.0/255.0.0.0".to_owned()),
        "scram-sha-256".to_owned(),
        Some("invalid authentication method".to_owned()),
    );
    assert_eq!(parse_err.risk, HbaRisk::Critical);
    assert_eq!(parse_err.risk.label(), "CRITICAL");
    assert!(
        parse_err
            .risk_reason
            .as_deref()
            .is_some_and(|r| r.contains("invalid authentication method"))
    );

    let remote_trust = HbaRule::new(
        Some(11),
        "hostssl".to_owned(),
        vec!["all".to_owned()],
        vec!["all".to_owned()],
        Some("0.0.0.0/0.0.0.0".to_owned()),
        "trust".to_owned(),
        None,
    );
    assert_eq!(remote_trust.risk, HbaRisk::Critical);

    let local_trust = HbaRule::new(
        Some(12),
        " local ".to_owned(),
        vec!["all".to_owned()],
        vec!["postgres".to_owned()],
        None,
        " Trust ".to_owned(),
        None,
    );
    assert_eq!(local_trust.risk, HbaRisk::Warning);
    assert_eq!(local_trust.risk.label(), "WARNING");

    let cleartext = HbaRule::new(
        Some(13),
        "host".to_owned(),
        vec!["app".to_owned()],
        vec!["app_user".to_owned()],
        Some("10.0.0.1/255.255.255.255".to_owned()),
        "password".to_owned(),
        None,
    );
    assert_eq!(cleartext.risk, HbaRisk::Warning);

    let legacy_md5 = HbaRule::new(
        Some(14),
        "host".to_owned(),
        vec!["app".to_owned()],
        vec!["app_user".to_owned()],
        Some("10.0.0.1/255.255.255.255".to_owned()),
        "MD5".to_owned(),
        None,
    );
    assert_eq!(legacy_md5.risk, HbaRisk::Warning);

    let scram = HbaRule::new(
        Some(15),
        "hostssl".to_owned(),
        vec!["app".to_owned()],
        vec!["app_user".to_owned()],
        Some("10.0.0.0/255.255.255.0".to_owned()),
        "scram-sha-256".to_owned(),
        None,
    );
    assert_eq!(scram.risk, HbaRisk::Ok);
    assert_eq!(scram.risk.label(), "OK");
    assert_eq!(scram.risk_reason, None);
}

#[test]
fn report_summary_counters_and_sorting_are_deterministic() {
    let report = sample_report();
    assert_eq!(report.pending_restart_count(), 1);
    assert_eq!(report.non_default_settings_count(), 2);
    assert_eq!(report.hba_warnings_count(), 2);
    assert_eq!(report.hba_critical_count(), 1);
    assert_eq!(report.upgradable_extensions_count(), 1);

    // Pending restart sorts first, then non-default, then alphabetical.
    assert_eq!(report.settings[0].name, "shared_buffers");
    assert_eq!(report.settings[1].name, "work_mem");
    assert_eq!(report.settings[2].name, "enable_seqscan");

    // Installed extensions sort before uninstalled extensions.
    assert_eq!(report.extensions[0].name, "pg_stat_statements");
    assert_eq!(report.extensions[1].name, "plpgsql");
    assert_eq!(report.extensions[2].name, "pg_trgm");
}

#[test]
fn report_filtering_respects_modified_only_and_text_filter() {
    let report = sample_report();
    let modified = report.filtered_settings(true, None);
    assert_eq!(modified.len(), 2);
    assert!(modified.iter().all(|s| s.name != "enable_seqscan"));

    let work_only = report.filtered_settings(true, Some("WORK"));
    assert_eq!(work_only.len(), 1);
    assert_eq!(work_only[0].name, "work_mem");

    let hba_crit = report.filtered_hba_rules(Some("critical"));
    assert_eq!(hba_crit.len(), 1);
    assert_eq!(hba_crit[0].line_number, Some(3));

    let ext_upgrade = report.filtered_extensions(Some("upgrade"));
    assert_eq!(ext_upgrade.len(), 1);
    assert_eq!(ext_upgrade[0].name, "pg_stat_statements");

    let fdw_match = report.filtered_fdws(Some("analytics"));
    assert_eq!(fdw_match.len(), 1);
    assert_eq!(fdw_match[0].name, "postgres_fdw");
}

#[test]
fn bounded_enforces_all_retention_limits_and_flags() {
    let settings = (0..=MAX_SETTINGS)
        .map(|i| sample_setting(&format!("guc_{i:04}"), "1", "default", false))
        .collect();
    let hba_rules = (0..=MAX_HBA_RULES)
        .map(|i| {
            HbaRule::new(
                Some(i as i32 + 1),
                "local".to_owned(),
                vec!["all".to_owned()],
                vec!["all".to_owned()],
                None,
                "peer".to_owned(),
                None,
            )
        })
        .collect();
    let extensions = (0..=MAX_EXTENSIONS)
        .map(|i| sample_extension(&format!("ext_{i:04}"), Some("1.0"), None))
        .collect();
    let tablespaces = (0..=MAX_TABLESPACES)
        .map(|i| TablespaceEntry {
            name: format!("ts_{i:04}"),
            owner: "postgres".to_owned(),
            location: Some(format!("/mnt/ts_{i:04}")),
            size_bytes: None,
            options: Vec::new(),
        })
        .collect();
    let fdws = (0..=MAX_FDWS)
        .map(|i| FdwEntry {
            name: format!("fdw_{i:04}"),
            owner: "postgres".to_owned(),
            handler: None,
            validator: None,
            servers: Vec::new(),
        })
        .collect();

    let bounded = ClusterReport {
        settings,
        hba_rules,
        extensions,
        tablespaces,
        fdws,
        ..ClusterReport::empty()
    }
    .bounded();

    assert_eq!(bounded.settings.len(), MAX_SETTINGS);
    assert_eq!(bounded.hba_rules.len(), MAX_HBA_RULES);
    assert_eq!(bounded.extensions.len(), MAX_EXTENSIONS);
    assert_eq!(bounded.tablespaces.len(), MAX_TABLESPACES);
    assert_eq!(bounded.fdws.len(), MAX_FDWS);
    assert!(bounded.truncated_settings);
    assert!(bounded.truncated_hba_rules);
    assert!(bounded.truncated_extensions);
    assert!(bounded.truncated_tablespaces);
    assert!(bounded.truncated_fdws);
}

#[test]
fn render_table_plain_and_restricted_hba_are_pure_ascii() {
    let mut report = sample_report();
    let table = report.render_table(ClusterSection::All, false, None);
    assert!(table.is_ascii());
    assert!(table.contains("shared_buffers"));
    assert!(table.contains("pg_stat_statements"));
    assert!(table.contains("fast_nvme"));
    assert!(table.contains("analytics_srv"));
    assert!(table.contains("CRITICAL"));
    assert!(table.contains("WARNING"));

    let plain = report.render_plain(ClusterSection::Settings, true, Some("work"));
    assert!(plain.is_ascii());
    assert!(plain.contains("work_mem"));
    assert!(!plain.contains("enable_seqscan"));
    assert!(!plain.contains("Host-Based Authentication"));

    report.hba_rules.clear();
    report.hba_access_restricted = true;
    let restricted_view = report.render_table(ClusterSection::Hba, false, None);
    assert!(restricted_view.contains("42501"));
    assert!(restricted_view.contains("lacks privilege"));
}

#[test]
fn sensitive_fdw_and_tablespace_options_are_masked_everywhere() {
    let raw_password = "super-secret-password-999";
    let raw_token = "tok_live_abc123xyz";
    let raw_api_key = "ak_secret_777";
    let raw_secret = "shh_private_value";

    let report = ClusterReport {
        tablespaces: vec![TablespaceEntry {
            name: "ts_enc".to_owned(),
            owner: "postgres".to_owned(),
            location: Some("/data/ts_enc".to_owned()),
            size_bytes: Some(4096),
            options: vec![
                "seq_page_cost=1.0".to_owned(),
                format!("encryption_key={raw_api_key}"),
            ],
        }],
        fdws: vec![FdwEntry {
            name: "custom_fdw".to_owned(),
            owner: "postgres".to_owned(),
            handler: None,
            validator: None,
            servers: vec![ForeignServerEntry {
                name: "remote_api".to_owned(),
                owner: "postgres".to_owned(),
                server_type: Some("http".to_owned()),
                server_version: Some("1.0".to_owned()),
                options: vec![
                    "host=api.example.com".to_owned(),
                    format!("password={raw_password}"),
                    format!("auth_token={raw_token}"),
                    format!("api_key={raw_api_key}"),
                    format!("client_secret={raw_secret}"),
                    "dsn=postgres://svc:uri_secret_pw@db.internal:5432/prod".to_owned(),
                ],
            }],
        }],
        ..ClusterReport::empty()
    };

    // Even before calling .bounded(), every output channel must mask secrets.
    let table = report.render_table(ClusterSection::All, false, None);
    let plain = report.render_plain(ClusterSection::All, false, None);
    let json = report.render_json();
    let server_sql = ClusterReport::create_server_sql(&report.fdws[0].servers[0], "custom_fdw");
    let ts_sql = ClusterReport::create_tablespace_sql(&report.tablespaces[0]);

    for output in [&table, &plain, &json, &server_sql, &ts_sql] {
        assert!(
            !output.contains(raw_password),
            "leaked password in {output}"
        );
        assert!(!output.contains(raw_token), "leaked token in {output}");
        assert!(!output.contains(raw_api_key), "leaked key in {output}");
        assert!(!output.contains(raw_secret), "leaked secret in {output}");
        assert!(
            !output.contains("uri_secret_pw"),
            "leaked URI password in {output}"
        );
        assert!(
            output.contains(REDACTED),
            "expected redaction marker in {output}"
        );
    }

    assert!(server_sql.contains("host 'api.example.com'"));
    assert!(server_sql.contains("password '[redacted]'"));
    assert!(ts_sql.contains("seq_page_cost = '1.0'"));
    assert!(ts_sql.contains("encryption_key = '[redacted]'"));

    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(
        parsed["fdws"][0]["servers"][0]["options"][0],
        "host=api.example.com"
    );
    assert_eq!(
        parsed["fdws"][0]["servers"][0]["options"][1],
        "password=[redacted]"
    );
}

#[test]
fn reviewed_sql_generators_quote_safely_and_match_pg_syntax() {
    assert_eq!(
        ClusterReport::alter_system_sql("work_mem", "64MB"),
        "ALTER SYSTEM SET \"work_mem\" = '64MB';"
    );
    assert_eq!(
        ClusterReport::alter_system_sql("pg_stat_statements.max", "5000"),
        "ALTER SYSTEM SET \"pg_stat_statements\".\"max\" = '5000';"
    );
    assert_eq!(
        ClusterReport::alter_system_sql("bad\"; DROP TABLE t;--", "o'hai"),
        "ALTER SYSTEM SET \"bad\"\"; DROP TABLE t;--\" = 'o''hai';"
    );
    assert_eq!(
        ClusterReport::reload_conf_sql(),
        "SELECT pg_catalog.pg_reload_conf();"
    );
    assert_eq!(
        ClusterReport::create_extension_sql("pg_stat_statements"),
        "CREATE EXTENSION IF NOT EXISTS \"pg_stat_statements\";"
    );
    assert_eq!(
        ClusterReport::alter_extension_update_sql("pg_stat_statements", "1.11"),
        "ALTER EXTENSION \"pg_stat_statements\" UPDATE TO '1.11';"
    );

    let fdw = FdwEntry {
        name: "postgres_fdw".to_owned(),
        owner: "postgres".to_owned(),
        handler: Some("extensions.postgres_fdw_handler".to_owned()),
        validator: Some("\"MySchema\".\"MyValidator\"".to_owned()),
        servers: Vec::new(),
    };
    assert_eq!(
        ClusterReport::create_fdw_sql(&fdw),
        "CREATE FOREIGN DATA WRAPPER \"postgres_fdw\"\n    HANDLER \"extensions\".\"postgres_fdw_handler\"\n    VALIDATOR \"MySchema\".\"MyValidator\";"
    );

    let server = ForeignServerEntry {
        name: "remote_pg".to_owned(),
        owner: "postgres".to_owned(),
        server_type: Some("postgres".to_owned()),
        server_version: Some("16.2".to_owned()),
        options: vec!["host=db.example.com".to_owned(), "port=5432".to_owned()],
    };
    assert_eq!(
        ClusterReport::create_server_sql(&server, "postgres_fdw"),
        "CREATE SERVER IF NOT EXISTS \"remote_pg\"\n    TYPE 'postgres'\n    VERSION '16.2'\n    FOREIGN DATA WRAPPER \"postgres_fdw\"\n    OPTIONS (host 'db.example.com', port '5432');"
    );

    let builtin_ts = TablespaceEntry {
        name: "pg_default".to_owned(),
        owner: "postgres".to_owned(),
        location: None,
        size_bytes: None,
        options: Vec::new(),
    };
    assert!(
        ClusterReport::create_tablespace_sql(&builtin_ts)
            .starts_with("-- Built-in tablespace \"pg_default\"")
    );

    let custom_ts = TablespaceEntry {
        name: "fast_ssd".to_owned(),
        owner: "dba".to_owned(),
        location: Some("/mnt/pg/fast_ssd".to_owned()),
        size_bytes: Some(1024),
        options: vec!["effective_io_concurrency=200".to_owned()],
    };
    assert_eq!(
        ClusterReport::create_tablespace_sql(&custom_ts),
        "CREATE TABLESPACE \"fast_ssd\"\n    OWNER \"dba\"\n    LOCATION '/mnt/pg/fast_ssd'\n    WITH (effective_io_concurrency = '200');"
    );

    let pg_global_ts = TablespaceEntry {
        name: "pg_global".to_owned(),
        owner: "postgres".to_owned(),
        location: None,
        size_bytes: None,
        options: Vec::new(),
    };
    assert!(
        ClusterReport::create_tablespace_sql(&pg_global_ts)
            .starts_with("-- Built-in tablespace \"pg_global\"")
    );

    let missing_loc_ts = TablespaceEntry {
        name: "custom_unknown".to_owned(),
        owner: "dba".to_owned(),
        location: None,
        size_bytes: None,
        options: vec!["seq_page_cost=1.0".to_owned()],
    };
    let missing_loc_sql = ClusterReport::create_tablespace_sql(&missing_loc_ts);
    assert!(missing_loc_sql.contains("Location unavailable"));
    assert!(missing_loc_sql.contains("LOCATION '/path/to/tablespace'"));
    assert!(missing_loc_sql.contains("WITH (seq_page_cost = '1.0')"));

    let bare_fdw = FdwEntry {
        name: "bare_fdw".to_owned(),
        owner: "postgres".to_owned(),
        handler: None,
        validator: None,
        servers: Vec::new(),
    };
    assert_eq!(
        ClusterReport::create_fdw_sql(&bare_fdw),
        "CREATE FOREIGN DATA WRAPPER \"bare_fdw\";"
    );

    let bare_server = ForeignServerEntry {
        name: "bare_srv".to_owned(),
        owner: "postgres".to_owned(),
        server_type: None,
        server_version: None,
        options: Vec::new(),
    };
    assert_eq!(
        ClusterReport::create_server_sql(&bare_server, "bare_fdw"),
        "CREATE SERVER IF NOT EXISTS \"bare_srv\"\n    FOREIGN DATA WRAPPER \"bare_fdw\";"
    );
}

#[test]
fn setting_display_value_and_extension_upgrade_edge_cases() {
    let mut s = sample_setting("work_mem", "4096", "session", false);
    s.unit = Some("kB".to_owned());
    assert_eq!(s.display_value(), "4096 kB");
    assert_eq!(s.state_label(), "MODIFIED");

    s.pending_restart = true;
    assert_eq!(s.state_label(), "RESTART,MODIFIED");

    s.is_non_default = false;
    assert_eq!(s.state_label(), "RESTART");

    s.pending_restart = false;
    s.unit = Some(String::new());
    assert_eq!(s.display_value(), "4096");
    assert_eq!(s.state_label(), "default");

    assert!(!AvailableExtension::has_upgrade(Some("1.0"), None));
    assert!(!AvailableExtension::has_upgrade(Some("1.0"), Some("1.0")));
    assert!(AvailableExtension::has_upgrade(Some("1.1"), Some("1.0")));

    let empty_err_hba = HbaRule::new(
        None,
        String::new(),
        Vec::new(),
        Vec::new(),
        None,
        String::new(),
        Some(String::new()),
    );
    assert_eq!(empty_err_hba.risk, HbaRisk::Critical);
    let report = ClusterReport {
        hba_rules: vec![empty_err_hba],
        ..ClusterReport::empty()
    };
    let rendered = report.render_table(ClusterSection::Hba, false, None);
    assert!(rendered.contains("CRITICAL"));
    assert!(rendered.contains("parse error"));
}
