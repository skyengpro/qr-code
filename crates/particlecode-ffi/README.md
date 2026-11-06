# particlecode-ffi

The Rust side of the Flutter plugin [`flutter/particle_code`](../../flutter/particle_code),
bridged with [flutter_rust_bridge](https://pub.dev/packages/flutter_rust_bridge) v2. Everything
is a thin reshaping of `particlecode-core` and `particlecode-vision` into types that cross the
Dart boundary cheaply.

| Module | What |
| --- | --- |
| `api` | opaque `ParticleEncoder` / `ParticleDecoder`, plain DTOs, motion helpers, `liveness_verify` |
| `json` | `frame_json(&FrameSpec)`, the serde form of a frame (debugging) |
| `frb_generated` | written by `flutter_rust_bridge_codegen`; do not edit |

## Build

The crate is a `cdylib`. The plugin's Native Assets hook
([`hook/build.dart`](../../flutter/particle_code/hook/build.dart)) runs `cargo build` for
each target the app is built for and bundles the library; there is no prebuilt binary and no
build script to run. Regenerate the bindings after changing `api`:

```sh
cd flutter/particle_code
flutter_rust_bridge_codegen generate   # reads flutter_rust_bridge.yaml
```

The crate, the Dart package `flutter_rust_bridge` and the code generator are pinned to the
same version (`=2.13.0`); the generated code refuses to load against any other.

## API

Opaque handles are the structs with private fields (`ParticleEncoder`, `ParticleDecoder`);
everything else is translated by value. Counts are `u32`, times are `f64` seconds, dots are
structure-of-arrays (`Float64List` / `Uint8List` in Dart), errors are `FfiError`.

Sender (sync calls are cheap; `frameAsync` builds a frame on a Rust thread):

```dart
final enc = ParticleEncoder(payload: cbor, config: EncoderConfig(...));
final info = enc.info();                              // profile, dots, block size, segments
final frame = await enc.frameAsync(i: i, unixSeconds: nowSecs);   // FrameDto for the painter
```

Receiver, from a camera frame held by the plugin's native code (pointer handoff):

```dart
final r = await dec.feedCameraFrame(frame: CameraFrameDto(
    width: w, height: h,
    yAddress: yAddr, yLen: yLen, uAddress: uAddr, uLen: uLen, vAddress: vAddr, vLen: vLen,
    yStride: yStride, uvStride: uvStride, uvPixelStride: uvPixelStride));
// release the native frame now, not before
// r.locked, r.anchors (image px, for the overlay), r.sharpness, r.liveness,
// r.progress, r.rejectReason, r.hint ("Too dark or too much glare ...")
```

`feed_camera_frame` reads the planes in place on a Rust worker thread: no copy into Dart, no
copy across the bridge. The caller guarantees that every address points at `len` readable
bytes that stay valid and unchanged until the returned future completes; Rust checks for null
and empty planes and that the lengths cover the dimensions and strides, and rejects the frame
otherwise. `uvPixelStride` is 2 for interleaved chroma (Android NV21-like `YUV_420_888`, iOS
NV12: pass the interleaved plane as `u` and the same address + 1 as `v`, or the reverse) and 1
for planar I420. `feed_yuv` (owned planes) and `feed_rgba` exist for tests and stills.

A frame that cannot be located or read is a normal result, not an exception: `progress` is
`rejected`, with `rejectReason` for logs and `hint` for the user. Payload bytes stream out of
`takeReady()` segment by segment; after `done`, everything taken equals the payload.

Symbol level, if the host classified the dots itself:

```dart
final p = await dec.feedSymbols(symbols: syms, confidences: conf);
// rejected / accepted(have, need, segmentsDone, segmentsTotal) / done(payload)
```

A handle must not be used by two calls at once; frb's opaque-handle locking enforces it.

## Painter contract

A painter draws a `FrameDto` (or the JSON `FrameSpec`) and must follow these rules; they are
what the receiver's calibration and classifier assume. The Rust renderer
(`particlecode-render`) is the reference.

**Coordinates.** Pitch units (the dot pitch `p`), origin at the frame center, `y` down,
angles in radians from +x towards +y. Scale to pixels by
`size_px / (2 * view_half_side)` with `view_half_side = frame_half_side + quiet_zone`
(both are fields of the frame). Do not hard-code any position or radius: dot radii
(`dot_r_small`, `dot_r_large`), `swatch_r`, `particle_r` and the anchor radii all come with
the frame.

**Colors.** Exact sRGB from `palette()`; never color-managed, never filtered. Dots are
anti-aliased filled circles with no stroke or gradient (an outline changes the blob area the
receiver measures). Hue `i` is `palette.hues[i]`; swatches are a hue index, `white` or
`black`.

**Draw order**, back to front:

1. background (`palette.background`, the whole square including the quiet zone);
2. anchors: ring (annulus `ring_in..ring_out`, `palette.anchor`), then the core
   (`core_r`, `palette.anchor`), then the pupil (`pupil_r > 0`, `palette.background`);
3. the 48 calibration swatches (radius `swatch_r`);
4. the 48 header dots (`header_dots`: parallel arrays `xs`, `ys`, `colors`, `sizes`);
5. the payload dots (`dots`), size class 0 = `dot_r_small`, 1 = `dot_r_large`;
6. the 6 particles (`palette.particle`, radius `particle_r`) inside the particle band.

**Timing.**

* The data frame changes on a **10 fps timer (`motion_constants().data_frame_interval`,
  0.1 s), independent of paint timing**. Never derive the frame index from the number of
  paints; a 120 Hz display repaints the same frame twelve times.
* Paint at the display rate. Animation is a pure function of the painter's clock `t` and the
  motion seed (`motion_seed(session_id)`): never accumulate offsets, so a dropped paint
  cannot drift the field.
* At a frame switch, **cross-fade 40 ms** (`crossfade_alpha(t_since_switch)` is the opacity
  of the new frame; the old frame is drawn below it). Particles glide over 60 ms
  (`particle_pose`).
* Dot motion is `dot_offset` / `dot_radius_scale` (drift at most 0.12 p, radius +-5%). Use
  the `*_batch` variants to animate a whole frame in one FFI call, or reimplement the curves
  from `particlecode_core::motion`. Optional layers (`anchor_scale`, `ripple_lighten`,
  `reveal_opacity`) follow the `MotionConfigDto` toggles.
* Particles show the liveness value of the wall-clock second: call
  `frame_at(i, unix_seconds)` with the real clock when displaying live.

**Screen.** Force **screen brightness to maximum** while a code is shown and restore it
afterwards; warn the user about night-light and color filters where the platform exposes them.
