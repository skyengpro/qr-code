# Particle Code — implementation design

`docs/spec.md` is the product spec. This file records how the implementation reads it,
and every place where the spec had to be interpreted or corrected. When code and this
file disagree, fix one of them in the same change.

## Coordinates and geometry

All geometry lives in `particlecode-core::layout`, in units of the dot pitch `p`, origin
at the frame center, `y` down. Painters and the vision decoder never hard-code positions.

| Item | Value | Notes |
| --- | --- | --- |
| Field | square `[-F, F]²`, `F = N/2` | shape design box (1000 × 1000) maps onto it |
| Hero anchors | radius `0.84 F` at −90°, 30°, 150° | spec's `0.67 R` with `R = 1.25 F` |
| Hero mark | core 0.70, ring 1.30–2.15 | ring/core area ≈ 6; top anchor has a 0.25 pupil (the "extra inner dot") |
| Center mark | core 0.33, ring 0.58–1.00 | sits on lattice point (0,0); keep-out 1.5 |
| Swatch ring | 16 swatches, radius 3, dot radius 0.42 | slot 0 points away from center; hue = `slot % 4`; top anchor: slot 0 white, slot 8 black |
| Header ring | 16 dots, radius 4, half-slot offset | header dot `j` → halo `j / 16`, slot `j % 16` |
| Hero keep-out | 5.0 | **spec says 4.6**; at 4.6 a large lattice dot can touch a header dot. 5.0 guarantees ≥ 1 p |
| Data dots | small 0.36, large 0.48 | area ratio 1.8 |
| Particle band | inner `max(1.0625 F, ink extent + 1.5)`, width `0.125 F` | for the disc this is the spec's 0.85–0.95 R |
| Frame half-side | `R = band.outer / 0.95` | quiet zone ≥ 1.5 p outside `[-R, R]²` |

Shapes whose ink reaches past the disc (rounded square, letters) push the particle band
outwards, so particles never cross the dot field (an animation rule in the spec). The frame
just gets a little larger in pitch units for those shapes.

Exact capacities (disc, profile M) are printed by `particlecode info`.

## Palette

sRGB, exact, never color-managed (`profile::colors`):

| Role | RGB | BT.601 luma |
| --- | --- | --- |
| blue (0) | 20, 110, 245 | 98 |
| green (1) | 40, 165, 30 | 112 |
| orange (2) | 235, 100, 0 | 129 |
| magenta (3) | 215, 40, 175 | 108 |
| background | 246, 244, 239 | 244 |
| anchor | 24, 28, 52 | 30 |
| particle | 120, 122, 128 | 122 |

Anchors are far darker than any hue, so a luma threshold at ~half the local mean isolates
them from the dot field. Hues are ≥ 68° apart in CbCr.

## Symbols

A dot symbol is `color << 1 | size` (3 bits). Bytes are packed into symbols MSB-first.

## Header

96 bits in spec order (see `header.rs`), CRC-8/SMBUS over the first 11 bytes.
`version` = format version (2 high bits, currently 2) ‖ shape table version (2 low bits).

**Spec correction:** RS(48,32) over GF(8) does not exist (GF(8) RS codes have length ≤ 7).
We use **RS(24,16) over GF(2⁶)** — same 48 dots, same 96 data bits. Code symbol `i` →
dots `2i` (high 3 bits) and `2i+1` (low 3 bits), both on the same halo; a whole lost halo
is 8 erasures, which the code still corrects. Guaranteed: 4 symbol errors (4–8 dots) or
8 erasures.

## Body

* One RaptorQ encoding symbol of `T` bytes per frame, `T` = `BodyCodec::block_size()`,
  a pure function of (dot count, ECC level). Format version 2 reserves four RS data bytes
  for CRC-32 over the big-endian session id, segment index, packet seed and packet bytes;
  a failed packet is rejected before it can contaminate a segment. Tiny synthetic layouts
  with fewer than four RS data bytes use the corresponding prefix of the checksum.
* **Whitening (addition to the spec):** the interleaved RS bytes are XORed with a keystream
  seeded by (session id, segment index, packet seed). This avoids large single-color regions
  and makes an old body paired with a new header fail RS decoding in normal conditions.
  The released version 0 whitened before RS and could accept such a mixed frame, resetting
  a whole segment when its CRC finally failed. An intermediate version 1 was used during
  local testing; the released cutover is directly from version 0 to version 2. This is
  error detection, not cryptographic authentication.
* `B = ⌊3·dots / 8⌋` coded bytes are split into `C = ⌈B / 255⌉` codewords of near-equal
  length; each codeword of length `n` carries `⌈n · parity₂₅₅ / 255⌉` parity bytes
  (64 → RS(255,191) for animated frames, 96 → RS(255,159) for static frames).
* Interleaving: byte `j` of codeword `c` goes to stream position `j·C + c` (holes from
  shorter codewords are skipped). Stream bytes → 3-bit symbols → dot `k` in serpentine order.
  Leftover dots (< 3) get filler symbols.
* Low-confidence symbols become byte erasures.

## Data layer

* Framed payload = `payload ‖ CRC-32(payload)`; header `payload length` is the bare
  payload length. Both CRC-32s (payload and segment) are appended big-endian.
* Segment `s` = framed bytes `[s·S, (s+1)·S)`. Each segment is one RaptorQ source block
  over `segment ‖ CRC-32(segment)` with symbol size `T` and alignment 1, so a completed
  segment is verified on its own before delivery.
* Schedule: pass `k`, segment `s` emits ESIs `k·(K_s + r) .. (k+1)·(K_s + r)` mod 4096,
  `r = 2` by default. Pass 0 starts with the `K_s` systematic source symbols.
* The receiver locks onto the first session id it sees and drops other sessions.
* Static sessions are one frame: one segment with `K = 1`, `r = 0`, so
  `payload + 8 ≤ T` (payload CRC + segment CRC).
* `ShapeChoice`: `Require(id)` fails if the shape is unknown or (static) the payload does not
  fit; `Prefer(id)` falls back to the disc; `Auto(ids)` takes the first listed shape that
  fits, then the disc. (The spec says "the largest shape that fits"; preference order is
  more predictable, and the caller controls it.)

## Particles and liveness

Particle `k` (0..6) owns the 60° arc starting at −90° + 60°·k; its sector (0..16) is a
nibble of the 24-bit value `seed_low8 ‖ rolling16`, particle 0 holding the top nibble.
`rolling16` = first two bytes of HMAC-SHA256(session key, big-endian u64 unix second).
The receiver can only *read* the value; verifying it needs the session key, which the
sender shares through the application layer (e.g. inside the signed COSE payload or via
the backend). The data path never depends on particles.

## Shapes

Built-in shapes (ids 0–30) are defined analytically in `core::shapes` and evaluated at
lattice points, so they are identical in every app without shipping bitmaps. Custom shapes
(ids 32–255) come from `shapes.bin`, produced by the `particlecode-shapes` tool.

`particlecode-shapes validate` (rule 7) renders real codewords through a deliberately harsh
synthetic camera (heavy glare, blur and noise make ~11 % of frames fail to lock, for every
shape alike). It compares the symbol error rate against the disc's under identical camera
chains. A shape passes only if both ratios are ≤ 1.3×:

* the ratio over all symbols, counting unlocked frames as all wrong (the spec's measure);
* the ratio over the frames that were read. This is the one that separates shapes, because
  lock failures are shape-independent and swamp the first ratio.

## Vision decoder

The spec's pipeline, with these refinements (details in `particlecode-vision`'s crate docs):

* **Lock.** Hero anchors are concentric (core, ring) dark components, area ratio ≈ 6, found
  with a `pixel < α · local mean` threshold. A candidate triple is accepted only after a
  strict check: exact swatch probes, ink on the header ring, and a displaced-halo permutation
  test. That check, not the triangle alone, is what rejects busy backgrounds.
* **Orientation and mirroring.** The white/black swatches on the top halo pick the rotation.
  The halo is mirror-symmetric, so `locate` only considers orientation-preserving maps and
  `read` detects a mirrored view from the cyclic order of the calibrated hue centroids.
* **Homography.** A coarse fit from the anchors is refined with lattice-phase measurements
  (Fourier phase of the ink image at the three reciprocal lattice vectors, ~150 windows).
  This is blur- and content-independent, and per-dot centroid refinement (≤ 0.3 p) absorbs
  lens distortion and the breathing wave.
* **Lighting.** The paper level is a plane fitted over the particle band and divided out.
  White balance uses the paper's chroma (the white swatch is only ~4 % brighter than the
  paper). Hue is classified by chroma angle against per-frame centroids.
* **Size.** Ink mass in two windows (r = 0.34 p and 0.52 p), normalised by the same-hue
  swatch and the local pitch², combined by a per-frame Fisher discriminant. The spec's
  six-neighbour median rule refines borderline dots.
* **Confidence.** Each dot gets the weakest of its hue margin, size margin and ink depth,
  on a 0–255 scale. Symbols below `rs::ERASURE_BELOW` (96) become Reed-Solomon erasures.

## Native bindings

* `particlecode-ffi` is the one binary surface: flutter_rust_bridge-style `api`, the C ABI
  (`include/particlecode.h`), and `native` (borrowed-buffer entry points for Rust hosts).
* iOS/macOS: `scripts/build-xcframework.sh` packages the C ABI as
  `ParticleCodeFFI.xcframework`; `apple/ParticleCodeKit` wraps it in Swift (NV12
  `CVPixelBuffer`s read in place, an `AVCaptureVideoDataOutput` delegate).
* Android: `particlecode-jni` exports JNI functions for `com.vaam.particlecode.NativeBridge`
  over the same decoder. Direct `ByteBuffer`s from CameraX are read in place, handles are
  table keys (not pointers), and results go into preallocated arrays. `android/` holds the
  Kotlin wrapper and CameraX analyzer.
* `raptorq`'s `std` feature (runtime SIMD dispatch) does not compile for 32-bit ARM on
  stable. Core therefore requests it through a target-specific dependency on every target
  except `target_arch = "arm"` and `target_os = "none"`, rather than through core's own
  `std` feature. Consequence: a host-target `--no-default-features` build still compiles
  raptorq with `std`. The real `no_std` guarantee is the bare-metal `thumbv7em` CI build.

## Browser sender (`particlecode-wasm`)

* The operator console draws the code in a `<canvas>`, so the web needs the sender but not the
  decoder. `particlecode-wasm` is `particlecode-core` + `particlecode-render` behind
  `wasm-bindgen`; it adds no format, so there is no `FORMAT_VERSION` change. Geometry and
  palette still come only from core and `render`.
* **Rust owns the clocks.** `ParticleCanvas::tick(elapsed_ms, now_unix_ms, size_px)` ports
  `ParticleCodeView._onTick`: data frame `floor(elapsed * fps / 1000)` (from time, never from
  the number of paints), the previous frame kept for the 40 ms cross-fade
  (`t_since_switch = t - index / fps`, no cross-fade without a previous frame), ripple and
  reveal as `render_animated` already derives them, and the liveness second from the
  wall-clock argument at the moment a frame is fetched. JavaScript supplies the timer and the
  clock readings and blits; nothing in the crate calls `SystemTime::now()`, which panics on
  `wasm32-unknown-unknown`. The session is `Session::with_identity` with a random id and key
  (getrandom's `wasm_js` backend, enabled on that target only) and the caller's clock origin.
* The painter returns a fresh `Pixmap` per call (tiny-skia owns it); `tick` copies it into one
  persistent RGBA buffer so a page keeps a single `Uint8ClampedArray` view over wasm memory
  (`pixels_ptr`/`pixels_len`; reading the address is safe Rust, only the view lives in JS).
  The background is opaque, so premultiplied and straight alpha are the same bytes.
  `motion = false` paints `render` (no drift, no cross-fade) and repaints only when the data
  frame or the size changes; `tick` returns whether pixels changed.
* Build: `scripts/build-wasm.sh` (`--profile release-wasm`, `opt-level = 2`; measured, `s` gave a
  794 KB module and 23 ms per profile S tick at 656 px, 2 gave 675 KB and 13 ms) then `wasm-bindgen --target web`. `wasm-bindgen` is pinned exactly in the workspace
  and the script refuses a CLI of another version. CI builds it and runs a Node smoke test
  (`.github/workflows/wasm.yml`); the native tests render through `tick` and decode the pixels
  with `ParticleDecoder::feed_rgba` (a 6-digit code and a 253-byte JSON payload, profile S).

## Not implemented in v1

* 8-hue palette (header bit reserved; encoder and receiver reject it).
* MP4/WebM packaging (pipe the PNG sequence into ffmpeg; see the README).
* Resuming a transfer across sessions.
