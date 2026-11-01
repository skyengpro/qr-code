# Particle Code on Apple platforms

The scanner side for iOS (and macOS): the Rust decoder from `particlecode-ffi` as an
`xcframework`, a small Swift package around it, and a podspec for plugins that still use
CocoaPods.

```
scripts/build-xcframework.sh         builds target/apple/ParticleCodeFFI.xcframework (+ .zip; $CARGO_TARGET_DIR/apple if set)
apple/ParticleCodeKit/               Swift package: ParticleDecoder, ParticleCaptureScanner
apple/ParticleCodeKit.podspec        the same sources for CocoaPods
```

The point of the layout: the iOS camera hands `CVPixelBuffer` planes to Rust on a serial
background queue, in place, with late frames discarded. Only progress goes back to the UI
(or to Flutter). Frames never cross into Dart, are never copied into a `Uint8List`, and the
UI isolate is never involved.

## Building

Needs macOS with Xcode and the Rust Apple targets (the repository's `rust-toolchain.toml`
pins the Rust version):

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
rustup target add aarch64-apple-darwin x86_64-apple-darwin      # only for --macos

scripts/build-xcframework.sh            # iOS device + simulator (arm64, x86_64)
scripts/build-xcframework.sh --macos    # plus a macOS slice (desktop / Tauri, swift test)
```

It works from any directory and prints the size of every slice and the SwiftPM checksum.
Environment: `IPHONEOS_DEPLOYMENT_TARGET` (default 13.0), `MACOSX_DEPLOYMENT_TARGET`
(11.0). `--no-zip` skips the zip, `--debug-info` keeps debug info.

Output:

| Path | What |
| --- | --- |
| `target/apple/ParticleCodeFFI.xcframework` | `ios-arm64`, `ios-arm64_x86_64-simulator`, with `--macos` also `macos-arm64_x86_64`; each has `particlecode.h` and a `module.modulemap` (`module ParticleCodeFFI`) |
| `target/apple/ParticleCodeFFI.xcframework.zip` | for a remote SwiftPM `binaryTarget` |
| `apple/ParticleCodeKit/Frameworks/ParticleCodeFFI.xcframework` | copy used by the Swift package and the podspec (git-ignored). SwiftPM wants local binary targets inside the package directory |

Only the static library is built (`cargo rustc --crate-type staticlib`), not the `cdylib`
the crate also declares. Rust's standard library needs `libiconv`; a static library cannot
carry linker flags, so the package (`linkedLibrary("iconv")`) and the podspec
(`s.libraries`) add it. If you link the xcframework from your own Xcode project, add
`-liconv`.

CI (`.github/workflows/apple.yml`) builds all of this on `macos-latest`, runs `swift test`
on the macOS slice, compiles the package for iOS device and simulator, and uploads the zip.

### Swift package

```sh
cd apple/ParticleCodeKit
swift build && swift test      # macOS; needs the --macos slice
xcodebuild -scheme ParticleCodeKit -destination 'generic/platform=iOS' build
```

To release, upload the zip, then swap the local binary target in `Package.swift` for
`.binaryTarget(name: "ParticleCodeFFI", url: "https://.../ParticleCodeFFI.xcframework.zip",
checksum: "<printed by the script>")`.

## API

```swift
import ParticleCodeKit

let decoder = ParticleDecoder()!                       // not thread-safe, see below
let result = decoder.feed(pixelBuffer: pixelBuffer)    // NV12 in place; BGRA with a copy

switch result.progress {
case .rejected(let reason):                      // normal: no code in view, blurred, ...
    show(result.hint)                            // user text, e.g. "Too dark or too much glare"
case .accepted(let have, let need, let done, let total):
    updateRing(Double(have) / Double(max(need, 1)))
case .done(let payload):                         // the whole COSE/CBOR payload
    handle(payload)
}
result.locked, result.anchors                    // overlay: anchors in image pixels
result.sharpness, result.profile, result.shapeId
result.liveness.verify(key: sessionKey)          // particles vs the shared 32-byte key
decoder.takeReady()                              // payload streamed segment by segment
ParticleDecoder.verifyLiveness(key:rolling:now:skew:)
```

* `ScanProgress` is named so because `Progress` would collide with Foundation's.
* `feed` never throws and never traps on bad input. Problems with the frame itself
  (unsupported format, buffer too short) come back as `result.error`, with `progress`
  `.rejected`. `.internalPanic` means a Rust panic was caught: drop the decoder and create
  a new one.
* `feedYUV420(...)` and `feedRGBA(...)` take raw memory for callers that do not use
  `CVPixelBuffer` (a plugin that gets planes some other way).
* Pixel formats: NV12 full or video range is read in place. **32BGRA** is supported as a
  fallback only: it costs a full-frame B/R swizzle (~8 MB of memory traffic at 1080p) and
  the library converts to luma again, so it is several times slower. Anything else returns
  `.unsupportedPixelFormat`.

### Capture

```swift
let scanner = ParticleCaptureScanner { result in /* main queue */ }!
let output = ParticleCaptureScanner.makeOutput()    // discards late frames, NV12 full range
let session = AVCaptureSession()
try ParticleCaptureScanner.configure(session: session,
                                     device: ParticleCaptureScanner.defaultCamera()!,
                                     output: output)
scanner.attach(to: output)                          // keep `scanner` alive: the output holds it weakly
DispatchQueue.global().async { session.startRunning() }
```

`configure` selects the 1080p preset, continuous autofocus with
`autoFocusRangeRestriction = .near` (the code is 15-40 cm away; the lens must not hunt to
infinity), continuous auto exposure, and pins the frame duration to 30 fps so dim rooms get
more ISO rather than longer, blurrier exposures. `configure(device:targetFPS:)` does only
the device part if you own the session. The code is rotation and mirror invariant, so do not
set `videoOrientation` (it can add a rotation pass).

## Using it from a Flutter plugin

The plugin's iOS side owns the camera, the preview and the decoder. Dart sees progress
events and the final payload, never pixels.

**SwiftPM** (Flutter 3.24 and later, `flutter config --enable-swift-package-manager`): in
the plugin, `ios/<plugin>/Package.swift` depends on the package,

```swift
dependencies: [.package(path: "../../../apple/ParticleCodeKit")]   // or a URL + version
// target dependencies: [.product(name: "ParticleCodeKit", package: "ParticleCodeKit")]
```

**CocoaPods**: in the plugin's `ios/<plugin>.podspec`,
`s.dependency 'ParticleCodeKit'` and, in the app's `Podfile`,
`pod 'ParticleCodeKit', :path => '<repo>/apple'`. This path is the least exercised one:
CI runs `pod lib lint` as a non-blocking check. Prefer SwiftPM when you can.

Progress goes back with an `EventChannel` (or Pigeon `@FlutterApi`). Send small maps, not
frames:

```swift
final class ScanPlugin: NSObject, FlutterPlugin, FlutterStreamHandler {
    private var sink: FlutterEventSink?
    private var scanner: ParticleCaptureScanner?

    func onListen(withArguments _: Any?, eventSink events: @escaping FlutterEventSink) -> FlutterError? {
        sink = events
        scanner = ParticleCaptureScanner { [weak self] r in      // already on the main queue
            switch r.progress {
            case .accepted(let have, let need, let done, let total):
                self?.sink?(["have": have, "need": need, "segments": done, "of": total])
            case .done(let payload):
                self?.sink?(["done": FlutterStandardTypedData(bytes: payload)])
            case .rejected:
                self?.sink?(["locked": r.locked, "hint": r.hint as Any])   // throttle if chatty
            }
        }
        // ... create the session, configure(session:device:output:), scanner?.attach(to:)
        return nil
    }
    func onCancel(withArguments _: Any?) -> FlutterError? { sink = nil; scanner = nil; return nil }
}
```

Anchors (`result.anchors`, image pixels in the buffer's own orientation) are enough to draw
an overlay in Dart or natively; map them to preview coordinates with the preview layer's
`layerPointConverted(fromCaptureDevicePoint:)` if you use `AVCaptureVideoPreviewLayer`.
Remember `NSCameraUsageDescription` in the app's `Info.plist`.

The Dart-side `ParticleDecoder` from `flutter_rust_bridge` (see
`crates/particlecode-ffi/README.md`) stays the right tool for Android and for tests; on iOS
this package replaces the camera-to-Dart-to-Rust path, which copies each frame at least
once and has to leave the UI isolate.

## Threading rules

1. A `ParticleDecoder` is **not thread-safe**. All calls, including `payload`, `hint`,
   `rejectReason` and `takeReady()`, happen on one thread at a time. `ParticleCaptureScanner`
   guarantees this by confining its decoder to `scanner.queue`, a private serial queue. Do
   not read `scanner`'s decoder from elsewhere.
2. Never feed from the main thread: finding the code in a 1080p frame takes tens of
   milliseconds on a phone.
3. `feed(pixelBuffer:)` borrows the pixel memory for the duration of the call (the base
   address is locked read-only). Do not keep the pointers, and do not retain the sample
   buffer beyond the delegate callback longer than needed: the capture pool is small and a
   held buffer starves the camera.
4. `alwaysDiscardsLateVideoFrames = true` is what turns the serial queue into a depth-one
   mailbox: while a frame decodes, newer ones are dropped by AVFoundation, so latency does
   not build up. The format tolerates it: each data frame is on screen for 100 ms, which is
   about three camera frames, and the decoder uses no timing between frames.
5. `onResult` runs on the result queue (main by default) once per processed frame, up to 30
   per second. Throttle before crossing into Dart if the payload is large.
6. A `ParticleDecoder` can be created and destroyed on any thread, as long as no call is in
   flight. To start a new transfer, use `scanner.reset()` or make a new decoder.

## Why full-range NV12

* The ISP produces bi-planar 4:2:0 natively, so `420YpCbCr8BiPlanarFullRange` is free:
  no conversion pass, 1.5 bytes per pixel instead of 4.
* The anchor locator works on luma. Full range gives it 0...255; video range squeezes the
  same scene into 16...235, about 14% less contrast between the anchors and the
  background.
* The library takes NV12 as it is: Y plane, plus the interleaved CbCr plane passed as
  `u` and the same plane one byte later as `v`, with a chroma pixel stride of 2. Only a
  quarter-size scratch buffer is allocated inside the library to de-interleave chroma.
* Video range still works (it is accepted), it is just worse. BGRA works as a fallback, slower.

## Expected binary size

Measured on a Linux host for `aarch64-apple-ios` (release, thin LTO, debug info stripped):

* The static archive is about 7 MB per architecture, and the whole xcframework is therefore
  roughly 20 MB for iOS (device + two simulator architectures) and about 35 MB with the
  macOS slice. About 2 MB per slice compresses into the zip.
* What an app pays is much less: the archive holds one object per codegen unit plus the
  standard library, and the linker keeps only what the exported `pc_*` functions reach. The
  `__TEXT` and `__DATA` of all objects in the archive add up to under 1 MB for arm64, so
  expect well under a megabyte in the app binary.

The script prints the exact archive sizes; check the app's size with an Xcode archive
(App Thinning Size Report) before quoting numbers.
