import Foundation

/// Density profile of the code in view (the header's profile field).
public enum CodeProfile: Int, Equatable, Sendable {
    case s = 0
    case m = 1
    case l = 2
}

/// A point in camera-image pixels (x right, y down, in the buffer's own orientation).
public struct ScanPoint: Equatable, Sendable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

/// Why a frame could not even be offered to the decoder. These are bugs or capture
/// misconfiguration, not "no code in view" (that is `ScanProgress.rejected`).
public enum ScanError: Error, Equatable, Sendable, CustomStringConvertible {
    /// The C layer saw a NULL handle or plane (`PC_ERR_NULL`).
    case nullArgument
    /// A size was zero or a buffer was shorter than width, height and strides imply
    /// (`PC_ERR_INVALID`), or the pixel buffer was not laid out as expected.
    case invalidBuffer
    /// A Rust panic was caught at the boundary (`PC_ERR_PANIC`). The decoder state is
    /// suspect: discard this `ParticleDecoder` and make a new one.
    case internalPanic
    /// `feed(pixelBuffer:)` handles NV12 (full/video range) and 32BGRA only.
    case unsupportedPixelFormat(UInt32)
    /// `CVPixelBufferLockBaseAddress` failed.
    case cannotLockPixelBuffer

    public var description: String {
        switch self {
        case .nullArgument: return "null handle or buffer"
        case .invalidBuffer: return "invalid or too short image buffer"
        case .internalPanic: return "internal error (decoder must be discarded)"
        case let .unsupportedPixelFormat(fourCC):
            return "unsupported pixel format \(fourCC) (use NV12 or 32BGRA)"
        case .cannotLockPixelBuffer: return "cannot lock the pixel buffer"
        }
    }
}

/// What the decoder did with a frame.
public enum ScanProgress: Equatable, Sendable {
    /// The frame was not used (no code found, unreadable, wrong session, ...). `reason` is
    /// technical, for logs; `ScanResult.hint` has the text to show a user.
    case rejected(reason: String)
    /// The frame was used. Counts are over the whole transfer.
    /// - `have`: distinct symbols held, summed over segments.
    /// - `need`: symbols needed if nothing is lost.
    case accepted(have: Int, need: Int, segmentsDone: Int, segmentsTotal: Int)
    /// The whole payload arrived. The associated value is the complete payload, equal to
    /// `ParticleDecoder.payload`.
    case done(Data)
}

/// The liveness proof shown by the six particles of the sender's frame.
public enum Liveness: Equatable, Sendable {
    /// The particles were not read in this frame.
    case notRead
    /// - `seedLow`: low byte of the packet seed the particles show.
    /// - `rolling`: HMAC-based value of the sender's wall-clock second; check it with
    ///   `verify(key:now:skew:)` or `ParticleDecoder.verifyLiveness`.
    /// - `consistent`: the particles' seed byte matches the frame header.
    case read(seedLow: UInt8, rolling: UInt16, consistent: Bool)

    /// `true` if the particles were read, are consistent with the header, and `rolling` is
    /// the value of some second within `skew` seconds of `now` for the 32-byte session `key`.
    public func verify(key: Data, now: Date = Date(), skew: UInt64 = 2) -> Bool {
        guard case let .read(_, rolling, consistent) = self, consistent else { return false }
        return ParticleDecoder.verifyLiveness(
            key: key, rolling: rolling, now: ParticleDecoder.unixSeconds(now), skew: skew)
    }
}

/// Result of feeding one camera frame.
public struct ScanResult: Equatable, Sendable {
    /// What the decoder did with the frame.
    public var progress: ScanProgress
    /// Set when the frame could not be processed at all; `progress` is then `.rejected`.
    public var error: ScanError?
    /// The three anchors were found (the overlay may be drawn).
    public var locked: Bool
    /// Density profile of the frame's header, once read.
    public var profile: CodeProfile?
    /// Shape id of the frame's header, once read.
    public var shapeId: Int?
    /// Particle liveness value, if read.
    public var liveness: Liveness
    /// Anchor points in image pixels: top, lower right, lower left, then the center
    /// (0, 3 or 4 points). Empty when not locked.
    public var anchors: [ScanPoint]
    /// Sharpness score, higher is sharper; 0 if not locked.
    public var sharpness: Double
    /// Advice for the user ("Too dark or too much glare ..."), if any.
    public var hint: String?

    static func failed(_ error: ScanError) -> ScanResult {
        ScanResult(
            progress: .rejected(reason: error.description), error: error, locked: false,
            profile: nil, shapeId: nil, liveness: .notRead, anchors: [], sharpness: 0, hint: nil)
    }
}
