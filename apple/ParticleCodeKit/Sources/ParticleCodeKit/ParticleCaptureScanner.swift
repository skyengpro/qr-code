#if canImport(AVFoundation) && canImport(CoreMedia) && canImport(CoreVideo)
import AVFoundation
import CoreMedia
import CoreVideo
import Foundation

/// Why `ParticleCaptureScanner.configure(session:device:output:)` could not set up capture.
public enum ScannerSetupError: Error, Equatable, Sendable {
    /// The session refused the camera input or the video data output.
    case cannotAddInputOrOutput
}

/// Glue between an `AVCaptureSession` and a `ParticleDecoder`.
///
/// The scanner is the delegate of an `AVCaptureVideoDataOutput` and owns one
/// `ParticleDecoder` that lives on a private serial queue. Camera pixel memory goes from
/// the `CMSampleBuffer` to Rust without a copy and never leaves that queue; only a small
/// `ScanResult` is delivered to `onResult` on `resultQueue` (the main queue by default),
/// which is where a Flutter plugin forwards progress to Dart. Frames never cross into Dart.
///
/// Typical use:
///
/// ```swift
/// guard let scanner = ParticleCaptureScanner(onResult: { result in /* UI, EventChannel */ }),
///       let camera = ParticleCaptureScanner.defaultCamera() else { return }
/// let session = AVCaptureSession()
/// let output = ParticleCaptureScanner.makeOutput()
/// try ParticleCaptureScanner.configure(session: session, device: camera, output: output)
/// scanner.attach(to: output)          // the output holds its delegate weakly: keep `scanner`
/// DispatchQueue.global().async { session.startRunning() }   // blocks; keep off main
/// ```
///
/// ## Threading and back-pressure
///
/// * Sample buffers arrive on `queue`, a serial queue; the decoder is only touched there.
/// * `makeOutput()` sets `alwaysDiscardsLateVideoFrames`, so while one frame is being
///   decoded, newer frames are dropped by AVFoundation instead of queuing up. Processing a
///   frame takes longer than the 33 ms frame interval on slow phones, and a backlog would
///   only add latency. This is intended: the sender repeats each data frame about three
///   times, and the decoder is built for dropped frames.
/// * After `.done` the scanner stops decoding until `reset()`.
public final class ParticleCaptureScanner: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    /// The serial queue the decoder lives on and the sample buffers are delivered on.
    public let queue: DispatchQueue

    private let resultQueue: DispatchQueue
    private let onResult: (ScanResult) -> Void
    // Both touched only on `queue`.
    private var decoder: ParticleDecoder
    private var finished = false

    /// - Parameters:
    ///   - resultQueue: Where `onResult` is called; the main queue by default.
    ///   - onResult: Called once per processed frame (up to the camera rate). Keep it cheap.
    /// - Returns: `nil` if the Rust decoder could not be created (see `ParticleDecoder.init`).
    public init?(
        resultQueue: DispatchQueue = .main,
        onResult: @escaping (ScanResult) -> Void
    ) {
        guard let decoder = ParticleDecoder() else { return nil }
        self.decoder = decoder
        self.resultQueue = resultQueue
        self.onResult = onResult
        self.queue = DispatchQueue(label: "com.particlecode.scanner", qos: .userInitiated)
        super.init()
    }

    /// Start a new transfer: drop everything received so far and resume decoding.
    public func reset() {
        queue.async { [self] in
            if let fresh = ParticleDecoder() {
                decoder = fresh
            }
            finished = false
        }
    }

    /// Make `output` deliver its frames to this scanner on `queue`. The output keeps only a
    /// weak reference to its delegate, so retain the scanner yourself.
    public func attach(to output: AVCaptureVideoDataOutput) {
        output.setSampleBufferDelegate(self, queue: queue)
    }

    // MARK: AVCaptureVideoDataOutputSampleBufferDelegate

    public func captureOutput(
        _ output: AVCaptureOutput,
        didOutput sampleBuffer: CMSampleBuffer,
        from connection: AVCaptureConnection
    ) {
        // Runs on `queue`.
        guard !finished, let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
            return
        }
        let result = decoder.feed(pixelBuffer: pixelBuffer)
        if case .done = result.progress {
            finished = true
        }
        let deliver = onResult
        resultQueue.async { deliver(result) }
    }

    // MARK: Capture configuration

    /// A video data output set up for the decoder:
    ///
    /// * `alwaysDiscardsLateVideoFrames = true` (see the class notes), and
    /// * NV12 full range (`kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`): luma in
    ///   0...255 and a layout the decoder reads in place. The ISP produces this natively, so
    ///   it costs nothing; BGRA would add a conversion pass and a 4x larger buffer.
    public static func makeOutput() -> AVCaptureVideoDataOutput {
        let output = AVCaptureVideoDataOutput()
        output.alwaysDiscardsLateVideoFrames = true
        output.videoSettings = [
            kCVPixelBufferPixelFormatTypeKey as String:
                Int(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange)
        ]
        return output
    }

    #if os(iOS)
    /// The back wide-angle camera: the one to point at a phone screen.
    public static func defaultCamera() -> AVCaptureDevice? {
        AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back)
    }

    /// Add `device` and `output` to `session` and tune the camera for reading a screen.
    ///
    /// * 1080p preset (`.hd1920x1080`) when available. The detector needs about 7 px per dot
    ///   at profile M; 720p is the fallback floor and 4K only costs decode time.
    /// * Continuous autofocus with `autoFocusRangeRestriction = .near`, so the lens does not
    ///   hunt towards infinity when the code is briefly out of view.
    /// * Continuous auto exposure, with frame duration pinned to `targetFPS` (30). Auto
    ///   exposure may otherwise stretch frames to 1/15 s or longer in dim rooms, which
    ///   blurs the 10 fps data frames; a pinned rate makes the camera raise ISO instead.
    ///
    /// Orientation does not matter: the code is rotation and mirror invariant, so there is
    /// no need to set `videoOrientation` (which can cost a rotation pass).
    ///
    /// Camera permission (`NSCameraUsageDescription`) is the caller's business. Starting the
    /// session (`startRunning()`) blocks, so do it off the main thread.
    public static func configure(
        session: AVCaptureSession,
        device: AVCaptureDevice,
        output: AVCaptureVideoDataOutput,
        targetFPS: Int = 30
    ) throws {
        let input = try AVCaptureDeviceInput(device: device)
        guard session.canAddInput(input), session.canAddOutput(output) else {
            throw ScannerSetupError.cannotAddInputOrOutput
        }
        session.beginConfiguration()
        if session.canSetSessionPreset(.hd1920x1080) {
            session.sessionPreset = .hd1920x1080
        }
        session.addInput(input)
        session.addOutput(output)
        session.commitConfiguration()
        // After the commit: the preset chooses the active format the frame rate must fit.
        try configure(device: device, targetFPS: targetFPS)
    }

    /// Only the focus, exposure and frame-rate part of `configure(session:...)`, for apps
    /// that build their own session.
    public static func configure(device: AVCaptureDevice, targetFPS: Int = 30) throws {
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
    #endif
}
#endif
