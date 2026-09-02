#!/usr/bin/env python3
"""Exercise one hosted release archive without connecting to a database.

The script is intentionally provider-neutral and non-publishing.  It verifies
the bounded candidate bundle, runs identity and configuration checks from the
extracted target binary, and performs a side-by-side configuration rollback
rehearsal.  Only sanitised, machine-readable outcomes are retained.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any


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


def run(binary: Path, args: list[str], env: dict[str, str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *args],
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
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
        known_good = root / "known-good" / expected_binary
        known_good.parent.mkdir(parents=True)
        shutil.copy2(candidate_binary, known_good)
        config_dir = root / "config"
        data_dir = root / "data"
        env = os.environ.copy()
        env.update(
            {
                "IGNATIUS_CONFIG_DIR": str(config_dir),
                "IGNATIUS_DATA_DIR": str(data_dir),
                "IGNATIUS_LOG": "off",
            }
        )

        version = run(candidate_binary, ["version", "--verbose"], env, root)
        expect_ok(checks, "version --verbose", version)
        version_text = version.stdout
        for label, expected in (
            ("product version", record.get("product_version")),
            ("source revision", record.get("source_revision")),
            ("build identity", record.get("build_identity")),
            ("target", options.target),
        ):
            if not isinstance(expected, str) or expected not in version_text:
                fail(f"version --verbose did not report the expected {label}")

        expect_ok(checks, "doctor --json", run(candidate_binary, ["doctor", "--json"], env, root))
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
        (data_dir / "rehearsal.txt").write_text("retained runtime rehearsal\n", encoding="utf-8")
        before = tree_digest(config_dir), tree_digest(data_dir)
        expect_ok(checks, "side-by-side candidate validation", run(candidate_binary, ["config", "validate"], env, root))
        if (tree_digest(config_dir), tree_digest(data_dir)) != before:
            fail("candidate validation changed retained configuration or data")

        # Keep the backup outside the state tree so the preservation digest
        # describes only user-owned configuration and data.
        backup = root / "config.toml.known-good"
        shutil.copy2(config_dir / "config.toml", backup)
        (config_dir / "config.toml").write_text("schema_version = 999\n", encoding="utf-8")
        expect_exit(
            checks,
            "controlled invalid configuration",
            run(candidate_binary, ["config", "validate"], env, root),
            3,
        )
        shutil.copy2(backup, config_dir / "config.toml")
        expect_ok(checks, "known-good rollback validation", run(known_good, ["config", "validate"], env, root))
        if (tree_digest(config_dir), tree_digest(data_dir)) != before:
            fail("rollback changed retained configuration or data")

    result = {
        "schema_version": 1,
        "status": "pass",
        "evidence_level": "observed",
        "product_version": record.get("product_version"),
        "source_revision": record.get("source_revision"),
        "build_identity": record.get("build_identity"),
        "target": options.target,
        "archive_name": artefact.get("name"),
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
