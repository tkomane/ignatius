"""Register one local test against the exact upstream integration runner."""

from pathlib import Path
import subprocess
import sys


REVISION = "0b2db6e0bf3fa3c19f4c1a94899eb67c634a6973"


def extend(text: str, anchor: str, addition: str) -> str:
    if text.count(anchor) != 1:
        raise SystemExit("Pinned integration registration anchor changed.")
    return text.replace(anchor, anchor + addition, 1)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("Require the pinned upstream checkout path.")
    root = Path(sys.argv[1]).resolve(strict=True)
    head = subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip()
    if head != REVISION:
        raise SystemExit("Unexpected integration source revision.")
    integration = root / "crates" / "integration" / "src"
    modules = integration / "test.rs"
    entrypoint = integration / "bin" / "integration.rs"
    module_text = extend(modules.read_text(encoding="utf-8"), "mod video_recording;\n",
                         "mod ignatius_native_renderer;\n")
    module_text = extend(module_text, "pub use video_recording::*;\n", "pub use ignatius_native_renderer::*;\n")
    entrypoint_text = extend(entrypoint.read_text(encoding="utf-8"), "    register_test!(test_video_recording);\n",
                             "    register_test!(test_ignatius_native_renderer_restore);\n")
    test = Path(__file__).resolve().with_name("ignatius_native_renderer.rs")
    with (integration / "test" / test.name).open("xb") as destination:
        destination.write(test.read_bytes())
    modules.write_text(module_text, encoding="utf-8")
    entrypoint.write_text(entrypoint_text, encoding="utf-8")


if __name__ == "__main__":
    main()
