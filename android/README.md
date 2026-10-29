# Particle Code for Android (CameraX + JNI)

An Android library (`com.vaam.particlecode`) that scans Particle Code frames from the camera.
A CameraX `ImageAnalysis` analyzer hands the three `ImageProxy` planes (direct `ByteBuffer`s)
straight to Rust on a background thread. Rust decodes them in place; only a few integers come
back. Nothing is copied through the JVM heap or Dart.

Why: copying 1080p YUV frames (about 3 MB each) at 30 fps through Dart, or even through the
JVM, is too costly on low-end phones. With `STRATEGY_KEEP_ONLY_LATEST` the analyzer always
works on the newest frame and drops the rest, so the pipeline never queues.

```
CameraX ImageProxy (YUV_420_888)
   │  planes[0..3].buffer, rowStride, pixelStride        (direct ByteBuffers, no copy)
   ▼
ParticleAnalyzer ──► ParticleScanner ──► NativeBridge ──► libparticlecode_jni.so (Rust)
   │                                                        locate, read, Reed-Solomon, RaptorQ
   ▼  ScanResult (progress, lock, hint, anchors)
Listener ──► (Flutter plugin) EventChannel: progress only
```

| Path | What |
| --- | --- |
| `particlecode/src/main/kotlin/.../NativeBridge.kt` | the `external` functions and the array layout constants |
| `.../ParticleScanner.kt` | `AutoCloseable` wrapper, `ScanResult`, `Progress`, `Liveness`, `Hint`, `feed(...)` overloads |
| `.../ParticleAnalyzer.kt` | CameraX `ImageAnalysis.Analyzer`, `newImageAnalysis()`, `refocusCenter()` |
| `../crates/particlecode-jni` | the Rust side (`libparticlecode_jni.so`) and its JVM tests |
| `../scripts/build-android.sh` | cross-compiles the library into `particlecode/src/main/jniLibs` |

## Build

Prerequisites: Rust (rustup), the Android SDK with an NDK (r27 or newer; r28 aligns pages to
16 KB by default, the script adds the flag for older ones), JDK 17, Gradle 8.9 or newer.

```sh
cargo install --locked cargo-ndk
scripts/build-android.sh            # release; --debug for a faster debug build
```

This runs `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 ... build -p particlecode-jni` and
writes `particlecode/src/main/jniLibs/<abi>/libparticlecode_jni.so`. The script finds the NDK
from `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, `ANDROID_NDK_LATEST_HOME` or
`$ANDROID_HOME/ndk/<newest>`.

Then package the AAR (there is no Gradle wrapper checked in; use a Gradle install, or
`gradle wrapper` once locally):

```sh
gradle -p android :particlecode:assembleRelease
# android/particlecode/build/outputs/aar/particlecode-release.aar
```

CI (`.github/workflows/android.yml`) does both on every pull request and uploads the AAR.
`cargo test -p particlecode-jni` runs the JNI exports under a real JVM without an Android
SDK (see below).

## Use from an Android app

```kotlin
// build.gradle.kts: implementation(files("libs/particlecode-release.aar")),
// plus camera-camera2 and camera-lifecycle for the camera itself.

val scanner = ParticleScanner()                       // one per scan screen
val analysis = ParticleAnalyzer.newImageAnalysis()    // KEEP_ONLY_LATEST, ~1920x1080, YUV_420_888
val executor = Executors.newSingleThreadExecutor()

ParticleAnalyzer.attach(analysis, executor, scanner, object : ParticleAnalyzer.Listener {
    override fun onResult(result: ScanResult) {
        // analyzer thread: forward progress, never frames
        when (val p = result.progress) {
            is Progress.Accepted -> showProgress(p.fraction, p.segmentsDone, p.segmentsTotal)
            is Progress.Done -> handlePayload(p.payload)      // COSE/CBOR bytes
            is Progress.Rejected -> showHint(result.hint)     // localize Hint in the app
        }
    }
})
val camera = cameraProvider.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, analysis)
ParticleAnalyzer.refocusCenter(camera.cameraControl)

// when the screen goes away
analysis.clearAnalyzer(); executor.shutdown(); scanner.close()
```

Verify liveness if you hold the session key: `(result.liveness as? Liveness.Read)?.verify(key,
System.currentTimeMillis() / 1000)`.

### Focus and exposure at close range

The sender's screen is 15 to 30 cm from the camera. Continuous autofocus (the CameraX default)
usually settles but can hunt at close range, and a blurred frame loses the dot colors and sizes.
Leave AF on; call `ParticleAnalyzer.refocusCenter(cameraControl)` when the camera starts and
again every couple of seconds while `ScanResult.locked` is false (it triggers a one-shot AF and
AE on the image center, then returns to continuous AF). If a phone cannot focus that close, the
hint `HoldSteady` with low `sharpness` means "move back a little". The sender must run at full
screen brightness (the app on the other side takes care of that).

## Use from a Flutter plugin

Frames must never cross into Dart. The plugin owns the camera on the Android side and talks to
Dart only about progress:

* **EventChannel** `particlecode/progress`: the `Listener` posts to the main thread
  (`Handler(Looper.getMainLooper())`) and sends a small map per change (for example `{locked,
  have, need, segmentsDone, segmentsTotal, hint}`) or the final payload as a `Uint8List`. The
  analyzer already throttles callbacks to progress changes plus one per 100 ms.
* **MethodChannel** `particlecode`: `start`, `stop`, `reset`, `refocus`. `start` creates the
  `ParticleScanner`, binds the `ImageAnalysis` (and a `Preview` if the app shows one; share its
  `SurfaceProducer` or use a texture entry) and `stop` closes everything.
* Pigeon works as well: generate a `ScannerHostApi` (methods above) and a `ScannerFlutterApi`
  (`onProgress`, `onDone`) and keep the same rule: only progress and the final payload cross.
* The overlay (anchors) can be drawn natively over the `Preview`, or the anchor list (at most 4
  points, in image pixels of the unrotated frame; `ImageInfo.rotationDegrees` says how to rotate)
  can be sent with the progress event.

## Threading and ownership

* A native handle is used by **one thread at a time**. `ParticleScanner` serializes every call
  with a lock, and the Rust side also serializes calls per handle, so a violation cannot corrupt
  memory; it only costs latency. In the intended design only the analyzer's single-thread
  executor feeds the scanner.
* `ParticleScanner` owns the handle. Call `close()` exactly when the scan screen is done
  (idempotent); a closed scanner throws `IllegalStateException`. Handles are table keys, not
  pointers, so a stale one can never touch freed memory.
* The analyzer **always closes the `ImageProxy`** (in a `finally`). Do not keep references to
  the plane buffers after `analyze` returns: CameraX recycles the memory. Rust reads it only
  during the call.
* `Listener` callbacks run on the analyzer thread. Do not block there: the next frame waits
  (and older ones are dropped).
* `NativeBridge` never throws for bad arguments; it returns `ERR_*` codes, which
  `ParticleScanner` maps to exceptions (`IllegalArgumentException`, `IllegalStateException`,
  `ParticleCodeException`). After `ERR_PANIC` (a bug) discard the scanner.
* Buffers are read from their base address for their full capacity, ignoring position and
  limit; `ParticleScanner` slices a buffer whose position is not 0 and falls back to a copying
  path for non-direct buffers.

## The native array layout

`NativeBridge.nativeFeedYuv` fills two preallocated arrays, so a frame costs no JVM allocation.
The indices are `const val`s in `NativeBridge.kt` and constants in
`crates/particlecode-jni/src/layout.rs`; a Rust test fails if they differ.

`out: IntArray(13)`: `0` locked, `1` profile (0 S, 1 M, 2 L, -1), `2` shape (-1), `3` have,
`4` need, `5` segmentsDone, `6` segmentsTotal, `7` livenessRead, `8` livenessConsistent,
`9` seedLow, `10` rolling, `11` anchorCount, `12` hint code.

`outGeom: FloatArray(9)`: `0..8` anchors as `x, y` pairs (top, lower right, lower left, center;
unused pairs 0), `8` sharpness.

Status: `0` rejected, `1` accepted, `2` done, `-1` null/freed handle or null argument, `-2`
panic, `-3` invalid sizes/strides/arrays, `-4` not a direct buffer, `-5` JNI failure.

## Tests

`cargo test -p particlecode-jni` renders a transfer, photographs it with the synthetic camera,
lays the frames out as NV21 with padded rows, and runs a Java driver against the real cdylib.
The Java class `com.vaam.particlecode.NativeBridge` there has the same name and `static native`
signatures as the Kotlin object, so the same `Java_com_vaam_particlecode_NativeBridge_*` symbols
are resolved. The driver wraps the bytes in direct buffers (U and V as overlapping views of the
interleaved VU buffer, pixel stride 2), runs to `Done`, compares the payload, and checks that
null/freed handles, short or heap buffers and bad strides or arrays return error codes without
crashing the JVM (it runs with `-Xcheck:jni`). Without a JDK on `PATH` the test prints a note
and returns.
