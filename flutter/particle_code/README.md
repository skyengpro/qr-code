# particle_code

Particle Code for Flutter. A phone screen shows an animated circular field of colored
dots that carries a COSE/CBOR payload; another phone's camera scans it.

* **Sender**: encode a payload with the Rust library (`dart:ffi`) and paint the animated
  code in pure Dart (`ParticleCodeView`).
* **Scanner**: a native camera pipeline (CameraX on Android, AVFoundation on iOS) decodes
  with the same Rust library. Camera frames never reach Dart; only small progress events
  and, at the end, the payload do.

The wire contract between the Dart and native sides is in [`PROTOCOL.md`](PROTOCOL.md).

## Show a code

```dart
import 'package:particle_code/particle_code.dart';

final encoder = ParticleEncoder.create(
  cborBytes, // Uint8List, typically a COSE_Sign1 / CBOR payload
  options: const EncoderOptions(
    profile: CodeProfile.m,
    shape: Shape.heart,        // Shape.disc, Shape.letter('A'), Shape.ring, ...
    fps: 10,
  ),
);

// In build():
ParticleCodeView(
  encoder: encoder,
  size: 320,                      // or omit to fill the available square
  motion: const MotionConfig(),   // breathing; MotionConfig.all(), or null for a static code
  showing: !dialogOpen,           // false releases the brightness lock
);

// When done (after the view left the tree):
encoder.dispose();
```

`ParticleEncoder.create` throws `ParticleCodeException` with the native error text for bad
options or payloads that do not fit. `encoder.info` reports the profile, shape, dots per
frame, segment count and frame geometry; `encoder.key` is the 32-byte session key to share
out of band with whoever verifies liveness (`livenessVerify`). Fixing `sessionId`, `key`
and `clockOriginUnix` in `EncoderOptions` makes frames deterministic.

What the view does for you:

* The data frame changes every `1000 / fps` ms, computed from elapsed time, never from the
  number of paints.
* The particles show the live wall-clock liveness value (`frame(i, unixSeconds: now)`).
* While mounted and `showing`, the platform forces maximum screen brightness and keeps the
  screen on (`particle_code/display`); both are restored when the view is disposed or
  `showing` turns `false`.
* Motion follows the core `motion` module exactly (a Dart port, parity-tested against the
  Rust functions): breathing or flow-field drift, radius pulse, 40 ms cross-fade between
  data frames, particle glide, and the optional anchor heartbeat, ripple, reveal, shimmer
  and particle trails.

Painting is `ParticleCodePainter`, a plain `CustomPainter` you can use yourself (for
example to render into an image). Colors come from `ParticleCodePalette` and must not be
altered or color-managed: the receiver's classifier depends on them.

### Frame generation runs off the UI isolate

Building a frame is Reed-Solomon coding in Rust plus JSON that has to be read in Dart: with
the release native library about 7 ms (profile M) and 15 ms (L) when parsed with
`jsonDecode`, and 3.3 ms / 5.3 ms with the package's streaming dot parser, on a desktop
core; a mid-range phone is two to three times slower. Doing that on the UI isolate every
100 ms would drop a 60 fps frame at every data frame, so `ParticleCodeView` (default
`useBackgroundIsolate: true`) works like this:

* A long-lived worker isolate builds its own `ParticleEncoder` from the same payload,
  options and pinned identity (session id, key), so its frames are identical to
  `encoder.frame(i, unixSeconds: s)`. It stays two data frames ahead of the clock.
* Frames come back as a few typed arrays (`FrameDots` is structure-of-arrays) and are
  rebuilt on the UI isolate in tens of microseconds.
* The liveness second is predicted from the data clock: the second at which the frame will
  be on screen, not the second at which it was built.
* The first frames (while the isolate starts) are built synchronously; if the worker cannot
  start, fails, or is more than about 250 ms late with a frame, the view uses the
  synchronous path, so it always works. If the worker is only slightly late, the view holds
  the current frame rather than blocking the UI.
* The worker is disposed with the view (and while `showing` is `false`).

Isolates of an app share one heap, and a young-generation collection pauses all of them.
Allocation-heavy parsing in the worker would therefore still hitch the UI, which is why
the dots are parsed from the library's UTF-8 buffer straight into typed arrays (no
per-dot `Map`s) with an exact fast path for numbers; `ParticleEncoder.frame` uses the same
parser, so the synchronous path is cheaper too. Anything unexpected in the JSON makes it fall
back to `jsonDecode`.

## Scan a code

```dart
final scanner = ParticleScanner();

scanner.events.listen((event) {
  switch (event) {
    case ScanFrame f:    // ~10 per second and whenever progress changes
      print('${f.have}/${f.need}  locked=${f.locked}  hint=${f.hint}');
    case ScanSegment s:  // newly verified in-order payload bytes (streaming)
      sink.add(s.bytes);
    case ScanDone d:     // whole payload; decoding pauses until reset()
      handle(d.payload, d.sessionId);
    case ScanError e:    // camera failure after start()
      print('${e.code}: ${e.message}');
  }
});

if (await scanner.requestPermission()) {
  try {
    await scanner.start(torch: false);
  } on ParticleScannerException catch (e) {
    // e.error: permissionDenied | noCamera | cameraError | unsupported | unknown
  }
}

// Layout:
ParticleScannerPreview(scanner: scanner)  // camera + lock anchors + progress ring

await scanner.reset();   // scan another code, camera keeps running
await scanner.stop();
scanner.dispose();
```

`ParticleScannerPreview` is a native view (`AndroidView` / `UiKitView`, view type
`particle_code/preview`); other platforms show a placeholder. Pass `overlayBuilder` to draw
your own overlay from the latest `ScanFrame`: `frame.anchors` are normalized (0..1) preview
coordinates, so multiply by the widget size (`ScanAnchorsPainter.toWidget`). `ScanProgressRing`
draws segments and symbols held on its own, wherever you want it. `ScanFrame.hintCode`
(`ScanHint`) is the localizable form of the advice text in `ScanFrame.hint`.

## Platform setup

**Android** (`android/app/src/main/AndroidManifest.xml`):

```xml
<uses-permission android:name="android.permission.CAMERA" />
<uses-feature android:name="android.hardware.camera" android:required="false" />
```

`minSdk` is 24. `requestPermission()` shows the runtime prompt.

**iOS** (`ios/Runner/Info.plist`):

```xml
<key>NSCameraUsageDescription</key>
<string>The camera reads Particle Codes.</string>
```

Only the scanner needs the camera permission; showing a code needs none.

## Native libraries

Nothing is built when you `flutter build`; the Rust libraries are prebuilt from the
workspace root:

```sh
scripts/build-android.sh          # libparticlecode_jni.so per ABI into android/.../jniLibs
scripts/build-xcframework.sh      # ParticleCodeFFI.xcframework for iOS (needs Xcode)
```

* Android: Dart opens `libparticlecode_jni.so`, which exports both the JNI scanner and the
  `pc_*` encoder C functions.
* iOS: the xcframework is statically linked and its `pc_*` symbols are kept alive by the
  plugin's `SymbolKeeper`; Dart uses `DynamicLibrary.process()`.
* Desktop and tests: `$PARTICLECODE_LIB`, else the newest
  `target/{debug,release}/libparticlecode_ffi.{so,dylib}` found by searching upward from the
  working directory. Run `cargo build -p particlecode-ffi` first.

The package checks `pc_abi_version` against the version it was written for and throws
`ParticleCodeException` on a mismatch.

## Development

```sh
cargo build -p particlecode-ffi        # host library for the tests
flutter pub get
flutter analyze
flutter test
```

Tests cover the FFI encoder against the real library, a bit-level parity check of the Dart
motion port against values computed by `particlecode-core` (fixtures in
`test/fixtures/`, generator `gen_motion_reference.rs`), pixel-exact painter checks,
the view's timing and brightness behavior, and the scanner channels with mocks.

The motion port uses 64-bit integer arithmetic and therefore targets the Dart VM
(Android, iOS, desktop); `dart:ffi` rules out the web anyway.
