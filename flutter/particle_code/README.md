# particle_code

Particle Code for Flutter. A phone screen shows an animated circular field of colored
dots that carries a COSE/CBOR payload; another phone's camera scans it.

* **Sender**: encode a payload with the Rust library (through
  [flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/)) and paint the animated
  code in pure Dart (`ParticleCodeView`).
* **Scanner**: a native camera pipeline (CameraX on Android, AVFoundation on iOS) decodes
  with the same Rust library. Camera frames never reach Dart; only small progress events
  and, at the end, the payload do.

The wire contract between the Dart and native sides is in [`PROTOCOL.md`](PROTOCOL.md).

## Show a code

Initialize the library once, before anything else of the sender, usually in `main()`:

```dart
import 'package:flutter/widgets.dart';
import 'package:particle_code/particle_code.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await ParticleCode.init();   // required; idempotent
  runApp(const MyApp());
}
```

`ParticleEncoder.create` (and `livenessVerify`) throw a `StateError` that says so if `init`
has not completed. Pass `ParticleCode.init(library: ...)` an `ExternalLibrary` to load the
native library yourself.

```dart
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

### Frame generation stays off the UI isolate

`encoder.frame(i)` builds a frame synchronously. With the release native library that is
roughly 0.3 ms (profile M) and 0.4 ms (L) on a desktop core: Reed-Solomon coding in Rust
and a copy of six typed arrays into Dart (positions, lattice cells, hues and sizes), with no
JSON and no per-dot objects. A mid-range phone is two to three times slower.
`encoder.frameAsync(i)` does the Rust part on flutter_rust_bridge's thread pool and
completes a `Future`, so the UI isolate only pays for the call and the conversion
(about 0.1 ms per frame on the same machine).

`ParticleCodeView` uses `frameAsync`:

* Frames `clockIdx .. clockIdx + 2` are kept in flight. The liveness second is predicted
  from the data clock: the second at which the frame will be on screen, not the second at
  which it was requested. Results for a frame that is already past are dropped.
* The first frame is built synchronously at mount, so there is never an empty first paint.
* If a frame is late, the view holds the current one (up to about 250 ms, or 3 s / fps),
  then builds the due frame synchronously. If `frameAsync` fails, it silently uses the
  synchronous path.
* Disposing the encoder while calls are in flight is safe: the native object is released
  when the last call completes.

`ParticleCodeView.useBackgroundIsolate` is deprecated and has no effect.

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

The sender's Rust API (`crates/particlecode-ffi/src/api`) is bound to Dart with
flutter_rust_bridge 2.13; the generated Dart is committed in `lib/src/rust/` and must not be
edited by hand. Regenerate it after changing the Rust API (the generator version must equal
the `flutter_rust_bridge` package and crate version, see `flutter_rust_bridge.yaml`; CI
fails on a diff):

```sh
cd flutter/particle_code
cargo install flutter_rust_bridge_codegen --version 2.13.0 --locked
flutter_rust_bridge_codegen generate
```

The plugin does not use flutter_rust_bridge's cargokit build integration. Nothing is built
when you `flutter build`; the Rust libraries are prebuilt by our scripts from the workspace
root:

```sh
scripts/build-android.sh          # libparticlecode_jni.so per ABI into android/.../jniLibs
scripts/build-xcframework.sh --flutter   # ParticleCodeFFI.xcframework into ios/particle_code/Frameworks (needs Xcode)
```

* Android: `ParticleCode.init()` opens `libparticlecode_jni.so`, which exports both the JNI
  scanner and the flutter_rust_bridge symbols.
* iOS: the xcframework is statically linked and the Rust side keeps the bridge symbols alive;
  `ParticleCode.init()` uses the process image (`ExternalLibrary.process`). The plugin
  supports both Swift Package Manager (`ios/particle_code/Package.swift`) and CocoaPods
  (`ios/particle_code.podspec`); with SwiftPM, set the app's Release/Profile
  `STRIP_STYLE = non-global` (see `ios/README.md`).
* Desktop and tests: `$PARTICLECODE_LIB`, else the newest
  `target/{debug,release}/libparticlecode_ffi.{so,dylib}` found by searching upward from the
  working directory. Run `cargo build -p particlecode-ffi` first.

`init` fails with `ParticleCodeException` if the library is missing or was built from a
different Rust API than the generated Dart (the bridge compares content hashes).

## Development

```sh
cargo build -p particlecode-ffi        # host library for the tests
flutter pub get
flutter analyze
flutter test
```

Tests cover the encoder (sync, async, `FrameSpec.fromDto` against the JSON form) against the
real library, a bit-level parity check of the Dart
motion port against values computed by `particlecode-core` (fixtures in
`test/fixtures/`, generator `gen_motion_reference.rs`), pixel-exact painter checks,
the view's timing and brightness behavior, and the scanner channels with mocks.

The motion port uses 64-bit integer arithmetic and therefore targets the Dart VM
(Android, iOS, desktop); the native bridge rules out the web anyway.
