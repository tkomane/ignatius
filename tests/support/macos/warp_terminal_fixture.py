#!/usr/bin/env python3
"""Run one archive-bound Ignatius session in an owned hosted Warp terminal."""

from __future__ import annotations

import json
import os
import re
import select
import socket
import subprocess
import sys
import termios
import time
import tty
from pathlib import Path


OSC11 = re.compile(rb"\x1b\]11;rgb:([\da-fA-F]{1,4}/[\da-fA-F]{1,4}/[\da-fA-F]{1,4})(?:\x07|\x1b\\)")
COOKED_FLAGS = termios.ICANON | termios.ECHO


def write_receipt(path: Path, result: dict) -> None:
    partial = path.with_suffix(".partial")
    with partial.open("x", encoding="utf-8") as output:
        json.dump(result, output, sort_keys=True)
        output.write("\n")
    # The fixture holds the exclusive claim on this private directory.
    # Rename publishes the complete receipt with a single link.
    partial.rename(path)


def read_background(controller: int) -> str | None:
    baseline = termios.tcgetattr(controller)
    try:
        tty.setraw(controller)
        os.write(controller, b"\x1b]11;?\x1b\\")
        received = b""
        deadline = time.monotonic() + 0.6
        while len(received) < 512 and time.monotonic() < deadline:
            if not select.select([controller], [], [], max(0.0, deadline - time.monotonic()))[0]:
                break
            try:
                chunk = os.read(controller, 512 - len(received))
            except BlockingIOError:
                continue
            if not chunk:
                break
            received += chunk
            match = OSC11.search(received)
            if match is not None:
                return match.group(1).decode("ascii").lower()
        return None
    finally:
        termios.tcsetattr(controller, termios.TCSANOW, baseline)


def wait_for_raw(child: subprocess.Popen, controller: int) -> bool:
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline and child.poll() is None:
        if (termios.tcgetattr(controller)[3] & COOKED_FLAGS) == 0:
            return True
        time.sleep(0.025)
    return False


def stop_child(child: subprocess.Popen) -> None:
    if child.poll() is not None:
        return
    child.terminate()
    try:
        child.wait(timeout=3)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait(timeout=3)


def observe(state: Path, binary: Path, nonce: str, controller: int, outcome: dict) -> None:
    if (termios.tcgetattr(controller)[3] & COOKED_FLAGS) != COOKED_FLAGS:
        outcome["result"] = "baseline_not_cooked"
        return
    config = state / "config"
    data = state / "data"
    config.mkdir(mode=0o700)
    data.mkdir(mode=0o700)
    (config / "config.toml").write_text(
        'schema_version = 1\n[ui]\ntheme = "material"\nreduced-motion = true\n', encoding="utf-8"
    )
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("PG")}
    env.update(
        IGNATIUS_CONFIG_DIR=str(config), IGNATIUS_DATA_DIR=str(data),
        IGNATIUS_LOG="off", PGPASSFILE=str(state / "absent-passfile"),
    )
    title = f"W07Warp-{nonce}"
    os.write(controller, f"\x1b]0;{title}\x07".encode("ascii"))
    outcome["osc11_before"] = read_background(controller)
    # Snapshot immediately before the Guard. Darwin can set PENDIN during
    # the probe's own transition from raw input back to canonical input.
    initial = termios.tcgetattr(controller)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as endpoint:
        endpoint.bind(("127.0.0.1", 0))
        target = f"postgres://fixture@127.0.0.1:{endpoint.getsockname()[1]}/warp_fixture?sslmode=disable&connect_timeout=2"
        child = subprocess.Popen([binary, "connect", "--no-history", target], env=env)
        try:
            outcome["guard_active"] = wait_for_raw(child, controller)
            if not outcome["guard_active"]:
                outcome["result"] = "raw_not_observed"
                return
            write_receipt(state / "active.json", {
                "schema": 1, "nonce": nonce, "pty": outcome["pty"], "window_title": title,
                "guard_active": True, "raw_input": True,
            })
            try:
                outcome["exit_code"] = child.wait(timeout=45)
            except subprocess.TimeoutExpired:
                outcome["result"] = "quit_timeout"
                return
            outcome["modes_restored"] = termios.tcgetattr(controller) == initial
            outcome["quit_completed"] = outcome["exit_code"] == 0 and outcome["modes_restored"]
            outcome["result"] = "passed" if outcome["quit_completed"] else "restoration_failed"
            if outcome["quit_completed"]:
                outcome["osc11_after"] = read_background(controller)
                os.write(controller, f"\r\nIgnatius fixture restored {nonce[:8]}\r\n".encode("ascii"))
        finally:
            stop_child(child)
            if termios.tcgetattr(controller) != initial:
                termios.tcsetattr(controller, termios.TCSANOW, initial)


def main() -> int:
    if len(sys.argv) != 4 or sys.platform != "darwin":
        return 2
    state, binary = (Path(value) for value in sys.argv[1:3])
    nonce = sys.argv[3]
    if not state.is_absolute() or not state.is_dir() or state.is_symlink():
        return 2
    if not binary.is_absolute() or not binary.is_file() or binary.is_symlink():
        return 2
    if re.fullmatch(r"[a-f0-9]{32}", nonce) is None or any(state.iterdir()):
        return 2
    try:
        with (state / "fixture.lock").open("x", encoding="ascii") as lock:
            lock.write(nonce)
    except FileExistsError:
        return 2
    outcome = {
        "schema": 1, "nonce": nonce, "pty": "", "result": "fixture_error",
        "guard_active": False, "modes_restored": False, "quit_completed": False,
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
    except (OSError, subprocess.SubprocessError, termios.error):
        outcome["result"] = "fixture_error"
        outcome["quit_completed"] = False
    write_receipt(state / "runtime.json", outcome)
    return 0 if outcome["quit_completed"] else 1


if __name__ == "__main__":
    sys.exit(main())
