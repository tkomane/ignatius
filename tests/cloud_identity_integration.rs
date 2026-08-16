//! Cloud identity authentication, proven against a real PostgreSQL server.
//!
//! No cloud account is involved and none ever will be: the point of the design
//! is that Azure, AWS and Google Cloud all reduce to "run a program, read a
//! token, present it as the password over an encrypted connection". So the
//! route is exercised with a provider defined in configuration whose program
//! prints the disposable container's synthetic password. Every line of code an
//! Entra connection would take is taken here - the subprocess, the extraction,
//! the `SecretString`, the transport refusal - except the identity of the
//! program, which is the one part configuration was made to vary.
//!
//! What this cannot prove is that `az`, `aws` and `gcloud` behave as their
//! documentation says. That needs someone with an account, and until it happens
//! `docs/support/compatibility.md` says so in a column of its own.

#![allow(clippy::print_stderr)]

use std::process::Command;

/// The TLS container's URI, split into a target with no password, and the
/// password. TLS rather than the plain server on purpose: this route refuses to
/// run at all without encryption, so the plain server cannot exercise it.
fn tls_target_and_password() -> Option<(String, String)> {
    let uri = std::env::var("IGNATIUS_TEST_PG_TLS_URI")
        .ok()
        .filter(|u| !u.is_empty())?;
    let (scheme, rest) = uri.split_once("://")?;
    let (userinfo, host_and_db) = rest.split_once('@')?;
    let (user, password) = userinfo.split_once(':')?;
    Some((
        format!("{scheme}://{user}@{host_and_db}"),
        password.to_owned(),
    ))
}

/// A directory holding a configuration file and a file the provider will read.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// Writes a provider whose program prints whatever is in a file.
    ///
    /// The credential reaches the provider through a file this test writes at
    /// run time, never through the configuration, an argument or the
    /// environment. A synthetic password in a committed fixture would still be
    /// a credential in a fixture, and the rule does not have an exception for
    /// synthetic ones.
    fn with_token(token: &str) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let token_path = dir.path().join("token");
        std::fs::write(&token_path, token).expect("write the token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))
                .expect("restrict the token file");
        }
        Self::write_config(
            &dir,
            &format!(
                "[auth.providers.container]\ncommand = [\"/bin/cat\", {:?}]\nremedy = \"this is a test provider\"\n",
                token_path.display().to_string()
            ),
        );
        Self { dir }
    }

    /// Writes a provider whose program would create a file, so that "it never
    /// ran" can be asserted rather than assumed.
    fn with_witness(witness: &std::path::Path) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        Self::write_config(
            &dir,
            &format!(
                "[auth.providers.container]\ncommand = [\"/usr/bin/touch\", {:?}]\n",
                witness.display().to_string()
            ),
        );
        Self { dir }
    }

    fn write_config(dir: &tempfile::TempDir, body: &str) {
        std::fs::write(dir.path().join("config.toml"), body).expect("write the configuration");
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ignatius"));
        command.env("IGNATIUS_CONFIG_DIR", self.dir.path());
        command.env("IGNATIUS_DATA_DIR", self.dir.path().join("data"));
        // The credential must come from the provider and from nowhere else, or
        // this would pass without ever running it.
        command.env_remove("PGPASSWORD");
        command.env_remove("PGPASSFILE");
        command
    }
}

#[cfg(unix)]
#[test]
fn a_provider_supplies_the_credential_and_the_session_opens_over_tls() {
    let Some((target, password)) = tls_target_and_password() else {
        eprintln!(
            "skipping: IGNATIUS_TEST_PG_TLS_URI is not set, or carries no password. \
             Start the servers with `cargo xtask db up`."
        );
        return;
    };

    let fixture = Fixture::with_token(&password);
    let output = fixture
        .command()
        .args([
            "query",
            "-c",
            "SELECT current_user AS who",
            "--auth",
            "container",
            // `require` rather than `verify-full`: the container's certificate
            // and its trust are a different feature's evidence, already covered
            // by the TLS tests. What matters here is that encryption is real,
            // which `require` establishes.
            "--sslmode",
            "require",
            "--format",
            "json",
            &target,
        ])
        .output()
        .expect("run the client");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the provider's credential did not open a session.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("who"),
        "the query did not return its row: {stdout}"
    );

    // The credential appears in nothing the run produced. It came from a file
    // the provider read and went into one connection attempt.
    assert!(
        !stdout.contains(&password) && !stderr.contains(&password),
        "the credential reached the output"
    );
}

#[cfg(unix)]
#[test]
fn nothing_is_run_when_the_transport_would_allow_plain_text() {
    // The strongest form of this claim is not "the token was discarded". It is
    // "the program never ran", which is why the provider's command would leave
    // a file behind and the assertion is that it does not exist.
    let witness =
        std::env::temp_dir().join(format!("ignatius-cloud-witness-{}", std::process::id()));
    let _ = std::fs::remove_file(&witness);

    let fixture = Fixture::with_witness(&witness);
    let output = fixture
        .command()
        .args([
            "query",
            "-c",
            "SELECT 1",
            "--auth",
            "container",
            "--sslmode",
            "disable",
            "postgres://someone@127.0.0.1:1/nothing",
        ])
        .output()
        .expect("run the client");

    assert!(
        !witness.exists(),
        "the provider's program ran for a target that permits an unencrypted connection"
    );
    assert_eq!(
        output.status.code(),
        Some(3),
        "a transport that forbids this route is a configuration error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("token") || stderr.contains("unencrypted"),
        "the reason names what the problem is: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn a_provider_whose_program_is_missing_names_the_program() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        dir.path().join("config.toml"),
        "[auth.providers.container]\ncommand = [\"ignatius-no-such-tool\"]\n",
    )
    .expect("write the configuration");

    let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
        .env("IGNATIUS_CONFIG_DIR", dir.path())
        .env("IGNATIUS_DATA_DIR", dir.path().join("data"))
        .args([
            "query",
            "-c",
            "SELECT 1",
            "--auth",
            "container",
            "--sslmode",
            "require",
            "postgres://someone@127.0.0.1:1/nothing",
        ])
        .output()
        .expect("run the client");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(3),
        "a missing tool is a missing prerequisite, not a rejected credential: {stderr}"
    );
    assert!(
        stderr.contains("ignatius-no-such-tool"),
        "the message names the program: {stderr}"
    );
}

#[test]
fn an_unknown_provider_is_refused_with_the_names_that_exist() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let output = Command::new(env!("CARGO_BIN_EXE_ignatius"))
        .env("IGNATIUS_CONFIG_DIR", dir.path())
        .env("IGNATIUS_DATA_DIR", dir.path().join("data"))
        .args([
            "query",
            "-c",
            "SELECT 1",
            "--auth",
            "azure",
            "--sslmode",
            "require",
            "postgres://someone@127.0.0.1:1/nothing",
        ])
        .output()
        .expect("run the client");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert!(
        stderr.contains("entra") && stderr.contains("aws") && stderr.contains("gcp"),
        "a near miss is answered with the names that do exist: {stderr}"
    );
}
