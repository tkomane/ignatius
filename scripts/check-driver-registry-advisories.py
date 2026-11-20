#!/usr/bin/env python3
"""Audit the crates.io release underlying the single reviewed driver patch."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

DRIVER = "tokio-postgres"
VERSION = "0.7.18"
SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
CHECKSUM = "a528f7d280f6d5b9cd149635c8705b0dd049754bc67d81d31fa25169a93809d3"


def manifest_without_patch(contents: bytes) -> bytes:
    manifest = tomllib.loads(contents.decode("utf-8"))
    if manifest.pop("patch", None) != {"crates-io": {DRIVER: {"path": "vendor/tokio-postgres"}}}:
        raise ValueError("the Cargo patch is not the reviewed local driver")
    if manifest["dependencies"][DRIVER]["version"] != f"={VERSION}":
        raise ValueError("the root manifest has a different driver version")

    lines = contents.splitlines(keepends=True)
    sections = [index for index, line in enumerate(lines) if line.lstrip().startswith(b"[")]
    matches = [index for index in sections if lines[index].strip() == b"[patch.crates-io]"]
    if len(matches) != 1:
        raise ValueError("expected exactly one driver patch table")
    start = matches[0]
    end = next((index for index in sections if index > start), len(lines))
    result = b"".join(lines[:start] + lines[end:])
    if tomllib.loads(result.decode("utf-8")) != manifest:
        raise ValueError("removing the driver patch changed other manifest values")
    return result


def registry_lock(contents: bytes) -> bytes:
    lock = tomllib.loads(contents.decode("utf-8"))
    entries = [entry for entry in lock["package"] if entry["name"] == DRIVER]
    if (
        len(entries) != 1
        or entries[0]["version"] != VERSION
        or "source" in entries[0]
        or "checksum" in entries[0]
    ):
        raise ValueError("the lockfile has a different local driver identity")

    header = f'[[package]]\nname = "{DRIVER}"\nversion = "{VERSION}"\n'.encode()
    if contents.count(header) != 1:
        raise ValueError("could not locate the exact locked driver")
    fields = f'source = "{SOURCE}"\nchecksum = "{CHECKSUM}"\n'.encode()
    result = contents.replace(header, header + fields, 1)
    entries[0].update(source=SOURCE, checksum=CHECKSUM)
    if tomllib.loads(result.decode("utf-8")) != lock:
        raise ValueError("restoring the driver source changed other locked values")
    return result


def audit(repo: Path, cargo_deny: Path, workspace: Path) -> None:
    manifest = workspace / "Cargo.toml"
    manifest.write_bytes(manifest_without_patch((repo / "Cargo.toml").read_bytes()))
    (workspace / "Cargo.lock").write_bytes(registry_lock((repo / "Cargo.lock").read_bytes()))
    for name in (".cargo/config.toml", "rust-toolchain.toml", "xtask/Cargo.toml"):
        destination = workspace / name
        destination.parent.mkdir(exist_ok=True)
        shutil.copyfile(repo / name, destination)
    # Cargo metadata needs the package target files. The audit compiles no code.
    for name in ("build.rs", "src/lib.rs", "src/main.rs", "xtask/src/main.rs"):
        target = workspace / name
        target.parent.mkdir(exist_ok=True)
        target.touch()

    metadata = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--all-features", "--format-version=1"],
        cwd=workspace,
    )
    graph = json.loads(metadata)
    drivers = [package for package in graph["packages"] if package["name"] == DRIVER]
    if (
        len(drivers) != 1
        or drivers[0]["version"] != VERSION
        or drivers[0]["source"] != SOURCE
        or not any(node["id"] == drivers[0]["id"] for node in graph["resolve"]["nodes"])
    ):
        raise ValueError("locked metadata did not select the exact crates.io driver")
    metadata_path = workspace / "metadata.json"
    metadata_path.write_bytes(metadata)

    subprocess.run(
        [
            str(cargo_deny),
            "--locked", "--offline", "--workspace", "--all-features",
            "--manifest-path", str(manifest),
            "--metadata-path", str(metadata_path),
            "--config", str(repo / "deny.toml"),
            "check", "advisories",
        ],
        cwd=workspace,
        check=True,
    )
    print(f"Crates.io advisory and yank check passed for {DRIVER} {VERSION} (SHA-256 {CHECKSUM}).")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cargo_deny", type=Path, help="path to the pinned cargo-deny binary")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="ignatius-registry-audit-") as directory:
        audit(repo, args.cargo_deny.resolve(strict=True), Path(directory))


if __name__ == "__main__":
    main()
