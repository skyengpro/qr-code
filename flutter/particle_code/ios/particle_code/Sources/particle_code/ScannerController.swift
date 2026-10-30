import AVFoundation
import Flutter
import UIKit

/// Owns the one `AVCaptureSession` and the `ParticleCaptureScanner`, and turns decoder
/// results into the event maps of PROTOCOL.md.
///
/// Threading:
/// * `sessionQueue` (serial): everything that configures or starts/stops the session and the
///   camera device. `startRunning()` blocks, so it never runs on the main thread.
/// * The decoder lives on `ParticleCaptureScanner.queue`; results come back on the main queue.
/// * All other state is main-thread only (method channel calls and results arrive there).
final class ScannerController {
    /// The shared session. It exists from the start (empty, not running) so that preview
    /// views can bind to it whether they are created before or after `start`.
    let session = AVCaptureSession()

    /// Delivers one event map to Dart. Set once by the plugin; called on the main thread.
    var eventSink: (([String: Any]) -> Void)?

    // MARK: Session-queue state

    private let sessionQueue = DispatchQueue(label: "com.particlecode.flutter.session")
    private var camera: AVCaptureDevice?
    private var configured = false
    private var torchOn = false
    private var errorObserver: NSObjectProtocol?

    /// Created in `init` and never replaced, so any thread may read it. `nil` only if the
    /// Rust decoder could not be created (missing library or ABI mismatch).
    private var scanner: ParticleCaptureScanner?

    // MARK: Main-thread state

    private var previews: [WeakPreview] = []
    /// Mirrors the number of `reset()` calls handed to the scanner (see `ScanUpdate.generation`).
    private var generation = 0
    private var progress = Counts()
    private var lastKey: ProgressKey?
    private var lastFrameTime: CFTimeInterval = 0
    private var doneSent = false

    /// Frame events are throttled to this interval unless the progress changed.
    private static let frameInterval: CFTimeInterval = 0.1

    private final class WeakPreview {
        weak var view: PreviewView?
        init(_ view: PreviewView) { self.view = view }
    }

    private struct Counts {
        var have = 0
        var need = 0
        var segmentsDone = 0
        var segmentsTotal = 0
    }

    private struct ProgressKey: Equatable {
        var status: String
        var have: Int
        var need: Int
        var segmentsDone: Int
        var segmentsTotal: Int
    }

    private enum StartError: Error {
        case noCamera
        case camera(String)
    }

    init() {
        scanner = ParticleCaptureScanner(
            resultQueue: .main, onUpdate: { [weak self] update in self?.handle(update) })
    }

    // MARK: - Permission

    func requestPermission(_ result: @escaping FlutterResult) {
        AVCaptureDevice.requestAccess(for: .video) { granted in
            DispatchQueue.main.async { result(granted) }
        }
    }

    // MARK: - start / stop

    func start(torch: Bool, result: @escaping FlutterResult) {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            startSession(torch: torch, result: result)
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                DispatchQueue.main.async {
                    if granted, let self = self {
                        self.startSession(torch: torch, result: result)
                    } else {
                        result(Self.permissionDenied())
                    }
                }
            }
        default:  // .denied, .restricted
            result(Self.permissionDenied())
        }
    }

    private func startSession(torch: Bool, result: @escaping FlutterResult) {
        sessionQueue.async { [weak self] in
            guard let self = self else {
                DispatchQueue.main.async { result(nil) }
                return
            }
            do {
                try self.configureIfNeeded()
            } catch StartError.noCamera {
                DispatchQueue.main.async {
                    result(FlutterError(code: "no_camera", message: "No camera available", details: nil))
                }
                return
            } catch StartError.camera(let message) {
                DispatchQueue.main.async {
                    result(FlutterError(code: "camera_error", message: message, details: nil))
                }
                return
            } catch {
                let message = error.localizedDescription
                DispatchQueue.main.async {
                    result(FlutterError(code: "camera_error", message: message, details: nil))
                }
                return
            }

            self.torchOn = torch
            if !self.session.isRunning {
                self.session.startRunning()
            }
            self.applyTorch()
            let running = self.session.isRunning
            DispatchQueue.main.async {
                if running {
                    result(nil)
                } else {
                    result(
                        FlutterError(
                            code: "camera_error", message: "The capture session did not start",
                            details: nil))
                }
            }
        }
    }

    /// Session queue. Builds input, output and scanner once; later starts reuse them.
    private func configureIfNeeded() throws {
        if configured { return }
        guard let device = ParticleCaptureScanner.defaultCamera() else {
            throw StartError.noCamera
        }
        guard let scanner = scanner else {
            throw StartError.camera("The particle decoder could not be created")
        }

        let output = ParticleCaptureScanner.makeOutput()
        do {
            // 1080p, near autofocus, 30 fps. The data output connection is left in the
            // sensor's native orientation on purpose: anchors are mapped to the preview
            // through the capture device's coordinate space (see `normalizedAnchors`).
            try ParticleCaptureScanner.configure(session: session, device: device, output: output)
        } catch {
            throw StartError.camera("Camera setup failed: \(error.localizedDescription)")
        }
        scanner.attach(to: output)  // the output holds its delegate weakly; `self.scanner` retains it

        self.camera = device
        configured = true

        errorObserver = NotificationCenter.default.addObserver(
            forName: .AVCaptureSessionRuntimeError, object: session, queue: .main
        ) { [weak self] note in
            let error = note.userInfo?[AVCaptureSessionErrorKey] as? NSError
            self?.eventSink?([
                "type": "error",
                "code": "camera_error",
                "message": error?.localizedDescription ?? "Camera runtime error",
            ])
        }
    }

    func stop(_ result: @escaping FlutterResult) {
        sessionQueue.async { [weak self] in
            self?.stopOnSessionQueue()
            DispatchQueue.main.async { result(nil) }
        }
    }

    private func stopOnSessionQueue() {
        if let camera = camera, camera.hasTorch, camera.torchMode != .off,
            (try? camera.lockForConfiguration()) != nil
        {
            camera.torchMode = .off
            camera.unlockForConfiguration()
        }
        if session.isRunning {
            session.stopRunning()
        }
    }

    /// Engine detach: stop the camera and drop the observer. No reply needed.
    func shutdown() {
        sessionQueue.async { [weak self] in
            guard let self = self else { return }
            self.stopOnSessionQueue()
            if let observer = self.errorObserver {
                NotificationCenter.default.removeObserver(observer)
                self.errorObserver = nil
            }
        }
    }

    // MARK: - reset / torch

    func reset() {
        generation += 1  // pairs with the increment in `ParticleCaptureScanner.reset()`
        progress = Counts()
        lastKey = nil
        doneSent = false
        scanner?.reset()  // hops to the scanner's own queue
    }

    func setTorch(on: Bool, result: @escaping FlutterResult) {
        sessionQueue.async { [weak self] in
            guard let self = self else {
                DispatchQueue.main.async { result(false) }
                return
            }
            self.torchOn = on
            let device = self.camera ?? ParticleCaptureScanner.defaultCamera()
            let hasTorch = device?.hasTorch ?? false
            self.applyTorch(to: device)
            DispatchQueue.main.async { result(hasTorch) }
        }
    }

    /// Session queue. Applies `torchOn` to the active camera.
    private func applyTorch(to device: AVCaptureDevice? = nil) {
        guard let device = device ?? camera, device.hasTorch else { return }
        let mode: AVCaptureDevice.TorchMode = torchOn ? .on : .off
        guard device.torchMode != mode, device.isTorchModeSupported(mode) else { return }
        do {
            try device.lockForConfiguration()
            device.torchMode = mode
            device.unlockForConfiguration()
        } catch {
            // The torch is a convenience; a busy device is not worth failing a scan for.
        }
    }

    private static func permissionDenied() -> FlutterError {
        FlutterError(
            code: "permission_denied", message: "Camera permission was denied", details: nil)
    }

    // MARK: - Previews

    /// Main thread. Called by `PreviewView` so anchors can be mapped to what it displays.
    func register(preview: PreviewView) {
        previews.removeAll { $0.view == nil }
        previews.append(WeakPreview(preview))
    }

    /// The preview that is on screen now (the most recently created one wins).
    private func activePreview() -> PreviewView? {
        for entry in previews.reversed() {
            if let view = entry.view, view.window != nil, view.bounds.width > 0 {
                return view
            }
        }
        return nil
    }

    // MARK: - Results (main thread)

    private func handle(_ update: ScanUpdate) {
        // In flight from before a reset: stale.
        guard update.generation == generation else { return }
        let result = update.result

        if let bytes = update.newBytes, !bytes.isEmpty {
            eventSink?(["type": "segment", "bytes": FlutterStandardTypedData(bytes: bytes)])
        }

        let status: String
        var rejectReason: Any = NSNull()
        switch result.progress {
        case .rejected(let reason):
            status = "rejected"
            rejectReason = reason
        case .accepted(let have, let need, let segmentsDone, let segmentsTotal):
            status = "accepted"
            progress = Counts(
                have: have, need: need, segmentsDone: segmentsDone, segmentsTotal: segmentsTotal)
        case .done:
            status = "done"
            progress.have = progress.need
            progress.segmentsDone = progress.segmentsTotal
        }

        let key = ProgressKey(
            status: status, have: progress.have, need: progress.need,
            segmentsDone: progress.segmentsDone, segmentsTotal: progress.segmentsTotal)
        let now = CACurrentMediaTime()
        if key != lastKey || now - lastFrameTime >= Self.frameInterval {
            lastKey = key
            lastFrameTime = now
            eventSink?(
                frameEvent(
                    update, status: status, rejectReason: rejectReason))
        }

        if case .done(let payload) = result.progress, !doneSent {
            doneSent = true
            eventSink?([
                "type": "done",
                "payload": FlutterStandardTypedData(bytes: payload),
                "sessionId": Int(update.sessionId ?? 0),
            ])
        }

        if result.error == .internalPanic {
            // The decoder asked to be discarded; start over with a fresh one.
            reset()
        }
    }

    private func frameEvent(_ update: ScanUpdate, status: String, rejectReason: Any) -> [String: Any] {
        let result = update.result
        var liveness: Any = NSNull()
        if case .read(let seedLow, let rolling, let consistent) = result.liveness {
            liveness = [
                "consistent": consistent,
                "seedLow": Int(seedLow),
                "rolling": Int(rolling),
            ] as [String: Any]
        }
        return [
            "type": "frame",
            "status": status,
            "locked": result.locked,
            "profile": result.profile?.rawValue ?? -1,
            "shapeId": result.shapeId ?? -1,
            "have": progress.have,
            "need": progress.need,
            "segmentsDone": progress.segmentsDone,
            "segmentsTotal": progress.segmentsTotal,
            "liveness": liveness,
            "anchors": normalizedAnchors(
                result.anchors, imageWidth: update.imageWidth, imageHeight: update.imageHeight),
            "sharpness": result.sharpness,
            "hint": Self.orNull(result.hint),
            "hintCode": Self.hintCode(result.hint),
            "rejectReason": rejectReason,
        ]
    }

    // MARK: - Anchors

    /// Anchors in image pixels -> `[x0, y0, x1, y1, ...]` normalized 0..1 to the preview
    /// layer's displayed content.
    ///
    /// The data output's connection is never rotated or mirrored, so the pixel buffer is in
    /// the capture device's native space. `layerPointConverted(fromCaptureDevicePoint:)` takes
    /// a point in exactly that space, normalized to 0..1 (top left origin), and applies the
    /// preview connection's rotation and mirroring plus `resizeAspectFill` cropping. Dividing
    /// by the layer's bounds gives 0..1 in the displayed view. Points outside 0..1 are
    /// possible (the code is partly cropped out of the preview) and are sent as they are.
    ///
    /// Without a preview on screen there is nothing to be relative to, so no anchors are sent.
    private func normalizedAnchors(
        _ anchors: [ScanPoint], imageWidth: Int, imageHeight: Int
    ) -> [Double] {
        guard !anchors.isEmpty, imageWidth > 0, imageHeight > 0,
            let preview = activePreview()
        else { return [] }
        let layer = preview.previewLayer
        guard layer.connection != nil else { return [] }
        let size = layer.bounds.size
        guard size.width > 0, size.height > 0 else { return [] }

        var out: [Double] = []
        out.reserveCapacity(anchors.count * 2)
        for point in anchors {
            let device = CGPoint(
                x: point.x / Double(imageWidth), y: point.y / Double(imageHeight))
            let p = layer.layerPointConverted(fromCaptureDevicePoint: device)
            let x = Double(p.x / size.width)
            let y = Double(p.y / size.height)
            guard x.isFinite, y.isFinite else { return [] }
            out.append(x)
            out.append(y)
        }
        return out
    }

    private static func orNull(_ value: String?) -> Any {
        if let value = value { return value }
        return NSNull()
    }

    // MARK: - Hints

    /// 0 none, 1 point at code, 2 move back, 3 hold steady, 4 lighting, 5 update app, 6 other.
    /// Keyword matching like `particlecode-jni`'s `hint_code`, so rewording the English text
    /// does not silently break the mapping.
    static func hintCode(_ hint: String?) -> Int {
        guard let text = hint?.lowercased() else { return 0 }
        if text.contains("newer version") || text.contains("update") { return 5 }
        if text.contains("too dark") || text.contains("glare") { return 4 }
        if text.contains("move back") || text.contains("whole code") { return 2 }
        if text.contains("steady") { return 3 }
        if text.contains("point the camera") { return 1 }
        return 6
    }
}
