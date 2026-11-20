use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde_json::Value;
use warp::integration_testing::terminal::{
    assert_active_block_received_precmd, assert_alt_grid_active, assert_alt_screen_output,
    assert_command_executed_for_single_terminal_in_tab, run_alt_grid_program,
    wait_until_bootstrapped_single_pane_for_tab,
};
use warp::integration_testing::view_getters::terminal_view;
use warpui_core::event::{Event, KeyEventDetails};
use warpui_core::integration::{ARTIFACTS_DIR_ENV_VAR, AssertionOutcome, TestStep};
use warpui_core::keymap::Keystroke;

use crate::Builder;

const TEST_NAME: &str = "test_ignatius_native_renderer_restore";
const ACTIVE: &str = "ignatius-active.png";
const RESTORED: &str = "ignatius-restored.png";
const SOURCE_REVISION: &str = "5e5712a25e75f08b02df7525b283d7a81ac355c9";
const BUILD_IDENTITY: &str = "ci/44721590021-1";
const RESULT_MARKER: &str = "warp_live_ok";

fn canonical_env_path(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect("required fixture path"));
    assert_eq!(path.canonicalize().expect("canonical fixture path"), path);
    path
}

fn empty_directory(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|metadata| metadata.is_dir())
        && path
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_none())
}

fn receipt(path: &Path) -> Option<Value> {
    let metadata = path.symlink_metadata().ok()?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return None;
    }
    let mut body = Vec::new();
    File::open(path)
        .ok()?
        .take(4097)
        .read_to_end(&mut body)
        .ok()?;
    (body.len() <= 4096)
        .then(|| serde_json::from_slice(&body).ok())
        .flatten()
}

fn live_fixture(value: &Value) -> bool {
    value["server_version_num"].as_u64() == Some(180_007)
        && value["client_tls_verified"].as_bool() == Some(true)
        && value["source_revision"].as_str() == Some(SOURCE_REVISION)
        && value["build_identity"].as_str() == Some(BUILD_IDENTITY)
}

fn ready(value: &Value, nonce: &str) -> bool {
    value["schema"].as_u64() == Some(1)
        && live_fixture(value)
        && value["nonce"].as_str() == Some(nonce)
        && value["window_title"].as_str() == Some(format!("W07Warp-{nonce}").as_str())
        && value["guard_active"].as_bool() == Some(true)
        && value["raw_input"].as_bool() == Some(true)
        && value["pty"].as_str().is_some_and(|name| {
            name.strip_prefix("/dev/tty").is_some_and(|suffix| {
                !suffix.is_empty()
                    && suffix.len() <= 16
                    && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
        })
}

fn captured_frame(root: &Path, name: &str) -> Option<Vec<u8>> {
    let mut runs = root.join(TEST_NAME).read_dir().ok()?;
    let run = runs.next()?.ok()?;
    if runs.next().is_some() || !run.file_type().ok()?.is_dir() {
        return None;
    }
    let image = run.path().join(name);
    let metadata = image.symlink_metadata().ok()?;
    if !metadata.is_file() || !(128..=16 * 1024 * 1024).contains(&metadata.len()) {
        return None;
    }
    let image = fs::read(image).ok()?;
    (image.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR")
        && image.get(16..24).is_some_and(|header| {
            u32::from_be_bytes(header[..4].try_into().expect("PNG width")) > 0
                && u32::from_be_bytes(header[4..].try_into().expect("PNG height")) > 0
        }))
    .then_some(image)
}

pub fn test_ignatius_native_renderer_restore() -> Builder {
    assert!(cfg!(target_os = "macos"));
    assert!(matches!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true")
    ));
    assert!(matches!(
        std::env::var("RUNNER_ENVIRONMENT").as_deref(),
        Ok("github-hosted")
    ));
    let root = canonical_env_path("IGNATIUS_WARP_FIXTURE_ROOT");
    let runner = canonical_env_path("RUNNER_TEMP");
    assert!(root != runner && root.starts_with(&runner));
    let wrapper = root.join("fixture.sh");
    let wrapper_text = wrapper.to_str().expect("ASCII fixture command path");
    assert!(
        wrapper_text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-')
        })
    );
    assert!(
        wrapper
            .symlink_metadata()
            .expect("owned fixture wrapper")
            .is_file()
    );
    let state = root.join("state");
    let artifacts = canonical_env_path(ARTIFACTS_DIR_ENV_VAR);
    assert_eq!(artifacts, root.join("capture"));
    assert!(empty_directory(&state) && empty_directory(&artifacts));

    let nonce = std::env::var("IGNATIUS_WARP_FIXTURE_NONCE").expect("fixture nonce");
    assert!(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|byte| { byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) })
    );

    let command = format!("/bin/bash {wrapper_text}");
    let active_path = state.join("active.json");
    let runtime_path = state.join("runtime.json");
    let nonce_for_active = nonce.clone();
    let active_check = active_path.clone();
    let runtime_check = runtime_path.clone();
    let active_artifacts = artifacts.clone();
    let runtime_during_query = runtime_path.clone();

    let verify_ready =
        TestStep::new("Verify the owned archive is ready on the synthetic TLS session")
            .set_timeout(Duration::from_secs(25))
            .add_named_assertion(
                "authenticated raw-mode and verified TLS receipt",
                move |_, _| {
                    if runtime_check.exists() {
                        return AssertionOutcome::immediate_failure(
                            "Ignatius ended before the active capture".into(),
                        );
                    }
                    if receipt(&active_check).is_some_and(|data| ready(&data, &nonce_for_active)) {
                        AssertionOutcome::Success
                    } else {
                        AssertionOutcome::failure(
                            "waiting for the Ignatius raw-mode and TLS receipt".into(),
                        )
                    }
                },
            )
            .add_named_assertion(
                "Ignatius branding in the alternate screen",
                assert_alt_screen_output(Regex::new(r"(?i)\bignatius\b").unwrap(), 0, 0),
            )
            .add_named_assertion(
                "the preloaded fixture query or table is visible",
                assert_alt_screen_output(Regex::new(r"\bwarp_acceptance\b").unwrap(), 0, 0),
            )
            .add_named_assertion(
                "the active alternate screen has no fixture result yet",
                |app, window_id| {
                    terminal_view(app, window_id, 0, 0).read(app, |view, _| {
                        let model = view.model.lock();
                        if !model.is_alt_screen_active() {
                            return AssertionOutcome::failure(
                                "waiting for the active alternate screen".into(),
                            );
                        }
                        if model
                            .alt_screen()
                            .output_to_string()
                            .contains(RESULT_MARKER)
                        {
                            return AssertionOutcome::immediate_failure(
                                "the fixture result was visible before Ctrl+R".into(),
                            );
                        }
                        AssertionOutcome::Success
                    })
                },
            );
    let capture_active =
        TestStep::new("Run the live query with Ctrl+R and capture its native result")
            .set_timeout(Duration::from_secs(12))
            .with_event(Event::KeyDown {
                keystroke: Keystroke::parse("ctrl-r").expect("Ctrl+R"),
                chars: "\u{12}".into(),
                details: KeyEventDetails::default(),
                is_composing: false,
            })
            .add_named_assertion("the owned archive is still running", move |_, _| {
                if runtime_during_query.exists() {
                    AssertionOutcome::immediate_failure(
                        "Ignatius ended before the result capture".into(),
                    )
                } else {
                    AssertionOutcome::Success
                }
            })
            .add_assertion(assert_alt_grid_active(0, 0, true))
            .add_named_assertion(
                "the read-only TLS query returned its exact marker in the alternate screen",
                assert_alt_screen_output(Regex::new(r"\bwarp_live_ok\b").unwrap(), 0, 0),
            )
            .with_take_screenshot(ACTIVE);
    let verify_active = TestStep::new("Require the native active frame before sending Ctrl+Q")
        .add_assertion(move |_, _| {
            if captured_frame(&active_artifacts, ACTIVE).is_some() {
                AssertionOutcome::Success
            } else {
                AssertionOutcome::failure("native active PNG unavailable".into())
            }
        });
    let quit =
        TestStep::new("Send Ctrl+Q in the native Warp terminal").with_event(Event::KeyDown {
            keystroke: Keystroke::parse("ctrl-q").expect("Ctrl+Q"),
            chars: "\u{11}".into(),
            details: KeyEventDetails::default(),
            is_composing: false,
        });

    let mut steps = run_alt_grid_program(
        &command,
        0,
        0,
        quit,
        vec![verify_ready, capture_active, verify_active],
    )
    .into_iter();
    let launch = steps
        .next()
        .expect("launch step for the owned fixture")
        .set_timeout(Duration::from_secs(25));

    Builder::new()
        .with_real_display()
        .with_timeout(Duration::from_secs(100))
        .with_setup(|utils| utils.set_env("WARP_SHELL_PATH", Some("/bin/zsh")))
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(launch)
        .with_steps(steps.collect())
        .with_step(
            TestStep::new("Confirm the original PTY modes and shell recovered")
                .set_timeout(Duration::from_secs(15))
                .add_named_assertion("authenticated restoration receipt", move |_, _| {
                    let Some(done) = receipt(&runtime_path) else {
                        return AssertionOutcome::failure("waiting for restoration".into());
                    };
                    if receipt(&active_path).is_some_and(|active| {
                        ready(&active, &nonce)
                            && done["schema"].as_u64() == Some(1)
                            && live_fixture(&done)
                            && done["nonce"].as_str() == Some(nonce.as_str())
                            && done["pty"] == active["pty"]
                            && done["result"].as_str() == Some("passed")
                            && done["guard_active"].as_bool() == Some(true)
                            && done["modes_restored"].as_bool() == Some(true)
                            && done["quit_completed"].as_bool() == Some(true)
                            && done["fixture_removed"].as_bool() == Some(true)
                            && done["exit_code"].as_i64() == Some(0)
                    }) {
                        AssertionOutcome::Success
                    } else {
                        AssertionOutcome::immediate_failure("Ignatius restoration failed".into())
                    }
                })
                .add_named_assertion(
                    "the owned fixture command completed",
                    assert_command_executed_for_single_terminal_in_tab(0, command),
                )
                .add_named_assertion(
                    "shell prompt returned",
                    assert_active_block_received_precmd(0, 0),
                )
                .add_named_assertion("fixture shell exit code is zero", |app, window_id| {
                    terminal_view(app, window_id, 0, 0).read(app, |view, _| {
                        let model = view.model.lock();
                        if model
                            .block_list()
                            .last_non_hidden_block()
                            .is_some_and(|block| block.exit_code().value() == 0)
                        {
                            AssertionOutcome::Success
                        } else {
                            AssertionOutcome::failure(
                                "fixture shell exit was nonzero or missing".into(),
                            )
                        }
                    })
                })
                .with_take_screenshot(RESTORED),
        )
        .with_step(
            TestStep::new("Verify both native frames differ").add_assertion(move |_, _| {
                if captured_frame(&artifacts, ACTIVE)
                    .zip(captured_frame(&artifacts, RESTORED))
                    .is_some_and(|(active, restored)| active != restored)
                {
                    AssertionOutcome::Success
                } else {
                    AssertionOutcome::failure("missing or identical native capture".into())
                }
            }),
        )
}
