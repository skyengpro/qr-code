import XCTest

import ParticleCodeKit

#if canImport(CoreVideo)
import CoreVideo
#endif

/// These run on macOS (`swift test`), which needs the macOS slice of the xcframework:
/// build it with `scripts/build-xcframework.sh --macos`.
final class ParticleCodeKitTests: XCTestCase {
    func testDecoderCreatesAndStartsEmpty() throws {
        let decoder = try XCTUnwrap(ParticleDecoder())
        XCTAssertNil(decoder.sessionId)
        XCTAssertNil(decoder.takeReady())
        XCTAssertTrue(decoder.payload.isEmpty)
    }

    func testLivenessRejectsWrongKeys() {
        // Only a 32-byte key is valid; the library reports anything else as "not live".
        XCTAssertFalse(ParticleDecoder.verifyLiveness(key: Data(), rolling: 0, now: 1_750_000_000, skew: 2))
        XCTAssertFalse(
            ParticleDecoder.verifyLiveness(
                key: Data(repeating: 7, count: 31), rolling: 0, now: 1_750_000_000, skew: 2))
        XCTAssertFalse(
            ParticleDecoder.verifyLiveness(
                key: Data(repeating: 7, count: 33), rolling: 0, now: 1_750_000_000, skew: 2))

        // A 32-byte key that is not the sender's: for a single second exactly one of the
        // 65536 rolling values is valid, so among these eight at most one can match.
        let wrong = Data(repeating: 0xAA, count: 32)
        let matches = (UInt16(0)..<8).filter {
            ParticleDecoder.verifyLiveness(key: wrong, rolling: $0, now: 1_750_000_000, skew: 0)
        }
        XCTAssertLessThanOrEqual(matches.count, 1)
    }

    func testLivenessNotReadNeverVerifies() {
        XCTAssertFalse(Liveness.notRead.verify(key: Data(repeating: 1, count: 32)))
        XCTAssertFalse(
            Liveness.read(seedLow: 1, rolling: 2, consistent: false)
                .verify(key: Data(repeating: 1, count: 32)))
    }

    #if canImport(CoreVideo)
    private func makePixelBuffer(
        format: OSType, width: Int = 640, height: Int = 480, fill: UInt8
    ) throws -> CVPixelBuffer {
        var buffer: CVPixelBuffer?
        let status = CVPixelBufferCreate(kCFAllocatorDefault, width, height, format, nil, &buffer)
        let pixelBuffer = try XCTUnwrap(buffer)
        XCTAssertEqual(status, kCVReturnSuccess)

        CVPixelBufferLockBaseAddress(pixelBuffer, [])
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, []) }
        if CVPixelBufferIsPlanar(pixelBuffer) {
            for plane in 0..<CVPixelBufferGetPlaneCount(pixelBuffer) {
                let base = try XCTUnwrap(CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, plane))
                let size =
                    CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, plane)
                    * CVPixelBufferGetHeightOfPlane(pixelBuffer, plane)
                memset(base, Int32(fill), size)
            }
        } else {
            let base = try XCTUnwrap(CVPixelBufferGetBaseAddress(pixelBuffer))
            memset(base, Int32(fill), CVPixelBufferGetBytesPerRow(pixelBuffer) * height)
        }
        return pixelBuffer
    }

    private func assertRejectedWithoutError(
        _ result: ScanResult, file: StaticString = #filePath, line: UInt = #line
    ) {
        XCTAssertNil(result.error, file: file, line: line)
        XCTAssertFalse(result.locked, file: file, line: line)
        guard case let .rejected(reason) = result.progress else {
            return XCTFail("expected .rejected, got \(result.progress)", file: file, line: line)
        }
        XCTAssertFalse(reason.isEmpty, file: file, line: line)
    }

    func testBlankNV12FullRangeFrameIsRejected() throws {
        let decoder = try XCTUnwrap(ParticleDecoder())
        let frame = try makePixelBuffer(
            format: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, fill: 128)
        assertRejectedWithoutError(decoder.feed(pixelBuffer: frame))
        XCTAssertNotNil(decoder.rejectReason)
    }

    func testBlankNV12VideoRangeFrameIsRejected() throws {
        let decoder = try XCTUnwrap(ParticleDecoder())
        let frame = try makePixelBuffer(
            format: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, fill: 128)
        assertRejectedWithoutError(decoder.feed(pixelBuffer: frame))
    }

    func testOddSizedNV12FrameDoesNotCrash() throws {
        let decoder = try XCTUnwrap(ParticleDecoder())
        let frame = try makePixelBuffer(
            format: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, width: 321, height: 241,
            fill: 90)
        assertRejectedWithoutError(decoder.feed(pixelBuffer: frame))
    }

    func testBlankBGRAFrameIsRejected() throws {
        let decoder = try XCTUnwrap(ParticleDecoder())
        let frame = try makePixelBuffer(format: kCVPixelFormatType_32BGRA, fill: 100)
        assertRejectedWithoutError(decoder.feed(pixelBuffer: frame))
    }

    func testUnsupportedPixelFormatReportsError() throws {
        let decoder = try XCTUnwrap(ParticleDecoder())
        let format = kCVPixelFormatType_32ARGB
        let frame = try makePixelBuffer(format: format, fill: 0)
        let result = decoder.feed(pixelBuffer: frame)
        XCTAssertEqual(result.error, .unsupportedPixelFormat(format))
        guard case .rejected = result.progress else { return XCTFail("expected .rejected") }
    }
    #endif
}
