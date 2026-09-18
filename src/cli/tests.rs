use super::*;
use clap::Parser;

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
