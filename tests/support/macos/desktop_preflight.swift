import AppKit
import ApplicationServices
import CoreGraphics
import Foundation
import ScreenCaptureKit

private struct ScreenSession: Encodable {
    let windowServer: Bool
    let onConsole: Bool
    let loggedIn: Bool
    let sameUser: Bool
    let displayWidth: Int
    let displayHeight: Int

    var activeAqua: Bool {
        windowServer && loggedIn && sameUser && displayWidth > 0 && displayHeight > 0
    }

    init() {
        let session = CGSessionCopyCurrentDictionary() as? [String: Any]
        let display = CGMainDisplayID()
        let activeDisplay = display != 0 && CGDisplayIsActive(display) != 0
        windowServer = session != nil
        onConsole = session?[kCGSessionOnConsoleKey] as? Bool == true
        loggedIn = session?[kCGSessionLoginDoneKey] as? Bool == true
        sameUser = (session?[kCGSessionUserIDKey] as? NSNumber)?.uint32Value == getuid()
        displayWidth = activeDisplay ? CGDisplayPixelsWide(display) : 0
        displayHeight = activeDisplay ? CGDisplayPixelsHigh(display) : 0
    }
}

private enum Sample: String, Encodable {
    case noActiveAqua, windowUnavailable, captureRejected, pixelsMismatch, matched
}

private struct Receipt: Encodable {
    let schema = 1
    let macOS: String
    let session: ScreenSession
    let activeAqua: Bool
    let screenCaptureAllowed = CGPreflightScreenCaptureAccess()
    let postEventAllowed = CGPreflightPostEventAccess()
    let accessibilityTrusted = AXIsProcessTrusted()
    let ownWindow: Sample
    let externalWindowCapturePreflight: Bool

    init(session: ScreenSession, ownWindow: Sample) {
        self.session = session
        self.ownWindow = ownWindow
        activeAqua = session.activeAqua
        externalWindowCapturePreflight = session.activeAqua && screenCaptureAllowed && ownWindow == .matched
        let version = ProcessInfo.processInfo.operatingSystemVersion
        macOS = "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)"
    }
}

@MainActor
private final class CalibrationView: NSView {
    override func draw(_ dirtyRect: NSRect) {
        NSColor(srgbRed: 1, green: 0, blue: 0, alpha: 1).setFill()
        bounds.fill()
        NSColor(srgbRed: 0, green: 1, blue: 0, alpha: 1).setFill()
        CGRect(x: bounds.midX, y: bounds.minY, width: bounds.width / 2, height: bounds.height).fill()
    }
}

private func pixel(_ image: CGImage, x: Int, y: Int, expected: [Int]) -> Bool {
    let point = CGRect(x: x, y: y, width: 1, height: 1)
    guard let pixel = image.cropping(to: point),
          let color = CGColorSpace(name: CGColorSpace.sRGB) else { return false }
    var rgba = [UInt8](repeating: 0, count: 4)
    return rgba.withUnsafeMutableBytes { buffer in
        let format = CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
        guard let context = CGContext(
            data: buffer.baseAddress, width: 1, height: 1, bitsPerComponent: 8,
            bytesPerRow: 4, space: color, bitmapInfo: format
        ) else { return false }
        context.draw(pixel, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        return zip(buffer.prefix(4), expected).allSatisfy { abs(Int($0.0) - $0.1) <= 12 }
    }
}

@MainActor
private func sampleOwnedWindow() async -> Sample {
    guard let display = NSScreen.main else { return .windowUnavailable }
    let width = 192
    let height = 128
    let available = display.visibleFrame
    guard available.width >= CGFloat(width), available.height >= CGFloat(height) else {
        return .windowUnavailable
    }
    let rect = CGRect(x: available.midX - CGFloat(width) / 2, y: available.midY - CGFloat(height) / 2,
                      width: CGFloat(width), height: CGFloat(height))
    let window = NSWindow(contentRect: rect, styleMask: .borderless, backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.hasShadow = false
    window.isOpaque = true
    window.contentView = CalibrationView(frame: CGRect(x: 0, y: 0, width: width, height: height))
    window.orderFrontRegardless()
    defer { window.close() }
    guard let windowID = CGWindowID(exactly: window.windowNumber), windowID != 0 else {
        return .windowUnavailable
    }

    do {
        let clock = ContinuousClock()
        let deadline = clock.now + .seconds(2)
        repeat {
            let contents = try await SCShareableContent.currentProcess
            if let own = contents.windows.first(where: {
                $0.windowID == windowID && $0.owningApplication?.processID == getpid() && $0.isOnScreen
            }) {
                let filter = SCContentFilter(desktopIndependentWindow: own)
                let config = SCStreamConfiguration()
                config.width = width
                config.height = height
                config.showsCursor = false
                config.ignoreShadowsSingleWindow = true
                let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
                guard image.width == width, image.height == height,
                      pixel(image, x: width / 4, y: height / 2, expected: [255, 0, 0, 255]),
                      pixel(image, x: 3 * width / 4, y: height / 2, expected: [0, 255, 0, 255]) else {
                    return .pixelsMismatch
                }
                return .matched
            }
            try await Task.sleep(for: .milliseconds(100))
        } while clock.now < deadline
        return .windowUnavailable
    } catch {
        return .captureRejected
    }
}

private func emit(_ receipt: Receipt) -> Never {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    guard var data = try? encoder.encode(receipt) else { exit(1) }
    data.append(10)
    FileHandle.standardOutput.write(data)
    exit(0)
}

@MainActor
private final class ProbeDelegate: NSObject, NSApplicationDelegate {
    let session: ScreenSession

    init(session: ScreenSession) { self.session = session }

    func applicationDidFinishLaunching(_ notification: Notification) {
        Task {
            let sample = await sampleOwnedWindow()
            emit(Receipt(session: session, ownWindow: sample))
        }
    }
}

@main
@MainActor
private enum DesktopPreflight {
    static func main() {
        let session = ScreenSession()
        guard session.activeAqua else { emit(Receipt(session: session, ownWindow: .noActiveAqua)) }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let delegate = ProbeDelegate(session: session)
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
}
