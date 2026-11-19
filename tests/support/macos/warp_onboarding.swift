import AppKit
import CoreGraphics
import CoreML
import Foundation
import ScreenCaptureKit
import Vision

func warpRecognizedLines(_ image: CGImage) throws -> [(String, CGRect)] {
    let request = VNRecognizeTextRequest()
    request.revision = VNRecognizeTextRequestRevision3
    request.recognitionLevel = .accurate
    request.recognitionLanguages = ["en-US"]
    request.usesLanguageCorrection = false
    for (stage, devices) in try request.supportedComputeStageDevices {
        if let cpu = devices.first(where: { device in
            if case .cpu = device { return true }
            return false
        }) {
            request.setComputeDevice(cpu, for: stage)
        }
    }
    try VNImageRequestHandler(cgImage: image).perform([request])
    return (request.results ?? []).prefix(64).compactMap { row in
        guard let candidate = row.topCandidates(1).first, candidate.confidence >= 0.5 else { return nil }
        return (candidate.string.trimmingCharacters(in: .whitespacesAndNewlines), row.boundingBox)
    }
}

func warpWelcomeContrast(_ image: CGImage) -> CGImage? {
    let width = image.width
    let height = image.height
    guard (160...1280).contains(width), (100...900).contains(height),
          let raster = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                                 bytesPerRow: 0, space: CGColorSpaceCreateDeviceGray(),
                                 bitmapInfo: CGImageAlphaInfo.none.rawValue),
          let pixels = raster.data?.assumingMemoryBound(to: UInt8.self) else { return nil }
    raster.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    // The observed welcome heading fades to dark grey. Amplify its contrast
    // without changing geometry or collecting additional screen content.
    let lookup = (0...255).map { UInt8(min(255, max(0, ($0 - 5) * 4))) }
    for row in 0..<height {
        let offset = row * raster.bytesPerRow
        for column in 0..<width { pixels[offset + column] = lookup[Int(pixels[offset + column])] }
    }
    return raster.makeImage()
}

func isKnownWarpWelcome(_ rows: [(String, CGRect)]) -> Bool {
    let centered = rows.filter { _, box in
        (0.34...0.66).contains(box.midX) && (0.28...0.65).contains(box.midY)
    }
    let heading = centered.filter { $0.0 == "Welcome to Warp" }
    let tagline = "A modern terminal with state of the art agents built in"
    let observedOCR = "A modem terminal with state of the art agents built in"
    let detail = centered.filter { [tagline, tagline + ".", observedOCR, observedOCR + "."].contains($0.0) }
    // Vision can read the Return glyph as a dash.
    let action = centered.filter { ["Get started", "Get started ↵", "Get started -"].contains($0.0) }
    guard heading.count == 1, detail.count == 1, action.count == 1,
          let title = heading.first?.1, let body = detail.first?.1, let button = action.first?.1 else { return false }
    return title.minY > body.maxY && body.minY > button.maxY
        && abs(title.midX - body.midX) < 0.07 && abs(body.midX - button.midX) < 0.07
}

@MainActor
func advanceOwnedWarpOnboarding(
    _ app: NSRunningApplication, birth: Date, matches: (NSRunningApplication) -> Bool
) async throws -> String {
    let environment = ProcessInfo.processInfo.environment
    guard environment["GITHUB_ACTIONS"] == "true", environment["RUNNER_ENVIRONMENT"] == "github-hosted",
          environment["RUNNER_OS"] == "macOS", CGPreflightScreenCaptureAccess(),
          CGPreflightPostEventAccess() else { return "access_unavailable" }
    let pid = app.processIdentifier

    func topWindow() -> (CGWindowID, CGRect)? {
        guard let current = NSRunningApplication(processIdentifier: pid), matches(current),
              current.launchDate == birth, current.isFinishedLaunching, current.isActive,
              NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
              let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                                                       kCGNullWindowID) as? [[String: Any]],
              let top = windows.first(where: { row in
                  (row[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid
                      && (row[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
              }), let id = (top[kCGWindowNumber as String] as? NSNumber)?.uint32Value, id != 0,
              let bounds = top[kCGWindowBounds as String] as? [String: NSNumber],
              let x = bounds["X"]?.doubleValue, let y = bounds["Y"]?.doubleValue,
              let width = bounds["Width"]?.doubleValue, let height = bounds["Height"]?.doubleValue else {
            return nil
        }
        return (id, CGRect(x: x, y: y, width: width, height: height))
    }

    func post(_ target: CGRect?, frame: CGRect, id: CGWindowID) -> Bool {
        guard CGPreflightPostEventAccess(), let (current, observed) = topWindow(),
              current == id, observed == frame, let source = CGEventSource(stateID: .privateState) else {
            return false
        }
        let down: CGEvent?
        let up: CGEvent?
        if let target {
            let point = CGPoint(x: frame.minX + target.midX * frame.width,
                                y: frame.minY + (1 - target.midY) * frame.height)
            guard frame.insetBy(dx: 4, dy: 4).contains(point) else { return false }
            down = CGEvent(mouseEventSource: source, mouseType: .leftMouseDown, mouseCursorPosition: point,
                           mouseButton: .left)
            up = CGEvent(mouseEventSource: source, mouseType: .leftMouseUp, mouseCursorPosition: point,
                         mouseButton: .left)
        } else {
            down = CGEvent(keyboardEventSource: source, virtualKey: 36, keyDown: true)
            up = CGEvent(keyboardEventSource: source, virtualKey: 36, keyDown: false)
        }
        guard let down, let up else { return false }
        down.flags = []
        up.flags = []
        down.postToPid(pid)
        up.postToPid(pid)
        return true
    }

    let clock = ContinuousClock()
    let deadline = clock.now + .seconds(40)
    var advanced: [String] = []
    while clock.now < deadline, let (id, frame) = topWindow() {
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        let candidates = content.windows.filter {
            $0.windowID == id && $0.isOnScreen && $0.windowLayer == 0
                && $0.owningApplication?.processID == pid
                && $0.owningApplication?.bundleIdentifier == "dev.warp.Warp-Stable"
        }
        guard candidates.count == 1, let window = candidates.first, topWindow()?.0 == id else {
            return "window_scope_changed"
        }
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let rect = filter.contentRect
        guard abs(rect.width - frame.width) <= 2, abs(rect.height - frame.height) <= 2,
              (160...1600).contains(frame.width), (100...1200).contains(frame.height) else {
            return "window_geometry_unverified"
        }
        let reduction = min(1, 1280 / frame.width, 900 / frame.height)
        let config = SCStreamConfiguration()
        config.width = Int((frame.width * reduction).rounded(.down))
        config.height = Int((frame.height * reduction).rounded(.down))
        config.showsCursor = false
        config.capturesAudio = false
        config.ignoreShadowsSingleWindow = true
        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
        guard image.width == config.width, image.height == config.height else { return "window_geometry_unverified" }
        guard let normalized = warpWelcomeContrast(image) else { return "window_geometry_unverified" }
        let raw = try warpRecognizedLines(image)
        let enhanced = try warpRecognizedLines(normalized)
        let lines = (raw + enhanced).map { text, box in
            (text.lowercased().replacingOccurrences(of: "’", with: "'"), box)
        }
        let transcript = lines.map(\.0).joined(separator: " ")
        func has(_ text: String) -> Bool { transcript.contains(text) }
        func button(_ title: String) -> CGRect? {
            let matches = lines.filter { text, box in
                [title, "\(title) ↵", "\(title) -"].contains(text) && (0.02...0.98).contains(box.midX)
                    && (0.02...0.98).contains(box.midY)
            }
            guard let first = matches.first?.1,
                  matches.allSatisfy({ abs($0.1.midX - first.midX) < 0.035
                      && abs($0.1.midY - first.midY) < 0.035 }) else { return nil }
            return first
        }

        // An observed agreement, account gate or unknown action is never
        // answered by the fixture. These are the release's setup-only screens.
        if has("terms of service") || has("agree to") || has("by continuing") || has("get warping") {
            return "terms_notice"
        }
        if has("create an account") || has("continue without signing in?") {
            return "account_notice"
        }
        var action: (stage: String, box: CGRect?)?
        if isKnownWarpWelcome(raw) || isKnownWarpWelcome(enhanced) {
            action = ("welcome", nil)
        } else if has("how do you want to work?") {
            if !advanced.contains("terminal_choice"), let choice = button("just use the terminal") {
                action = ("terminal_choice", choice)
            } else if advanced.contains("terminal_choice"), let next = button("next") {
                action = ("terminal_next", next)
            }
        } else if has("are you sure you don't want ai?"), let confirm = button("i don't want ai") {
            action = ("terminal_confirmation", confirm)
        } else if has("customize your warp"), has("tab styling"), has("tools panel"), has("code review"),
                  let next = button("next"), next.midX > 0.68, next.midY < 0.22 {
            // The captured control advertises Return. Deliver it to the
            // verified foreground process, as on the welcome screen.
            action = ("customize", nil)
        } else if has("choose a theme"), let next = button("next"), next.midX > 0.68, next.midY < 0.22 {
            // The release's account-first theme screen has this exact Next
            // action and leads to the account notice guarded above.
            action = ("theme_next", nil)
        }
        if let action, !advanced.contains(action.stage) {
            guard clock.now < deadline, post(action.box, frame: frame, id: id) else { return "window_scope_changed" }
            advanced.append(action.stage)
        }
        try await clock.sleep(until: min(deadline, clock.now + .milliseconds(320)))
    }
    return advanced.last.map { "setup_\($0)" } ?? "no_recognized_setup"
}
