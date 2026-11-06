# particle_code, iOS side

Pure Swift plugin (iOS 15+) over AVFoundation. It contains no Rust, links no Rust library and
needs no symbol keeping. The channel contract is `../PROTOCOL.md`.

* `ScannerController` owns the `AVCaptureSession` (1080p, NV12 full range, near continuous
  autofocus, 30 fps), the torch and the permission. For each camera frame it keeps the
  `CVPixelBuffer` retained and its base address locked, and sends a `frame` event on
  `particle_code/camera_frames` with the plane addresses and the `toPreview` affine. At most
  one frame is in flight; `releaseFrame(id)` (after Dart's `feedCameraFrame` finished) unlocks
  and drops it. Decoding, progress and hints are Dart/Rust.
* `PreviewView` is the `particle_code/preview` platform view: an `AVCaptureVideoPreviewLayer`
  with `resizeAspectFill`, rotated to the interface orientation. `toPreview` is computed
  through it (`layerPointConverted(fromCaptureDevicePoint:)`).
* `DisplayController` is `particle_code/display`: maximum brightness and idle timer off while a
  code is shown.
* `PrivacyInfo.xcprivacy` is the plugin's (empty) privacy manifest. The app declares its own
  `NSCameraUsageDescription`:

```xml
<key>NSCameraUsageDescription</key>
<string>The camera reads Particle Codes shown on another screen.</string>
```

The Rust library (`particlecode_ffi.framework`) is built and bundled by the Native Assets hook
`hook/build.dart`; flutter_rust_bridge opens it from Dart.

## Swift Package Manager and CocoaPods

Both are supported: `particle_code/Package.swift` (the default in current Flutter) and
`particle_code.podspec` compile the same `particle_code/Sources/particle_code` files.
