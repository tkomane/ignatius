"""Stage the exact archived Ignatius binary in a disposable hosted runner."""

import os
from pathlib import Path
import secrets
import shlex
import sys
import tempfile

from warp_bootstrap import extract_ignatius


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
    extract_ignatius(archive_root, binary)
    nonce = secrets.token_hex(16)
    actor = Path(__file__).resolve().with_name("warp_terminal_fixture.py")
    wrapper = root / "fixture.sh"
    command = shlex.join([sys.executable, str(actor), str(state), str(binary), nonce])
    wrapper.write_text(f"#!/bin/bash\nset -eu\nexec {command}\n", encoding="ascii")
    wrapper.chmod(0o700)
    with open(os.environ["GITHUB_ENV"], "a", encoding="ascii") as ci_env:
        print(f"IGNATIUS_WARP_FIXTURE_ROOT={root}", file=ci_env)
        print(f"IGNATIUS_WARP_FIXTURE_NONCE={nonce}", file=ci_env)
        print(f"WARP_INTEGRATION_TEST_ARTIFACTS_DIR={capture}", file=ci_env)


if __name__ == "__main__":
    main()
