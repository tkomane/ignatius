//! Short-lived credentials obtained from a cloud's own command-line tool.
//!
//! Azure, AWS and Google Cloud authenticate a PostgreSQL connection the same
//! way, which is the only reason one implementation serves all three honestly:
//! a tool returns a bearer token, the token is presented as the password, and
//! the transport must be encrypted. Only the command differs, so the command is
//! data rather than code and configuration can define its own.
//!
//! Two rules hold everywhere in this module and are worth stating before the
//! types:
//!
//! - **Nothing is fetched for a connection that could be unencrypted.** The
//!   check happens before the program runs, so a misconfigured target costs
//!   nothing and reveals nothing. A bearer token on a plain connection is handed
//!   to anyone on the path, and unlike a password it is immediately usable
//!   elsewhere.
//! - **No shell, ever.** The command is an argument vector and the substitutions
//!   are whole elements, so a database named `; rm -rf /` is inert text.

use crate::connection::target::ConnectionTarget;
use crate::diagnostics::{Diagnostic, DiagnosticKind, redaction};
use secrecy::SecretString;
use std::collections::BTreeMap;
use std::time::Duration;

/// How long a provider's program may take before it is treated as hung.
///
/// Generous, because a first sign-in can be slow and a device-code flow slower,
/// and a timeout that fires during a legitimate login is worse than waiting.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Where the token is in a program's standard output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extract {
    /// The whole of standard output, with surrounding whitespace removed.
    Raw,
    /// A named field of a JSON object printed on standard output.
    JsonField(String),
}

/// A way of obtaining a short-lived credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// The name used on a target and in configuration.
    pub name: String,
    /// The program and its arguments. Element zero is the program.
    pub command: Vec<String>,
    /// Where the token is in what the program prints.
    pub extract: Extract,
    /// How long to wait for it.
    pub timeout: Duration,
    /// What to suggest when the program fails, such as `az login`.
    pub remedy: String,
}

/// Non-secret provider facts suitable for a user-facing trust surface.
///
/// This is deliberately separate from [`Provider`]. A provider contains the
/// executable argument vector that the runtime uses; a presentation contains
/// only the small, safe subset that explains the route to a person. In
/// particular, it never copies an argument that could contain a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPresentation {
    /// The configured provider name.
    pub name: String,
    /// A human-readable provider label.
    pub display_name: String,
    /// The executable that supplies the credential.
    pub command_name: String,
    /// A safe invocation hint for built-in providers, or a generic custom hint.
    pub invocation: String,
    /// Where the provider expects the credential in its output.
    pub extraction: String,
    /// The documented or provider-defined lifetime caveat.
    pub lifetime: String,
    /// The transport requirement for the route.
    pub transport: String,
    /// What a person should do if the provider cannot supply a credential.
    pub remedy: String,
}

impl Provider {
    /// Builds safe, display-only facts for the connection details surface.
    ///
    /// The command text is intentionally allowlisted for built-ins. A custom
    /// provider may put arbitrary text in its arguments, so only its executable
    /// name is shown and the rest stays behind the provider's own remedy.
    #[must_use]
    pub fn presentation(&self) -> ProviderPresentation {
        let name = self.name.clone();
        let lower = self.name.to_ascii_lowercase();
        let (display_name, invocation, lifetime) = match lower.as_str() {
            "entra" => (
                "Microsoft Entra ID".to_owned(),
                "az account get-access-token (OSS RDBMS resource)".to_owned(),
                "short-lived OAuth 2.0 token, typically 5-60 minutes".to_owned(),
            ),
            "aws" => (
                "AWS IAM database authentication".to_owned(),
                "aws rds generate-db-auth-token".to_owned(),
                "short-lived signed token, valid for 15 minutes".to_owned(),
            ),
            "gcp" => (
                "Google Cloud SQL IAM".to_owned(),
                "gcloud sql generate-login-token".to_owned(),
                "short-lived OAuth 2.0 token, valid for about 1 hour".to_owned(),
            ),
            _ => (
                format!("Configured provider {name:?}"),
                "configured provider command (arguments hidden)".to_owned(),
                "short-lived credential, lifetime defined by the provider".to_owned(),
            ),
        };

        let command_name = self
            .command
            .first()
            .cloned()
            .unwrap_or_else(|| "not configured".to_owned());
        let extraction = match &self.extract {
            Extract::Raw => "trimmed standard output".to_owned(),
            Extract::JsonField(field) => format!("JSON field {field:?}"),
        };
        let remedy = if self.remedy.trim().is_empty() {
            "check the provider command and its sign-in state".to_owned()
        } else {
            self.remedy.clone()
        };

        ProviderPresentation {
            name,
            display_name,
            command_name,
            invocation,
            extraction,
            lifetime,
            transport: "TLS required before a bearer token is requested".to_owned(),
            remedy,
        }
    }
}

/// The providers this build knows without being told.
///
/// Every command here is transcribed from the vendor's own documentation,
/// checked on 2026-08-16 and cited in `specs/011-cloud-identity/spec.md`. They
/// are defaults rather than law: a cloud can change its tool between our
/// releases, and configuration replaces any of these without waiting for one.
#[must_use]
pub fn built_in() -> Vec<Provider> {
    vec![
        Provider {
            name: "entra".into(),
            // Azure Database for PostgreSQL Flexible Server. The token is an
            // OAuth 2.0 access token for the OSS RDBMS resource, valid for
            // between 5 and 60 minutes.
            command: vec![
                "az".into(),
                "account".into(),
                "get-access-token".into(),
                "--resource-type".into(),
                "oss-rdbms".into(),
                "--output".into(),
                "json".into(),
            ],
            extract: Extract::JsonField("accessToken".into()),
            timeout: DEFAULT_TIMEOUT,
            remedy: "sign in with `az login`, and check the subscription with `az account show`"
                .into(),
        },
        Provider {
            name: "aws".into(),
            // RDS and Aurora IAM authentication. The token is a signed request
            // valid for 15 minutes. The region comes from the AWS CLI's own
            // configuration and environment, which is why it is not named here.
            command: vec![
                "aws".into(),
                "rds".into(),
                "generate-db-auth-token".into(),
                "--hostname".into(),
                "{host}".into(),
                "--port".into(),
                "{port}".into(),
                "--username".into(),
                "{user}".into(),
            ],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "check credentials with `aws sts get-caller-identity`, and make sure a \
                     region is set - the token cannot be signed without one"
                .into(),
        },
        Provider {
            name: "gcp".into(),
            // Cloud SQL IAM database authentication. An OAuth 2.0 token with
            // the Cloud SQL Admin API scope, valid for one hour.
            command: vec!["gcloud".into(), "sql".into(), "generate-login-token".into()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "sign in with `gcloud auth login`, and check the project with \
                     `gcloud config list`"
                .into(),
        },
    ]
}

/// Every provider this run understands, built-ins first and configuration last.
///
/// Configuration replacing a built-in is deliberate and supported: a cloud may
/// change its tool between our releases, and waiting for a release to reach a
/// database is not an answer anyone should accept.
#[must_use]
pub fn registry(configured: &BTreeMap<String, Provider>) -> BTreeMap<String, Provider> {
    let mut all: BTreeMap<String, Provider> = built_in()
        .into_iter()
        .map(|provider| (provider.name.clone(), provider))
        .collect();
    for (name, provider) in configured {
        all.insert(name.clone(), provider.clone());
    }
    all
}

/// Looks a provider up by name, refusing an unknown one with the list.
pub fn find(
    registry: &BTreeMap<String, Provider>,
    name: &str,
) -> Result<Provider, Box<Diagnostic>> {
    registry.get(name).cloned().ok_or_else(|| {
        let known: Vec<&str> = registry.keys().map(String::as_str).collect();
        Box::new(
            Diagnostic::new(
                DiagnosticKind::Config,
                format!("unknown authentication provider {name:?}"),
                "resolving how to authenticate",
            )
            .likely_cause("the name is not a built-in one and configuration does not define it")
            .next_action(format!(
                "use one of: {}. Any other is defined under [auth.providers] in config.toml",
                known.join(", ")
            )),
        )
    })
}

/// Refuses a target that would let a bearer token travel unencrypted.
///
/// Separate from fetching, and called before it, so that the answer to "was the
/// token ever at risk" is "it was never asked for".
pub fn require_encryption(
    target: &ConnectionTarget,
    provider: &str,
) -> Result<(), Box<Diagnostic>> {
    if target.sslmode.requires_tls() {
        return Ok(());
    }
    Err(Box::new(
        Diagnostic::new(
            DiagnosticKind::Config,
            format!(
                "sslmode={} would allow an unencrypted connection, and {provider} authenticates \
                 with a bearer token",
                target.sslmode.as_str()
            ),
            "checking the transport before asking for a credential",
        )
        .likely_cause(
            "a token presented as a password is usable by anyone who sees it, so unlike a \
             password it may not be sent on a connection that might not be encrypted",
        )
        .next_action(
            "set sslmode=require or stronger. verify-full is what every cloud's own \
             documentation uses, and is the default for a target that is not loopback",
        )
        .technical("Provider", provider.to_owned())
        .technical("Requested sslmode", target.sslmode.as_str().to_owned()),
    ))
}

/// Gives a target the credential its provider supplies, if it names one.
///
/// The single place every path goes through, so that "did this connection get
/// its token" has one answer rather than seven. A target naming no provider is
/// returned untouched, which is what makes this safe to call unconditionally.
///
/// # Errors
///
/// When the provider is unknown, the transport would allow plain text, or the
/// credential could not be obtained. In none of those cases is a connection
/// attempted: there is nothing to attempt it with, and asking a person for a
/// password instead would be a lie about what is wrong.
pub async fn authenticate(
    target: ConnectionTarget,
    configured: &BTreeMap<String, Provider>,
) -> Result<ConnectionTarget, Box<Diagnostic>> {
    let Some(name) = target.auth.clone() else {
        return Ok(target);
    };
    let provider = find(&registry(configured), &name)?;
    let token = fetch(&provider, &target).await?;
    Ok(target.with_password(token))
}

/// Runs a provider's program and returns the credential it printed.
///
/// The token reaches this process on the program's standard output and goes
/// straight into a `SecretString`. It is never an environment variable and never
/// an argument, so it cannot appear in a process listing.
///
/// # Errors
///
/// When the program is absent, fails, times out, or prints nothing usable. No
/// message from here ever carries part of a token.
pub async fn fetch(
    provider: &Provider,
    target: &ConnectionTarget,
) -> Result<SecretString, Box<Diagnostic>> {
    require_encryption(target, &provider.name)?;

    let (program, arguments) = command_for(provider, target)?;

    let mut command = tokio::process::Command::new(&program);
    command
        .args(&arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let started = command.output();
    let output = match tokio::time::timeout(provider.timeout, started).await {
        Err(_) => return Err(timed_out(provider, &program)),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(not_found(provider, &program));
        }
        Ok(Err(error)) => return Err(could_not_run(provider, &program, &error.to_string())),
        Ok(Ok(output)) => output,
    };

    if !output.status.success() {
        // The program's own words are the most useful thing here and the least
        // trustworthy: they are text from elsewhere heading for a terminal, and
        // they may quote a credential back at us.
        let reason = redaction::redact_text(&String::from_utf8_lossy(&output.stderr));
        return Err(refused(provider, &program, reason.trim()));
    }

    let printed = String::from_utf8_lossy(&output.stdout);
    let token = match &provider.extract {
        Extract::Raw => printed.trim().to_owned(),
        Extract::JsonField(field) => extract_field(&printed, field, provider, &program)?,
    };

    if token.is_empty() {
        return Err(Box::new(
            Diagnostic::new(
                DiagnosticKind::Authentication,
                format!("{} produced no credential", provider.name),
                "reading the credential from the provider",
            )
            .likely_cause("the program succeeded and printed nothing where the token should be")
            .next_action(provider.remedy.clone())
            .technical("Program", program),
        ));
    }

    Ok(SecretString::from(token))
}

/// Builds the argument vector, substituting the target's own facts.
///
/// Substitution is by whole element. There is no shell, no word splitting and no
/// quoting to get wrong, so a database or user named to break a command line is
/// simply a long argument.
fn command_for(
    provider: &Provider,
    target: &ConnectionTarget,
) -> Result<(String, Vec<String>), Box<Diagnostic>> {
    let Some((program, rest)) = provider.command.split_first() else {
        return Err(Box::new(
            Diagnostic::new(
                DiagnosticKind::Config,
                format!("the {} provider has no command to run", provider.name),
                "preparing to ask for a credential",
            )
            .next_action("give it a command, starting with the program to run"),
        ));
    };

    let port = target.port.to_string();
    let host = target.host.display();
    let substitute = |value: &String| -> String {
        value
            .replace("{host}", &host)
            .replace("{port}", &port)
            .replace("{user}", &target.user)
            .replace("{database}", &target.database)
    };

    Ok((substitute(program), rest.iter().map(substitute).collect()))
}

fn extract_field(
    printed: &str,
    field: &str,
    provider: &Provider,
    program: &str,
) -> Result<String, Box<Diagnostic>> {
    let parsed: serde_json::Value = serde_json::from_str(printed.trim()).map_err(|error| {
        Box::new(
            Diagnostic::new(
                DiagnosticKind::Authentication,
                format!("{} did not print the JSON it was asked for", provider.name),
                "reading the credential from the provider",
            )
            // The parse error names a position, never content: printing what
            // failed to parse would print the token when the failure is trailing
            // output rather than a missing field.
            .likely_cause(format!("the output could not be parsed as JSON: {error}"))
            .next_action(provider.remedy.clone())
            .technical("Program", program.to_owned()),
        )
    })?;

    parsed
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Box::new(
                Diagnostic::new(
                    DiagnosticKind::Authentication,
                    format!("{} printed no {field:?} field", provider.name),
                    "reading the credential from the provider",
                )
                .likely_cause("the tool's output shape has changed, or it printed an error object")
                .next_action(format!(
                    "check the tool by hand, or set `extract` for this provider in config.toml. \
                     {}",
                    provider.remedy
                ))
                .technical("Program", program.to_owned()),
            )
        })
}

fn not_found(provider: &Provider, program: &str) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(
            DiagnosticKind::Config,
            format!("{program} is not installed, or is not on the path"),
            format!("asking {} for a credential", provider.name),
        )
        // Deliberately not an authentication failure. Nothing has been
        // authenticated or refused; a prerequisite is missing, and saying
        // "authentication failed" would send someone to check their account.
        .likely_cause(format!(
            "the {} provider runs {program}, which this machine does not have",
            provider.name
        ))
        .next_action(format!("install it, then {}", provider.remedy))
        .technical("Provider", provider.name.clone()),
    )
}

fn timed_out(provider: &Provider, program: &str) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(
            DiagnosticKind::Connection,
            format!(
                "{program} did not answer within {} seconds",
                provider.timeout.as_secs()
            ),
            format!("asking {} for a credential", provider.name),
        )
        .likely_cause(
            "the tool may be waiting for an interactive sign-in that nothing here can see",
        )
        .next_action(format!(
            "run it by hand once, then try again. {}",
            provider.remedy
        ))
        .technical("Provider", provider.name.clone()),
    )
}

fn could_not_run(provider: &Provider, program: &str, reason: &str) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(
            DiagnosticKind::Connection,
            format!("{program} could not be run"),
            format!("asking {} for a credential", provider.name),
        )
        .likely_cause(reason.to_owned())
        .technical("Provider", provider.name.clone()),
    )
}

fn refused(provider: &Provider, program: &str, reason: &str) -> Box<Diagnostic> {
    let mut diagnostic = Diagnostic::new(
        DiagnosticKind::Authentication,
        format!("{program} could not provide a credential"),
        format!("asking {} for a credential", provider.name),
    )
    .next_action(provider.remedy.clone())
    .technical("Provider", provider.name.clone());
    if !reason.is_empty() {
        diagnostic = diagnostic.likely_cause(format!("the tool said: {reason}"));
    }
    Box::new(diagnostic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::target::{Host, SslMode};

    fn target(sslmode: SslMode) -> ConnectionTarget {
        ConnectionTarget {
            host: Host::Tcp("db.example.com".into()),
            port: 5432,
            database: "orders".into(),
            user: "someone@example.com".into(),
            password: None,
            sslmode,
            root_cert: None,
            client_cert: None,
            client_key: None,
            application_name: "ignatius".into(),
            connect_timeout: Duration::from_secs(5),
            environment: crate::connection::Environment::Local,
            read_only: false,
            notes: Vec::new(),
            auth: Some("entra".into()),
        }
    }

    #[test]
    fn the_built_in_providers_are_the_three_clouds_and_each_has_a_command() {
        let names: Vec<String> = built_in().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["entra", "aws", "gcp"]);
        for provider in built_in() {
            assert!(
                !provider.command.is_empty(),
                "{} has no command",
                provider.name
            );
            assert!(
                !provider.remedy.is_empty(),
                "{} has no remedy to suggest",
                provider.name
            );
        }
    }

    #[test]
    fn provider_presentation_names_the_cloud_and_never_copies_arguments() {
        let entra = built_in()
            .into_iter()
            .find(|provider| provider.name == "entra")
            .expect("built-in Entra provider");
        let view = entra.presentation();
        assert_eq!(view.display_name, "Microsoft Entra ID");
        assert_eq!(view.command_name, "az");
        assert!(view.invocation.contains("OSS RDBMS"));
        assert!(view.lifetime.contains("5-60 minutes"));
        assert!(view.transport.contains("TLS required"));
        assert!(view.extraction.contains("accessToken"));
        assert!(view.remedy.contains("az login"));
        assert!(
            !view.invocation.contains("{host}"),
            "display metadata must not expose substituted arguments"
        );
    }

    #[test]
    fn custom_provider_presentation_hides_arbitrary_arguments() {
        let provider = Provider {
            name: "corp".into(),
            command: vec!["corp-token".into(), "--secret-value".into()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "run corp login".into(),
        };
        let view = provider.presentation();
        assert!(view.display_name.contains("corp"));
        assert_eq!(view.command_name, "corp-token");
        assert!(view.invocation.contains("arguments hidden"));
        assert!(!view.invocation.contains("secret-value"));
        assert_eq!(view.remedy, "run corp login");
    }

    #[test]
    fn every_built_in_cloud_has_a_named_route_and_lifetime() {
        for (name, display, command, lifetime) in [
            (
                "entra",
                "Microsoft Entra ID",
                "az account get-access-token",
                "5-60 minutes",
            ),
            (
                "aws",
                "AWS IAM database authentication",
                "aws rds generate-db-auth-token",
                "15 minutes",
            ),
            (
                "gcp",
                "Google Cloud SQL IAM",
                "gcloud sql generate-login-token",
                "about 1 hour",
            ),
        ] {
            let provider = built_in()
                .into_iter()
                .find(|provider| provider.name == name)
                .expect("built-in provider");
            let view = provider.presentation();
            assert_eq!(view.display_name, display);
            assert!(view.invocation.contains(command), "{name}: {view:?}");
            assert!(view.lifetime.contains(lifetime), "{name}: {view:?}");
            assert!(view.transport.contains("TLS required"), "{name}: {view:?}");
        }
    }

    #[test]
    fn configuration_replaces_a_built_in_rather_than_being_ignored() {
        let mut configured = BTreeMap::new();
        configured.insert(
            "entra".to_owned(),
            Provider {
                name: "entra".into(),
                command: vec!["mine".into()],
                extract: Extract::Raw,
                timeout: DEFAULT_TIMEOUT,
                remedy: "mine".into(),
            },
        );
        let all = registry(&configured);
        assert_eq!(all["entra"].command, vec!["mine".to_owned()]);
        assert!(all.contains_key("aws"), "the others are still there");
    }

    #[test]
    fn an_unknown_provider_is_refused_with_the_names_that_exist() {
        let error = find(&registry(&BTreeMap::new()), "azure").expect_err("unknown");
        let text = format!("{error:?}");
        assert!(text.contains("entra"), "it lists what exists: {text}");
        assert_eq!(error.kind, DiagnosticKind::Config);
    }

    #[test]
    fn a_transport_that_might_be_unencrypted_is_refused_before_anything_runs() {
        for mode in [SslMode::Disable, SslMode::Prefer] {
            let error =
                require_encryption(&target(mode), "entra").expect_err("must refuse {mode:?}");
            assert_eq!(error.kind, DiagnosticKind::Config);
        }
        for mode in [SslMode::Require, SslMode::VerifyCa, SslMode::VerifyFull] {
            assert!(
                require_encryption(&target(mode), "entra").is_ok(),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn the_targets_own_facts_are_substituted_as_whole_arguments() {
        let aws = built_in().into_iter().find(|p| p.name == "aws").unwrap();
        let (program, arguments) =
            command_for(&aws, &target(SslMode::VerifyFull)).expect("a command");
        assert_eq!(program, "aws");
        assert!(arguments.contains(&"db.example.com".to_owned()));
        assert!(arguments.contains(&"5432".to_owned()));
        assert!(arguments.contains(&"someone@example.com".to_owned()));
        assert!(
            !arguments.iter().any(|a| a.contains('{')),
            "nothing is left unsubstituted: {arguments:?}"
        );
    }

    #[test]
    fn a_hostile_database_name_is_one_argument_and_not_a_command() {
        let provider = Provider {
            name: "test".into(),
            command: vec!["tool".into(), "--db".into(), "{database}".into()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: String::new(),
        };
        let mut hostile = target(SslMode::VerifyFull);
        hostile.database = "x; rm -rf /".into();
        let (_, arguments) = command_for(&provider, &hostile).expect("a command");
        assert_eq!(
            arguments,
            vec!["--db".to_owned(), "x; rm -rf /".to_owned()],
            "no shell means no splitting, so it stays one argument"
        );
    }

    #[tokio::test]
    async fn a_program_that_does_not_exist_is_a_missing_prerequisite_not_a_refusal() {
        let provider = Provider {
            name: "test".into(),
            command: vec!["ignatius-no-such-program-exists".into()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "install it".into(),
        };
        let error = fetch(&provider, &target(SslMode::VerifyFull))
            .await
            .expect_err("no such program");
        assert_eq!(
            error.kind,
            DiagnosticKind::Config,
            "a missing tool is configuration, not a rejected credential"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_raw_token_is_read_from_standard_output() {
        let provider = Provider {
            name: "test".into(),
            command: vec!["/bin/echo".into(), "  a-token  ".into()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: String::new(),
        };
        let secret = fetch(&provider, &target(SslMode::Require))
            .await
            .expect("a token");
        assert_eq!(secrecy::ExposeSecret::expose_secret(&secret), "a-token");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_json_token_is_read_from_its_field() {
        let provider = Provider {
            name: "test".into(),
            command: vec![
                "/bin/echo".into(),
                r#"{"accessToken":"from-json","expiresOn":"later"}"#.into(),
            ],
            extract: Extract::JsonField("accessToken".into()),
            timeout: DEFAULT_TIMEOUT,
            remedy: String::new(),
        };
        let secret = fetch(&provider, &target(SslMode::Require))
            .await
            .expect("a token");
        assert_eq!(secrecy::ExposeSecret::expose_secret(&secret), "from-json");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failing_program_is_an_authentication_failure_carrying_its_own_reason() {
        let provider = Provider {
            name: "test".into(),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "echo 'please run: log in' >&2; exit 1".into(),
            ],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "sign in".into(),
        };
        let error = fetch(&provider, &target(SslMode::Require))
            .await
            .expect_err("it failed");
        assert_eq!(error.kind, DiagnosticKind::Authentication);
        let text = format!("{error:?}");
        assert!(
            text.contains("log in"),
            "the tool's reason survives: {text}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_provider_that_echoes_a_credential_shape_never_leaks_it_into_the_diagnostic() {
        const SENTINEL: &str = "w04-sentinel-4f2a";
        let provider = Provider {
            name: "test".into(),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                format!(
                    "echo 'password={SENTINEL}' >&2; \
                     echo 'Authorization: Bearer {SENTINEL}' >&2; exit 1"
                ),
            ],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: "sign in".into(),
        };
        let error = fetch(&provider, &target(SslMode::Require))
            .await
            .expect_err("it failed");
        assert_eq!(error.kind, DiagnosticKind::Authentication);

        let mut displayable = vec![error.headline.clone(), error.attempted.clone()];
        if let Some(cause) = &error.likely_cause {
            displayable.push(cause.clone());
        }
        if let Some(action) = &error.next_action {
            displayable.push(action.clone());
        }
        for field in &error.technical {
            displayable.push(field.label.clone());
            displayable.push(field.value.clone());
        }
        for text in displayable {
            assert!(
                !text.contains(SENTINEL),
                "provider stderr leaked the sentinel into {text:?}"
            );
        }
        let cause = error.likely_cause.as_deref().unwrap_or_default();
        assert!(
            cause.contains("[redacted]"),
            "the provider's reason should survive redaction rather than be dropped: {cause:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_provider_that_never_answers_times_out_without_reading_its_output() {
        let provider = Provider {
            name: "test".into(),
            command: vec!["/bin/sleep".into(), "3".into()],
            extract: Extract::Raw,
            timeout: Duration::from_secs(1),
            remedy: "sign in".into(),
        };
        let error = fetch(&provider, &target(SslMode::Require))
            .await
            .expect_err("it hung");
        assert_eq!(
            error.kind,
            DiagnosticKind::Connection,
            "a timeout is a connection failure, not a rejected credential"
        );
        assert!(
            error.headline.contains("did not answer within 1 seconds"),
            "the provider's own timeout drives the path: {}",
            error.headline
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn nothing_is_run_at_all_when_the_transport_would_allow_plain_text() {
        // The claim is not that the token is discarded. It is that the program
        // never ran, so there was never a token to discard.
        let witness = std::env::temp_dir().join(format!(
            "ignatius-provider-witness-{}-{}",
            std::process::id(),
            "plain"
        ));
        let _ = std::fs::remove_file(&witness);
        let provider = Provider {
            name: "test".into(),
            command: vec!["/usr/bin/touch".into(), witness.display().to_string()],
            extract: Extract::Raw,
            timeout: DEFAULT_TIMEOUT,
            remedy: String::new(),
        };
        let error = fetch(&provider, &target(SslMode::Prefer))
            .await
            .expect_err("it must refuse");
        assert_eq!(error.kind, DiagnosticKind::Config);
        assert!(
            !witness.exists(),
            "the provider's command ran despite the transport being refused"
        );
    }
}
