import Foundation
import ParticleCodeFFI

#if canImport(CoreVideo)
import CoreVideo
#endif

/// Receiver side of Particle Code: feed camera frames, get progress and, in the end, the
/// payload. A thin owner of the Rust `PcDecoder` handle.
///
/// ## Threading
///
/// **A `ParticleDecoder` is not thread-safe.** Every call, including property reads such as
/// `hint`, `rejectReason` and `payload`, must happen on one thread at a time. The usual
/// arrangement is a private serial `DispatchQueue` (see `ParticleCaptureScanner`). Several
/// decoders on several queues are fine. `deinit` frees the Rust handle, so make sure no
/// call is in flight on another thread when the last reference goes away.
///
/// ## Frames
///
/// Frames are borrowed, never retained: the pixel memory is read in place during the call.
/// Locating the code in a 1080p frame is real work (tens of milliseconds on a phone); never
/// call it on the main thread.
public final class ParticleDecoder {
    private let handle: OpaquePointer

    /// Everything handed out by `takeReady()` so far (and by the internal drain at
    /// `.done`), in order. After `ScanProgress.done` this is the complete payload.
    public private(set) var payload = Data()

    /// A new decoder, or `nil` if the Rust library could not allocate one or its C ABI
    /// version differs from the header this package was built against.
    public init?() {
        guard pc_abi_version() == UInt32(PC_ABI_VERSION), let handle = pc_decoder_new() else {
            return nil
        }
        self.handle = handle
    }

    deinit {
        pc_decoder_free(handle)
    }

    // MARK: - Camera frames

    /// Feed one planar or semi-planar YUV 4:2:0 frame. The memory is only read during the
    /// call.
    ///
    /// Sample `(col, row)` of chroma is at byte `row * uvStride + col * uvPixelStride`. For
    /// NV12 (what iOS delivers), `u` is the interleaved CbCr plane and `v` the same plane
    /// plus one byte, with `uvPixelStride` 2; `vLength` is then one less than `uLength`.
    /// For I420, pass the three planes with `uvPixelStride` 1.
    ///
    /// - Parameters:
    ///   - width: Luma width in pixels (chroma is `ceil(width / 2)` wide).
    ///   - height: Luma height in pixels.
    ///   - yStride: Bytes between luma rows.
    ///   - uvStride: Bytes between chroma rows.
    public func feedYUV420(
        width: Int, height: Int,
        y: UnsafePointer<UInt8>, yLength: Int, yStride: Int,
        u: UnsafePointer<UInt8>, uLength: Int,
        v: UnsafePointer<UInt8>, vLength: Int,
        uvStride: Int, uvPixelStride: Int
    ) -> ScanResult {
        guard let w = UInt32(exactly: width), let h = UInt32(exactly: height),
            let ys = UInt32(exactly: yStride), let uvs = UInt32(exactly: uvStride),
            let ps = UInt32(exactly: uvPixelStride), yLength >= 0, uLength >= 0, vLength >= 0
        else {
            return .failed(.invalidBuffer)
        }
        var raw = PcFrameResult()
        let status = pc_decoder_feed_yuv(
            handle, w, h, y, yLength, u, uLength, v, vLength, ys, uvs, ps, &raw)
        return makeResult(status: status, raw: raw)
    }

    /// Feed one tightly packed RGBA8 frame (`rgba.count >= 4 * width * height`, alpha
    /// ignored). Slower than YUV (the library converts to luma/chroma itself); meant for
    /// desktop and tests.
    public func feedRGBA(width: Int, height: Int, rgba: UnsafeRawBufferPointer) -> ScanResult {
        guard let w = UInt32(exactly: width), let h = UInt32(exactly: height),
            let base = rgba.baseAddress
        else {
            return .failed(.invalidBuffer)
        }
        var raw = PcFrameResult()
        let status = pc_decoder_feed_rgba(
            handle, w, h, base.assumingMemoryBound(to: UInt8.self), rgba.count, &raw)
        return makeResult(status: status, raw: raw)
    }

    // MARK: - State

    /// Take the payload bytes delivered, in order, since the last call; `nil` if there are
    /// none. Bytes arrive segment by segment, so a large transfer can be consumed while it
    /// runs. The bytes are also appended to `payload`.
    @discardableResult
    public func takeReady() -> Data? {
        var ptr: UnsafeMutablePointer<UInt8>?
        var len = 0
        guard pc_decoder_take_ready(handle, &ptr, &len) == 1, let bytes = ptr else {
            return nil
        }
        defer { pc_bytes_free(bytes, len) }
        let chunk = Data(bytes: bytes, count: len)
        payload.append(chunk)
        return chunk
    }

    /// Technical reason the last feed was rejected, `nil` if it was not.
    public var rejectReason: String? {
        Self.string(pc_decoder_reject_reason(handle))
    }

    /// Advice for the user after the last camera frame ("Too dark or too much glare ..."),
    /// `nil` if there is none.
    public var hint: String? {
        Self.string(pc_decoder_hint(handle))
    }

    /// The session id being received, once a frame was accepted.
    public var sessionId: UInt16? {
        var id: UInt16 = 0
        return pc_decoder_session_id(handle, &id) == 1 ? id : nil
    }

    // MARK: - Liveness

    /// Check a particle liveness value (`Liveness.read`'s `rolling`) against the 32-byte
    /// session key the sender shares out of band.
    ///
    /// - Parameters:
    ///   - key: Exactly 32 bytes; any other length is `false`.
    ///   - rolling: The value read from the particles.
    ///   - now: Verifier's clock, seconds since the Unix epoch.
    ///   - skew: Accepted clock difference in seconds; 0 accepts only the current second.
    public static func verifyLiveness(
        key: Data, rolling: UInt16, now: UInt64 = ParticleDecoder.unixSeconds(Date()), skew: UInt64 = 2
    ) -> Bool {
        key.withUnsafeBytes { buffer -> Bool in
            guard let base = buffer.baseAddress else { return false }
            return pc_liveness_verify(
                base.assumingMemoryBound(to: UInt8.self), buffer.count, rolling, now, skew) == 1
        }
    }

    static func unixSeconds(_ date: Date) -> UInt64 {
        UInt64(max(0, date.timeIntervalSince1970))
    }

    // MARK: - Mapping from the C layer

    private func makeResult(status: Int32, raw: PcFrameResult) -> ScanResult {
        if status < 0 {
            // `raw` is not filled for errors.
            switch status {
            case PC_ERR_NULL: return .failed(.nullArgument)
            case PC_ERR_PANIC: return .failed(.internalPanic)
            default: return .failed(.invalidBuffer)
            }
        }
        let progress: ScanProgress
        switch status {
        case PC_ACCEPTED:
            progress = acceptedProgress()
        case PC_DONE:
            takeReady()
            progress = .done(payload)
        default:
            progress = .rejected(reason: rejectReason ?? "frame rejected")
        }
        return ScanResult(
            progress: progress, error: nil, locked: raw.locked == 1,
            profile: CodeProfile(rawValue: Int(raw.profile)),
            shapeId: raw.shape_id >= 0 ? Int(raw.shape_id) : nil,
            liveness: Self.liveness(raw), anchors: Self.anchors(raw),
            sharpness: raw.sharpness, hint: hint)
    }

    private func acceptedProgress() -> ScanProgress {
        var p = PcProgress()
        guard pc_decoder_progress(handle, &p) == 0 else {
            return .accepted(have: 0, need: 0, segmentsDone: 0, segmentsTotal: 0)
        }
        return .accepted(
            have: Int(p.have), need: Int(p.need), segmentsDone: Int(p.segments_done),
            segmentsTotal: Int(p.segments_total))
    }

    private static func liveness(_ raw: PcFrameResult) -> Liveness {
        guard raw.liveness_read == 1 else { return .notRead }
        return .read(
            seedLow: UInt8(truncatingIfNeeded: raw.liveness_seed_low),
            rolling: UInt16(truncatingIfNeeded: raw.liveness_rolling),
            consistent: raw.liveness_consistent == 1)
    }

    private static func anchors(_ raw: PcFrameResult) -> [ScanPoint] {
        // `double anchors[8]` is imported as an 8-tuple; read it as a flat Double buffer.
        var tuple = raw.anchors
        let flat: [Double] = withUnsafeBytes(of: &tuple) { Array($0.bindMemory(to: Double.self)) }
        let count = min(Int(raw.anchor_count), flat.count / 2)
        return (0..<count).map { ScanPoint(x: flat[2 * $0], y: flat[2 * $0 + 1]) }
    }

    /// Copy a library-owned C string into Swift and release it.
    private static func string(_ cString: UnsafeMutablePointer<CChar>?) -> String? {
        guard let cString else { return nil }
        defer { pc_string_free(cString) }
        return String(cString: cString)
    }
}

#if canImport(CoreVideo)
extension ParticleDecoder {
    /// Feed one camera frame straight from a `CVPixelBuffer`, without copying (NV12) or
    /// with one swizzling copy (32BGRA).
    ///
    /// - `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` and `...VideoRange` (NV12): plane
    ///   0 is Y, plane 1 interleaved CbCr. The planes are passed to Rust in place; only a
    ///   quarter-size chroma scratch buffer is allocated inside the library. Full range is
    ///   preferred (see apple/README.md): the detector works on luma, and video range
    ///   squeezes it into 16...235.
    /// - `kCVPixelFormatType_32BGRA`: supported as a fallback for sources that cannot
    ///   deliver YUV (screenshots, some simulators). It costs a full-frame copy that swaps
    ///   B and R (about 8 MB at 1080p), and the library then converts back to luma, so it is
    ///   several times slower than NV12. Do not use it for a live camera.
    /// - Anything else returns `ScanResult.error == .unsupportedPixelFormat`.
    ///
    /// The base address is locked read-only for the duration of the call. Like every other
    /// method this must not run concurrently with another call on the same decoder.
    public func feed(pixelBuffer: CVPixelBuffer) -> ScanResult {
        guard CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly) == kCVReturnSuccess else {
            return .failed(.cannotLockPixelBuffer)
        }
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }

        let format = CVPixelBufferGetPixelFormatType(pixelBuffer)
        switch format {
        case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
            return feedNV12(pixelBuffer)
        case kCVPixelFormatType_32BGRA:
            return feedBGRA(pixelBuffer)
        default:
            return .failed(.unsupportedPixelFormat(format))
        }
    }

    private func feedNV12(_ buffer: CVPixelBuffer) -> ScanResult {
        guard CVPixelBufferGetPlaneCount(buffer) == 2,
            let yBase = CVPixelBufferGetBaseAddressOfPlane(buffer, 0),
            let cBase = CVPixelBufferGetBaseAddressOfPlane(buffer, 1)
        else {
            return .failed(.invalidBuffer)
        }
        let width = CVPixelBufferGetWidthOfPlane(buffer, 0)
        let height = CVPixelBufferGetHeightOfPlane(buffer, 0)
        let yStride = CVPixelBufferGetBytesPerRowOfPlane(buffer, 0)
        let cWidth = CVPixelBufferGetWidthOfPlane(buffer, 1)
        let cHeight = CVPixelBufferGetHeightOfPlane(buffer, 1)
        let cStride = CVPixelBufferGetBytesPerRowOfPlane(buffer, 1)
        // Chroma is half resolution (rounded up); one CbCr pair is two bytes.
        guard width > 0, height > 0, yStride >= width,
            cWidth >= (width + 1) / 2, cHeight >= (height + 1) / 2, cStride >= 2 * cWidth
        else {
            return .failed(.invalidBuffer)
        }

        let y = UnsafePointer(yBase.assumingMemoryBound(to: UInt8.self))
        let u = UnsafePointer(cBase.assumingMemoryBound(to: UInt8.self))  // Cb at even bytes
        let v = u + 1  // Cr at odd bytes: the same plane, one byte in
        let cLength = cStride * cHeight
        return feedYUV420(
            width: width, height: height,
            y: y, yLength: yStride * height, yStride: yStride,
            u: u, uLength: cLength,
            v: v, vLength: cLength - 1,
            uvStride: cStride, uvPixelStride: 2)
    }

    private func feedBGRA(_ buffer: CVPixelBuffer) -> ScanResult {
        guard let base = CVPixelBufferGetBaseAddress(buffer) else {
            return .failed(.invalidBuffer)
        }
        let width = CVPixelBufferGetWidth(buffer)
        let height = CVPixelBufferGetHeight(buffer)
        let stride = CVPixelBufferGetBytesPerRow(buffer)
        guard width > 0, height > 0, width <= 16384, height <= 16384, stride >= 4 * width else {
            return .failed(.invalidBuffer)
        }

        var rgba = [UInt8](repeating: 255, count: 4 * width * height)
        rgba.withUnsafeMutableBufferPointer { dst in
            for row in 0..<height {
                let src = (base + row * stride).assumingMemoryBound(to: UInt8.self)
                let out = row * width * 4
                for x in 0..<width {
                    let s = 4 * x
                    dst[out + s] = src[s + 2]  // R from B,G,R,A order
                    dst[out + s + 1] = src[s + 1]
                    dst[out + s + 2] = src[s]
                }
            }
        }
        return rgba.withUnsafeBytes { feedRGBA(width: width, height: height, rgba: $0) }
    }
}
#endif
