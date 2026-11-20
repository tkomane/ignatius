#!/usr/bin/env python3
"""Run the pinned archive against a task-owned server in Warp Integration."""

from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import sys
import termios

from prepare_warp_source_fixture import BUILD_IDENTITY, SOURCE_REVISION
from warp_postgres_fixture import FixtureError, QUERY, start_postgres
from warp_terminal_fixture import COOKED_FLAGS, read_background, stop_child, wait_for_raw, write_receipt


def observe(state: Path, binary: Path, nonce: str, controller: int, outcome: dict) -> None:
    if (termios.tcgetattr(controller)[3] & COOKED_FLAGS) != COOKED_FLAGS:
        outcome["result"] = "baseline_not_cooked"
        return

    config = state / "config"
    data = state / "data"
    config.mkdir(mode=0o700)
    data.mkdir(mode=0o700)
    (config / "config.toml").write_text(
        'schema_version = 1\n[ui]\ntheme = "material"\nreduced-motion = true\n', encoding="ascii",
    )
    query_file = state / "verification.sql"
    query_file.write_text(QUERY, encoding="ascii")
    env = {
        key: value for key, value in os.environ.items()
        if not key.upper().startswith(("PG", "IGNATIUS_")) and key.upper() != "RUST_LOG"
    }
    env.update(IGNATIUS_CONFIG_DIR=str(config), IGNATIUS_DATA_DIR=str(data), IGNATIUS_LOG="off")

    with start_postgres() as fixture:
        env.update(fixture.environment)
        title = f"W07Warp-{nonce}"
        os.write(controller, f"\x1b]0;{title}\x07".encode("ascii"))
        outcome["osc11_before"] = read_background(controller)
        # Read again after probing OSC 11 because Darwin can set PENDIN.
        initial = termios.tcgetattr(controller)
        child = subprocess.Popen(
            [binary, "connect", "--no-history", "--read-only", "--file", query_file, fixture.target],
            env=env,
        )
        try:
            outcome["guard_active"] = wait_for_raw(child, controller)
            if not outcome["guard_active"]:
                outcome["result"] = "raw_not_observed"
                return
            fixture.wait_for_client(timeout=8)
            outcome.update(server_version_num=fixture.version_num, client_tls_verified=True)
            write_receipt(state / "active.json", {
                "schema": 1, "nonce": nonce, "pty": outcome["pty"], "window_title": title,
                "guard_active": True, "raw_input": True,
                "source_revision": SOURCE_REVISION, "build_identity": BUILD_IDENTITY,
                "server_version_num": fixture.version_num, "client_tls_verified": True,
            })
            try:
                outcome["exit_code"] = child.wait(timeout=40)
            except subprocess.TimeoutExpired:
                outcome["result"] = "quit_timeout"
                return
            outcome["modes_restored"] = termios.tcgetattr(controller) == initial
            if outcome["exit_code"] != 0 or not outcome["modes_restored"]:
                outcome["result"] = "restoration_failed"
                return
            outcome["osc11_after"] = read_background(controller)
        finally:
            stop_child(child)
            if termios.tcgetattr(controller) != initial:
                termios.tcsetattr(controller, termios.TCSANOW, initial)

    outcome.update(fixture_removed=True, quit_completed=True, result="passed")
    os.write(controller, f"\r\nIgnatius fixture restored {nonce[:8]}\r\n".encode("ascii"))


def main() -> int:
    if len(sys.argv) != 4 or sys.platform != "darwin" or os.uname().machine != "arm64":
        return 2
    state, binary = (Path(value) for value in sys.argv[1:3])
    nonce = sys.argv[3]
    runner_temp = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    if (state.parent.parent != runner_temp or not state.parent.name.startswith("ignatius-warp-integration.")
            or state.name != "state" or not state.is_dir() or state.resolve(strict=True) != state
            or binary != state.parent / "ignatius" or not binary.is_file() or binary.is_symlink()
            or re.fullmatch(r"[a-f0-9]{32}", nonce) is None or any(state.iterdir())):
        return 2
    with (state / "fixture.lock").open("x", encoding="ascii") as lock:
        lock.write(nonce)

    outcome = {
        "schema": 1, "nonce": nonce, "pty": "", "result": "fixture_error",
        "source_revision": SOURCE_REVISION, "build_identity": BUILD_IDENTITY,
        "guard_active": False, "modes_restored": False, "quit_completed": False, "fixture_removed": False,
        "exit_code": None, "osc11_before": None, "osc11_after": None,
    }
    try:
        if os.environ.get("TERM_PROGRAM") != "WarpTerminal" or not sys.stdin.isatty() or not sys.stdout.isatty():
            outcome["result"] = "not_native_warp_terminal"
        else:
            outcome["pty"] = os.ttyname(sys.stdin.fileno())
            controller = os.open("/dev/tty", os.O_RDWR | os.O_NONBLOCK)
            try:
                observe(state, binary, nonce, controller, outcome)
            finally:
                os.close(controller)
    except FixtureError as error:
        outcome.update(result="fixture_error", quit_completed=False, fixture_error=str(error))
    except (OSError, subprocess.SubprocessError, termios.error):
        outcome["result"] = "fixture_error"
        outcome["quit_completed"] = False
    write_receipt(state / "runtime.json", outcome)
    return 0 if outcome["quit_completed"] else 1


if __name__ == "__main__":
    sys.exit(main())
