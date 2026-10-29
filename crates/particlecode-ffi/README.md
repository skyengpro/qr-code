# particlecode-ffi

The foreign-function surface of Particle Code. Everything is a thin reshaping of
`particlecode-core` (and, from phase 2, `particlecode-vision`) into types that cross a
language boundary cheaply.

| Module | What | Consumers |
| --- | --- | --- |
| `api` | `flutter_rust_bridge` v2 style API: opaque `ParticleEncoder` / `ParticleDecoder`, plain DTOs, motion helpers, `liveness_verify` | Flutter (generated Dart), Tauri backend (plain Rust) |
| `capi` + `include/particlecode.h` | C ABI over the same handles, JSON in/out | anything that can load a C library |
| `json` | `frame_json(&FrameSpec)`, the serde form of a frame | webview painters, debugging |

The crate builds as `lib`, `cdylib` and `staticlib`. `unsafe` exists only in `capi`, and
every block carries a `// SAFETY:` comment.

## Flutter (flutter_rust_bridge v2)

This crate does **not** depend on `flutter_rust_bridge`; the generator reads the plain
Rust in `crate::api` from the Flutter repository:

```sh
flutter_rust_bridge_codegen generate \
    --rust-input crate::api \
    --rust-root path/to/crates/particlecode-ffi \
    --dart-output lib/src/rust
```

Opaque handles are the structs with private fields (`ParticleEncoder`, `ParticleDecoder`);
everything else is translated by value. Conventions that keep the generated Dart small:
counts are `u32`, times are `f64` seconds, dots are structure-of-arrays (`Float64List` /
`Uint8List` on the Dart side), the error type is `FfiError` (`message()` for UI text).

Sender:

```dart
final enc = await ParticleEncoder.newInstance(payload: cbor, config: EncoderConfig(...));
final info = enc.info();                       // profile, dots, block size, segments
final frame = enc.frameAt(i, unixSeconds: nowSecs);   // FrameDto for the painter
```

Receiver (phase 1, symbol level; phase 2 adds `feedYuv` for camera planes):

```dart
final dec = ParticleDecoder();
final p = dec.feedSymbols(symbols: syms, confidences: conf);
// p is ProgressDto.rejected / .accepted(have, need, segmentsDone, segmentsTotal) / .done(payload)
final bytes = dec.takeReady();                 // payload streamed segment by segment
```

Keep frb's default async mode (a Rust thread pool) for `frame*` and
`feed*`: building a frame at profile L is a few milliseconds of Reed-Solomon, which does not
belong on the UI isolate. Prefetch the next frame while the current one is shown.

## Tauri

Two options, to be picked after measuring which keeps 30 fps on the vendor's hardware:

1. **Rust dependency (default).** Add `particlecode-ffi` to the backend crate and call
   `api::encoder::ParticleEncoder` / `api::decoder::ParticleDecoder` from `#[tauri::command]`
   functions. For the sender, expose one command per frame returning `FrameDto` (it is
   `Serialize`) or `frame_json`; the webview paints it on a canvas. The `ParticleEncoder`
   is `Send + Sync`: keep it in Tauri state.
2. **wasm (later).** `particlecode-core` is `no_std + alloc`, so the decoder can be built for
   `wasm32-unknown-unknown` and run in the webview next to the camera stream; that needs a
   wasm-bindgen wrapper and is not part of this crate yet.

The C ABI (`include/particlecode.h`) is also available when a Tauri plugin prefers to load
the `cdylib`.

## C ABI

```c
char *err = NULL;
PcEncoder *enc = pc_encoder_new(payload, len, "{}", 2, &err);   // err via pc_string_free
char *json = pc_encoder_frame_json(enc, 0);                      // pc_string_free
...
PcDecoder *dec = pc_decoder_new();
int32_t st = pc_decoder_feed_symbols(dec, syms, NULL, n);  // PC_REJECTED / PC_ACCEPTED / PC_DONE
```

* NULL pointers are checked and reported (NULL return or a negative `PC_ERR_*`); panics are
  caught at the boundary (`PC_ERR_PANIC`).
* The config is JSON; every field is optional and `{}` (or NULL with length 0) means
  defaults:

  ```json
  {
    "segment_kb_log2": 3, "profile": "M", "fps": 10, "is_static": false,
    "shape": {"kind": "require", "id": 0},
    "repair_per_pass": 2, "min_pitch_mm": 1.6,
    "display": {"width_px": 1080, "height_px": 2400, "width_mm": 68.0, "height_mm": 151.0},
    "session_id": 4660, "key": [1, 2, 3, "... 32 numbers ..."],
    "clock_origin_unix": 1700000000
  }
  ```

  `profile` is `"S"`, `"M"`, `"L"` or `null` (pick from `display`). `shape` is `require`,
  `prefer` (`id`) or `auto` (`ids`). Omitted `session_id`, `key` and `clock_origin_unix` are
  random / the wall clock; fixing all three makes frames deterministic.
* Frame JSON is the serde form of the core `FrameSpec` (colors as indices, lattice
  coordinates `q`/`r` included). The `api` DTO (`FrameDto`) is the flattened painter form.
* The payload comes out of `pc_decoder_take_ready`: after `PC_DONE`, the concatenation of
  everything taken equals the payload.
* A handle must not be used by two threads at once.

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

## Phase 2: camera decoding

`ParticleDecoder::feed_yuv(width, height, y, u, v, y_stride, uv_stride, uv_pixel_stride) ->
FrameResultDto` will call `particlecode_vision::locate` and `read` and then feed the result to
the same `Receiver` that `feed_symbols` uses. The marked spot is the `PHASE 2` comment in
`src/api/decoder.rs`; the C ABI gets a matching `pc_decoder_feed_yuv`.
