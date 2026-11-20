"""Stage the exact archived Ignatius binary in a disposable hosted runner."""

import hashlib
import io
import json
import os
from pathlib import Path
import secrets
import shlex
import subprocess
import sys
import tarfile
import tempfile


SOURCE_REVISION = "5e5712a25e75f08b02df7525b283d7a81ac355c9"
BUILD_IDENTITY = "ci/44721590021-1"
TARGET = "aarch64-apple-darwin"
ARCHIVE_NAME = f"ignatius-0.1.0-{TARGET}.tar.gz"
MANIFEST_SHA256 = "a7321380ea073b09abd324319f8ca616d7467e6be01f607a704aeac794245908"
ARCHIVE_SHA256 = "368bfa594f01567c1daa63ab464f83a2f55ef4d311f94d76157124026a641e2f"
BINARY_SHA256 = "71c5169a53f16abc8b41fc905f4ea9b7b0e97bc774db779872222a7b61817ca9"
ARCHIVE_BYTES = 5_875_883
MAX_BINARY_BYTES = 64 * 1024 * 1024


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def read_regular(path: Path, limit: int) -> bytes:
    require(not path.is_symlink() and path.is_file(), "Expected a regular retained file.")
    with path.open("rb") as source:
        data = source.read(limit + 1)
    require(0 < len(data) <= limit, "Retained file exceeded its reviewed size.")
    return data


def extract_candidate(aggregate: Path, output: Path) -> dict:
    manifest = read_regular(aggregate / "evidence/release-manifest.json", 64 * 1024)
    require(hashlib.sha256(manifest).hexdigest() == MANIFEST_SHA256, "Aggregate manifest digest mismatch.")
    metadata = json.loads(manifest)
    require(metadata["source_revision"] == SOURCE_REVISION and metadata["build_identity"] == BUILD_IDENTITY,
            "Expected the reviewed candidate source and build identity.")
    selected = [entry for entry in metadata["artefacts"] if entry["target"] == TARGET]
    require(len(selected) == 1 and selected[0] == {
        "name": ARCHIVE_NAME, "target": TARGET, "format": "tar.gz", "size_bytes": ARCHIVE_BYTES,
        "checksum_algorithm": "sha256", "checksum": ARCHIVE_SHA256,
    }, "Manifest does not uniquely identify the reviewed target archive.")

    archive = read_regular(aggregate / "artefacts" / ARCHIVE_NAME, ARCHIVE_BYTES)
    require(len(archive) == ARCHIVE_BYTES and hashlib.sha256(archive).hexdigest() == ARCHIVE_SHA256,
            "Retained target archive digest mismatch.")
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as package:
        member = package.next()
        require(member is not None and member.name == "ignatius" and member.isreg()
                and member.mode == 0o755 and 0 < member.size < MAX_BINARY_BYTES,
                "Unexpected archive executable.")
        with package.extractfile(member) as handle:
            content = handle.read(member.size + 1)
        require(len(content) == member.size and package.next() is None, "Unexpected archive layout.")
    require(content.startswith(bytes.fromhex("cffaedfe0c000001"))
            and hashlib.sha256(content).hexdigest() == BINARY_SHA256, "Expected the exact arm64 executable.")

    with output.open("xb") as destination:
        destination.write(content)
    output.chmod(0o700)
    identity = subprocess.run([output, "version", "--verbose"], cwd=output.parent, stdin=subprocess.DEVNULL,
                              capture_output=True, text=True, timeout=5)
    require(identity.returncode == 0 and not identity.stderr and {
        "Ignatius 0.1.0", f"  source revision: {SOURCE_REVISION} (clean)",
        f"  build identity:  {BUILD_IDENTITY}", f"  target:          {TARGET}",
    }.issubset(identity.stdout.splitlines()), "The staged executable reported a different identity.")
    return {
        "source_revision": SOURCE_REVISION, "build_identity": BUILD_IDENTITY, "target": TARGET,
        "manifest_sha256": MANIFEST_SHA256, "archive_sha256": ARCHIVE_SHA256, "binary_sha256": BINARY_SHA256,
    }


def main() -> None:
    if len(sys.argv) != 2 or sys.platform != "darwin" or os.uname().machine != "arm64":
        raise SystemExit("Require the disposable arm64 Mac and its archive path.")
    if (os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or os.environ.get("GITHUB_REF") != "refs/heads/codex/warp-integration-check"):
        raise SystemExit("Require the scoped hosted test branch.")
    os.umask(0o077)
    runner_temp = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    archive_root = Path(sys.argv[1]).resolve(strict=True)
    if not archive_root.is_relative_to(runner_temp):
        raise SystemExit("Require a run-owned archive directory.")
    root = Path(tempfile.mkdtemp(prefix="ignatius-warp-integration.", dir=runner_temp))
    state, capture = root / "state", root / "capture"
    state.mkdir()
    capture.mkdir()
    binary = root / "ignatius"
    candidate = extract_candidate(archive_root, binary)
    with (root / "candidate.json").open("x", encoding="ascii") as receipt:
        json.dump(candidate, receipt, sort_keys=True)
        receipt.write("\n")
    nonce = secrets.token_hex(16)
    actor = Path(__file__).resolve().with_name("warp_live_terminal_fixture.py")
    wrapper = root / "fixture.sh"
    # The integration terminal can use its own shell environment. Pass only
    # the reviewed fixture paths across that boundary.
    fixture_env = [f"{key}={os.environ[key]}" for key in (
        "RUNNER_TEMP", "IGNATIUS_WARP_PGBIN", "IGNATIUS_WARP_OPENSSL",
    )]
    command = shlex.join(["/usr/bin/env", *fixture_env, sys.executable, str(actor),
                          str(state), str(binary), nonce])
    wrapper.write_text(f"#!/bin/bash\nset -eu\nexec {command}\n", encoding="ascii")
    wrapper.chmod(0o700)
    with open(os.environ["GITHUB_ENV"], "a", encoding="ascii") as ci_env:
        print(f"IGNATIUS_WARP_FIXTURE_ROOT={root}", file=ci_env)
        print(f"IGNATIUS_WARP_FIXTURE_NONCE={nonce}", file=ci_env)
        print(f"WARP_INTEGRATION_TEST_ARTIFACTS_DIR={capture}", file=ci_env)


if __name__ == "__main__":
    main()
