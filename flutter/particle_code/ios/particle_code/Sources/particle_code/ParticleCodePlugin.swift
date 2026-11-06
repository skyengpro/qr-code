import AVFoundation
import Flutter
import UIKit

/// Flutter entry point of the iOS side. The contract (channel names, event schemas) is
/// `flutter/particle_code/PROTOCOL.md`; keep both in step.
///
/// * `particle_code/scanner` (methods): camera session and decoder, see `ScannerController`.
/// * `particle_code/scan_events` (events): progress, payload and errors.
/// * `particle_code/display` (methods): brightness and idle timer while a code is shown.
/// * `particle_code/preview` (platform view): the camera preview of the scanner session.
@objc(ParticleCodePlugin)
public final class ParticleCodePlugin: NSObject, FlutterPlugin {
    private let scanner = ScannerController()
    private let display = DisplayController()
    private let events = ScanEventStreamHandler()

    public static func register(with registrar: FlutterPluginRegistrar) {
        // Keeps the Rust entry points Dart looks up alive in the link (see SymbolKeeper.swift).
        ParticleCodeSymbols.keep()

        let messenger = registrar.messenger()
        let plugin = ParticleCodePlugin()
        plugin.scanner.eventSink = { [weak events = plugin.events] event in events?.send(event) }

        let scannerChannel = FlutterMethodChannel(
            name: "particle_code/scanner", binaryMessenger: messenger)
        scannerChannel.setMethodCallHandler { [weak plugin] call, result in
            guard let plugin = plugin else {
                result(FlutterMethodNotImplemented)
                return
            }
            plugin.handleScanner(call, result)
        }

        let displayChannel = FlutterMethodChannel(
            name: "particle_code/display", binaryMessenger: messenger)
        displayChannel.setMethodCallHandler { [weak plugin] call, result in
            guard let plugin = plugin else {
                result(FlutterMethodNotImplemented)
                return
            }
            plugin.handleDisplay(call, result)
        }

        let eventChannel = FlutterEventChannel(
            name: "particle_code/scan_events", binaryMessenger: messenger)
        eventChannel.setStreamHandler(plugin.events)

        registrar.register(
            PreviewFactory(controller: plugin.scanner), withId: "particle_code/preview")

        // Retains the plugin for the engine's lifetime and gives us `detachFromEngine`.
        registrar.publish(plugin)
    }

    public func detachFromEngine(for registrar: FlutterPluginRegistrar) {
        scanner.shutdown()
        display.setShowing(false)
    }

    // MARK: particle_code/scanner

    private func handleScanner(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
        let args = call.arguments as? [String: Any]
        switch call.method {
        case "requestPermission":
            scanner.requestPermission(result)
        case "start":
            scanner.start(torch: args?["torch"] as? Bool ?? false, result: result)
        case "stop":
            scanner.stop(result)
        case "reset":
            scanner.reset()
            result(nil)
        case "setTorch":
            scanner.setTorch(on: args?["on"] as? Bool ?? false, result: result)
        default:
            result(FlutterMethodNotImplemented)
        }
    }

    // MARK: particle_code/display

    private func handleDisplay(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
        switch call.method {
        case "setShowingCode":
            let args = call.arguments as? [String: Any]
            display.setShowing(args?["on"] as? Bool ?? false)
            result(nil)
        default:
            result(FlutterMethodNotImplemented)
        }
    }
}

/// `particle_code/scan_events` stream handler. Listening does not start the camera.
final class ScanEventStreamHandler: NSObject, FlutterStreamHandler {
    // Main thread only.
    private var sink: FlutterEventSink?

    func send(_ event: [String: Any]) {
        sink?(event)
    }

    func onListen(
        withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink
    ) -> FlutterError? {
        sink = events
        return nil
    }

    func onCancel(withArguments arguments: Any?) -> FlutterError? {
        sink = nil
        return nil
    }
}
