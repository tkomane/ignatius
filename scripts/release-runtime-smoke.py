#!/usr/bin/env python3
"""Exercise one hosted release archive without connecting to a database.

The script is intentionally provider-neutral and non-publishing.  It verifies
the bounded candidate bundle, runs identity, diagnostics and loopback refusal
checks from the extracted binary, and rehearses side-by-side state preservation
and rollback. Only sanitised, machine-readable outcomes are retained.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any


PASSWORD_MARKER = "ignatius-fixture-only-credential-marker"
CELL_MARKER = "ignatius_fixture_only_cell_marker"
QUERY_MARKER = "ignatius_fixture_only_query_marker"
PRIVATE_MARKERS = (PASSWORD_MARKER, CELL_MARKER, QUERY_MARKER, "tokio_postgres")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def fail(message: str) -> "NoReturn":
    raise RuntimeError(message)


def ordinary_file(path: Path, label: str) -> None:
    if path.is_symlink():
        fail(f"{label} must not be a symlink")
    if not path.is_file():
        fail(f"{label} is not a regular file")


def load_record(bundle: Path, target: str) -> tuple[dict[str, Any], dict[str, Any], Path]:
    record_path = bundle / "record" / "release-record.json"
    manifest_path = bundle / "evidence" / "release-manifest.json"
    checksums_path = bundle / "evidence" / "SHA256SUMS"
    for path, label in (
        (record_path, "release record"),
        (manifest_path, "release manifest"),
        (checksums_path, "checksum list"),
    ):
        ordinary_file(path, label)

    with record_path.open(encoding="utf-8") as handle:
        document = json.load(handle)
    records = document.get("records")
    if not isinstance(records, list) or len(records) != 1:
        fail("release record must contain exactly one record")
    record = records[0]
    if not isinstance(record, dict):
        fail("release record is not an object")
    artefacts = record.get("artefacts")
    if not isinstance(artefacts, list) or len(artefacts) != 1:
        fail("runtime bundle must contain exactly one artefact")
    artefact = artefacts[0]
    if not isinstance(artefact, dict) or artefact.get("target") != target:
        fail("release artefact target does not match the runtime target")
    name = artefact.get("name")
    if not isinstance(name, str) or not name:
        fail("release artefact has no name")

    with manifest_path.open(encoding="utf-8") as handle:
        manifest = json.load(handle)
    manifest_items = manifest.get("artefacts")
    if not isinstance(manifest_items, list) or len(manifest_items) != 1:
        fail("release manifest must contain exactly one artefact")
    manifest_item = manifest_items[0]
    if not isinstance(manifest_item, dict) or manifest_item.get("name") != name:
        fail("release manifest does not match the release record")

    checksum_lines = checksums_path.read_text(encoding="utf-8").splitlines()
    if len(checksum_lines) != 1:
        fail("checksum list must contain exactly one artefact")
    digest, separator, checksum_name = checksum_lines[0].partition("  ")
    if separator != "  " or checksum_name != name or len(digest) != 64:
        fail("checksum list has a non-canonical entry")

    archive = bundle / "artefacts" / name
    ordinary_file(archive, "release archive")
    observed = hashlib.sha256(archive.read_bytes()).hexdigest()
    if observed != digest:
        fail("release archive checksum does not match SHA256SUMS")
    if observed != manifest_item.get("checksum"):
        fail("release archive checksum does not match release-manifest.json")
    if observed != artefact.get("checksum"):
        fail("release archive checksum does not match release-record.json")
    if archive.stat().st_size != artefact.get("size_bytes"):
        fail("release archive size does not match release-record.json")
    return record, artefact, archive


def safe_member_name(name: str, expected: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and len(path.parts) == 1
        and path.parts[0] == expected
    )


def extract_archive(archive: Path, expected_binary: str, destination: Path) -> Path:
    destination.mkdir(parents=True)
    if archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "r:gz") as source:
            members = source.getmembers()
            files = [member for member in members if member.isfile()]
            if len(files) != 1 or not safe_member_name(files[0].name, expected_binary):
                fail("tar archive does not contain exactly one expected root binary")
            if any(member.name != expected_binary for member in members):
                fail("tar archive contains an unexpected member")
            member = files[0]
            output = destination / expected_binary
            with source.extractfile(member) as input_stream, output.open("wb") as output_stream:
                if input_stream is None:
                    fail("tar archive binary could not be read")
                shutil.copyfileobj(input_stream, output_stream)
    elif archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive) as source:
            members = source.infolist()
            files = [member for member in members if not member.is_dir()]
            if len(files) != 1 or not safe_member_name(files[0].filename, expected_binary):
                fail("zip archive does not contain exactly one expected root binary")
            if any(member.filename != expected_binary for member in members):
                fail("zip archive contains an unexpected member")
            output = destination / expected_binary
            with source.open(files[0]) as input_stream, output.open("wb") as output_stream:
                shutil.copyfileobj(input_stream, output_stream)
    else:
        fail(f"unsupported archive format: {archive.name}")
    ordinary_file(output, "extracted binary")
    if os.name != "nt":
        output.chmod(0o755)
    return output


def run(
    binary: Path,
    args: list[str],
    env: dict[str, str],
    cwd: Path,
    input_text: str | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *args],
        cwd=cwd,
        env=env,
        capture_output=True,
        input=input_text,
        encoding="utf-8",
        timeout=30,
        check=False,
    )


def tree_digest(root: Path) -> str:
    digest = hashlib.sha256()
    if not root.exists():
        return digest.hexdigest()
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix().encode()
        if path.is_symlink():
            fail(f"runtime state contains a symlink: {relative.decode()}")
        digest.update(relative)
        if path.is_file():
            digest.update(b"\\0file\\0")
            digest.update(path.read_bytes())
        elif path.is_dir():
            digest.update(b"\\0dir\\0")
    return digest.hexdigest()


def expect_ok(checks: list[dict[str, Any]], name: str, result: subprocess.CompletedProcess[str]) -> None:
    checks.append({"name": name, "status": "pass" if result.returncode == 0 else "fail", "exit_code": result.returncode})
    if result.returncode != 0:
        fail(f"{name} failed with exit code {result.returncode}")


def expect_exit(checks: list[dict[str, Any]], name: str, result: subprocess.CompletedProcess[str], expected: int) -> None:
    checks.append({"name": name, "status": "pass" if result.returncode == expected else "fail", "exit_code": result.returncode})
    if result.returncode != expected:
        fail(f"{name} returned {result.returncode}, expected {expected}")


def verify_doctor(report: Any, version: str) -> None:
    if not isinstance(report, dict) or report.get("version") != version:
        fail("doctor --json did not identify the expected product version")
    entries = report.get("checks")
    summary = report.get("summary")
    status_counts = {"OK": 0, "WARN": 0, "FAIL": 0, "SKIPPED": 0}
    if not isinstance(entries, list) or not entries:
        fail("doctor --json did not report any checks")
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("name"), str):
            fail("doctor --json returned an invalid check")
        status = entry.get("status")
        if not isinstance(status, str) or status not in status_counts:
            fail("doctor --json returned an invalid check status")
        status_counts[status] += 1
    if status_counts["FAIL"] or summary != {status.lower(): count for status, count in status_counts.items()}:
        fail("doctor --json reported failed checks or an inconsistent summary")


def verify_privacy(
    name: str, result: subprocess.CompletedProcess[str], *files: Path
) -> None:
    markers = tuple(marker.encode("utf-8") for marker in PRIVATE_MARKERS)
    streams = [result.stdout.encode("utf-8"), result.stderr.encode("utf-8")]
    for path in files:
        ordinary_file(path, "isolated runtime state")
        streams.append(path.read_bytes())
    if any(marker in stream for stream in streams for marker in markers):
        fail(f"{name} disclosed a synthetic sentinel or an internal driver target")


def probe_plain_refusal(
    checks: list[dict[str, Any]], binary: Path, env: dict[str, str], root: Path, data_dir: Path
) -> None:
    # A bound socket without a listener keeps the port task-owned and closed.
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as endpoint:
        endpoint.bind(("127.0.0.1", 0))
        port = endpoint.getsockname()[1]
        address = f"postgres://rehearsal@127.0.0.1:{port}/rehearsal?sslmode=disable&connect_timeout=2"
        result = run(
            binary,
            ["--plain", "connect", address],
            dict(env, IGNATIUS_LOG="debug"),
            root,
            input_text=f"SELECT '{CELL_MARKER}' WHERE '{QUERY_MARKER}' = '1';\n",
        )
    expect_exit(checks, "plain loopback refusal and privacy", result, 4)
    if result.stdout:
        fail("plain loopback refusal produced data output")
    logs = data_dir / "logs" / "ignatius.log"
    verify_privacy("plain loopback refusal", result, logs, data_dir / "history.jsonl")


def main() -> int:
    options = parse_args()
    bundle = options.bundle.resolve()
    record, artefact, archive = load_record(bundle, options.target)
    expected_binary = "ignatius.exe" if options.target == "x86_64-pc-windows-msvc" else "ignatius"
    checks: list[dict[str, Any]] = []
    with tempfile.TemporaryDirectory(prefix="ignatius-runtime-") as temporary:
        root = Path(temporary)
        extraction = root / "extracted"
        candidate_binary = extract_archive(archive, expected_binary, extraction)
        binary_sha256 = hashlib.sha256(candidate_binary.read_bytes()).hexdigest()
        known_good = root / "known-good" / expected_binary
        known_good.parent.mkdir(parents=True)
        shutil.copy2(candidate_binary, known_good)
        config_dir = root / "config"
        data_dir = root / "data"
        env = {key: value for key, value in os.environ.items() if not key.upper().startswith("PG")}
        env.update(
            {
                "IGNATIUS_CONFIG_DIR": str(config_dir),
                "IGNATIUS_DATA_DIR": str(data_dir),
                "IGNATIUS_LOG": "off",
                "PGPASSWORD": PASSWORD_MARKER,
                "PGPASSFILE": str(root / "absent-passfile"),
            }
        )

        version = run(candidate_binary, ["version", "--verbose"], env, root)
        expect_ok(checks, "version --verbose", version)
        version_lines = version.stdout.splitlines()
        product_version = record.get("product_version")
        if not isinstance(product_version, str) or not version_lines or not version_lines[0].endswith(f" {product_version}"):
            fail("version --verbose did not report the expected product version")
        identity = {}
        for line in version_lines[1:]:
            label, separator, value = line.partition(":")
            if separator:
                identity[label.strip()] = value.strip()
        for label, expected in (
            ("source revision", record.get("source_revision")),
            ("build identity", record.get("build_identity")),
            ("target", options.target),
        ):
            if not isinstance(expected, str):
                fail(f"candidate record did not specify {label}")
            expected_label = f"{expected} (clean)" if label == "source revision" else expected
            if identity.get(label) != expected_label:
                fail(f"version --verbose did not report the expected {label}")

        doctor = run(candidate_binary, ["doctor", "--json"], env, root)
        expect_ok(checks, "doctor --json", doctor)
        verify_doctor(json.loads(doctor.stdout), product_version)
        verify_privacy("doctor --json", doctor)
        expect_ok(checks, "config paths", run(candidate_binary, ["config", "paths"], env, root))
        expect_ok(checks, "config init", run(candidate_binary, ["config", "init"], env, root))
        expect_ok(checks, "config validate", run(candidate_binary, ["config", "validate"], env, root))
        expect_exit(
            checks,
            "non-interactive connection refusal",
            run(candidate_binary, ["connect", "postgres://x@127.0.0.1:1/y?connect_timeout=2"], env, root),
            2,
        )

        query_dir = config_dir / "queries"
        query_dir.mkdir(parents=True, exist_ok=True)
        (query_dir / "rehearsal.txt").write_text("retained runtime rehearsal\n", encoding="utf-8")
        data_dir.mkdir(parents=True, exist_ok=True)
        history_file = data_dir / "history.jsonl"
        history = json.dumps(
            {
                "at": "2026-11-19T00:00:00+00:00",
                "target": "rehearsal@127.0.0.1:5432/rehearsal",
                "database": "rehearsal",
                "environment": "development",
                "sql": "SELECT 42",
                "outcome": "succeeded",
                "elapsed_ms": 1,
            },
            separators=(",", ":"),
        ).encode("utf-8") + b"\n"
        history_file.write_bytes(history)
        if os.name != "nt":
            history_file.chmod(0o600)
        before = tree_digest(config_dir), tree_digest(data_dir)
        expect_ok(checks, "side-by-side candidate validation", run(candidate_binary, ["config", "validate"], env, root))
        if (tree_digest(config_dir), tree_digest(data_dir)) != before:
            fail("candidate validation changed retained configuration or data")

        # Keep the backup outside the state tree so the preservation digest
        # describes only user-owned configuration and data.
        backup = root / "config.toml.known-good"
        config_file = config_dir / "config.toml"
        shutil.copy2(config_file, backup)
        config_file.write_text("schema_version = [\n", encoding="utf-8")
        malformed_doctor = run(candidate_binary, ["doctor"], env, root)
        expect_exit(checks, "malformed configuration doctor refusal", malformed_doctor, 3)
        verify_privacy("malformed configuration doctor refusal", malformed_doctor)
        config_file.write_text("schema_version = 999\n", encoding="utf-8")
        expect_exit(
            checks,
            "controlled invalid configuration",
            run(candidate_binary, ["config", "validate"], env, root),
            3,
        )
        shutil.copy2(backup, config_file)
        expect_ok(checks, "known-good rollback validation", run(known_good, ["config", "validate"], env, root))
        if (tree_digest(config_dir), tree_digest(data_dir)) != before:
            fail("rollback changed retained configuration or data")

        probe_plain_refusal(checks, candidate_binary, env, root, data_dir)
        if history_file.read_bytes() != history:
            fail("plain loopback refusal changed retained history")

    result = {
        "schema_version": 1,
        "status": "pass",
        "evidence_level": "observed",
        "product_version": record.get("product_version"),
        "source_revision": record.get("source_revision"),
        "build_identity": record.get("build_identity"),
        "target": options.target,
        "archive_name": artefact.get("name"),
        "binary_sha256": binary_sha256,
        "runner": {
            "os": platform.system(),
            "release": platform.release(),
            "architecture": platform.machine(),
        },
        "checks": checks,
        "scope": "hosted runtime smoke and side-by-side rollback; no database or publication action",
    }
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"Runtime smoke passed for {options.target}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        print(f"runtime smoke failed: {error}", file=sys.stderr)
        raise SystemExit(1)
