import AVFoundation
import CoreMedia
import CoreVideo
import Flutter
import UIKit

/// Owns the one `AVCaptureSession` and hands camera frames to Dart by address
/// (PROTOCOL.md). It does no decoding.
///
/// Threading:
/// * `sessionQueue` (serial): everything that configures or starts/stops the session and the
///   camera device. `startRunning()` blocks, so it never runs on the main thread.
/// * `captureQueue` (serial): `captureOutput` runs here.
/// * The main thread: method channel calls, `toPreview` (it reads the preview layer) and
///   event delivery.
/// * `frameLock` guards the one in-flight frame, which the capture queue sets and the main
///   thread clears.
final class ScannerController: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    /// The shared session. It exists from the start (empty, not running) so that preview
    /// views can bind to it whether they are created before or after `start`.
    let session = AVCaptureSession()

    /// Delivers one event map to Dart; returns `false` when nobody listens. Set once by the
    /// plugin; called on the main thread.
    var eventSink: (([String: Any]) -> Bool)?

    // MARK: Session-queue state

    private let sessionQueue = DispatchQueue(label: "com.particlecode.flutter.session")
    private let captureQueue = DispatchQueue(
        label: "com.particlecode.flutter.capture", qos: .userInitiated)
    private var camera: AVCaptureDevice?
    private var configured = false
    private var torchOn = false
    private var errorObserver: NSObjectProtocol?

    // MARK: In-flight frame (guarded by `frameLock`)

    /// A camera frame whose base address is locked until Dart releases it.
    private struct HeldFrame {
        let id: Int
        let sampleBuffer: CMSampleBuffer
        let pixelBuffer: CVPixelBuffer
        /// Host time (`hostNanos()`) at which `captureOutput` got the frame.
        let receivedNanos: UInt64
    }

    private let frameLock = NSLock()
    private var held: HeldFrame?
    private var lastFrameId = 0
    /// Frames `captureOutput` closed because one was held, since the last frame event.
    private var skippedSinceEvent = 0
    /// `didDrop` callbacks (AVFoundation dropped the frame before delivery) since the last
    /// frame event.
    private var droppedSinceEvent = 0

    // MARK: Main-thread state

    private var previews: [WeakPreview] = []

    private final class WeakPreview {
        weak var view: PreviewView?
        init(_ view: PreviewView) { self.view = view }
    }

    private enum StartError: Error {
        case noCamera
        case camera(String)
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
        // A frame still held belongs to a previous session whose Dart side is gone.
        releaseHeldFrame()
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
            let started = Self.startInfo(for: self.camera)
            DispatchQueue.main.async {
                if running {
                    result(started)
                } else {
                    result(
                        FlutterError(
                            code: "camera_error", message: "The capture session did not start",
                            details: nil))
                }
            }
        }
    }

    /// Session queue. Builds input and output once; later starts reuse them.
    private func configureIfNeeded() throws {
        if configured { return }
        guard let device = Self.defaultCamera() else {
            throw StartError.noCamera
        }

        let output = Self.makeOutput()
        do {
            // 1080p, near autofocus, 30 fps. The data output connection is left in the
            // sensor's native orientation on purpose: `toPreview` maps buffer pixels to the
            // preview through the capture device's coordinate space.
            try Self.configure(session: session, device: device, output: output)
        } catch {
            throw StartError.camera("Camera setup failed: \(error.localizedDescription)")
        }
        // The output holds its delegate weakly; this controller lives as long as the plugin.
        output.setSampleBufferDelegate(self, queue: captureQueue)

        self.camera = device
        configured = true

        errorObserver = NotificationCenter.default.addObserver(
            forName: .AVCaptureSessionRuntimeError, object: session, queue: .main
        ) { [weak self] note in
            let error = note.userInfo?[AVCaptureSessionErrorKey] as? NSError
            _ = self?.eventSink?([
                "type": "error",
                "code": "camera_error",
                "message": error?.localizedDescription ?? "Camera runtime error",
            ])
        }
    }

    /// Stops capture. An unreleased frame stays held until `releaseFrame` or the next `start`.
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

    /// Engine detach: stop the camera, drop the observer and any held frame. No reply needed.
    func shutdown() {
        sessionQueue.async { [weak self] in
            guard let self = self else { return }
            self.stopOnSessionQueue()
            if let observer = self.errorObserver {
                NotificationCenter.default.removeObserver(observer)
                self.errorObserver = nil
            }
        }
        // The engine, and with it Dart, is gone: nobody will call `releaseFrame`.
        releaseHeldFrame()
    }

    // MARK: - torch

    func setTorch(on: Bool, result: @escaping FlutterResult) {
        sessionQueue.async { [weak self] in
            guard let self = self else {
                DispatchQueue.main.async { result(false) }
                return
            }
            self.torchOn = on
            let device = self.camera ?? Self.defaultCamera()
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

    /// The `start` result: `{"cameraId": String?, "facing": String}`. The id names the device
    /// type and position (no unique device identifier).
    private static func startInfo(for device: AVCaptureDevice?) -> [String: Any] {
        [
            "cameraId": cameraIdValue(device),
            "facing": facing(device),
        ]
    }

    private static func cameraId(_ device: AVCaptureDevice?) -> String? {
        guard let device = device else { return nil }
        return "\(device.deviceType.rawValue):\(facing(device))"
    }

    /// `cameraId` as a channel value (`NSNull` when unknown).
    private static func cameraIdValue(_ device: AVCaptureDevice?) -> Any {
        if let id = cameraId(device) { return id }
        return NSNull()
    }

    private static func facing(_ device: AVCaptureDevice?) -> String {
        switch device?.position {
        case .back?: return "back"
        case .front?: return "front"
        default: return "unknown"
        }
    }

    /// Monotonic host time in nanoseconds (`mach_absolute_time` units converted, not counting
    /// sleep): the clock of `CACurrentMediaTime()` and `CMClockGetHostTimeClock()`.
    private static func hostNanos() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_UPTIME_RAW)
    }

    /// `CVPixelBufferGetPixelFormatType` as a name: the NV12 codes by name, else the FourCC.
    private static func formatName(_ code: OSType) -> String {
        switch code {
        case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange:
            return "420YpCbCr8BiPlanarFullRange"
        case kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
            return "420YpCbCr8BiPlanarVideoRange"
        default:
            let bytes = [24, 16, 8, 0].map { UInt8(truncatingIfNeeded: code >> UInt32($0)) }
            if bytes.allSatisfy({ $0 >= 0x20 && $0 < 0x7f }) {
                return String(decoding: bytes, as: UTF8.self)
            }
            return String(format: "0x%08x", code)
        }
    }

    /// The frame's presentation time in nanoseconds of the host time clock. The data output
    /// stamps buffers with the session's synchronization clock (iOS 15.4+; earlier the
    /// master clock, which is the host clock on device); converted when it differs.
    private func captureNanos(_ sampleBuffer: CMSampleBuffer) -> Int {
        var time = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        if #available(iOS 15.4, *), let clock = session.synchronizationClock,
            !CFEqual(clock, CMClockGetHostTimeClock())
        {
            time = CMSyncConvertTime(time, from: clock, to: CMClockGetHostTimeClock())
        }
        guard time.isValid, time.isNumeric else { return 0 }
        let nanos = CMTimeConvertScale(time, timescale: 1_000_000_000, method: .default)
        return Int(clamping: nanos.value)
    }

    private static func permissionDenied() -> FlutterError {
        FlutterError(
            code: "permission_denied", message: "Camera permission was denied", details: nil)
    }

    // MARK: - Frames

    /// Capture queue. Keeps the newest frame locked and announces it, unless one is in flight.
    func captureOutput(
        _ output: AVCaptureOutput,
        didOutput sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    ) {
        let receivedNanos = Self.hostNanos()
        frameLock.lock()
        if held != nil {
            skippedSinceEvent += 1
            frameLock.unlock()
            return
        }
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer),
            CVPixelBufferGetPlaneCount(pixelBuffer) >= 2,
            CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly) == kCVReturnSuccess
        else {
            frameLock.unlock()
            return
        }
        guard let yBase = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0),
            let uvBase = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 1)
        else {
            CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly)
            frameLock.unlock()
            return
        }
        lastFrameId += 1
        let id = lastFrameId
        held = HeldFrame(
            id: id, sampleBuffer: sampleBuffer, pixelBuffer: pixelBuffer,
            receivedNanos: receivedNanos)
        let skipped = skippedSinceEvent
        let dropped = droppedSinceEvent
        skippedSinceEvent = 0
        droppedSinceEvent = 0
        frameLock.unlock()

        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let yStride = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0)
        let uvStride = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 1)
        let yLen = yStride * CVPixelBufferGetHeightOfPlane(pixelBuffer, 0)
        // NV12: plane 1 is interleaved CbCr. U starts at its base, V one byte later.
        let uLen = uvStride * CVPixelBufferGetHeightOfPlane(pixelBuffer, 1)
        let uAddress = Int(bitPattern: uvBase)

        let event: [String: Any] = [
            "type": "frame",
            "id": id,
            "width": width,
            "height": height,
            "yAddress": Int(bitPattern: yBase),
            "yLen": yLen,
            "uAddress": uAddress,
            "uLen": uLen,
            "vAddress": uAddress + 1,
            "vLen": uLen - 1,
            "yStride": yStride,
            "uvStride": uvStride,
            "uvPixelStride": 2,
            "format": Self.formatName(CVPixelBufferGetPixelFormatType(pixelBuffer)),
            // The output connection stays in the sensor's native orientation and the
            // rotation to upright is not computed here (`toPreview` carries it).
            "rotationDegrees": NSNull(),
            "cameraId": Self.cameraIdValue(camera),
            "timestampNanos": captureNanos(sampleBuffer),
            "timestampClock": "ios_host",
            "receivedNanos": Int(clamping: receivedNanos),
            "skippedBefore": skipped,
            "droppedByOutputBefore": dropped,
        ]
        DispatchQueue.main.async { [weak self] in
            self?.deliver(event, id: id, width: width, height: height)
        }
    }

    /// Capture queue. AVFoundation dropped a frame before delivering it (not a plugin skip).
    func captureOutput(
        _ output: AVCaptureOutput,
        didDrop sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    ) {
        frameLock.lock()
        droppedSinceEvent += 1
        frameLock.unlock()
    }

    /// Main thread. Adds `toPreview` and sends the event; frees the frame when it cannot be sent.
    private func deliver(_ base: [String: Any], id: Int, width: Int, height: Int) {
        // `start` or `shutdown` may have released it while the hop to the main thread was pending.
        frameLock.lock()
        let stillHeld = held?.id == id
        frameLock.unlock()
        guard stillHeld else { return }

        var event = base
        event["toPreview"] = toPreview(width: width, height: height)
        if eventSink?(event) != true {
            // Nobody listens: Dart will never release this frame.
            releaseFrame(id: id)
        }
    }

    /// Unlocks and drops the frame `id`; unknown ids are ignored. Any thread. Returns the host
    /// nanoseconds from receipt to now, or nil for an unknown id.
    @discardableResult
    func releaseFrame(id: Int) -> Int? {
        frameLock.lock()
        defer { frameLock.unlock() }
        guard let frame = held, frame.id == id else { return nil }
        let now = Self.hostNanos()
        CVPixelBufferUnlockBaseAddress(frame.pixelBuffer, .readOnly)
        held = nil
        return Int(clamping: now >= frame.receivedNanos ? now - frame.receivedNanos : 0)
    }

    /// Unlocks and drops whatever frame is held. Any thread.
    private func releaseHeldFrame() {
        frameLock.lock()
        defer { frameLock.unlock() }
        guard let frame = held else { return }
        CVPixelBufferUnlockBaseAddress(frame.pixelBuffer, .readOnly)
        held = nil
    }

    // MARK: - Previews

    /// Main thread. Called by `PreviewView` so frames can be mapped to what it displays.
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

    // MARK: - toPreview

    /// Main thread. The affine `[a, b, c, d, e, f]` with `x' = a*x + b*y + c`,
    /// `y' = d*x + e*y + f` mapping buffer pixels to the preview, normalized 0..1.
    ///
    /// The data output's connection is never rotated or mirrored, so the pixel buffer is in
    /// the capture device's native space. `layerPointConverted(fromCaptureDevicePoint:)` takes
    /// a point in exactly that space, normalized to 0..1 (top left origin), and applies the
    /// preview connection's rotation and mirroring plus the `resizeAspectFill` crop. Dividing
    /// by the layer's bounds gives 0..1 in the displayed view. That conversion is affine, so
    /// three corner points (0,0), (w,0), (0,h) determine it. Points outside 0..1 are possible
    /// (cropped out of the preview) and are sent as they are.
    private func toPreview(width: Int, height: Int) -> [Double] {
        let w = Double(width)
        let h = Double(height)
        if w > 0, h > 0, let preview = activePreview() {
            let layer = preview.previewLayer
            let size = layer.bounds.size
            if layer.connection != nil, size.width > 0, size.height > 0 {
                func normalized(_ x: Double, _ y: Double) -> (x: Double, y: Double) {
                    let p = layer.layerPointConverted(
                        fromCaptureDevicePoint: CGPoint(x: x / w, y: y / h))
                    return (Double(p.x / size.width), Double(p.y / size.height))
                }
                let p0 = normalized(0, 0)
                let p1 = normalized(w, 0)
                let p2 = normalized(0, h)
                let affine = [
                    (p1.x - p0.x) / w, (p2.x - p0.x) / h, p0.x,
                    (p1.y - p0.y) / w, (p2.y - p0.y) / h, p0.y,
                ]
                if affine.allSatisfy({ $0.isFinite }) { return affine }
            }
        }
        return Self.uprightAffine(width: w, height: h)
    }

    /// Without a laid-out preview: the upright image itself. The sensor's native orientation
    /// is landscape (right), so the portrait picture is the buffer rotated 90 degrees
    /// clockwise: (x, y) -> (h - y, x), divided by the upright size (h, w).
    private static func uprightAffine(width w: Double, height h: Double) -> [Double] {
        guard w > 0, h > 0 else { return [1, 0, 0, 0, 1, 0] }
        return [0, -1 / h, 1, 1 / w, 0, 0]
    }

    // MARK: - Capture configuration

    /// NV12 full range (`kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`): luma in 0...255 and
    /// planes Rust reads in place. The ISP produces it natively. Late frames are discarded, so
    /// AVFoundation never queues behind a slow consumer.
    private static func makeOutput() -> AVCaptureVideoDataOutput {
        let output = AVCaptureVideoDataOutput()
        output.alwaysDiscardsLateVideoFrames = true
        output.videoSettings = [
            kCVPixelBufferPixelFormatTypeKey as String:
                Int(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange)
        ]
        return output
    }

    /// The back wide-angle camera: the one to point at a phone screen.
    private static func defaultCamera() -> AVCaptureDevice? {
        AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back)
    }

    /// Adds `device` and `output` to `session` and tunes the camera for reading a screen:
    /// 1080p when available, continuous near autofocus, continuous auto exposure and the
    /// frame duration pinned to 30 fps (so dim rooms raise ISO instead of blurring frames).
    private static func configure(
        session: AVCaptureSession,
        device: AVCaptureDevice,
        output: AVCaptureVideoDataOutput,
        targetFPS: Int = 30
    ) throws {
        let input = try AVCaptureDeviceInput(device: device)
        guard session.canAddInput(input), session.canAddOutput(output) else {
            throw StartError.camera("The capture session refused the camera input or output")
        }
        session.beginConfiguration()
        if session.canSetSessionPreset(.hd1920x1080) {
            session.sessionPreset = .hd1920x1080
        }
        session.addInput(input)
        session.addOutput(output)
        session.commitConfiguration()

        // After the commit: the preset chooses the active format the frame rate must fit.
        try device.lockForConfiguration()
        defer { device.unlockForConfiguration() }
        if device.isFocusModeSupported(.continuousAutoFocus) {
            device.focusMode = .continuousAutoFocus
        }
        if device.isAutoFocusRangeRestrictionSupported {
            device.autoFocusRangeRestriction = .near
        }
        if device.isExposureModeSupported(.continuousAutoExposure) {
            device.exposureMode = .continuousAutoExposure
        }
        let fps = Double(targetFPS)
        let supported = device.activeFormat.videoSupportedFrameRateRanges.contains {
            $0.minFrameRate <= fps && fps <= $0.maxFrameRate
        }
        if supported, targetFPS > 0 {
            let duration = CMTime(value: 1, timescale: CMTimeScale(targetFPS))
            device.activeVideoMinFrameDuration = duration
            device.activeVideoMaxFrameDuration = duration
        }
    }
}
