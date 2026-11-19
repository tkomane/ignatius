import AppKit
import ApplicationServices
import CoreGraphics
import CryptoKit
import Darwin
import Foundation
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers

private struct DriverFailure: Error {
    let reason: String

    init(_ reason: String) {
        self.reason = reason
    }
}

private struct Inputs {
    let app: URL
    let pid: pid_t
    let nonce: String
    let readiness: URL
    let output: URL

    init(_ args: [String]) throws {
        let names = Set(["--app", "--pid", "--nonce", "--readiness", "--output"])
        var values: [String: String] = [:]
        guard args.count == names.count * 2 else { throw DriverFailure("invalid_arguments") }
        for index in stride(from: 0, to: args.count, by: 2) {
            guard names.contains(args[index]), values.updateValue(args[index + 1], forKey: args[index]) == nil else {
                throw DriverFailure("invalid_arguments")
            }
        }
        guard let pidText = values["--pid"], let pidValue = pid_t(pidText), pidValue > 1,
              let nonceValue = values["--nonce"], nonceValue.utf8.count == 32,
              nonceValue.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }),
              let appPath = values["--app"], let readyPath = values["--readiness"],
              let outputPath = values["--output"] else { throw DriverFailure("invalid_arguments") }
        pid = pidValue
        nonce = nonceValue
        app = try Self.directory(appPath)
        readiness = try Self.directory(readyPath)
        output = try Self.directory(outputPath)
        guard app.pathExtension == "app", app != readiness, app != output, readiness != output,
              !output.path.hasPrefix(app.path + "/"), !output.path.hasPrefix(readiness.path + "/"),
              !readiness.path.hasPrefix(output.path + "/"),
              try FileManager.default.contentsOfDirectory(atPath: output.path).isEmpty else {
            throw DriverFailure("invalid_output_scope")
        }
    }

    func verifyHostedScope() throws -> URL {
        let environment = ProcessInfo.processInfo.environment
        guard environment["GITHUB_ACTIONS"] == "true", environment["RUNNER_ENVIRONMENT"] == "github-hosted",
              environment["RUNNER_OS"] == "macOS", let temp = environment["RUNNER_TEMP"], temp.hasPrefix("/"),
              let run = environment["GITHUB_RUN_ID"], let attempt = environment["GITHUB_RUN_ATTEMPT"],
              Self.positiveDecimal(run), Self.positiveDecimal(attempt) else {
            throw DriverFailure("requires_owned_hosted_macos_job")
        }
        let owned = try Self.directory(temp + "/ignatius-warp-" + run + "-" + attempt)
        let prefix = owned.path + "/"
        guard [app, readiness, output].allSatisfy({ $0.path.hasPrefix(prefix) }) else {
            throw DriverFailure("path_outside_owned_warp_job")
        }
        return owned
    }

    private static func positiveDecimal(_ value: String) -> Bool {
        guard let first = value.utf8.first, (49...57).contains(first), value.utf8.count <= 19 else { return false }
        return value.utf8.allSatisfy { (48...57).contains($0) }
    }

    private static func directory(_ path: String) throws -> URL {
        guard path.hasPrefix("/") else { throw DriverFailure("nonabsolute_directory") }
        let url = URL(fileURLWithPath: path, isDirectory: true).resolvingSymlinksInPath()
        var mode = stat()
        guard lstat(url.path, &mode) == 0, mode.st_mode & S_IFMT == S_IFDIR,
              mode.st_uid == geteuid() else { throw DriverFailure("unowned_directory") }
        return url.standardizedFileURL
    }
}

private struct Active: Decodable, Equatable {
    let schema: Int
    let nonce: String
    let window_title: String
    let pty: String
    let guard_active: Bool
    let raw_input: Bool

    func verify(nonce expected: String) throws {
        let prefix = "/dev/ttys"
        let suffix = pty.dropFirst(prefix.count).utf8
        guard schema == 1, nonce == expected, window_title == "W07Warp-" + expected,
              pty.hasPrefix(prefix), (3...6).contains(suffix.count),
              suffix.allSatisfy({ (48...57).contains($0) }), guard_active, raw_input else {
            throw DriverFailure("invalid_active_receipt")
        }
    }
}

private struct Runtime: Decodable {
    let schema: Int
    let nonce: String
    let pty: String
    let modes_restored: Bool
    let quit_completed: Bool
    let exit_code: Int
    let guard_active: Bool

    func verify(active: Active) throws {
        guard schema == 1, nonce == active.nonce, pty == active.pty, modes_restored,
              quit_completed, exit_code == 0, guard_active else {
            throw DriverFailure("invalid_restoration_receipt")
        }
    }
}

private func readReceipt<T: Decodable>(_ file: URL, as type: T.Type) throws -> T? {
    let fd = Darwin.open(file.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
    if fd == -1, errno == ENOENT { return nil }
    guard fd >= 0 else { throw DriverFailure("receipt_open_failed") }
    defer { _ = Darwin.close(fd) }
    var mode = stat()
    guard fstat(fd, &mode) == 0, mode.st_mode & S_IFMT == S_IFREG, mode.st_uid == geteuid(),
          mode.st_nlink == 1, (1...8192).contains(mode.st_size) else {
        throw DriverFailure("receipt_file_outside_bounds")
    }
    let expectedSize = Int(mode.st_size)
    var bytes = [UInt8](repeating: 0, count: expectedSize + 1)
    var used = 0
    while used < bytes.count {
        let capacity = bytes.count - used
        let count = bytes.withUnsafeMutableBytes {
            Darwin.read(fd, $0.baseAddress?.advanced(by: used), capacity)
        }
        if count == 0 { break }
        guard count > 0 else { throw DriverFailure("receipt_read_failed") }
        used += count
    }
    guard used == expectedSize else { throw DriverFailure("receipt_changed_during_read") }
    do {
        return try JSONDecoder().decode(type, from: Data(bytes.prefix(used)))
    } catch {
        throw DriverFailure("invalid_receipt_json")
    }
}

private struct Capture: Encodable {
    let width: Int
    let height: Int
    let bytes: Int
    let sha256: String
    let completed_uptime_ns: UInt64
}

private struct Receipt: Encodable {
    let schema = 1
    let status = "passed"
    let app_pid: pid_t
    let window_id: CGWindowID
    let nonce: String
    let window_selection: String
    let active: Capture
    let restored: Capture
    let virtual_key: CGKeyCode = 12
    let control_modified = true
    let key_posted_uptime_ns: UInt64
}

private struct StartupReceipt: Encodable {
    let schema = 1
    let status = "startup_diagnostic"
    let reason = "active_receipt_timeout"
    let product_evidence = false
    let input_posted = false
    let app_pid: pid_t
    let nonce: String
    let launch_claim: String
    var window_id: CGWindowID?
    var first_window: Capture?
    var capture_failure: String?
}

private func launchClaimStatus(in owned: URL) -> String {
    let marker = owned.appendingPathComponent("launch.claim")
    var mode = stat()
    if lstat(marker.path, &mode) != 0 {
        return errno == ENOENT ? "absent" : "unavailable"
    }
    return mode.st_mode & S_IFMT == S_IFDIR && mode.st_uid == geteuid() ? "present" : "unexpected"
}

private func requireStartupReceiptsAbsent(in readiness: URL) throws {
    for name in ["active.json", "runtime.json"] {
        var mode = stat()
        if lstat(readiness.appendingPathComponent(name).path, &mode) == 0 {
            throw DriverFailure("fixture_receipt_appeared_after_timeout")
        }
        guard errno == ENOENT else { throw DriverFailure("startup_receipt_status_unavailable") }
    }
}

private func publish(_ data: Data, named name: String, to output: URL) throws {
    let temporary = output.appendingPathComponent("." + name + ".part")
    let destination = output.appendingPathComponent(name)
    let fd = Darwin.open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, S_IRUSR | S_IWUSR)
    guard fd >= 0 else { throw DriverFailure("create_owned_output_failed") }
    defer { _ = Darwin.unlink(temporary.path) }
    let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
    try file.write(contentsOf: data)
    try file.close()
    guard Darwin.link(temporary.path, destination.path) == 0 else {
        throw DriverFailure("publish_owned_output_failed")
    }
}

@MainActor
private struct OwnedWarp {
    let inputs: Inputs
    let identifier: String
    let executable: URL
    let launchDate: Date

    init(_ inputs: Inputs) throws {
        guard let bundle = Bundle(url: inputs.app), bundle.bundleIdentifier == "dev.warp.Warp-Stable",
              let name = bundle.object(forInfoDictionaryKey: "CFBundleExecutable") as? String,
              !name.isEmpty, name != ".", name != "..", !name.contains("/"),
              let launched = NSRunningApplication(processIdentifier: inputs.pid),
              let birth = launched.launchDate else { throw DriverFailure("warp_identity_unavailable") }
        self.inputs = inputs
        identifier = "dev.warp.Warp-Stable"
        executable = inputs.app.appendingPathComponent("Contents/MacOS").appendingPathComponent(name)
            .resolvingSymlinksInPath().standardizedFileURL
        launchDate = birth
        guard executable.path.hasPrefix(inputs.app.path + "/Contents/MacOS/"),
              FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw DriverFailure("warp_bundle_executable_unavailable")
        }
        try verify()
    }

    func verify(requireForeground: Bool = false) throws {
        let applications = NSRunningApplication.runningApplications(withBundleIdentifier: identifier)
        guard applications.count == 1, let app = applications.first, !app.isTerminated,
              app.processIdentifier == inputs.pid, app.launchDate == launchDate,
              app.bundleURL?.resolvingSymlinksInPath().standardizedFileURL.path == inputs.app.path,
              app.executableURL?.resolvingSymlinksInPath().standardizedFileURL.path == executable.path else {
            throw DriverFailure("warp_identity_changed")
        }
        if requireForeground, !app.isActive || NSWorkspace.shared.frontmostApplication?.processIdentifier != inputs.pid {
            throw DriverFailure("owned_warp_is_not_foreground")
        }
    }

    private func focusedWindowMatches(_ title: String) throws {
        let app = AXUIElementCreateApplication(inputs.pid)
        guard AXUIElementSetMessagingTimeout(app, 0.3) == .success else {
            throw DriverFailure("warp_focus_timeout_unavailable")
        }
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &focused) == .success,
              let focused, CFGetTypeID(focused) == AXUIElementGetTypeID() else {
            throw DriverFailure("warp_focused_window_unavailable")
        }
        let window = focused as! AXUIElement
        var actualTitle: CFTypeRef?
        guard AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &actualTitle) == .success,
              actualTitle as? String == title else {
            throw DriverFailure("warp_focused_window_mismatch")
        }
    }

    func regularWindow(title: String, expectedID: CGWindowID? = nil, uniqueOnly: Bool = false) async throws -> (SCWindow, String)? {
        try verify()
        let listing = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        try verify()
        let candidates = listing.windows.filter {
            $0.owningApplication?.processID == inputs.pid
                && $0.owningApplication?.bundleIdentifier == identifier
                && $0.isOnScreen && $0.windowLayer == 0 && $0.frame.width >= 160 && $0.frame.height >= 100
        }
        guard !candidates.isEmpty else { return nil }
        let matching = candidates.filter { $0.title == title }
        let selected: SCWindow
        let method: String
        if candidates.count == 1, let single = candidates.first {
            selected = single
            method = single.title == title ? "exact_fixture_title" : "one_owned_regular_window"
        } else {
            guard !uniqueOnly, matching.count == 1, let exact = matching.first else {
                throw DriverFailure("ambiguous_owned_warp_window")
            }
            try focusedWindowMatches(title)
            selected = exact
            method = "exact_focused_fixture_title"
        }
        guard selected.windowID != 0, expectedID == nil || selected.windowID == expectedID else {
            throw DriverFailure("warp_window_identity_changed")
        }
        return (selected, method)
    }

    func capture(window: SCWindow, name: String, startupTitle: String? = nil) async throws -> Capture {
        guard CGPreflightScreenCaptureAccess() else { throw DriverFailure("warp_capture_access_unavailable") }
        try verify(requireForeground: true)
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let rect = filter.contentRect
        let scale = Double(filter.pointPixelScale)
        let width = Double(rect.width) * scale
        let height = Double(rect.height) * scale
        guard scale.isFinite, width.isFinite, height.isFinite, width >= 160, height >= 100 else {
            throw DriverFailure("invalid_warp_window_geometry")
        }
        let reduction = min(1, 1920 / width, 1200 / height)
        let config = SCStreamConfiguration()
        config.width = Int((width * reduction).rounded(.down))
        config.height = Int((height * reduction).rounded(.down))
        config.showsCursor = false
        config.capturesAudio = false
        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
        if let startupTitle {
            guard try await regularWindow(title: startupTitle, expectedID: window.windowID, uniqueOnly: true) != nil else {
                throw DriverFailure("startup_warp_window_disappeared")
            }
            try requireStartupReceiptsAbsent(in: inputs.readiness)
        }
        try verify(requireForeground: true)
        guard image.width == config.width, image.height == config.height,
              image.width > 0, image.height > 0 else { throw DriverFailure("unexpected_warp_image_bounds") }
        let encoded = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(encoded, UTType.png.identifier as CFString, 1, nil) else {
            throw DriverFailure("create_png_failed")
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination), (1...8_388_608).contains(encoded.length) else {
            throw DriverFailure("warp_png_outside_bounds")
        }
        let data = encoded as Data
        try publish(data, named: name, to: inputs.output)
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return Capture(width: image.width, height: image.height, bytes: data.count,
                       sha256: digest, completed_uptime_ns: DispatchTime.now().uptimeNanoseconds)
    }

    func postQuitChord(focusedTitle: String?) throws -> UInt64 {
        guard CGPreflightPostEventAccess(), AXIsProcessTrusted(),
              let source = CGEventSource(stateID: .privateState),
              let down = CGEvent(keyboardEventSource: source, virtualKey: 12, keyDown: true),
              let up = CGEvent(keyboardEventSource: source, virtualKey: 12, keyDown: false) else {
            throw DriverFailure("warp_event_post_unavailable")
        }
        down.flags = .maskControl
        up.flags = .maskControl
        if let focusedTitle { try focusedWindowMatches(focusedTitle) }
        try verify(requireForeground: true)
        down.postToPid(inputs.pid)
        up.postToPid(inputs.pid)
        return DispatchTime.now().uptimeNanoseconds
    }
}

@available(macOS 14.0, *)
@main
@MainActor
private struct WarpDriver {
    static func main() async {
        // The watchdog also bounds framework calls whose continuations may never return.
        let watchdog = DispatchSource.makeTimerSource(queue: .global(qos: .userInitiated))
        watchdog.schedule(deadline: .now() + .seconds(48))
        watchdog.setEventHandler { Darwin._exit(124) }
        watchdog.resume()
        defer { watchdog.cancel() }
        do {
            try await run(Inputs(Array(CommandLine.arguments.dropFirst())))
        } catch {
            let reason = (error as? DriverFailure)?.reason ?? "framework_or_io_failure"
            try? FileHandle.standardError.write(contentsOf: Data("warp-driver: \(reason)\n".utf8))
            Darwin.exit(1)
        }
    }

    static func run(_ inputs: Inputs) async throws {
        let owned = try inputs.verifyHostedScope()
        guard CGPreflightScreenCaptureAccess(), CGPreflightPostEventAccess(), AXIsProcessTrusted() else {
            throw DriverFailure("required_macos_access_unavailable")
        }
        let app = try OwnedWarp(inputs)
        let activeFile = inputs.readiness.appendingPathComponent("active.json")
        let runtimeFile = inputs.readiness.appendingPathComponent("runtime.json")
        let clock = ContinuousClock()

        let active: Active
        do {
            active = try await poll(until: clock.now + .seconds(24), missing: "active_receipt_timeout") {
                try app.verify()
                if try readReceipt(runtimeFile, as: Runtime.self) != nil {
                    throw DriverFailure("restoration_reported_before_key")
                }
                return try readReceipt(activeFile, as: Active.self)
            }
        } catch let failure as DriverFailure where failure.reason == "active_receipt_timeout" {
            await retainStartupDiagnostic(app: app, owned: owned)
            throw failure
        }
        try active.verify(nonce: inputs.nonce)

        let (window, selection): (SCWindow, String) = try await poll(
            until: clock.now + .seconds(6), missing: "owned_warp_window_timeout"
        ) {
            guard NSWorkspace.shared.frontmostApplication?.processIdentifier == inputs.pid else { return nil }
            return try await app.regularWindow(title: active.window_title)
        }
        let first = try await app.capture(window: window, name: "ignatius-active.png")

        guard try readReceipt(activeFile, as: Active.self) == active,
              try readReceipt(runtimeFile, as: Runtime.self) == nil,
              let (_, keySelection) = try await app.regularWindow(title: active.window_title, expectedID: window.windowID),
              try readReceipt(runtimeFile, as: Runtime.self) == nil else {
            throw DriverFailure("active_window_or_receipt_changed_before_key")
        }
        let focusedTitle = keySelection == "exact_focused_fixture_title" ? active.window_title : nil
        let posted = try app.postQuitChord(focusedTitle: focusedTitle)

        let runtime: Runtime = try await poll(until: clock.now + .seconds(10), missing: "runtime_receipt_timeout") {
            try app.verify(requireForeground: true)
            return try readReceipt(runtimeFile, as: Runtime.self)
        }
        try runtime.verify(active: active)

        guard let (restoredWindow, _) = try await app.regularWindow(title: active.window_title, expectedID: window.windowID) else {
            throw DriverFailure("restored_warp_window_missing")
        }
        let second = try await app.capture(window: restoredWindow, name: "ignatius-restored.png")
        let receipt = Receipt(app_pid: inputs.pid, window_id: window.windowID, nonce: inputs.nonce,
                              window_selection: selection, active: first, restored: second, key_posted_uptime_ns: posted)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(receipt)
        try publish(data + Data("\n".utf8), named: "driver.json", to: inputs.output)
        try FileHandle.standardOutput.write(contentsOf: data + Data("\n".utf8))
    }

    static func retainStartupDiagnostic(app: OwnedWarp, owned: URL) async {
        let inputs = app.inputs
        var receipt = StartupReceipt(app_pid: inputs.pid, nonce: inputs.nonce, launch_claim: launchClaimStatus(in: owned))
        do {
            try app.verify(requireForeground: true)
            let title = "W07Warp-" + inputs.nonce
            guard let (window, _) = try await app.regularWindow(title: title, uniqueOnly: true) else {
                throw DriverFailure("startup_warp_window_missing")
            }
            try requireStartupReceiptsAbsent(in: inputs.readiness)
            receipt.window_id = window.windowID
            receipt.first_window = try await app.capture(window: window, name: "warp-first-window.png", startupTitle: title)
        } catch {
            receipt.capture_failure = (error as? DriverFailure)?.reason ?? "framework_or_io_failure"
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        do {
            let data = try encoder.encode(receipt) + Data("\n".utf8)
            try publish(data, named: "driver-startup.json", to: inputs.output)
            try FileHandle.standardOutput.write(contentsOf: data)
        } catch {
            try? FileHandle.standardError.write(contentsOf: Data("warp-driver: startup_receipt_unavailable\n".utf8))
        }
    }

    static func poll<T>(until deadline: ContinuousClock.Instant, missing: String,
                        _ read: () async throws -> T?) async throws -> T {
        let clock = ContinuousClock()
        while clock.now < deadline {
            if let value = try await read(), clock.now < deadline { return value }
            try await clock.sleep(until: min(deadline, clock.now + .milliseconds(100)))
        }
        throw DriverFailure(missing)
    }
}
