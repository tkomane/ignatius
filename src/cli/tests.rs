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
            read_only: true,
            ..ConnectionOptions::default()
        },
        ConnectionOptions {
            profile: Some("orders-prod".into()),
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
