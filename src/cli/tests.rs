use super::*;
use clap::Parser;

#[test]
fn configured_colour_doctor_uses_loaded_mode_and_keeps_raw_capability_facts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "schema_version = 1\n[ui]\ntheme = \"custom\"\ncolor = \"never\"\n[theme]\nextends = \"dark\"\n").unwrap();
    let paths = Paths::rooted_at(dir.path());
    let facts = doctor::TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: Some("truecolor".into()),
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
        background_rgb: None,
    };
    let initial = resolve_presentation(&GlobalArgs::default(), &facts);
    let mut out = Vec::new();
    assert_eq!(
        doctor_command(true, None, &paths, &facts, &initial, &mut out).unwrap(),
        ExitCode::Success
    );
    let report: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let check = |name| {
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == name)
            .unwrap()
    };
    assert_eq!(check("colour depth")["detail"], "none, source forced off");
    assert_eq!(
        check("colour")["detail"],
        "colour available; meaning is also carried by text labels"
    );
    assert!(!facts.no_color);
    assert!(facts.is_terminal);
    assert_eq!(facts.term.as_deref(), Some("xterm-256color"));
}

#[test]
fn configured_colour_doctor_dumb_is_absolute_without_faking_no_color() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "schema_version = 1\n[ui]\ntheme = \"custom\"\ncolor = \"always\"\n[theme]\nextends = \"dark\"\n").unwrap();
    let facts = doctor::TerminalFacts {
        is_terminal: true,
        term: Some("dumb".into()),
        term_program: None,
        no_color: false,
        colorterm: Some("truecolor".into()),
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
        background_rgb: None,
    };
    let initial = resolve_presentation(
        &GlobalArgs {
            color: Some(ColorChoice::Always),
            ..Default::default()
        },
        &facts,
    );
    let mut out = Vec::new();
    assert_eq!(
        doctor_command(
            true,
            None,
            &Paths::rooted_at(dir.path()),
            &facts,
            &initial,
            &mut out
        )
        .unwrap(),
        ExitCode::Success
    );
    let report: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let check = |name| {
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["name"] == name)
            .unwrap()
    };
    assert_eq!(check("colour depth")["detail"], "none, source forced off");
    let colour = check("colour")["detail"].as_str().unwrap();
    assert!(colour.contains("TERM=dumb"), "{colour}");
    assert!(!colour.contains("available") && !colour.contains("NO_COLOR"));
    assert!(!facts.no_color);
    assert!(facts.is_terminal);
    assert_eq!(facts.term.as_deref(), Some("dumb"));
}

fn retention_status_execution() -> crate::query::result::Execution {
    use crate::query::result::{
        Execution, ExecutionStatus, JobId, ResultSet, StatementResult, TransactionState,
    };
    use crate::query::value::Cell;
    let mut full = ResultSet::new(vec!["value".into()], 2);
    full.push(vec![Cell::Text("synthetic-private-row".into())]);
    let mut capped = ResultSet::new(vec!["value".into()], 1);
    for _ in 0..3 {
        capped.push(vec![Cell::Text("synthetic-private-row".into())]);
    }
    let mut empty = ResultSet::new(vec!["value".into()], 0);
    for _ in 0..2 {
        empty.push(vec![Cell::Text("synthetic-private-row".into())]);
    }
    let statement = |result_set: Option<ResultSet>| StatementResult {
        rows_affected: Some(result_set.as_ref().map_or(9, |set| set.rows_seen)),
        result_set,
        elapsed: std::time::Duration::ZERO,
        notices: Vec::new(),
    };
    Execution {
        notice_report: crate::query::result::NoticeReport::default(),
        job: JobId(1),
        statements: vec![
            statement(None),
            statement(Some(full)),
            statement(None),
            statement(Some(capped)),
            statement(Some(empty)),
        ],
        status: ExecutionStatus::Succeeded,
        elapsed: std::time::Duration::ZERO,
        error: None,
        transaction: TransactionState::Autocommit,
    }
}

#[test]
fn buffered_truncation_ordinals_include_untruncated_sets_but_ignore_dml_tags() {
    let mut status = Vec::new();
    write_buffered_truncation(&mut status, &retention_status_execution(), Format::Csv).unwrap();
    let status = String::from_utf8(status).unwrap();
    assert_eq!(
        status,
        "Result set 2 truncated: retained 1 of 3 received rows. For a future complete streaming export, choose --output before execution.\nResult set 3 truncated: retained 0 of 2 received rows. For a future complete streaming export, choose --output before execution.\n"
    );
    assert!(!status.contains("synthetic-private-row"));
}

struct FailedStatus(std::io::ErrorKind);

impl std::io::Write for FailedStatus {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::from(self.0))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn buffered_truncation_stderr_failures_keep_the_ordinary_io_boundary() {
    for kind in [
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::BrokenPipe,
    ] {
        let error = write_buffered_truncation(
            &mut FailedStatus(kind),
            &retention_status_execution(),
            Format::Csv,
        )
        .unwrap_err();
        assert_eq!(error.kind(), kind);
        let diagnostic = io_diagnostic(error);
        assert_eq!(diagnostic.kind, DiagnosticKind::Internal);
        assert_eq!(diagnostic.exit_code(), ExitCode::Internal);
    }
}

#[test]
fn buffered_truncation_does_not_write_for_inband_or_nontruncated_results() {
    let mut execution = retention_status_execution();
    for format in [Format::Table, Format::Json] {
        assert!(
            write_buffered_truncation(
                &mut FailedStatus(std::io::ErrorKind::Other),
                &execution,
                format
            )
            .is_ok()
        );
    }
    execution.statements.retain(|statement| {
        statement
            .result_set
            .as_ref()
            .is_some_and(|set| !set.is_truncated())
    });
    assert!(
        write_buffered_truncation(
            &mut FailedStatus(std::io::ErrorKind::Other),
            &execution,
            Format::Csv
        )
        .is_ok()
    );
}

#[test]
fn streamed_output_help_states_the_value_row_and_driver_memory_boundary() {
    let mut tree = Cli::command();
    let help = tree
        .find_subcommand_mut("query")
        .unwrap()
        .render_long_help()
        .to_string();
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(help.contains("entire result is not retained in memory"));
    assert!(help.contains("Individual values, rows and driver frames still use memory"));
    assert!(!help.contains("rows never pass through memory"));
    assert!(!help.contains("size of the result does not matter"));
}

#[test]
fn the_command_tree_parses_and_has_no_conflicting_definitions() {
    Cli::command().debug_assert();
}

#[test]
fn commands_parse_the_way_the_documentation_says() {
    let cli = Cli::try_parse_from(["ignatius", "query", "-c", "SELECT 1", "--format", "csv"])
        .expect("parse");
    let Some(Command::Query {
        command, format, ..
    }) = cli.command
    else {
        panic!("expected query");
    };
    assert_eq!(command.as_deref(), Some("SELECT 1"));
    assert_eq!(format, Format::Csv);

    let cli = Cli::try_parse_from(["ignatius"]).expect("parse");
    assert!(
        cli.command.is_none(),
        "no subcommand opens the interactive client"
    );

    let cli = Cli::try_parse_from(["ignatius", "doctor", "--json"]).expect("parse");
    assert!(matches!(
        cli.command,
        Some(Command::Doctor { json: true, .. })
    ));
}

#[test]
fn query_parses_repeatable_parameter_environment_mappings() {
    let cli = Cli::try_parse_from([
        "ignatius",
        "query",
        "-c",
        "SELECT :customer_id, :status",
        "--param-env",
        "customer_id=IGNATIUS_CUSTOMER_ID",
        "--param-env",
        "status=IGNATIUS_STATUS",
    ])
    .expect("parse");
    let Some(Command::Query { parameter_env, .. }) = cli.command else {
        panic!("expected query");
    };
    assert_eq!(
        parameter_env,
        vec![
            "customer_id=IGNATIUS_CUSTOMER_ID".to_owned(),
            "status=IGNATIUS_STATUS".to_owned()
        ]
    );
}

#[test]
fn parameter_environment_mappings_are_validated_without_target_work() {
    let missing = parameter_bindings_from_env("SELECT :customer_id", &[])
        .expect_err("prompting is not allowed here");
    assert_eq!(missing.kind, DiagnosticKind::Usage);
    assert!(
        missing
            .next_action
            .as_deref()
            .is_some_and(|action| action.contains(":customer_id"))
    );

    let duplicate = parameter_bindings_from_env(
        "SELECT :customer_id",
        &["customer_id=PATH".to_owned(), "customer_id=PATH".to_owned()],
    )
    .expect_err("duplicate mapping");
    assert!(duplicate.headline.contains("mapped more than once"));

    let extra = parameter_bindings_from_env("SELECT :customer_id", &["other=PATH".to_owned()])
        .expect_err("extra mapping");
    assert!(extra.headline.contains("unknown parameter"));

    let malformed = parameter_bindings_from_env(
        "SELECT :customer_id",
        &["customer_id=PATH=OTHER".to_owned()],
    )
    .expect_err("malformed mapping");
    assert!(malformed.headline.contains("NAME=VARIABLE"));

    let no_parameters = parameter_bindings_from_env("SELECT 1", &["customer_id=PATH".to_owned()])
        .expect_err("mapping without a placeholder");
    assert!(no_parameters.headline.contains("no named parameters"));
}

#[test]
fn a_complete_parameter_environment_mapping_keeps_values_secret() {
    if std::env::var_os("PATH").is_none() {
        return;
    }
    let bindings = parameter_bindings_from_env("SELECT :value", &["value=PATH".to_owned()])
        .expect("PATH is available")
        .expect("one binding");
    assert_eq!(bindings.names(), vec!["value"]);
    let debug = format!("{bindings:?}");
    assert!(!debug.contains("/"), "environment values stay out of debug");
}

#[test]
fn command_and_file_cannot_both_be_given() {
    let err = Cli::try_parse_from(["ignatius", "query", "-c", "SELECT 1", "-f", "x.sql"])
        .expect_err("must conflict");
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn an_unknown_sslmode_is_a_usage_error_with_guidance() {
    let options = ConnectionOptions {
        sslmode: Some("sort-of".into()),
        ..ConnectionOptions::default()
    };
    let err = options.to_args().expect_err("must reject");
    assert_eq!(err.kind, DiagnosticKind::Usage);
    assert_eq!(err.exit_code(), ExitCode::Usage);
    assert!(err.next_action.is_some());
}

#[test]
fn environment_is_parsed_from_the_flag_only() {
    let options = ConnectionOptions {
        environment: Some("production".into()),
        ..ConnectionOptions::default()
    };
    let args = options.to_args().expect("parse");
    assert_eq!(args.environment, Some(Environment::Production));
    assert!(
        ConnectionOptions::default()
            .to_args()
            .expect("parse")
            .environment
            .is_none()
    );
}

#[test]
fn implicit_interactive_start_is_kept_only_when_no_route_option_was_typed() {
    assert!(!ConnectionOptions::default().has_explicit_route());

    for options in [
        ConnectionOptions {
            host: Some("db.example.net".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            dbname: Some("orders".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            sslmode: Some("verify-full".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            target_session_attrs: Some("read-write".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            read_only: true,
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            profile: Some("orders-prod".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            statement_timeout: Some("30s".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            lock_timeout: Some("5s".into()),
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            idle_in_transaction_session_timeout: Some("2m".into()),
            ..ConnectionOptions::default()
        },
    ] {
        assert!(
            options.has_explicit_route(),
            "a typed connection or safety option must bypass the picker: {options:?}"
        );
    }
}

#[test]
fn the_startup_picker_requires_profiles_and_an_implicit_route() {
    let defaults = ConnectionOptions::default();
    assert!(should_open_connection_picker(None, &defaults, true));
    assert!(!should_open_connection_picker(None, &defaults, false));
    assert!(!should_open_connection_picker(
        Some("@orders-dev"),
        &defaults,
        true
    ));
    assert!(!should_open_connection_picker(
        None,
        &ConnectionOptions {
            profile: Some("orders-dev".into()),
            ..ConnectionOptions::default()
        },
        true
    ));
}

#[test]
fn plain_implies_no_colour_and_no_unicode() {
    let facts = doctor::TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: None,
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
        background_rgb: None,
    };
    let presentation = resolve_presentation(
        &GlobalArgs {
            plain: true,
            ..GlobalArgs::default()
        },
        &facts,
    );
    assert!(!presentation.color);
    assert!(!presentation.unicode());
    assert_eq!(presentation.glyphs, crate::ui::GlyphTier::Ascii);
}

#[test]
fn the_nerd_font_tier_is_only_ever_chosen_explicitly() {
    let facts = doctor::TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: Some("WarpTerminal".into()),
        no_color: false,
        colorterm: Some("truecolor".into()),
        size: Some((100, 30)),
        unicode: true,
        color_depth: crate::ui::theme::ColorDepth::TrueColor,
        color_depth_source: crate::ui::theme::DepthSource::Default,
        background_rgb: None,
    };

    // Nothing about a capable terminal implies a patched font.
    let automatic = resolve_presentation(&GlobalArgs::default(), &facts);
    assert_eq!(automatic.glyphs, crate::ui::GlyphTier::Unicode);
    assert!(
        automatic.glyph_override.is_none(),
        "configuration still decides"
    );

    let asked = resolve_presentation(
        &GlobalArgs {
            glyphs: Some(GlyphChoice::NerdFont),
            ..GlobalArgs::default()
        },
        &facts,
    );
    assert_eq!(asked.glyphs, crate::ui::GlyphTier::Nerd);

    // --plain wins over an explicit request, because it is what someone
    // reaches for when the terminal cannot cope.
    let plain = resolve_presentation(
        &GlobalArgs {
            glyphs: Some(GlyphChoice::NerdFont),
            plain: true,
            ..GlobalArgs::default()
        },
        &facts,
    );
    assert_eq!(plain.glyphs, crate::ui::GlyphTier::Ascii);
}

#[test]
fn version_output_separates_the_four_identity_facts() {
    let mut out = Vec::new();
    version(true, &mut out).expect("write");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains(branding::VERSION));
    assert!(text.contains("source revision:"));
    assert!(text.contains("build identity:"));
    assert!(text.contains("target:"));
}

#[test]
fn config_paths_lists_every_location() {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = Paths::rooted_at(dir.path());
    let mut out = Vec::new();
    let code = config_command(&ConfigAction::Paths, &paths, &mut out).expect("run");
    assert_eq!(code, ExitCode::Success);
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("Configuration file:"));
    assert!(text.contains("Logs:"));
}

#[test]
fn config_validate_reports_success_on_a_fresh_machine() {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = Paths::rooted_at(dir.path());
    let mut out = Vec::new();
    let code = config_command(&ConfigAction::Validate, &paths, &mut out).expect("run");
    assert_eq!(code, ExitCode::Success);
    assert!(String::from_utf8(out).expect("utf8").contains("valid"));
}

fn with_profiles(toml: &str) -> crate::config::Config {
    toml::from_str(toml).expect("a valid configuration")
}

fn options(profile: Option<&str>) -> ConnectionOptions {
    ConnectionOptions {
        profile: profile.map(str::to_owned),
        ..ConnectionOptions::default()
    }
}

#[test]
fn a_profile_fills_in_what_was_not_said_and_pins_the_environment() {
    // The classification is the reason profiles exist: it cannot be
    // forgotten once it is written down, and forgetting it is what puts a
    // write on the wrong database.
    let config = with_profiles(
        "[profiles.orders-prod]\n\
         host = \"db.example.net\"\n\
         port = 6432\n\
         dbname = \"orders\"\n\
         user = \"app\"\n\
         sslmode = \"verify-full\"\n\
         environment = \"production\"\n",
    );

    let (target, args) = resolve_target_and_profile(Some("@orders-prod"), &options(None), &config)
        .expect("resolves");
    assert!(target.is_none(), "the profile is the target");
    assert_eq!(args.host.as_deref(), Some("db.example.net"));
    assert_eq!(args.port, Some(6432));
    assert_eq!(args.dbname.as_deref(), Some("orders"));
    assert_eq!(args.username.as_deref(), Some("app"));
    assert_eq!(args.sslmode, Some(SslMode::VerifyFull));
    assert_eq!(args.environment, Some(Environment::Production));
    assert!(args.environment.expect("set").is_production());

    // The flag and the shorthand are the same thing.
    let (_, from_flag) =
        resolve_target_and_profile(None, &options(Some("orders-prod")), &config).expect("resolves");
    assert_eq!(from_flag.host, args.host);
}

#[test]
fn a_flag_the_user_typed_beats_the_profile_and_safety_only_adds() {
    let config = with_profiles(
        "[profiles.p]\nhost = \"from-profile\"\ndbname = \"orders\"\nread-only = true\n",
    );
    let typed = ConnectionOptions {
        host: Some("from-the-flag".to_owned()),
        ..options(Some("p"))
    };
    let (_, args) = resolve_target_and_profile(None, &typed, &config).expect("resolves");
    assert_eq!(args.host.as_deref(), Some("from-the-flag"));
    assert_eq!(
        args.dbname.as_deref(),
        Some("orders"),
        "the rest is filled in"
    );
    assert!(
        args.read_only,
        "a profile can make a session safer; nothing here can make it less safe"
    );

    // And --read-only on a profile that does not ask for it still applies.
    let config = with_profiles("[profiles.p]\nhost = \"h\"\n");
    let read_only = ConnectionOptions {
        read_only: true,
        ..options(Some("p"))
    };
    let (_, args) = resolve_target_and_profile(None, &read_only, &config).expect("resolves");
    assert!(args.read_only);
}

#[test]
fn a_profile_and_a_target_together_are_refused_rather_than_merged() {
    let config = with_profiles("[profiles.p]\nhost = \"h\"\n");
    let error = resolve_target_and_profile(
        Some("postgres://elsewhere/db"),
        &options(Some("p")),
        &config,
    )
    .expect_err("must refuse");
    assert_eq!(error.exit_code(), crate::ExitCode::Usage);
    assert!(
        error
            .next_action
            .unwrap_or_default()
            .contains("one of them")
    );

    // Naming the same profile twice is not a disagreement, so it is allowed.
    let (target, args) =
        resolve_target_and_profile(Some("@p"), &options(Some("p")), &config).expect("resolves");
    assert!(target.is_none());
    assert_eq!(args.host.as_deref(), Some("h"));
}

#[test]
fn an_unknown_profile_lists_the_ones_that_exist() {
    let config = with_profiles(
        "[profiles.orders-prod]\nhost = \"a\"\n\n[profiles.orders-dev]\nhost = \"b\"\n",
    );
    let error = resolve_target_and_profile(Some("@orders-stage"), &options(None), &config)
        .expect_err("must refuse");
    let cause = error.likely_cause.unwrap_or_default();
    assert!(cause.contains("orders-prod"), "{cause}");
    assert!(cause.contains("orders-dev"), "{cause}");

    let empty = crate::config::Config::default();
    let error = resolve_target_and_profile(Some("@anything"), &options(None), &empty)
        .expect_err("must refuse");
    assert!(
        error
            .likely_cause
            .unwrap_or_default()
            .contains("no profiles"),
        "an empty file says so rather than listing nothing"
    );
}

#[test]
fn a_profile_that_tries_to_hold_a_password_is_refused_by_name() {
    for field in ["password", "pgpassword", "sslpassword"] {
        let config = with_profiles(&format!(
            "[profiles.p]\nhost = \"h\"\n{field} = \"hunter2\"\n"
        ));
        let error =
            resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
        assert_eq!(error.exit_code(), crate::ExitCode::Config);
        assert!(
            error.headline.contains("password"),
            "the refusal names what it is about: {}",
            error.headline
        );
        assert!(
            !format!("{error:?}").contains("hunter2"),
            "and never repeats the value back"
        );
        let action = error.next_action.clone().unwrap_or_default();
        assert!(action.contains(".pgpass"), "{action}");
    }

    // A password file is a path rather than a secret, and is still refused
    // here, with the reason and the route that does work.
    let config = with_profiles("[profiles.p]\npassfile = \"/tmp/x\"\n");
    let error =
        resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
    assert!(error.next_action.unwrap_or_default().contains("PGPASSFILE"));

    // Anything else unknown is refused with the fields that exist.
    let config = with_profiles("[profiles.p]\nhosst = \"h\"\n");
    let error =
        resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
    assert!(error.likely_cause.unwrap_or_default().contains("dbname"));
}

#[test]
fn a_profile_with_a_value_this_build_cannot_read_says_which_one() {
    let config = with_profiles("[profiles.p]\nsslmode = \"maybe\"\n");
    let error =
        resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
    assert!(error.headline.contains("sslmode"), "{}", error.headline);
    assert!(
        error
            .next_action
            .unwrap_or_default()
            .contains("verify-full")
    );

    let config = with_profiles("[profiles.p]\nenvironment = \"\"\n");
    let error =
        resolve_target_and_profile(Some("@p"), &options(None), &config).expect_err("refuse");
    assert!(error.headline.contains("environment"), "{}", error.headline);
}

#[test]
fn completions_are_generated_for_every_documented_shell() {
    for shell in [
        clap_complete::Shell::Zsh,
        clap_complete::Shell::Bash,
        clap_complete::Shell::Fish,
        clap_complete::Shell::PowerShell,
    ] {
        let mut out = Vec::new();
        completion(shell, &mut out).expect("generate");
        let text = String::from_utf8(out).expect("utf8");
        assert!(
            text.contains("ignatius"),
            "{shell:?} produced nothing useful"
        );
        assert!(
            text.len() > 200,
            "{shell:?} produced a suspiciously short script"
        );
    }
}

#[test]
fn query_without_sql_or_a_pipe_refuses_instead_of_waiting() {
    // Standard input is a terminal only when a person is at the keyboard;
    // under `cargo test` it is not, so this asserts the file branch instead.
    let err = read_sql(None, Some(std::path::Path::new("/nonexistent/query.sql")))
        .expect_err("must fail");
    assert_eq!(err.kind, DiagnosticKind::Usage);
    assert!(err.next_action.is_some());
}

#[test]
fn the_color_depth_flag_parses_each_pinned_value() {
    for (text, expected) in [
        ("auto", config::ColorDepthChoice::Auto),
        ("truecolor", config::ColorDepthChoice::TrueColor),
        ("256", config::ColorDepthChoice::Indexed256),
        ("16", config::ColorDepthChoice::Basic16),
    ] {
        let cli = Cli::try_parse_from(["ignatius", "--color-depth", text]).expect("parse");
        assert_eq!(cli.global.color_depth, Some(expected), "{text}");
    }
}

#[test]
fn the_color_depth_flag_defaults_to_absent() {
    let cli = Cli::try_parse_from(["ignatius"]).expect("parse");
    assert_eq!(cli.global.color_depth, None);
}

#[test]
fn an_unknown_color_depth_is_refused_by_the_parser() {
    let err = Cli::try_parse_from(["ignatius", "--color-depth", "64"])
        .expect_err("an unknown depth must not parse");
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    assert_ne!(err.exit_code(), 0, "a parse failure is a non-zero exit");
}

#[test]
fn the_color_depth_flag_is_global_and_leaves_plain_and_color_alone() {
    let cli = Cli::try_parse_from([
        "ignatius",
        "query",
        "-c",
        "SELECT 1",
        "--color-depth",
        "256",
    ])
    .expect("parse");
    assert_eq!(
        cli.global.color_depth,
        Some(config::ColorDepthChoice::Indexed256)
    );
    assert!(!cli.global.plain, "an unrelated global keeps its default");
    assert_eq!(
        cli.global.color, None,
        "an unrelated global keeps its default"
    );
}

#[test]
fn color_depth_resolution_prefers_the_flag_then_configuration_then_detection() {
    use crate::config::ColorDepthChoice;
    use crate::ui::theme::{ColorDepth, DepthSource};

    let facts = doctor::TerminalFacts {
        is_terminal: true,
        term: Some("xterm-256color".into()),
        term_program: None,
        no_color: false,
        colorterm: Some("24bit".into()),
        size: Some((100, 30)),
        unicode: true,
        color_depth: ColorDepth::TrueColor,
        color_depth_source: DepthSource::Default,
        background_rgb: None,
    };

    assert_eq!(
        resolve_depth(
            Some(ColorDepthChoice::Basic16),
            ColorDepthChoice::TrueColor,
            true,
            &facts,
        ),
        (ColorDepth::Basic16, DepthSource::Flag),
        "the flag wins and is reported as the flag"
    );
    assert_eq!(
        resolve_depth(None, ColorDepthChoice::Indexed256, true, &facts),
        (ColorDepth::Indexed256, DepthSource::Configuration),
        "a configured value wins over the environment"
    );
    assert_eq!(
        resolve_depth(None, ColorDepthChoice::Auto, true, &facts),
        (ColorDepth::TrueColor, DepthSource::Colorterm),
        "auto falls through to detection"
    );

    let term_only = doctor::TerminalFacts {
        colorterm: None,
        ..facts.clone()
    };
    assert_eq!(
        resolve_depth(None, ColorDepthChoice::Auto, true, &term_only),
        (ColorDepth::Indexed256, DepthSource::Term)
    );
    let plain_term = doctor::TerminalFacts {
        colorterm: None,
        term: Some("xterm".into()),
        ..facts.clone()
    };
    assert_eq!(
        resolve_depth(None, ColorDepthChoice::Auto, true, &plain_term),
        (ColorDepth::Basic16, DepthSource::Default)
    );
    assert_eq!(
        resolve_depth(
            Some(ColorDepthChoice::TrueColor),
            ColorDepthChoice::TrueColor,
            false,
            &facts,
        ),
        (ColorDepth::None, DepthSource::ForcedOff),
        "colour resolved off defeats every setting"
    );
}

#[test]
fn a_target_that_requires_encryption_names_the_tls_stage() {
    use crate::app::model::ConnectingStep;
    use crate::connection::SslMode;

    for (sslmode, expected) in [
        (SslMode::Disable, ConnectingStep::ServerHandshake),
        (SslMode::Prefer, ConnectingStep::ServerHandshake),
        (SslMode::Require, ConnectingStep::TlsHandshake),
        (SslMode::VerifyCa, ConnectingStep::TlsHandshake),
        (SslMode::VerifyFull, ConnectingStep::TlsHandshake),
    ] {
        assert_eq!(
            super::interactive::connecting_step_for(sslmode),
            expected,
            "{sslmode:?}"
        );
    }
}

fn notice_output_execution() -> crate::query::result::Execution {
    use crate::query::result::{NoticeCount, NoticeCounts, NoticeReport, NoticeScope};
    let mut execution = retention_status_execution();
    execution.statements = vec![execution.statements.remove(1)];
    execution.notice_report = NoticeReport {
        scope: NoticeScope::Query,
        retained: Some(256),
        counts: NoticeCounts {
            omitted: NoticeCount::Exact(1),
            ..NoticeCounts::default()
        },
        ..NoticeReport::default()
    };
    execution
}

#[test]
fn notice_output_preserves_data_in_every_format_and_json_has_one_status() {
    for format in [
        Format::Table,
        Format::Csv,
        Format::Tsv,
        Format::Ndjson,
        Format::Json,
        Format::Markdown,
        Format::Insert,
    ] {
        let execution = notice_output_execution();
        let options = output::OutputOptions {
            format,
            insert_table: Some("synthetic_snapshot".into()),
            ..output::OutputOptions::default()
        };
        let mut expected = Vec::new();
        output::write_execution(&mut expected, &execution, &options).unwrap();
        let (mut data, mut status) = (Vec::new(), Vec::new());
        assert_eq!(
            write_query_execution(&mut data, &mut status, &execution, &options).unwrap(),
            ExitCode::Success
        );
        assert_eq!(data, expected, "{format:?} bytes must be unchanged");
        if format == Format::Json {
            let json: serde_json::Value = serde_json::from_slice(&status).unwrap();
            assert_eq!(json["notice_report"]["omitted"]["count"], 1);
            assert!(json.get("exit_code").is_none());
        } else {
            assert_eq!(
                String::from_utf8(status).unwrap(),
                execution.notice_report.counts.lines().join("\n") + "\n"
            );
        }
    }
}

#[test]
fn notice_output_errors_keep_one_json_diagnostic_and_reporting_io_is_ordinary() {
    let execution = notice_output_execution();
    let options = output::OutputOptions {
        format: Format::Json,
        ..output::OutputOptions::default()
    };
    let mut status = Vec::new();
    assert_eq!(
        write_query_execution(
            &mut FailedStatus(std::io::ErrorKind::PermissionDenied),
            &mut status,
            &execution,
            &options
        )
        .unwrap(),
        ExitCode::Internal
    );
    let json: serde_json::Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(json["exit_code"], 70);
    assert_eq!(json["notice_report"]["retained"], 256);
    assert_eq!(
        execution.status,
        crate::query::result::ExecutionStatus::Succeeded,
        "I/O does not rewrite SQL outcome"
    );
    let options = output::OutputOptions {
        format: Format::Csv,
        ..output::OutputOptions::default()
    };
    let error = write_query_execution(
        &mut Vec::new(),
        &mut FailedStatus(std::io::ErrorKind::BrokenPipe),
        &execution,
        &options,
    )
    .unwrap_err();
    assert_eq!(error.kind, DiagnosticKind::Internal);
    let mut status = Vec::new();
    assert_eq!(
        write_query_execution(
            &mut FailedStatus(std::io::ErrorKind::BrokenPipe),
            &mut status,
            &execution,
            &options
        )
        .unwrap(),
        ExitCode::Success
    );
    assert!(
        status.is_empty(),
        "closed data sink preserves the documented non-delivery boundary"
    );
}

#[test]
fn notice_output_unknown_retained_is_null_and_zero_reports_are_silent() {
    let mut execution = notice_output_execution();
    execution.notice_report =
        crate::query::result::NoticeReport::unavailable(crate::query::result::NoticeScope::Query);
    let options = output::OutputOptions {
        format: Format::Json,
        ..output::OutputOptions::default()
    };
    let mut status = Vec::new();
    write_query_execution(&mut Vec::new(), &mut status, &execution, &options).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&status).unwrap();
    assert!(json["notice_report"]["retained"].is_null());
    execution.notice_report = crate::query::result::NoticeReport::default();
    let mut status = Vec::new();
    write_query_execution(&mut Vec::new(), &mut status, &execution, &options).unwrap();
    assert!(status.is_empty());
}

#[test]
fn unavailable_description_status_is_exact_once_instead_of_row_truncation() {
    let mut execution = retention_status_execution();
    let mut set = crate::query::result::ResultSet::unavailable(3, 100);
    set.rows_seen = 7;
    execution.statements[3].result_set = Some(set);
    let mut out = Vec::new();
    write_buffered_truncation(&mut out, &execution, Format::Csv).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("Result set 2 data unavailable: retained no description for 3 source columns and 7 received rows because the retained-result limit was reached. For a future complete streaming export, choose --output before execution.\n"), "{text}");
    assert!(!text.contains("Result set 2 truncated"));
    assert_eq!(text.lines().count(), 2);
}

#[test]
fn unavailable_description_json_keeps_completed_prefix_and_single_notice_failure_status() {
    use crate::query::result::{ExecutionStatus, ResultSet, TransactionState};
    for kind in [
        None,
        Some(DiagnosticKind::Query),
        Some(DiagnosticKind::Cancelled),
        Some(DiagnosticKind::Connection),
    ] {
        let mut execution = notice_output_execution();
        let mut set = ResultSet::unavailable(3, 100);
        set.rows_seen = 7;
        execution.statements[0].result_set = Some(set);
        execution.error = kind.map(|kind| {
            Diagnostic::new(
                kind,
                "synthetic server outcome",
                "reading the requested statement",
            )
        });
        execution.status = match kind {
            None => ExecutionStatus::Succeeded,
            Some(DiagnosticKind::Cancelled) => ExecutionStatus::Cancelled,
            Some(DiagnosticKind::Connection) => ExecutionStatus::ConnectionLost,
            _ => ExecutionStatus::Failed,
        };
        if kind == Some(DiagnosticKind::Connection) {
            execution.transaction = TransactionState::Unknown;
        }
        let expected_exit = execution
            .error
            .as_ref()
            .map_or(ExitCode::Success, Diagnostic::exit_code);
        let (mut data, mut status) = (Vec::new(), Vec::new());
        let options = output::OutputOptions {
            format: Format::Json,
            ..output::OutputOptions::default()
        };
        assert_eq!(
            write_query_execution(&mut data, &mut status, &execution, &options).unwrap(),
            expected_exit
        );
        let payload: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(payload[0]["data_unavailable"], "retained_result_limit");
        assert_eq!(payload[0]["source_column_count"], 3);
        assert_eq!(payload[0]["rows_seen"], 7);
        let diagnostic: serde_json::Value = serde_json::from_slice(&status).unwrap();
        assert_eq!(diagnostic["notice_report"]["omitted"]["count"], 1);
        assert_eq!(status.iter().filter(|b| **b == b'\n').count(), 1);
        assert!(
            !String::from_utf8(status)
                .unwrap()
                .contains("data unavailable:")
        );
    }
}

#[test]
fn replication_and_cluster_commands_parse_flags() {
    Cli::command().debug_assert();

    let rep = Cli::try_parse_from([
        "ignatius",
        "replication",
        "postgres://localhost/db",
        "--section",
        "slots",
        "--format",
        "json",
        "-o",
        "rep.json",
        "--force",
    ])
    .expect("parse replication");
    let Some(Command::Replication(rep_args)) = rep.command else {
        panic!("expected replication command");
    };
    assert_eq!(rep_args.target.as_deref(), Some("postgres://localhost/db"));
    assert_eq!(rep_args.section, CliReplicationSection::Slots);
    assert_eq!(rep_args.format, CliTelemetryFormat::Json);
    assert_eq!(
        rep_args.output.as_deref(),
        Some(std::path::Path::new("rep.json"))
    );
    assert!(rep_args.force);

    let cl = Cli::try_parse_from([
        "ignatius",
        "cluster",
        "--section",
        "settings",
        "--modified-only",
        "--filter",
        "work_mem",
        "--format",
        "plain",
    ])
    .expect("parse cluster");
    let Some(Command::Cluster(cl_args)) = cl.command else {
        panic!("expected cluster command");
    };
    assert_eq!(cl_args.section, CliClusterSection::Settings);
    assert!(cl_args.modified_only);
    assert_eq!(cl_args.filter.as_deref(), Some("work_mem"));
    assert_eq!(cl_args.format, CliTelemetryFormat::Plain);
}

#[test]
fn query_command_handles_headless_metacommands_and_refusals() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::rooted_at(dir.path());

    // 1. \? outputs built-in help in table and json formats without requiring a DB.
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = query_command(
        QueryRequest {
            target: None,
            command: Some("\\?"),
            file: None,
            options: output::OutputOptions {
                format: Format::Json,
                ..output::OutputOptions::default()
            },
            max_rows: None,
            output: None,
            force: false,
            allow_write: false,
            parameter_env: &[],
            connection: &ConnectionOptions::default(),
        },
        &paths,
        &mut out,
        &mut err,
    )
    .expect("\\? json succeeds");
    assert_eq!(code, ExitCode::Success);
    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(!json[0]["rows"].as_array().unwrap().is_empty());

    // 2. \? also supports --output streaming export and --force protection.
    let export_path = dir.path().join("help.csv");
    std::fs::write(&export_path, "existing").unwrap();
    let refused = query_command(
        QueryRequest {
            target: None,
            command: Some("\\?"),
            file: None,
            options: output::OutputOptions {
                format: Format::Csv,
                ..output::OutputOptions::default()
            },
            max_rows: None,
            output: Some(&export_path),
            force: false,
            allow_write: false,
            parameter_env: &[],
            connection: &ConnectionOptions::default(),
        },
        &paths,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .expect_err("existing export path without --force must refuse");
    assert_eq!(refused.kind, DiagnosticKind::Usage);

    let code = query_command(
        QueryRequest {
            target: None,
            command: Some("\\?"),
            file: None,
            options: output::OutputOptions {
                format: Format::Csv,
                ..output::OutputOptions::default()
            },
            max_rows: None,
            output: Some(&export_path),
            force: true,
            allow_write: false,
            parameter_env: &[],
            connection: &ConnectionOptions::default(),
        },
        &paths,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .expect("\\? csv export with --force succeeds");
    assert_eq!(code, ExitCode::Success);
    let csv = std::fs::read_to_string(&export_path).unwrap();
    assert!(csv.contains("\\watch"));

    // 3. Interactive-only metacommands (\x, \timing, \watch) and \! refuse with Usage.
    for cmd in [
        "\\x",
        "\\timing on",
        "SELECT 1; \\watch 2",
        "\\watch stop",
        "\\! id",
    ] {
        let diag = query_command(
            QueryRequest {
                target: None,
                command: Some(cmd),
                file: None,
                options: output::OutputOptions::default(),
                max_rows: None,
                output: None,
                force: false,
                allow_write: false,
                parameter_env: &[],
                connection: &ConnectionOptions::default(),
            },
            &paths,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .expect_err("interactive or shell metacommand must refuse headlessly");
        assert_eq!(diag.kind, DiagnosticKind::Usage);
        assert_eq!(diag.exit_code(), ExitCode::Usage);
    }
}

#[test]
fn filter_replication_and_cluster_reports_respect_sections_and_filters() {
    let mut rep = crate::replication::ReplicationReport::default();
    rep.publications
        .push(crate::replication::PublicationSummary {
            name: "pub_orders".into(),
            owner: "postgres".into(),
            all_tables: true,
            ..Default::default()
        });
    rep.subscriptions
        .push(crate::replication::SubscriptionSummary {
            name: "sub_orders".into(),
            owner: "postgres".into(),
            enabled: true,
            conninfo_redacted: "host=db password=[redacted]".into(),
            ..Default::default()
        });
    rep.slots.push(crate::replication::ReplicationSlotSummary {
        slot_name: "slot_a".into(),
        slot_type: "logical".into(),
        active: false,
        wal_status: "reserved".into(),
        retained_bytes: Some(crate::replication::INACTIVE_SLOT_CRITICAL_BYTES + 1),
        ..Default::default()
    });

    let slots_only = filter_replication_report(&rep, crate::replication::ReplicationSection::Slots);
    assert!(slots_only.publications.is_empty());
    assert!(slots_only.subscriptions.is_empty());
    assert_eq!(slots_only.slots.len(), 1);
    let json =
        filter_replication_report(&rep, crate::replication::ReplicationSection::Subscriptions)
            .render_json();
    assert!(json.contains("password=[redacted]"));

    let mut cluster = crate::cluster::ClusterReport::empty();
    cluster.settings.push(crate::cluster::SettingEntry {
        name: "work_mem".into(),
        setting: "64MB".into(),
        unit: None,
        category: "Resource Usage / Memory".into(),
        context: "user".into(),
        source: "configuration file".into(),
        boot_val: Some("4MB".into()),
        reset_val: Some("64MB".into()),
        pending_restart: false,
        is_non_default: true,
    });
    cluster.settings.push(crate::cluster::SettingEntry {
        name: "shared_buffers".into(),
        setting: "128MB".into(),
        unit: None,
        category: "Resource Usage / Memory".into(),
        context: "postmaster".into(),
        source: "default".into(),
        boot_val: Some("128MB".into()),
        reset_val: Some("128MB".into()),
        pending_restart: false,
        is_non_default: false,
    });
    cluster.extensions.push(crate::cluster::AvailableExtension {
        name: "pg_stat_statements".into(),
        default_version: Some("1.10".into()),
        installed_version: None,
        comment: None,
        upgrade_available: false,
    });

    let filtered = filter_cluster_report(
        &cluster,
        crate::cluster::ClusterSection::Settings,
        true,
        Some("work"),
    );
    assert_eq!(filtered.settings.len(), 1);
    assert_eq!(filtered.settings[0].name, "work_mem");
    assert!(filtered.extensions.is_empty());
}

#[test]
fn browse_plan_diff_and_analytics_commands_parse_flags() {
    Cli::command().debug_assert();

    let browse = Cli::try_parse_from([
        "ignatius",
        "browse",
        "postgres://localhost/app",
        "--table",
        "sales.orders",
        "--page-size",
        "25",
        "--sort",
        "created_at",
        "--desc",
        "--after",
        "100,200",
        "--prev",
        "--filter",
        "status = 'open'",
        "--format",
        "json",
        "-o",
        "page.json",
        "--force",
    ])
    .expect("parse browse");
    let Some(Command::Browse(browse_args)) = browse.command else {
        panic!("expected browse command");
    };
    assert_eq!(
        browse_args.target.as_deref(),
        Some("postgres://localhost/app")
    );
    assert_eq!(browse_args.table, "sales.orders");
    assert_eq!(browse_args.page_size, 25);
    assert_eq!(browse_args.sort.as_deref(), Some("created_at"));
    assert!(browse_args.desc);
    assert_eq!(browse_args.after, vec!["100", "200"]);
    assert!(browse_args.prev);
    assert_eq!(browse_args.filter.as_deref(), Some("status = 'open'"));
    assert_eq!(browse_args.format, CliTelemetryFormat::Json);
    assert_eq!(
        browse_args.output.as_deref(),
        Some(std::path::Path::new("page.json"))
    );
    assert!(browse_args.force);

    let browse_repeatable = Cli::try_parse_from([
        "ignatius", "browse", "-t", "users", "--after", "1", "--after", "2",
    ])
    .expect("parse repeatable after");
    let Some(Command::Browse(browse_rep_args)) = browse_repeatable.command else {
        panic!("expected browse command");
    };
    assert_eq!(browse_rep_args.table, "users");
    assert_eq!(browse_rep_args.page_size, crate::crud::DEFAULT_PAGE_SIZE);
    assert_eq!(browse_rep_args.after, vec!["1", "2"]);
    assert_eq!(browse_rep_args.format, CliTelemetryFormat::Table);

    let plan_diff = Cli::try_parse_from([
        "ignatius",
        "plan-diff",
        "postgres://localhost/app",
        "-a",
        "SELECT 1",
        "-b",
        "SELECT 2",
        "--analyze",
        "--format",
        "plain",
        "-o",
        "diff.txt",
        "--force",
    ])
    .expect("parse plan-diff");
    let Some(Command::PlanDiff(diff_args)) = plan_diff.command else {
        panic!("expected plan-diff command");
    };
    assert_eq!(
        diff_args.target.as_deref(),
        Some("postgres://localhost/app")
    );
    assert_eq!(diff_args.baseline.as_deref(), Some("SELECT 1"));
    assert_eq!(diff_args.candidate.as_deref(), Some("SELECT 2"));
    assert!(diff_args.analyze);
    assert_eq!(diff_args.format, CliTelemetryFormat::Plain);
    assert_eq!(
        diff_args.output.as_deref(),
        Some(std::path::Path::new("diff.txt"))
    );
    assert!(diff_args.force);

    let analytics_html = Cli::try_parse_from([
        "ignatius",
        "analytics",
        "postgres://localhost/app",
        "-c",
        "SELECT id, amount FROM orders",
        "--format",
        "html",
        "--title",
        "Orders Profile",
        "-o",
        "orders.html",
        "--force",
    ])
    .expect("parse analytics html");
    let Some(Command::Analytics(html_args)) = analytics_html.command else {
        panic!("expected analytics command");
    };
    assert_eq!(
        html_args.target.as_deref(),
        Some("postgres://localhost/app")
    );
    assert_eq!(
        html_args.command.as_deref(),
        Some("SELECT id, amount FROM orders")
    );
    assert_eq!(html_args.format, CliAnalyticsFormat::Html);
    assert_eq!(html_args.title, "Orders Profile");
    assert_eq!(
        html_args.output.as_deref(),
        Some(std::path::Path::new("orders.html"))
    );
    assert!(html_args.force);

    let analytics_parquet = Cli::try_parse_from([
        "ignatius",
        "analytics",
        "-f",
        "query.sql",
        "--format",
        "parquet",
        "-o",
        "orders.parquet",
    ])
    .expect("parse analytics parquet");
    let Some(Command::Analytics(parquet_args)) = analytics_parquet.command else {
        panic!("expected analytics command");
    };
    assert_eq!(
        parquet_args.file.as_deref(),
        Some(std::path::Path::new("query.sql"))
    );
    assert_eq!(parquet_args.format, CliAnalyticsFormat::Parquet);
    assert_eq!(parquet_args.title, "Ignatius Result Analytics");
}

#[test]
fn browse_table_name_and_m13_cli_helpers_validate_and_render() {
    assert_eq!(
        parse_browse_table_name("users").unwrap(),
        ("public".to_owned(), "users".to_owned())
    );
    assert_eq!(
        parse_browse_table_name("sales.orders").unwrap(),
        ("sales".to_owned(), "orders".to_owned())
    );
    assert_eq!(
        parse_browse_table_name("\"MySchema\".\"My.Table\"").unwrap(),
        ("MySchema".to_owned(), "My.Table".to_owned())
    );
    assert_eq!(
        parse_browse_table_name("\"my.table\"").unwrap(),
        ("public".to_owned(), "my.table".to_owned())
    );
    for invalid in ["", "   ", "a.b.c", ".users", "public.", "\"unterminated"] {
        let diag = parse_browse_table_name(invalid).expect_err("invalid table name");
        assert_eq!(diag.kind, DiagnosticKind::Usage);
    }

    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.sql");
    let cand_path = dir.path().join("cand.sql");
    std::fs::write(&base_path, "SELECT 1").unwrap();
    std::fs::write(&cand_path, "SELECT 2").unwrap();

    let empty_diff = PlanDiffArgs {
        target: None,
        baseline: None,
        baseline_file: None,
        candidate: None,
        candidate_file: None,
        analyze: false,
        format: CliTelemetryFormat::Table,
        output: None,
        force: false,
        connection: ConnectionOptions::default(),
    };
    assert_eq!(
        resolve_plan_diff_inputs(&empty_diff).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let both_base = PlanDiffArgs {
        baseline: Some("SELECT 1".to_owned()),
        baseline_file: Some(base_path.clone()),
        ..empty_diff.clone()
    };
    assert_eq!(
        resolve_plan_diff_inputs(&both_base).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let both_cand = PlanDiffArgs {
        baseline: Some("SELECT 1".to_owned()),
        candidate: Some("SELECT 2".to_owned()),
        candidate_file: Some(cand_path.clone()),
        ..empty_diff.clone()
    };
    assert_eq!(
        resolve_plan_diff_inputs(&both_cand).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let file_inputs = PlanDiffArgs {
        baseline_file: Some(base_path.clone()),
        candidate_file: Some(cand_path),
        ..empty_diff
    };
    assert_eq!(
        resolve_plan_diff_inputs(&file_inputs).unwrap(),
        ("SELECT 1".to_owned(), Some("SELECT 2".to_owned()))
    );

    let mut settings = std::collections::BTreeMap::new();
    settings.insert("work_mem".to_owned(), "64MB".to_owned());
    let snapshot = crate::plandiff::PlanSnapshot {
        label: "Baseline (A)".to_owned(),
        analyzed: true,
        planning_time_ms: Some(0.25),
        execution_time_ms: Some(1.5),
        total_cost: 12.5,
        startup_cost: 0.0,
        plan_rows: 10,
        actual_rows: Some(10),
        shared_hit_blocks: 8,
        shared_read_blocks: 2,
        shared_dirtied_blocks: 0,
        shared_written_blocks: 0,
        local_hit_blocks: 0,
        local_read_blocks: 0,
        temp_read_blocks: 0,
        temp_written_blocks: 0,
        wal_records: 3,
        wal_fpi: 1,
        wal_bytes: 256,
        settings,
        nodes: vec![crate::plandiff::PlanNodeSummary {
            path: "0".to_owned(),
            depth: 0,
            node_type: "Seq Scan".to_owned(),
            relation_name: Some("orders".to_owned()),
            index_name: None,
            startup_cost: 0.0,
            total_cost: 12.5,
            plan_rows: 10,
            actual_rows: Some(10),
            actual_total_time_ms: Some(1.2),
            shared_hit_blocks: 8,
            shared_read_blocks: 2,
            temp_written_blocks: 0,
            wal_bytes: 256,
        }],
        buffer_hit_ratio_pct: Some(80.0),
    };
    let snap_table = render_plan_snapshot_table(&snapshot);
    assert!(snap_table.contains("EXTENDED EXPLAIN SNAPSHOT: Baseline (A)"));
    assert!(snap_table.contains("work_mem = 64MB"));
    assert!(snap_table.contains("Seq Scan"));
    let snap_plain = render_plan_snapshot_plain(&snapshot);
    assert!(snap_plain.contains("wal: records=3 fpi=1 bytes=256"));
    assert!(snap_plain.contains("setting: work_mem=64MB"));

    assert_eq!(
        resolve_analytics_sql(None, None).unwrap_err().kind,
        DiagnosticKind::Usage
    );
    assert_eq!(
        resolve_analytics_sql(Some("SELECT 1"), Some(&base_path))
            .unwrap_err()
            .kind,
        DiagnosticKind::Usage
    );
    assert_eq!(
        resolve_analytics_sql(None, Some(&base_path)).unwrap(),
        "SELECT 1"
    );

    let no_rows_exec = crate::query::result::Execution {
        job: crate::query::result::JobId(1),
        statements: vec![crate::query::result::StatementResult {
            result_set: None,
            rows_affected: Some(1),
            elapsed: std::time::Duration::from_millis(1),
            notices: Vec::new(),
        }],
        status: crate::query::result::ExecutionStatus::Succeeded,
        elapsed: std::time::Duration::from_millis(1),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
        notice_report: crate::query::result::NoticeReport::default(),
    };
    assert_eq!(
        extract_analytics_result_set(no_rows_exec).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let mut set = crate::query::result::ResultSet::new(vec!["id".to_owned()], 10);
    set.push(vec![crate::query::value::Cell::Text("42".to_owned())]);
    let parquet_bytes = crate::analytics::export_parquet_bytes(&set).unwrap();
    let parquet_path = dir.path().join("out.parquet");
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(
        write_binary_output(
            &parquet_bytes,
            Some(&parquet_path),
            false,
            "Parquet analytics export",
            &mut out,
            &mut err
        )
        .unwrap(),
        ExitCode::Success
    );
    assert!(out.is_empty());
    assert_eq!(std::fs::read(&parquet_path).unwrap(), parquet_bytes);
    let mut stdout_bin = Vec::new();
    assert_eq!(
        write_binary_output(
            &parquet_bytes,
            None,
            false,
            "Parquet analytics export",
            &mut stdout_bin,
            &mut err
        )
        .unwrap(),
        ExitCode::Success
    );
    assert_eq!(stdout_bin, parquet_bytes);
}

#[test]
fn bench_snapshot_and_runbook_commands_parse_flags() {
    Cli::command().debug_assert();

    let bench = Cli::try_parse_from([
        "ignatius",
        "bench",
        "postgres://localhost/app",
        "-c",
        "SELECT 1",
        "--candidate",
        "SELECT 2",
        "--warmup",
        "3",
        "--runs",
        "25",
        "--no-rollback",
        "--fail-on-regression-pct",
        "12.5",
        "--format",
        "json",
        "-o",
        "bench.json",
        "--force",
    ])
    .expect("parse bench");
    let Some(Command::Bench(bench_args)) = bench.command else {
        panic!("expected bench command");
    };
    assert_eq!(
        bench_args.target.as_deref(),
        Some("postgres://localhost/app")
    );
    assert_eq!(bench_args.command.as_deref(), Some("SELECT 1"));
    assert_eq!(bench_args.candidate.as_deref(), Some("SELECT 2"));
    assert_eq!(bench_args.warmup, 3);
    assert_eq!(bench_args.runs, 25);
    assert!(bench_args.no_rollback);
    assert_eq!(bench_args.fail_on_regression_pct, Some(12.5));
    assert_eq!(bench_args.format, CliTelemetryFormat::Json);
    assert_eq!(
        bench_args.output.as_deref(),
        Some(std::path::Path::new("bench.json"))
    );
    assert!(bench_args.force);

    let bench_defaults = Cli::try_parse_from([
        "ignatius",
        "bench",
        "-f",
        "base.sql",
        "--candidate-file",
        "cand.sql",
    ])
    .expect("parse bench defaults");
    let Some(Command::Bench(bench_def_args)) = bench_defaults.command else {
        panic!("expected bench command");
    };
    assert_eq!(
        bench_def_args.file.as_deref(),
        Some(std::path::Path::new("base.sql"))
    );
    assert_eq!(
        bench_def_args.candidate_file.as_deref(),
        Some(std::path::Path::new("cand.sql"))
    );
    assert_eq!(bench_def_args.warmup, crate::bench::DEFAULT_WARMUP_RUNS);
    assert_eq!(bench_def_args.runs, crate::bench::DEFAULT_MEASURED_RUNS);
    assert!(!bench_def_args.no_rollback);
    assert_eq!(bench_def_args.fail_on_regression_pct, None);
    assert_eq!(bench_def_args.format, CliTelemetryFormat::Table);

    let snap_capture = Cli::try_parse_from([
        "ignatius",
        "snapshot",
        "capture",
        "postgres://localhost/app",
        "-s",
        "public",
        "-f",
        "table",
        "-o",
        "snap.txt",
        "--force",
    ])
    .expect("parse snapshot capture");
    let Some(Command::Snapshot(SnapshotArgs {
        action: SnapshotAction::Capture(cap_args),
    })) = snap_capture.command
    else {
        panic!("expected snapshot capture command");
    };
    assert_eq!(cap_args.target.as_deref(), Some("postgres://localhost/app"));
    assert_eq!(cap_args.schema.as_deref(), Some("public"));
    assert_eq!(cap_args.format, CliTelemetryFormat::Table);
    assert_eq!(
        cap_args.output.as_deref(),
        Some(std::path::Path::new("snap.txt"))
    );
    assert!(cap_args.force);

    let snap_capture_default =
        Cli::try_parse_from(["ignatius", "snapshot", "capture"]).expect("capture defaults");
    let Some(Command::Snapshot(SnapshotArgs {
        action: SnapshotAction::Capture(cap_def),
    })) = snap_capture_default.command
    else {
        panic!("expected snapshot capture command");
    };
    assert_eq!(cap_def.format, CliTelemetryFormat::Json);

    let snap_check = Cli::try_parse_from([
        "ignatius",
        "snapshot",
        "check",
        "postgres://localhost/app",
        "--against",
        "baseline.json",
        "-s",
        "public",
        "--fail-on",
        "warning",
        "-f",
        "json",
        "-o",
        "drift.json",
        "--force",
    ])
    .expect("parse snapshot check");
    let Some(Command::Snapshot(SnapshotArgs {
        action: SnapshotAction::Check(check_args),
    })) = snap_check.command
    else {
        panic!("expected snapshot check command");
    };
    assert_eq!(
        check_args.target.as_deref(),
        Some("postgres://localhost/app")
    );
    assert_eq!(
        check_args.against.as_path(),
        std::path::Path::new("baseline.json")
    );
    assert_eq!(check_args.schema.as_deref(), Some("public"));
    assert_eq!(check_args.fail_on, CliDriftFailOn::Warning);
    assert_eq!(check_args.format, CliTelemetryFormat::Json);
    assert!(check_args.force);

    let snap_check_default =
        Cli::try_parse_from(["ignatius", "snapshot", "check", "--against", "b.json"])
            .expect("check defaults");
    let Some(Command::Snapshot(SnapshotArgs {
        action: SnapshotAction::Check(check_def),
    })) = snap_check_default.command
    else {
        panic!("expected snapshot check command");
    };
    assert_eq!(check_def.fail_on, CliDriftFailOn::Breaking);
    assert_eq!(check_def.format, CliTelemetryFormat::Table);

    let runbook = Cli::try_parse_from([
        "ignatius",
        "runbook",
        "postgres://localhost/app",
        "-f",
        "ops.md",
        "--cell",
        "smoke",
        "--dry-run",
        "--allow-writes",
        "--stop-on-error",
        "--format",
        "markdown",
        "-o",
        "report.md",
        "--force",
    ])
    .expect("parse runbook");
    let Some(Command::Runbook(rb_args)) = runbook.command else {
        panic!("expected runbook command");
    };
    assert_eq!(rb_args.target.as_deref(), Some("postgres://localhost/app"));
    assert_eq!(rb_args.file.as_path(), std::path::Path::new("ops.md"));
    assert_eq!(rb_args.cell.as_deref(), Some("smoke"));
    assert!(rb_args.dry_run);
    assert!(rb_args.allow_writes);
    assert!(rb_args.stop_on_error);
    assert_eq!(rb_args.format, CliRunbookFormat::Markdown);
    assert_eq!(
        rb_args.output.as_deref(),
        Some(std::path::Path::new("report.md"))
    );
    assert!(rb_args.force);

    let runbook_offline = Cli::try_parse_from(["ignatius", "runbook", "-f", "ops.md", "--dry-run"])
        .expect("parse offline dry-run runbook");
    let Some(Command::Runbook(rb_off_args)) = runbook_offline.command else {
        panic!("expected runbook command");
    };
    assert_eq!(rb_off_args.target, None);
    assert!(rb_off_args.dry_run);
    assert_eq!(rb_off_args.format, CliRunbookFormat::Table);
}

#[test]
fn bench_snapshot_and_runbook_cli_helpers_validate_and_render() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.sql");
    let cand_path = dir.path().join("cand.sql");
    std::fs::write(&base_path, "SELECT 1").unwrap();
    std::fs::write(&cand_path, "SELECT 2").unwrap();

    assert_eq!(validate_regression_pct(None).unwrap(), None);
    assert_eq!(validate_regression_pct(Some(10.0)).unwrap(), Some(10.0));
    for bad in [-0.1, f64::NAN, f64::INFINITY] {
        assert_eq!(
            validate_regression_pct(Some(bad)).unwrap_err().kind,
            DiagnosticKind::Usage
        );
    }

    let empty_bench = BenchArgs {
        target: None,
        command: None,
        file: None,
        candidate: None,
        candidate_file: None,
        warmup: 2,
        runs: 10,
        no_rollback: false,
        fail_on_regression_pct: None,
        format: CliTelemetryFormat::Table,
        output: None,
        force: false,
        allow_write: false,
        connection: ConnectionOptions::default(),
    };
    assert_eq!(
        resolve_bench_inputs(&empty_bench).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let both_base = BenchArgs {
        command: Some("SELECT 1".to_owned()),
        file: Some(base_path.clone()),
        ..empty_bench.clone()
    };
    assert_eq!(
        resolve_bench_inputs(&both_base).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let both_cand = BenchArgs {
        command: Some("SELECT 1".to_owned()),
        candidate: Some("SELECT 2".to_owned()),
        candidate_file: Some(cand_path.clone()),
        ..empty_bench.clone()
    };
    assert_eq!(
        resolve_bench_inputs(&both_cand).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let both_stdin = BenchArgs {
        file: Some(PathBuf::from("-")),
        candidate_file: Some(PathBuf::from("-")),
        ..empty_bench.clone()
    };
    assert_eq!(
        resolve_bench_inputs(&both_stdin).unwrap_err().kind,
        DiagnosticKind::Usage
    );

    let file_bench = BenchArgs {
        file: Some(base_path),
        candidate_file: Some(cand_path),
        ..empty_bench
    };
    assert_eq!(
        resolve_bench_inputs(&file_bench).unwrap(),
        ("SELECT 1".to_owned(), Some("SELECT 2".to_owned()))
    );

    // Snapshot render & JSON reader validation
    let mut snapshot = crate::snapshot::SchemaSnapshot::new(Some("public".to_owned()));
    snapshot.tables.push(crate::snapshot::TableSnapshot {
        schema: "public".to_owned(),
        name: "orders".to_owned(),
        columns: vec![crate::snapshot::ColumnSnapshot {
            name: "order_id".to_owned(),
            data_type: "bigint".to_owned(),
            nullable: false,
            default_expr: None,
            ordinal: 1,
        }],
        constraints: Vec::new(),
        indexes: Vec::new(),
        rls_enabled: true,
        rls_forced: false,
        policies: Vec::new(),
    });
    snapshot.views.push(crate::snapshot::ViewSnapshot {
        schema: "public".to_owned(),
        name: "recent_orders".to_owned(),
        materialized: false,
        definition_sha256: "abc123".to_owned(),
    });
    snapshot.sequences.push(crate::snapshot::SequenceSnapshot {
        schema: "public".to_owned(),
        name: "orders_order_id_seq".to_owned(),
        data_type: "bigint".to_owned(),
        start_value: 1,
        increment_by: 1,
        cycle: false,
    });
    snapshot.enums.push(crate::snapshot::EnumSnapshot {
        schema: "public".to_owned(),
        name: "order_tier".to_owned(),
        labels: vec!["standard".to_owned(), "priority".to_owned()],
    });
    let snapshot = snapshot.finalize();

    let table_view = render_snapshot_table(&snapshot);
    assert!(table_view.is_ascii());
    assert!(table_view.contains("public.orders"));
    assert!(table_view.contains("public.recent_orders"));
    assert!(table_view.contains("public.orders_order_id_seq"));
    assert!(table_view.contains("public.order_tier"));

    let plain_view = render_snapshot_plain(&snapshot);
    assert!(plain_view.is_ascii());
    assert!(plain_view.contains("table=public.orders cols=1"));
    assert!(plain_view.contains("view=public.recent_orders"));

    let snap_path = dir.path().join("snapshot.json");
    std::fs::write(&snap_path, snapshot.render_json()).unwrap();
    let loaded_snap = read_expected_snapshot(&snap_path).unwrap();
    assert_eq!(loaded_snap, snapshot);
    assert_eq!(
        read_expected_snapshot(&dir.path().join("missing.json"))
            .unwrap_err()
            .kind,
        DiagnosticKind::Usage
    );

    // Runbook offline dry-run & cell execution evaluator
    let md_path = dir.path().join("ops.md");
    std::fs::write(
        &md_path,
        "# Ops Runbook\n\n## Inspect\n```sql\n-- @name: read-orders\n-- @expect-rows: 3\nSELECT order_id FROM public.orders;\n```\n\n## Mutate\n```sql\n-- @name: purge\nDELETE FROM public.orders;\n```\n",
    )
    .unwrap();

    assert_eq!(
        load_markdown_runbook(&md_path, Some("missing-cell"))
            .unwrap_err()
            .kind,
        DiagnosticKind::Usage
    );

    let report_md_path = dir.path().join("dry_run.md");
    let cfg_dir = dir.path().join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let dry_cli = Cli::try_parse_from([
        "ignatius",
        "--config-dir",
        cfg_dir.to_str().unwrap(),
        "runbook",
        "-f",
        md_path.to_str().unwrap(),
        "--dry-run",
        "--format",
        "markdown",
        "-o",
        report_md_path.to_str().unwrap(),
    ])
    .unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(run(&dry_cli, &mut out, &mut err), ExitCode::Success);
    assert!(out.is_empty());
    let written_md = std::fs::read_to_string(&report_md_path).unwrap();
    assert!(written_md.contains("# Runbook Report:"));
    assert!(written_md.contains("read-orders"));
    assert!(written_md.contains("blocked-by-write-guard"));

    // Refuses overwrite without --force in dry-run too
    let dry_overwrite = Cli::try_parse_from([
        "ignatius",
        "--config-dir",
        cfg_dir.to_str().unwrap(),
        "runbook",
        "-f",
        md_path.to_str().unwrap(),
        "--dry-run",
        "-o",
        report_md_path.to_str().unwrap(),
    ])
    .unwrap();
    let mut err_overwrite = Vec::new();
    assert_eq!(
        run(&dry_overwrite, &mut Vec::new(), &mut err_overwrite),
        ExitCode::Usage
    );

    // Cell execution evaluator checks expect_rows and SQL errors
    let rb = load_markdown_runbook(&md_path, None).unwrap();
    let read_cell = &rb.cells[0];
    let mut three_rows = crate::query::result::ResultSet::new(vec!["order_id".to_owned()], 100);
    for id in ["1", "2", "3"] {
        three_rows.push(vec![crate::query::value::Cell::Text(id.to_owned())]);
    }
    let ok_exec = crate::query::result::Execution {
        job: crate::query::result::JobId(1),
        statements: vec![crate::query::result::StatementResult {
            result_set: Some(three_rows),
            rows_affected: Some(3),
            elapsed: std::time::Duration::from_millis(2),
            notices: Vec::new(),
        }],
        status: crate::query::result::ExecutionStatus::Succeeded,
        elapsed: std::time::Duration::from_millis(2),
        error: None,
        transaction: crate::query::result::TransactionState::Autocommit,
        notice_report: crate::query::result::NoticeReport::default(),
    };
    let passed = evaluate_runbook_cell_execution(read_cell, &ok_exec, 2);
    assert_eq!(passed.status, crate::runbook::CellStatus::Passed);
    assert_eq!(passed.rows_returned, Some(3));
    assert_eq!(passed.rows_affected, None);

    let one_row_exec = crate::query::result::Execution {
        statements: vec![crate::query::result::StatementResult {
            result_set: Some({
                let mut set = crate::query::result::ResultSet::new(vec!["order_id".to_owned()], 10);
                set.push(vec![crate::query::value::Cell::Text("1".to_owned())]);
                set
            }),
            rows_affected: Some(1),
            elapsed: std::time::Duration::from_millis(1),
            notices: Vec::new(),
        }],
        ..ok_exec.clone()
    };
    let mismatch = evaluate_runbook_cell_execution(read_cell, &one_row_exec, 1);
    assert_eq!(mismatch.status, crate::runbook::CellStatus::Failed);
    assert!(
        mismatch
            .message
            .as_deref()
            .is_some_and(|m| m.contains("expected 3 row(s), got 1"))
    );

    let err_exec = crate::query::result::Execution {
        statements: Vec::new(),
        status: crate::query::result::ExecutionStatus::Failed,
        error: Some(
            Diagnostic::new(
                DiagnosticKind::Query,
                "relation \"nope\" does not exist",
                "running query",
            )
            .technical("SQLSTATE", "42P01"),
        ),
        ..ok_exec
    };
    let failed = evaluate_runbook_cell_execution(read_cell, &err_exec, 1);
    assert_eq!(failed.status, crate::runbook::CellStatus::Failed);
    assert!(
        failed
            .message
            .as_deref()
            .is_some_and(|m| m.contains("[42P01]"))
    );
}

#[test]
fn bench_production_write_guard_requires_allow_write_for_non_read_statements() {
    use crate::connection::Environment;

    let parsed = Cli::try_parse_from([
        "ignatius",
        "bench",
        "--allow-write",
        "-c",
        "DELETE FROM orders",
    ])
    .expect("bench --allow-write parses");
    match parsed.command {
        Some(Command::Bench(args)) => assert!(args.allow_write),
        other => panic!("expected Command::Bench, got {other:?}"),
    }

    // Production refuses non-read baseline or candidate unless --allow-write is passed.
    let err = check_bench_production_write_guard(
        &Environment::Production,
        false,
        "DELETE FROM orders",
        None,
    )
    .expect_err("production DELETE must be refused");
    assert_eq!(err.kind, DiagnosticKind::Usage);
    assert!(err.headline.contains("classified as production"));
    assert!(
        err.next_action
            .as_deref()
            .is_some_and(|s| s.contains("--allow-write") && s.contains("not a security boundary"))
    );

    let cand_err = check_bench_production_write_guard(
        &Environment::Production,
        false,
        "SELECT 1",
        Some("UPDATE orders SET total = 1"),
    )
    .expect_err("production candidate write must be refused");
    assert_eq!(cand_err.kind, DiagnosticKind::Usage);

    let explain_err = check_bench_production_write_guard(
        &Environment::Production,
        false,
        "EXPLAIN ANALYZE DELETE FROM orders",
        None,
    )
    .expect_err("production EXPLAIN ANALYZE write must be refused");
    assert_eq!(explain_err.kind, DiagnosticKind::Usage);

    // Reads on production, explicit --allow-write, and non-production writes pass.
    check_bench_production_write_guard(
        &Environment::Production,
        false,
        "SELECT 1",
        Some("SELECT 2"),
    )
    .expect("production reads are allowed");
    check_bench_production_write_guard(
        &Environment::Production,
        true,
        "DELETE FROM orders",
        Some("UPDATE orders SET total = 1"),
    )
    .expect("--allow-write unlocks production benchmarks");
    check_bench_production_write_guard(
        &Environment::Development,
        false,
        "DELETE FROM orders",
        None,
    )
    .expect("non-production writes need no --allow-write");
}

#[test]
fn target_session_attrs_cli_flag_validates_and_overrides_profile() {
    Cli::command().debug_assert();

    let cli = Cli::try_parse_from([
        "ignatius",
        "query",
        "-c",
        "SELECT 1",
        "--target-session-attrs",
        "read-only",
    ])
    .expect("parse --target-session-attrs");
    let Some(Command::Query { connection, .. }) = cli.command else {
        panic!("expected query command");
    };
    assert_eq!(
        connection.target_session_attrs.as_deref(),
        Some("read-only")
    );
    let args = connection.to_args().expect("to_args");
    assert_eq!(
        args.target_session_attrs,
        Some(TargetSessionAttrs::ReadOnly)
    );

    let bad_options = ConnectionOptions {
        target_session_attrs: Some("bogus".into()),
        ..ConnectionOptions::default()
    };
    let err = bad_options.to_args().expect_err("invalid flag must fail");
    assert_eq!(err.kind, DiagnosticKind::Usage);
    assert_eq!(err.exit_code(), ExitCode::Usage);
    assert!(err.next_action.is_some());

    let config = with_profiles(
        "[profiles.replica]\n\
         host = \"replica.example.net\"\n\
         target-session-attrs = \"standby\"\n",
    );
    let (_, profile_args) =
        resolve_target_and_profile(Some("@replica"), &options(None), &config).expect("profile");
    assert_eq!(
        profile_args.target_session_attrs,
        Some(TargetSessionAttrs::Standby)
    );

    let cli_override = ConnectionOptions {
        target_session_attrs: Some("primary".into()),
        ..options(Some("replica"))
    };
    let (_, override_args) =
        resolve_target_and_profile(None, &cli_override, &config).expect("cli overrides profile");
    assert_eq!(
        override_args.target_session_attrs,
        Some(TargetSessionAttrs::Primary)
    );

    let bad_profile = with_profiles(
        "[profiles.broken]\n\
         host = \"db.example.net\"\n\
         target-session-attrs = \"invalid\"\n",
    );
    let err = resolve_target_and_profile(Some("@broken"), &options(None), &bad_profile)
        .expect_err("invalid profile target-session-attrs must fail");
    assert_eq!(err.kind, DiagnosticKind::Config);
}

#[test]
fn result_diff_cli_runs_offline_across_formats_and_writes_output() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ignatius-cli-result-diff-{unique}"));
    std::fs::create_dir_all(&dir).unwrap();

    let baseline = dir.join("baseline.json");
    let current = dir.join("current.ndjson");
    let out_file = dir.join("report.json");

    std::fs::write(
        &baseline,
        r#"{"columns":["id","state"],"rows":[{"id":"1","state":"open"},{"id":"2","state":null}]}"#,
    )
    .unwrap();
    std::fs::write(
        &current,
        "{\"id\":\"1\",\"state\":\"closed\"}\n{\"id\":\"3\",\"state\":\"new\"}\n",
    )
    .unwrap();

    let cli = Cli::try_parse_from([
        "ignatius",
        "result-diff",
        baseline.to_str().unwrap(),
        current.to_str().unwrap(),
        "--key",
        "id",
        "--format",
        "json",
        "--output",
        out_file.to_str().unwrap(),
    ])
    .expect("parse result-diff");

    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&cli, &mut out, &mut err);
    assert_eq!(
        code,
        ExitCode::Success,
        "stderr: {}",
        String::from_utf8_lossy(&err)
    );
    assert!(out.is_empty());

    let written = std::fs::read_to_string(&out_file).unwrap();
    let json: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["counts"]["changed"], 1);
    assert_eq!(json["counts"]["removed"], 1);
    assert_eq!(json["counts"]["added"], 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn queries_cli_lists_filters_and_shows_saved_queries_offline() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let cfg_dir = std::env::temp_dir().join(format!("ignatius-cli-queries-{unique}"));
    let queries_dir = cfg_dir.join("queries");
    let library = crate::queries::Library::new(&queries_dir);
    library
        .save(
            "billing/monthly-revenue",
            "-- @tags: finance, monthly\n-- @description: Monthly gross revenue\nSELECT sum(total) FROM orders;",
        )
        .expect("save billing query");
    library
        .save(
            "ops/locks",
            "-- @tag: locks\nSELECT pid FROM pg_stat_activity;",
        )
        .expect("save ops query");
    library
        .save("health", "SELECT 1;")
        .expect("save root query");

    let cli = Cli::try_parse_from([
        "ignatius",
        "--config-dir",
        cfg_dir.to_str().unwrap(),
        "queries",
        "list",
        "--collection",
        "billing",
        "--tag",
        "#finance",
        "--format",
        "json",
    ])
    .expect("parse queries list");
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(run(&cli, &mut out, &mut err), ExitCode::Success);
    assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
    let listed: serde_json::Value = serde_json::from_slice(&out).expect("valid queries list json");
    let arr = listed.as_array().expect("json array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "billing/monthly-revenue");
    assert_eq!(arr[0]["collection"], "billing");
    assert_eq!(arr[0]["tags"], serde_json::json!(["finance", "monthly"]));
    assert_eq!(arr[0]["description"], "Monthly gross revenue");

    let show_cli = Cli::try_parse_from([
        "ignatius",
        "--config-dir",
        cfg_dir.to_str().unwrap(),
        "queries",
        "show",
        "billing/monthly-revenue",
    ])
    .expect("parse queries show");
    let mut show_out = Vec::new();
    let mut show_err = Vec::new();
    assert_eq!(
        run(&show_cli, &mut show_out, &mut show_err),
        ExitCode::Success
    );
    assert!(show_err.is_empty());
    let sql = String::from_utf8(show_out).unwrap();
    assert!(sql.contains("SELECT sum(total) FROM orders;"), "{sql}");

    let bad_cli = Cli::try_parse_from([
        "ignatius",
        "--config-dir",
        cfg_dir.to_str().unwrap(),
        "queries",
        "show",
        "../secret",
    ])
    .expect("parse invalid show");
    let mut bad_out = Vec::new();
    let mut bad_err = Vec::new();
    assert_eq!(run(&bad_cli, &mut bad_out, &mut bad_err), ExitCode::Usage);
    assert!(bad_out.is_empty());
    assert!(!bad_err.is_empty());

    let _ = std::fs::remove_dir_all(&cfg_dir);
}

#[test]
fn session_timeout_cli_flags_validate_and_override_profile() {
    use std::time::Duration;

    Cli::command().debug_assert();

    let cli = Cli::try_parse_from([
        "ignatius",
        "query",
        "-c",
        "SELECT 1",
        "--statement-timeout",
        "500ms",
        "--lock-timeout",
        "2s",
        "--idle-in-transaction-session-timeout",
        "3m",
    ])
    .expect("parse timeout flags");
    let Some(Command::Query { connection, .. }) = cli.command else {
        panic!("expected query command");
    };
    assert_eq!(connection.statement_timeout.as_deref(), Some("500ms"));
    assert_eq!(connection.lock_timeout.as_deref(), Some("2s"));
    assert_eq!(
        connection.idle_in_transaction_session_timeout.as_deref(),
        Some("3m")
    );
    let args = connection.to_args().expect("to_args");
    assert_eq!(args.statement_timeout, Some(Duration::from_millis(500)));
    assert_eq!(args.lock_timeout, Some(Duration::from_secs(2)));
    assert_eq!(
        args.idle_in_transaction_session_timeout,
        Some(Duration::from_secs(180))
    );

    let bad = ConnectionOptions {
        statement_timeout: Some("-5s".into()),
        ..ConnectionOptions::default()
    };
    let err = bad.to_args().expect_err("negative flag must fail");
    assert_eq!(err.kind, DiagnosticKind::Usage);
    assert_eq!(err.exit_code(), ExitCode::Usage);

    let config = with_profiles(
        "[profiles.timed]\n\
         host = \"db.example.net\"\n\
         statement-timeout = \"30s\"\n\
         lock-timeout = \"5s\"\n\
         idle-in-transaction-session-timeout = \"4m\"\n",
    );
    let (_, profile_args) =
        resolve_target_and_profile(Some("@timed"), &options(None), &config).expect("profile");
    assert_eq!(
        profile_args.statement_timeout,
        Some(Duration::from_secs(30))
    );
    assert_eq!(profile_args.lock_timeout, Some(Duration::from_secs(5)));
    assert_eq!(
        profile_args.idle_in_transaction_session_timeout,
        Some(Duration::from_secs(240))
    );

    let cli_override = ConnectionOptions {
        statement_timeout: Some("0".into()),
        lock_timeout: Some("250ms".into()),
        idle_in_transaction_session_timeout: Some("45s".into()),
        ..options(Some("timed"))
    };
    let (_, override_args) =
        resolve_target_and_profile(None, &cli_override, &config).expect("cli overrides profile");
    assert_eq!(override_args.statement_timeout, Some(Duration::ZERO));
    assert_eq!(override_args.lock_timeout, Some(Duration::from_millis(250)));
    assert_eq!(
        override_args.idle_in_transaction_session_timeout,
        Some(Duration::from_secs(45))
    );

    let bad_profile = with_profiles(
        "[profiles.broken]\n\
         host = \"db.example.net\"\n\
         lock-timeout = \"not-a-duration\"\n",
    );
    let err = resolve_target_and_profile(Some("@broken"), &options(None), &bad_profile)
        .expect_err("invalid profile lock-timeout must fail");
    assert_eq!(err.kind, DiagnosticKind::Config);
}
