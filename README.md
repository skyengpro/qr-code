# Particle Code

**An optical code that looks like generative art.** Particle Code moves COSE/CBOR payloads
of a few hundred bytes up to several hundred kilobytes from one phone screen to another
phone's camera in a few seconds. It shows them as a living circular field of colored dots,
with a slow wave moving through it and a few drifting particles. It has no square finder
patterns and no black-and-white grid.

<p align="center">
  <img src="docs/images/hero.png" alt="A Particle Code frame: a circular field of blue, green, orange and magenta dots with three ring anchors and orbiting particles" width="420">
  <img src="docs/images/animated.gif" alt="The same code animated: dots breathe with a travelling wave while particles orbit" width="300">
</p>

Every frame carries one fountain-coded packet, protected by Reed-Solomon. The receiver
collects packets from any frames, in any order, until it can rebuild the payload, so a
missed frame costs one more loop of the animation, never a restart. Large payloads arrive
in segments, and each segment is verified and handed to the app as soon as it completes.

* Spec: [`docs/spec.md`](docs/spec.md)
* Implementation decisions and spec corrections: [`docs/design.md`](docs/design.md)
* Runnable examples: [`examples/`](examples/README.md)

## Shapes

The dot field can take any shape with enough ink: a disc (the default), a rounded square,
a heart, a hexagon, a ring, the letters A–Z, or your own SVG path. The anchors and halos
that the decoder locks onto are the same in every shape.

<p align="center">
  <img src="docs/images/shapes.png" alt="Contact sheet: heart, rounded square, hexagon, ring and the letters P, A, R, T as dot fields" width="820">
</p>

## How it works

| Layer | What it does | Where |
| --- | --- | --- |
| Geometry | Hex lattice in pitch units. 3 hero anchors at −90°/30°/150° plus a center anchor. Each hero anchor has a halo of 16 calibration swatches and 16 header dots. Particles orbit outside all ink. | `core::layout` |
| Symbols | 4 hues × 2 sizes = 3 bits per dot. The receiver calibrates the hues from the swatches in every frame. | `core::symbol`, `core::profile` |
| Header | 96 bits (session, profile, shape, segment, packet seed, …) in RS(24,16) over GF(64), on the 48 halo dots | `core::header`, `core::rs` |
| Body | One RaptorQ symbol per frame. It is split into RS(255,191) codewords (RS(255,159) for print), interleaved and whitened across the field. | `core::body` |
| Fountain | The payload is cut into segments (8 KiB by default), each a RaptorQ block with its own CRC-32. Each pass sends K + 2 packets per segment. | `core::session`, `core::receiver` |
| Liveness | The 6 particle angles carry the packet seed and a rolling HMAC of the session key and the current second | `core::liveness` |
| Motion | Breathing wave, flow field, ripple, heartbeat, trails, reveal, cross-fade. These are pure functions of time and seed, kept inside the decoder's sampling budget. | `core::motion` |
| Vision | Finds the anchors, fits the homography and refines it with the lattice phase, calibrates color per frame, then samples and classifies the dots with confidences. Uncertain dots are passed to Reed-Solomon as erasures. | `particlecode-vision` |

| Profile | Dots across | Disc dots | Bytes / frame | 6 KB payload |
| --- | --- | --- | --- | --- |
| S | 48 | 1,821 | 506 | 13 + 2 frames |
| M (default) | 72 | 4,410 | 1,229 | 5 + 2 frames |
| L | 96 | 8,076 | 2,256 | 3 + 2 frames |

At profile M and 10 fps that is about 12 KB/s. A 6 KB COSE_Sign1 transfers in half a second
of clean frames. `cargo pc info` prints the exact numbers for every shape.

## Decoding through a (synthetic) camera

<p align="center">
  <img src="docs/images/degraded.png" alt="The code as a camera would see it: tilted, rotated, color-cast, blurred and noisy on a dark desk" width="480">
</p>

The vision decoder is tested against synthetic camera views: tilt, rotation, blur, noise,
color casts, glare, lens distortion and lighting gradients. Typical results, measured in
release on 1080p frames at 6–9 px per pitch:

| Condition | Lock rate | Body symbol error rate |
| --- | --- | --- |
| Blur σ 1 px, noise σ 4, 15° tilt, gain 0.8 | 100 % | 0.000 % |
| Breathing wave animation (0.12 p drift) | 100 % | ≤ 0.006 % |
| Warm / dim / night-light color casts | 100 % | ≤ 0.02 % |
| Lighting gradient ±30 %, vignette | 100 % | ≤ 0.3 % |
| Large glare patch | 100 % | 1–4 % (mostly flagged as erasures) |

On one 1920×1080 frame, `locate` takes about 4 ms and `read` 30–40 ms (single thread).
These numbers come from synthetic images only. Real phone photos (JPEG, moiré, rolling
shutter) are the next step; see the build plan in the spec.

## Workspace

```
crates/particlecode-core     no_std + alloc: layout, shapes, header, RS, body codec, RaptorQ sessions, liveness, motion
crates/particlecode-vision   camera image -> header + symbols + liveness (locate, read)
crates/particlecode-render   FrameSpec -> pixels (tiny-skia): PNG, animated GIF, SVG, PNG sequence + manifest; synthetic camera
crates/particlecode-shapes   shape tool: SVG path -> lattice bitmaps, rule checks, synthetic validation, shapes.bin
crates/particlecode-cli      the `particlecode` command
crates/particlecode-ffi      flutter_rust_bridge API (the Flutter sender) and a C ABI for other hosts
crates/particlecode-jni      Android JNI shim over the decoder (CameraX buffers read in place)
crates/particlecode-wasm     browser sender: payload -> animated RGBA frames for a <canvas> (wasm-bindgen); scripts/build-wasm.sh
android/                     Kotlin wrapper + CameraX analyzer (library module); scripts/build-android.sh
apple/                       ParticleCodeKit Swift package + podspec over ParticleCodeFFI.xcframework; scripts/build-xcframework.sh
flutter/particle_code/       Flutter plugin (sender over flutter_rust_bridge, scanner on the native pipelines) + example app
examples/                    runnable examples and sample CBOR payloads
fuzz/                        cargo-fuzz target for the header decoder
```

## Quick start

```sh
# Encode a payload to a PNG, an animated GIF, an SVG poster or a PNG sequence + manifest.json
cargo pc encode examples/data/credential.cbor -o out/credential.gif --all-moods
cargo pc encode examples/data/ticket.cbor -o out/poster.svg --static --prefer-shape heart
cargo pc encode examples/data/credential.cbor -o out/frames --shape P

# Decode frames back into the payload
cargo pc decode out/frames -o out/decoded.cbor

# Capacities of every shape and profile
cargo pc info

# Bring your own shape
cargo run --release -p particlecode-shapes -- check crates/particlecode-shapes/assets/star.svg
cargo run --release -p particlecode-shapes -- build crates/particlecode-shapes/assets/star.svg --id 32 -o shapes.bin
```

`cargo pc` is an alias for `cargo run --release -p particlecode-cli --`. More in
[`examples/README.md`](examples/README.md).

### As a library

```rust
use particlecode_core::{DisplayInfo, Progress, Receiver, Session, SessionConfig};

let display = DisplayInfo { width_px: 1080, height_px: 2400, width_mm: 68.0, height_mm: 151.0 };
let session = Session::new(&payload, display, &SessionConfig::default())?;

// Sender: draw `session.frame(i)` (a plain-data FrameSpec) every 100 ms.
// Receiver: per camera frame
let mut rx = Receiver::new();
if let Some(lock) = particlecode_vision::locate(&yuv.y) {
    let (header, body, _liveness) =
        particlecode_vision::read(&yuv, &lock, &|p, s| rx.dot_order(p, s))?;
    if let Progress::Done(bytes) = rx.feed_decoded(&header, &body) { /* … */ }
    // `rx.take_ready()` streams verified segments in order while the rest arrives.
}
```

## Flutter

[`flutter/particle_code`](flutter/particle_code) is a Flutter plugin for Android and iOS. The
sender encodes through the Rust library over [flutter_rust_bridge](https://pub.dev/packages/flutter_rust_bridge)
(frames are built on Rust threads) and paints the animated code in Dart; the scanner is the native camera pipeline (CameraX, AVFoundation) and only reports progress and the
final payload to Dart.

<img src="docs/images/flutter_send.png" alt="The example app's Send tab showing a heart-shaped code" width="260">

```dart
import 'package:particle_code/particle_code.dart';

// Once, before using the sender (loads the Rust library).
await ParticleCode.init();

// Sender: show a payload as an animated field of dots.
final encoder = ParticleEncoder.create(
  cborBytes,
  options: const EncoderOptions(profile: CodeProfile.m, shape: Shape.heart),
);
ParticleCodeView(encoder: encoder, size: 320, motion: const MotionConfig());

// Scanner: camera preview with anchors and progress, then the payload.
final scanner = ParticleScanner();
scanner.events.listen((e) {
  if (e is ScanDone) handle(e.payload);
});
if (await scanner.requestPermission()) await scanner.start();
// ... ParticleScannerPreview(scanner: scanner)
```

Build the native libraries first (`scripts/build-android.sh`, `scripts/build-xcframework.sh
--flutter`); see the [plugin README](flutter/particle_code/README.md). The
[example app](flutter/particle_code/example) has a Send tab (sample ticket and credential or free
text, profile, shape, mood) and a Scan tab (camera, progress, result sheet), and
`.github/workflows/flutter.yml` builds it for Android and iOS on every pull request.

## Web

[`crates/particlecode-wasm`](crates/particlecode-wasm) draws the animated code in a browser
`<canvas>`, so a page can show a code that the Flutter scanner reads. It is the Flutter
`ParticleCodeView` ported to Rust: the same frame schedule (data frame from elapsed time, 40 ms
cross-fade, the liveness second from the wall clock) over `particlecode_render::render_animated`,
so the pixels come from the one painter. Rust owns the clocks and the cross-fade; the page owns
the timer and only blits.

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked wasm-bindgen-cli --version <the pin in Cargo.toml>   # exact match required
scripts/build-wasm.sh out          # out/particlecode_wasm.js + out/particlecode_wasm_bg.wasm (~675 KB)
node crates/particlecode-wasm/tests/smoke.mjs out
```

```js
import { initSync, ParticleCanvas, wasm_memory } from "./out/particlecode_wasm.js";
initSync({ module: wasmBytes }); // or: await init() to fetch particlecode_wasm_bg.wasm

// payload bytes, profile "s" | "m" | "l", data frames per second, Unix seconds of frame 0, animated?
const code = new ParticleCanvas(payload, "s", 10, Date.now() / 1000, true);
const t0 = performance.now();
function frame() {
  const size = canvas.width; // square, in pixels
  if (code.tick(performance.now() - t0, Date.now(), size)) {
    const px = new Uint8ClampedArray(wasm_memory().buffer, code.pixels_ptr(), code.pixels_len());
    ctx.putImageData(new ImageData(px, size, size), 0, 0); // zero-copy; re-read the pointer every time
  }
  requestAnimationFrame(frame);
}
frame();
```

`tick` returns `false` when nothing changed (a static code between data frames), so a page can skip the
blit. A profile S frame at 656 px costs about 13 ms in Node, profile M about twice that; draw at no more
than 30 fps and stop the loop while the tab is hidden. Pass `motion = false` for a static code (for
`prefers-reduced-motion`). `code.free()` releases the session.

## Development

The toolchain is pinned in `rust-toolchain.toml`. Before every commit, run what CI runs:

```sh
cargo fmt --all --check
cargo lint          # clippy --workspace --all-targets --all-features -D warnings (pedantic)
cargo t             # test --workspace --all-features
cargo deny check    # licenses, advisories, bans, sources (deny.toml)
cargo build -p particlecode-core --no-default-features   # core stays no_std
```

The repo ships Claude Code skills in `.claude/skills/`: `rust-practices`,
`cargo-workflow`, `cargo-deny` and `particlecode-format`. See [`CLAUDE.md`](CLAUDE.md).

## License

MIT, see [`LICENSE`](LICENSE).
