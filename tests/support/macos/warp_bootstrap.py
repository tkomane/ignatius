"""Stage one pinned Warp renderer fixture on the disposable Actions runner."""

import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import secrets
import shlex
import subprocess
import sys
import tarfile


WARP_VERSION = "v0.2026.11.11.08.17.stable_01"
WARP_DMG_SHA = "8767bc168e167bfb43f7a5d3a66dc19eb576edab22fd8c6a909fa4f152fd85cb"
WARP_BUNDLE_ID = "dev.warp.Warp-Stable"
ARCHIVE_RUN = 44627234677
ARCHIVE_SHA = "f431e13cb9918c2f5ab4a4bd03828ae72955f12fbc60be735e863ce66b9a34dc"
IGNATIUS_SHA = "31be7f92813a8002fb5c50e5023a1c8e6503871bfc245a1241a12ebfa0e922e8"
ARCHIVE = "ignatius-0.1.0-aarch64-apple-darwin.tar.gz"


# Explicit Launch Services route to the exact copy, with no AppleScript
# permissions, keyboard events, app substitution or recent-items changes.
LAUNCH_HELPER = r"""
import AppKit
import CoreGraphics
import Foundation

@main
@MainActor
enum OwnedWarp {
    static func main() async {
        let args = CommandLine.arguments
        guard args.count == 5 else { exit(2) }
        let mode = args[1]
        let bundle = URL(fileURLWithPath: args[2]).resolvingSymlinksInPath().path
        let executable = URL(fileURLWithPath: args[3]).resolvingSymlinksInPath().path
        let identifier = "dev.warp.Warp-Stable"
        let live = NSRunningApplication.runningApplications(withBundleIdentifier: identifier)
        func matches(_ app: NSRunningApplication) -> Bool {
            app.processIdentifier > 0 && !app.isTerminated
                && app.bundleIdentifier == identifier
                && app.bundleURL?.resolvingSymlinksInPath().path == bundle
                && app.executableURL?.resolvingSymlinksInPath().path == executable
        }
        if mode == "terminate" {
            let owned = live.filter(matches)
            guard owned.count <= 1 else { exit(3) }
            guard let app = owned.first else { return }
            if !args[4].isEmpty && String(app.processIdentifier) != args[4] { exit(3) }
            guard app.terminate() else { exit(4) }
            for _ in 0..<40 {
                if app.isTerminated { return }
                try? await Task.sleep(for: .milliseconds(100))
            }
            guard matches(app), app.forceTerminate() else { exit(4) }
            for _ in 0..<30 {
                if app.isTerminated { return }
                try? await Task.sleep(for: .milliseconds(100))
            }
            exit(4)
        }
        guard mode == "launch", live.isEmpty, let uri = URL(string: args[4]),
              uri.scheme == "warp", uri.host == "tab_config" else { exit(3) }
        let config = NSWorkspace.OpenConfiguration()
        config.createsNewApplicationInstance = true
        config.allowsRunningApplicationSubstitution = false
        config.addsToRecentItems = false
        config.activates = true
        config.promptsUserIfNeeded = false
        do {
            let target = URL(fileURLWithPath: bundle)
            let app = try await NSWorkspace.shared.openApplication(at: target, configuration: config)
            guard matches(app), let birth = app.launchDate else { exit(3) }
            let claim = target.deletingLastPathComponent().appendingPathComponent("launch.claim")
            var windowReady = false
            for _ in 0..<140 {
                guard matches(app) else { exit(3) }
                let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                                                        kCGNullWindowID) as? [[String: Any]] ?? []
                windowReady = app.isFinishedLaunching && app.isActive && windows.contains { window in
                    (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == app.processIdentifier
                        && (window[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
                }
                if windowReady { break }
                try? await Task.sleep(for: .milliseconds(100))
            }
            guard windowReady else { exit(6) }

            let setup = (try? await advanceOwnedWarpOnboarding(app, birth: birth, matches: matches))
                ?? "onboarding_observer_error"
            if setup == "terms_notice" || setup == "account_notice" {
                print("\(app.processIdentifier) 0 \(setup)")
                return
            }
            guard matches(app), app.launchDate == birth else { exit(3) }
            config.createsNewApplicationInstance = false
            let accepted = try await NSWorkspace.shared.open([uri], withApplicationAt: target, configuration: config)
            guard matches(accepted), accepted.processIdentifier == app.processIdentifier,
                  accepted.launchDate == birth else { exit(3) }
            for _ in 0..<35 {
                if FileManager.default.fileExists(atPath: claim.path) { break }
                try? await Task.sleep(for: .milliseconds(100))
            }
            print("\(app.processIdentifier) 1 \(setup)")
        } catch {
            exit(5)
        }
    }
}
"""


def require(condition, category):
    if not condition:
        raise SystemExit(f"Warp fixture setup stopped: {category}")


def run(category, argv, timeout):
    try:
        completed = subprocess.run(argv, capture_output=True, check=False, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        raise SystemExit(f"Warp fixture tool unavailable or timed out: {category}") from None
    if completed.returncode != 0:
        raise SystemExit(f"Warp fixture tool {category} exited {completed.returncode}")
    return completed.stdout


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def verified_runner():
    require(os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted", "unowned runner")
    event = os.environ.get("GITHUB_EVENT_NAME")
    ref = os.environ.get("GITHUB_REF")
    require(event == "workflow_dispatch" or
            (event == "push" and ref == "refs/heads/codex/macos-desktop-preflight"), "unapproved trigger")
    require(os.environ.get("RUNNER_OS") == "macOS" and os.environ.get("RUNNER_ARCH") == "ARM64"
            and os.uname().machine == "arm64", "native arm64 runner required")
    run_id, attempt = os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"]
    require(run_id.isascii() and run_id.isdecimal() and attempt.isascii() and attempt.isdecimal(),
            "unexpected runner identifiers")
    runner_temp = Path(os.environ["RUNNER_TEMP"]).resolve()
    require(runner_temp.is_dir(), "runner temp unavailable")
    return runner_temp, runner_temp / f"ignatius-warp-{run_id}-{attempt}"


def require_preflight(temp):
    path = temp / "ignatius-macos-desktop-preflight.json"
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 4096,
            "no bounded desktop receipt")
    record = json.loads(path.read_bytes())
    probe = record.get("probe", {})
    require(record.get("revision") == os.environ["GITHUB_SHA"]
            and record.get("runnerArchitecture") == "ARM64"
            and all(probe.get(key) is True for key in (
                "externalWindowCapturePreflight", "postEventAllowed", "accessibilityTrusted"
            )),
            "desktop prerequisite unavailable")


def write_outputs(**outputs):
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as target:
        for key, value in outputs.items():
            text = str(value)
            require("\n" not in text and "\r" not in text, "invalid output")
            target.write(f"{key}={text}\n")


def extract_ignatius(aggregate, output):
    archive = aggregate / "artefacts" / ARCHIVE
    require(archive.is_file() and not archive.is_symlink() and digest(archive) == ARCHIVE_SHA,
            "retained target archive hash mismatch")
    with tarfile.open(archive, "r:gz") as package:
        entries = package.getmembers()
        require(len(entries) == 1 and entries[0].isreg() and entries[0].name == "ignatius",
                "retained archive member scope mismatch")
        with package.extractfile(entries[0]) as source:
            content = source.read()
    require(hashlib.sha256(content).hexdigest() == IGNATIUS_SHA, "retained executable hash mismatch")
    with output.open("xb") as destination:
        destination.write(content)
    output.chmod(0o700)


def stage_warp(root):
    url = f"https://releases.warp.dev/stable/{WARP_VERSION}/Warp.dmg"
    dmg = root / "Warp.dmg"
    run("vendor_dmg_download", ["/usr/bin/curl", "--fail", "--silent", "--show-error", "--location",
                               "--http1.1", "--max-redirs", "3", "--proto", "=https", "--proto-redir", "=https",
                               "--retry", "2", "--retry-all-errors", "--retry-max-time", "60",
                               "--connect-timeout", "10", "--max-time", "80", "--output", dmg, url], 110)
    require(digest(dmg) == WARP_DMG_SHA, "Warp DMG hash mismatch")
    mount = root / "mount"
    mount.mkdir()
    app = root / "Warp.app"
    try:
        run("read_only_dmg_mount", ["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse",
                                    "-noautoopen", "-mountpoint", mount, dmg], 35)
        source = mount / "Warp.app"
        require(source.is_dir() and source.resolve().parent == mount, "vendor app missing from DMG")
        run("copy_signed_bundle", ["/usr/bin/ditto", "--rsrc", "--extattr", source, app], 40)
    finally:
        if os.path.ismount(mount):
            run("detach_owned_dmg", ["/usr/bin/hdiutil", "detach", mount], 20)

    info = plistlib.loads((app / "Contents" / "Info.plist").read_bytes())
    require(info.get("CFBundleIdentifier") == WARP_BUNDLE_ID, "unexpected Warp bundle identifier")
    name = info.get("CFBundleExecutable", "")
    require(re.fullmatch(r"[A-Za-z0-9_.-]+", name) is not None, "unexpected Warp executable name")
    executable = (app / "Contents" / "MacOS" / name).resolve()
    require(executable.is_file() and executable.is_relative_to(app), "Warp executable outside bundle")
    run("verify_apple_signature", ["/usr/bin/codesign", "--verify", "--deep", "--strict", app], 30)
    run("gatekeeper_assessment", ["/usr/sbin/spctl", "--assess", "--type", "execute", app], 35)
    architectures = run("warp_architectures", ["/usr/bin/xcrun", "lipo", "-archs", executable], 15)
    require(b"arm64" in architectures.split(), "Warp bundle has no arm64 slice")
    return app, executable, info.get("CFBundleShortVersionString")


def make_config(root, state, actor, binary, nonce):
    # This is a new disposable runner account, not a user's existing Warp home.
    settings = Path.home() / ".warp"
    require(not settings.exists() and not settings.is_symlink(), "Warp settings root is not fresh")
    settings.mkdir(mode=0o700)
    configs = settings / "tab_configs"
    configs.mkdir(mode=0o700)
    wrapper = root / "fixture.sh"
    command = shlex.join([sys.executable, str(actor), str(state), str(binary), nonce])
    wrapper.write_text(
        "#!/bin/bash\nset -eu\numask 077\n"
        f"mkdir {shlex.quote(str(root / 'launch.claim'))}\nexec {command}\n", encoding="utf-8"
    )
    wrapper.chmod(0o700)
    title = f"W07Warp-{nonce}"
    configuration = (
        f'name = "{title}"\ntitle = "{title}"\n\n[[panes]]\n'
        'id = "main"\ntype = "terminal"\nshell = "bash"\nis_focused = true\n'
        f'directory = {json.dumps(str(root))}\n'
        f'commands = [{json.dumps(shlex.join(["/bin/bash", str(wrapper)]))}]\n'
    )
    (configs / f"{title}.toml").write_text(configuration, encoding="utf-8")
    return f"warp://tab_config/{title}"


def launch(temp, root, binary):
    require_preflight(temp)
    binary = binary.resolve()
    require(binary.is_file() and binary.is_relative_to(temp) and digest(binary) == IGNATIUS_SHA,
            "exact retained arm64 Ignatius required")
    actor = Path(os.environ["GITHUB_WORKSPACE"]) / "tests" / "support" / "macos" / "warp_terminal_fixture.py"
    require(actor.is_file(), "synthetic fixture entry point unavailable")
    root.mkdir(mode=0o700)
    state, capture = root / "state", root / "capture"
    state.mkdir(mode=0o700)
    capture.mkdir(mode=0o700)
    app, executable, bundle_version = stage_warp(root)
    helper, source = root / "owned-warp-launch", root / "owned-warp-launch.swift"
    source.write_text(LAUNCH_HELPER, encoding="utf-8")
    onboarding = actor.with_name("warp_onboarding.swift")
    run("compile_launch_identity_helper",
        ["/usr/bin/xcrun", "swiftc", "-swift-version", "6", "-parse-as-library", "-warnings-as-errors",
         "-target", "arm64-apple-macos15.0", source, onboarding, "-o", helper], 35)
    nonce = secrets.token_hex(16)
    uri = make_config(root, state, actor, binary, nonce)
    # Write the one owned identity before issuing Launch Services requests so
    # the cleanup step can find our copy even if the launch step is interrupted.
    launch_record = root / "launch-identity.json"
    identity = {"app": str(app), "executable": str(executable), "nonce": nonce}
    launch_record.write_text(json.dumps(identity), encoding="utf-8")
    delivered = run("owned_vendor_uri_launch", [helper, "launch", app, executable, uri], 90).decode().split()
    require(len(delivered) == 3 and delivered[1] in {"0", "1"} and
            re.fullmatch(r"[a-z_]{1,40}", delivered[2]), "invalid URI delivery receipt")
    pid, deliveries, setup = delivered
    require(pid.isascii() and pid.isdecimal() and 0 < int(pid) <= 2147483647, "invalid app PID")
    identity["pid"] = int(pid)
    pending_record = root / "launch-identity.pending"
    pending_record.write_text(json.dumps(identity), encoding="utf-8")
    pending_record.replace(launch_record)
    write_outputs(app=app, pid=pid, nonce=nonce, readiness=state, capture=capture)
    receipt = {"schema": 1, "warpVersion": WARP_VERSION, "observedBundleVersion": bundle_version,
               "vendorDmgSha256": WARP_DMG_SHA, "appleSignatureVerified": True, "gatekeeperAccepted": True,
               "warpArm64Slice": True, "ignatiusArchiveRun": ARCHIVE_RUN, "ignatiusArchiveSha256": ARCHIVE_SHA,
               "ignatiusExecutableSha256": IGNATIUS_SHA, "ownedWarpPid": int(pid),
               "launchServicesRequests": int(deliveries), "onboarding": setup,
               "launchClaimObserved": (root / "launch.claim").is_dir()}
    encoded = json.dumps(receipt, sort_keys=True)
    (temp / "ignatius-warp-bootstrap.json").write_text(encoded + "\n", encoding="utf-8")
    print(encoded)


def terminate(root):
    record = root / "launch-identity.json"
    if not record.is_file():
        return
    data = json.loads(record.read_bytes())
    app = (root / "Warp.app").resolve()
    executable = Path(data["executable"]).resolve()
    require(Path(data["app"]).resolve() == app and executable.is_relative_to(app), "cleanup identity mismatch")
    run("terminate_only_owned_warp", [root / "owned-warp-launch", "terminate", app, executable,
                                     str(data.get("pid", ""))], 12)

    # Report known startup markers from this fresh runner account's Warp logs.
    log_markers = {"receivedOwnedUri": False, "configParseError": False, "missingNamedConfiguration": False}
    nonce = str(data.get("nonce", ""))
    if re.fullmatch(r"[a-f0-9]{32}", nonce):
        token = f"w07warp-{nonce}".encode("ascii")
        for name in ("warp.log", "warp.log.1"):
            log = Path.home() / "Library" / "Logs" / name
            if not log.is_file() or log.is_symlink():
                continue
            with log.open("rb") as source:
                source.seek(max(0, log.stat().st_size - 1024 * 1024))
                tail = source.read(1024 * 1024).lower()
            log_markers["receivedOwnedUri"] |= any(b"received url" in line and token in line for line in tail.splitlines())
            log_markers["configParseError"] |= b"failed to parse config file at" in tail
            log_markers["missingNamedConfiguration"] |= b"couldn't find a matching file path for" in tail
    state = root / "state"
    startup = {
        "schema": 1, "launchClaim": (root / "launch.claim").is_dir(),
        "fixtureLock": (state / "fixture.lock").is_file(),
        "activeReceipt": (state / "active.json").is_file(),
        "runtimeReceipt": (state / "runtime.json").is_file(), **log_markers,
    }
    print(json.dumps(startup, sort_keys=True))
    (root / "startup.json").write_text(json.dumps(startup, sort_keys=True) + "\n", encoding="utf-8")


def main():
    os.umask(0o077)
    temp, root = verified_runner()
    args = sys.argv[1:]
    if len(args) == 2 and args[0] == "extract":
        require_preflight(temp)
        aggregate = Path(args[1]).resolve()
        require(aggregate.is_dir() and aggregate.is_relative_to(temp), "unowned archive directory")
        candidate = aggregate / "verified-ignatius"
        extract_ignatius(aggregate, candidate)
        write_outputs(ignatius=candidate)
    elif len(args) == 2 and args[0] == "launch":
        launch(temp, root, Path(args[1]))
    elif args == ["terminate"]:
        terminate(root)
    else:
        raise SystemExit("Usage: warp_bootstrap.py extract <aggregate> | launch <ignatius> | terminate")


if __name__ == "__main__":
    main()
