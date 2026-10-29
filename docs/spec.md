# Particle Code — Aesthetic Dense Optical Code Spec

Oct 29, 2026 · @SSegning

## Goals and non-goals

Particle Code moves COSE payloads of 1–20 KB from one phone screen to another phone camera in a few seconds, and looks like a living piece of generative art rather than a barcode. Both the sender app and the scanner app are ours, so no compatibility with QR, Data Matrix or any third-party scanner is required.

Goals, in priority order:

1. Looks designed, not scanned: a circular field of colored dots with a slow wave moving through it and a few drifting particles. No square finder patterns, no black-and-white grid.
2. Reliable decode on mid-range Android phones (Cameroon market: 720p–1080p rear cameras, often scratched lenses) in 2–6 seconds at 15–40 cm.
3. Density within \~30% of cimbar per frame; the rest recovered by animation and fountain coding.
4. Liveness: a screenshot, printout or screen recording replayed later does not decode as a fresh session.
5. Payload-agnostic and size-agnostic: raw CBOR bytes in, raw bytes out, from 100 bytes to several hundred KB. Large payloads arrive progressively, in configurable segments, so the app can show the first part while the rest streams.

Non-goals:

- Animated print. Paper gets the static profile only (one frame, one payload that fits it); animation exists on screens and in animated image files.
- Interop with any existing scanner app or standard.
- Encoding data in particle trajectories. Particles carry only a frame index and a nonce; the payload lives in the dot field (see Animation for why).

## Frame layout

The frame is split into an invariant part and a free part. The invariant part is three hero anchors in a fixed equilateral triangle, a fourth small anchor at its center, and a halo around each hero anchor that carries the header and the calibration swatches. The free part is the mask: which lattice points carry payload dots. The default mask is a disc; a letter, a logo or any other shape is just a different mask (see Shapes).

&#91;embedded content: frame layout · 5 parts, circular, no square finders\]

The four anchors give the decoder position, scale, orientation and a full homography; the halos tell it which profile and mask to use and what the four colors look like under this camera; only then does it read the mask.

| Part | Geometry | Job |
| --- | --- | --- |
| Hero anchors | 3 ring-and-core marks at −90°, 30°, 150° on a circle of radius 0.67 R (R = half the frame's side). Core solid accent, ring 3 dot pitches thick. The −90° anchor carries an extra inner dot to break rotational symmetry. | Detection, orientation, scale |
| Center anchor | 1 smaller ring-and-core mark at the triangle's center, keep-out 1.5 p. | Fourth point for a full homography |
| Anchor halos | Around each hero anchor: 16 calibration swatches on a ring of radius 3 p (cycling the palette; the −90° halo swaps two swatches for white and black), then 16 header dots on a ring of radius 4 p. Keep-out for the mask: 4.6 p from the anchor center. | Per-frame color reference; 48-dot header |
| Mask | Hexagonal lattice, pitch p, spanning the frame; a dot is drawn where the shape's bitmap for this profile says so, outside all keep-outs. Dot radius 0.36 p (small) or 0.48 p (large). | Payload symbols |
| Particle layer | 5–8 small neutral specks drifting along arcs on the outer circle, radius 0.85–0.95 R. | Frame index + nonce, liveness |
| Quiet zone | Plain background, width ≥ 1.5 p around the frame's bounding box. | Keeps anchors and halos detectable against the app UI |

Sizes are relative to R so the same encoder works on any screen; the sender picks p from the display's physical size (target: p ≥ 1.6 mm, see Risks).

## Shapes

A shape is a bitmap over the hex lattice, one per profile, generated at build time from an SVG path and shipped inside both apps. The decoder reads the shape id from the header, looks the bitmap up in its table, and samples exactly those lattice points. Everything that finds the frame is independent of the shape, which is why the shape can be anything with enough ink.

**Shape table.** Ids 0–31 are built in and identical in every app: 0 disc (default), 1 rounded square, 2–27 the letters A–Z in a heavy geometric face, 28 heart, 29 hexagon, 30 ring, 31 reserved. Ids 32–255 are for shapes an app defines for itself; both ends of a transfer must ship the same table version (`shape_table_version` is part of the header `version` field). A receiver that sees an unknown id rejects the frame and tells the user to update, and the sender can be told to prefer the disc for compatibility.

**Capacity by shape at profile M** (approximate; the build tool prints the exact count):

| Shape | Usable dots | Net bytes per frame |
| --- | --- | --- |
| Disc | \~4,500 | \~1,250 |
| Rounded square | \~5,600 | \~1,550 |
| Heart | \~3,300 | \~900 |
| A, V, M, W (heavy) | \~2,000–2,400 | \~550–650 |
| I, L, T (heavy) | \~1,300–1,600 | \~350–450 |

Animated transfers simply take more frames with a sparse shape; static outputs (print, single image) check that the payload fits and fall back per `ShapeChoice`: `Require(id)` fails if it does not fit, `Prefer(id)` falls back to the disc, `Auto` picks the largest shape from a preference list that fits.

**Bringing your own shape.** The `particlecode-shapes` tool turns a path into bitmaps and tells you whether the result will decode. The rules it checks, which are the rules a shape has to meet:

1. Give it an SVG path in a 1000 × 1000 box; the frame's bounding square maps onto it. Fill rule nonzero; strokes are outlined first.
2. Ink must cover at least 25% of the box (about 1,400 dots at M). Below that the RS interleaving has too little to spread over, and the frame budget becomes silly.
3. Every stroke must be at least 4 p wide at the target profile (the tool reports the thinnest stroke in pitches). Thinner strokes starve the size classifier, which compares a dot to its six neighbors.
4. No island smaller than 60 dots. Small islands lose their neighbors and their interleaving partners.
5. The anchor and halo keep-outs are punched out automatically; design around the triangle rather than fighting it, since the anchors are visible in every shape.
6. Connected shapes decode better than disconnected ones, but disconnection is allowed when 2–4 hold for every part.
7. `validate` renders 1,000 synthetic frames of the shape through blur, perspective, noise and glare and reports symbol error rate against the disc. A shape passes at ≤ 1.3× the disc's rate.
8. Ship the generated `shapes.bin` to both apps and bump the table version. Ids 32–255 are yours; keep a registry so two apps in the same family never collide.

What the tool does not check, and you should: that the shape reads as what you mean at arm's length on a 6-inch screen, with the three anchors in it. Letters with a counter (A, B, D, O, P, Q, R) benefit from the center anchor landing inside the counter; the built-in alphabet is tuned so it does.

## Symbol encoding

Each dot carries 3 bits: 2 from its color (4-hue palette) and 1 from its size (small or large). A frame holds one of three profiles, chosen by the sender from its screen size and announced in the header, so the receiver never guesses the lattice.

**Palette.** Four hues spaced \~90° apart on the hue wheel, all at similar lightness so size classification is not fooled by brightness: blue, green, orange, magenta. The decoder never trusts absolute RGB; it classifies each dot by nearest centroid in a chroma plane, with the four centroids measured from the calibration ring in that same frame. White and black swatches on the ring set the exposure scale.

**Size.** Small radius 0.36 p, large 0.48 p (area ratio 1.8). Blur grows every blob equally, so the decoder compares a dot's measured area to the median of its six hex neighbors rather than to a fixed threshold.

**Symbol order.** Dots are numbered in a fixed serpentine over axial hex coordinates (row by row, alternating direction), skipping the anchor keep-outs. Symbol i of the codeword goes to dot i. The order is part of the profile, never transmitted.

| Profile | Dots across field (N) | Usable dots | Raw bits | Raw bytes | Net payload (after RS \~25%) | Camera px per dot at 25 cm, 1080p |
| --- | --- | --- | --- | --- | --- | --- |
| S | 48 | \~2,020 | 6,060 | 757 | \~560 B | \~11 |
| M | 72 | \~4,630 | 13,890 | 1,736 | \~1,300 B | \~7.5 |
| L | 96 | \~8,290 | 24,870 | 3,109 | \~2,300 B | \~5.6 |

Dot count per profile is ≈ 0.907 N² (hex packing in a disc) minus \~68 dots lost to the three anchor keep-outs. M is the default: it matches a v40 QR at ECC M per frame and still gives the camera 7+ pixels per dot. L is for 1080p-or-better cameras held close; S is the fallback the receiver can request when M fails to lock.

A later profile flag may raise the palette to 8 hues (4 bits per dot, +33%) once field data shows the 4-hue classifier has margin to spare.

## Data layer

A frame carries one fountain-coded packet, protected by Reed-Solomon. The payload is split into equal source blocks on the sender; the receiver collects packets from any frames, in any order, until it can reconstruct the whole payload. This is what makes a looping animation forgiving: a missed or misread frame costs one more loop iteration, never a restart.

**Frame header (first 48 symbols, RS(48,32) over GF(8): 96 data bits, corrects 8 symbol errors).** Decoded first, on its own, so the receiver knows how to read the rest of the frame:

| Field | Bits | Meaning |
| --- | --- | --- |
| magic | 6 | constant, rejects false locks |
| version | 4 | format + shape table version |
| profile | 2 | S / M / L |
| palette | 1 | 4 or 8 hues |
| static | 1 | 1 = this frame is the whole payload (print, single image); receiver does not wait for more |
| shape id | 8 | mask to sample (0 = disc; see Shapes) |
| session id | 16 | random per transfer; packets from other sessions are dropped |
| payload length | 24 | bytes, up to 16 MB |
| segment size | 4 | 2^n KB, n = 0..15 (1 KB .. 32 MB); default n = 3 (8 KB) |
| segment index | 10 | which segment this packet belongs to (up to 1,024; 8 MB at 8 KB segments) |
| packet seed | 12 | RaptorQ encoding symbol id within the segment |
| header CRC | 8 | over the fields above |

Segment count is derived (⌈length / segment size⌉), never sent.

**Body.** The remaining dots carry one RaptorQ encoding symbol (RFC 6330) of `block size` bytes, split into RS codewords of 255 symbols over GF(256): 3-bit dot symbols are packed into bytes before coding, so one RS symbol spans 2⅔ dots. Each codeword has 64 parity symbols (RS(255,191), corrects 32 symbol errors). Codewords are interleaved across the dot field so that a smudge or glare patch on one region spreads its damage across many codewords instead of killing one.

**Segmented fountain coding.** The payload is cut into segments of a configurable size (default 8 KB), and each segment is its own RaptorQ block. Sender: for segment s, split into K source blocks of `block size` bytes (K ≈ 6 at 8 KB and profile M), build the encoder, and emit its encoding symbols. Cycle through segments in order, emitting K + r symbols of each per pass (r = 2 by default), then start the next pass over all segments. Receiver: hand every body that passes RS to the decoder for its segment; when a segment completes, verify its CRC-32 and deliver it to the app immediately, in order. The transfer is done when every segment is delivered.

RaptorQ is systematic: the first K encoding symbols are the source blocks themselves. On a clean read a segment therefore completes in exactly K frames, and the repair symbols only matter when frames were missed. Smaller segments mean earlier first data (8 KB ≈ 6 frames ≈ 0.6 s at 10 fps) at the cost of more repair symbols per pass under loss; 32 KB segments lose less to overhead but show nothing for 2.5 s. Both are one config value: `SessionConfig { segment_kb_log2: 3, profile: M, palette: Hues4, fps: 10, static: false }`. The segment size is also what makes a transfer resumable: an interrupted session keeps its completed segments, and a later session with the same payload hash only needs the rest.

Overhead per frame at profile M: 48 header dots + 25% RS parity ≈ 1,290 net bytes from 1,736 raw, so about 13 KB/s at 10 fps. A 6 KB COSE\_Sign1 needs 5 frames; a 600 KB document needs \~470 frames, roughly 50 s of perfect lock and 1–2 min in practice, with the first 8 KB segment usable after the first second. Profile L with 8 hues roughly doubles the rate; 15 fps data frames on a 30 fps camera adds another 50%.

**Payload framing.** The bytes handed to the fountain encoder are: `CBOR payload` ‖ `CRC-32`. COSE structures go in as raw CBOR. The format does no compression itself; the application compresses the inner COSE payload (zstd with a shared dictionary works well for repetitive CBOR) before signing.

## Animation

The animation has two layers with different jobs: the breathing wave makes the field look alive without touching the data, and the particles carry liveness. Neither depends on the camera's frame rate, because every frame is self-contained and the receiver only needs any frame to be sharp.

**Data frames.** The dot field shows one fountain packet per frame and holds it for 100 ms (10 fps). The receiver's camera runs at 30 fps, so every data frame is captured about three times; the decoder takes the sharpest one (highest Laplacian variance over the field). Switching the field at 10 fps keeps per-frame motion blur low and gives auto-exposure time to settle.

**Breathing wave.** Every dot drifts around its lattice point by at most 0.12 p, along a vector that follows a slow sine wave travelling across the field (period 2.5 s, wavelength 0.6 R, phase seeded from the session id). The decoder samples a disc of radius 0.3 p around each lattice point, so the drift is invisible to it. Dot radius also pulses by ±5% with the same wave; the size classifier compares to neighbors, which pulse together, so this is invisible too.

**Color handover.** When the field switches to the next packet, dots that change color cross-fade over 40 ms instead of snapping. This reads as a shimmer rather than a flicker. The receiver's sharpest-frame selection naturally rejects frames captured mid-fade, because their blurred color edges score low.

**Particle layer.** Six specks orbit the outer circle (0.85–0.95 R) on independent arcs, outside every mask. At any instant their angular positions, quantized to 16 sectors each, encode 24 bits: the 8 low bits of the packet seed and a 16-bit rolling value derived from HMAC(session key, second counter). The receiver reads this once per lock to confirm the sender is live: a screenshot or replayed recording shows a stale counter. The data path never depends on particles; a receiver that cannot read them still decodes the payload and simply reports liveness as unknown.

**Timing invariance.** Nothing in the decoder uses velocity, inter-frame timing or frame order. Frames are identified by the packet seed in their own header. This is the rule that keeps the format robust on a camera that drops frames, changes exposure mid-transfer, or runs at 24, 30 or 60 fps.

**Aesthetic vocabulary.** The decoder reads lattice position, hue class and relative size, sampled over a disc of 0.3 p. Anything that leaves those three alone is free. The sender ships a small set of named "moods", each a deterministic function of (t, session seed, transfer progress), so a brand or an app screen can pick one:

| Motion | What it does | Safe because |
| --- | --- | --- |
| Breathing wave (default) | dots drift ≤ 0.12 p along a travelling sine | stays inside the sampling disc |
| Ripple on segment completion | a ring of brightness expands from the center once per completed segment; dots on the ring lighten by 15% for 300 ms | hue and size untouched; luma is calibrated per frame and the classifier works in chroma |
| Flow field | dots drift ≤ 0.12 p along a smooth noise field (simplex, seeded) instead of a sine; looks like wind through grass | same bound as the wave |
| Anchor heartbeat | the three hero rings swell by 10% in a slow, offset pulse | anchors are found by shape ratio, not absolute size |
| Particle trails | specks leave a fading 0.8 p tail along their arc | trails live in the rim band, outside the lattice |
| Reveal | on session start, dots fade in from the center over 600 ms before the first data frame | no data frame is shown until all dots are at full opacity |
| Color shimmer | the 40 ms cross-fade between data frames, with hue rotation along the shortest arc | rejected by the sharpness gate, so never sampled |

What is not allowed, because it changes what the decoder measures: drift above 0.15 p, per-dot size changes that are not shared with the six neighbors, any hue shift larger than 10°, dots that leave the lattice, overlays crossing the dot field, and gradients inside a dot. A "mood" is validated by rendering 1,000 random frames of it and checking that the synthetic decoder reads them at the same symbol error rate as the static frame.

On screens the moods run live from the painter's clock; in animated image files (GIF, video) the packager bakes the same functions into the frame sequence, so an email looks like the app.

## Decode pipeline

The decoder is a per-frame function from camera image to one fountain packet, plus a small cross-frame accumulator. Steps 1–9 are stateless; only steps 10–12 hold state, and that state is the RaptorQ decoder.

&#91;embedded content: decode pipeline · 12 steps, 9 per frame, 3 across frames\]

Every per-frame step either produces its output or rejects the frame; a rejected frame costs nothing because the next one arrives in 33 ms.

Notes on the hard steps:

- **Anchor detection (3).** Threshold the luma plane adaptively, find connected components, keep blobs whose shape is a filled disc inside a ring of the same hue (two concentric components, area ratio \~6). Three such blobs whose centers form an equilateral triangle within 5% tolerance, plus a smaller one near the triangle's centroid, is a candidate lock. The mask is irrelevant at this stage, which is what lets any shape work. This is cheap enough to run on every frame at 1080p on a mid-range SoC.
- **Homography (5).** Three hero anchors give an affine fit; the center anchor is the fourth point that makes it a full homography, so a tilted phone still maps correctly. The halo rings add 48 more correspondences for a least-squares refinement once the coarse homography has found them. Residual error must be under 0.15 p at the anchors or the frame is rejected.
- **Color calibration (6).** Sample the 48 halo swatches (16 per hero anchor) through the homography, cluster into 4 centroids in a chroma plane (CbCr or a linear YCbCr projection), and read white and black swatches for the luma range. Centroids that are closer to each other than 2× their spread mean the lighting is unusable; reject and tell the UI to prompt the user ("too dark", "glare").
- **Lattice sampling (7).** For each lattice point (through the homography), take the mean chroma over a disc of radius 0.3 p and the blob area from a local threshold. Sub-pixel accuracy matters at profile L; sample with bilinear interpolation.
- **Classify (8).** Hue by nearest centroid; size by comparing area to the median of the six neighbors (large if > 1.35× median, small if < 0.75×, else use the frame-wide bimodal threshold). Emit a per-symbol confidence; low-confidence symbols become RS erasures, which doubles their correction value (RS corrects 2 erasures for the price of 1 error).

## Encode pipeline and rendering

The encoder is the simple half: bytes in, a stream of frame descriptions out. Rendering is a separate layer so the same frame description can be drawn by Flutter (CustomPainter), by a Tauri webview (Canvas 2D or WebGL) or by a test harness that writes PNGs.

1. **Prepare.** Append CRC-32 to the CBOR payload. Pick the profile from the display's physical size: p ≥ 1.6 mm → L if that still fits 96 dots across, else M, else S. Pick a random 16-bit session id and a session key for the particle HMAC.
2. **Fountain.** Split into K source blocks, build the RaptorQ encoder. For frame i, produce encoding symbol i.
3. **Protect.** Build the header, RS-encode it. Pack the body into bytes, RS-encode each 255-symbol codeword, interleave codewords across the dot order (symbol j of codeword c goes to dot index j × C + c, where C is the codeword count).
4. **Map.** Convert the byte stream to 3-bit symbols, assign symbol k to dot k in the profile's serpentine order. A `FrameSpec` comes out: `{profile, dots: [(hex q, hex r, color 0..3, size 0|1)], particles: [angle × 6]}`.
5. **Render.** The painter draws, from one `FrameSpec` and a wall-clock time t: the quiet background, the calibration ring, the anchors, every dot at its lattice point plus the wave offset for t, and the particles at their encoded angles. Two consecutive specs are cross-faded for 40 ms at the switch.

Rendering rules that protect decodability:

- Draw in sRGB with the exact palette; never let the OS apply a night-light or color filter without warning the user (Android exposes this; iOS mostly does not, so the receiver's calibration ring is the real defense).
- Dots are anti-aliased circles with no stroke. A thin outline changes the blob area the receiver measures.
- Screen brightness is forced to maximum while the code is shown and restored afterwards.
- The wave offset is computed once per frame from t and the session seed, never accumulated, so a dropped render frame cannot drift the field.
- Frame rate of the painter is the display's (60–120 Hz); the data frame changes every 100 ms on a timer, independent of paint timing.

**Backend outputs.** The same `FrameSpec` → pixels function on the server (`tiny-skia`, pure Rust) serves every non-app use. One renderer, several packagers:

| Use | What is rendered | Format | Notes |
| --- | --- | --- | --- |
| Email, shown on a screen and scanned by the app | all frames of the session | animated GIF, 10 fps, looping | palette-exact and lossless for 4–8 flat colors; plays inline in Gmail and Apple Mail (Outlook desktop shows frame 1 only). \~15–25 KB per 600 px frame, so a 600 KB payload is \~7–10 MB; always attach the raw `.cbor` as well |
| S3 / asset store, composed later | every frame as PNG + a `manifest.json` (session, profile, segment map, frame order, palette, timing) | PNG sequence | lets a designer or a later job pick frames, re-time them or build a video; never JPEG (4:2:0 chroma subsampling smears the colors; if JPEG is forced, 4:4:4 at quality ≥ 92) |
| Flyer, poster, sticker | one static frame, `static = 1`, payload ≤ one frame | SVG (vector) or PNG at ≥ 600 dpi | profile S or M; ECC raised to RS(255,159); dot pitch ≥ 1.2 mm on paper; palette printed from the sRGB values with no color management surprises, and the calibration ring does the rest at scan time |
| Video / social | all frames as MP4 or WebM | H.264 at CRF ≤ 18, 4:4:4 if the player supports it | lossy; treat as best-effort and test on the target player before relying on it |

Every packager takes the same `Session` and a `RenderConfig { size_px, background, margin, fps }`, so adding a new output is a packager, not a new encoder.

## Rust architecture

One workspace, two library crates and one FFI crate, so the same decoder runs in the Flutter buyer app, the Tauri vendor app and a CLI you can fuzz and benchmark on a laptop.

&#91;embedded content: Rust workspace · 2 library crates, 1 FFI crate, 3 consumers\]

`particlecode-core` has no camera or pixel code and no allocation beyond `alloc`; it can be tested exhaustively with property tests (encode → corrupt → decode). `particlecode-vision` is the only crate that touches images, so it is the only one that needs a device for real testing.

**Public surface of `particlecode-core`:**

```rust
pub struct Session { /* id, key, profile, encoder */ }
impl Session {
    pub fn new(payload: &[u8], display: DisplayInfo) -> Session;
    pub fn frame(&self, i: u32) -> FrameSpec;      // deterministic
}
pub struct Receiver { /* RaptorQ decoder, session id */ }
impl Receiver {
    pub fn new() -> Receiver;
    pub fn feed(&mut self, symbols: &[Sym3]) -> Progress; // Progress::{Rejected, Accepted{have,need}, Done(Vec<u8>)}
}
```

**Public surface of `particlecode-vision`:**

```rust
pub fn locate(luma: &Plane) -> Option<Lock>;           // anchors + homography
pub fn read(frame: &YuvFrame, lock: &Lock) -> Option<(Header, Vec<Sym3>, Liveness)>;
```

**FFI.** For Flutter, `flutter_rust_bridge` generates the Dart side; the camera plugin hands over YUV planes as zero-copy byte views, `read` runs on a Rust thread pool, and `Progress` is streamed back for the UI's progress ring. For Tauri, the same crate is a dependency of the backend and the webview calls a command per frame, or the decoder is compiled to wasm and runs in the webview next to the camera stream; pick after measuring which one keeps 30 fps on the vendor's hardware.

**Rendering is not in Rust.** `FrameSpec` is plain data (positions, 2-bit color, 1-bit size, 6 particle angles); each app's painter draws it. This keeps the GPU path native to each platform and keeps the Rust crates headless.

## Risks, testing and build plan

The format is only as good as the vision stage on cheap phones, so the plan front-loads that and keeps everything else behind a mock.

| Risk | Why it bites | Mitigation |
| --- | --- | --- |
| Color classification fails under warm indoor light or night-light filters | Hues shift together; blue and magenta can merge | Per-frame calibration ring; reject frames whose centroids overlap; fall back to the 4-hue palette only, never 8, until measured |
| Profile L too dense for scratched or 720p cameras | < 6 px per dot leaves no margin for blur | Receiver sends a "go smaller" hint via the UI (no back-channel in v1, so the sender offers a profile toggle); M is the default |
| Screen reflections and moiré on OLED | Periodic lattice + camera sensor = moiré bands | Hex lattice at an irrational-looking pitch relative to common sensor pitches; the breathing wave also breaks static moiré |
| Anchor false positives on busy backgrounds | Rings exist in logos and UI | Require the equilateral triple plus the center anchor plus the header magic before accepting a lock |
| Low-end phone can't decode at 30 fps | 1080p blob analysis on a 2018 SoC | Run detection on a 4× downscaled luma plane; sample the full-res frame only after a lock |
| Liveness bypass by live re-broadcast | A relay shows a live feed of the real sender | Out of scope for v1; the COSE layer's own replay protection (nonces, timestamps) is the real defense |

**Testing strategy.**

- Core: property tests (proptest) over encode → corrupt N symbols → decode, for every profile; fuzz the header parser.
- Vision, offline: a corpus of 500+ phone photos and videos of real frames on 5 phone models, labeled with the ground-truth symbols from the encoder. Track symbol error rate and lock rate per phone, per lighting condition. This corpus is the single most valuable asset of the project; start collecting it in week 1 with the mock renderer.
- Vision, synthetic: render frames, apply blur, perspective, noise, glare and JPEG, decode, and measure. Cheap, fast, catches regressions.
- End to end: two phones on a bench, scripted transfers of 1, 5 and 20 KB payloads, measure time-to-complete.

**Build plan.**

1. **Week 1–2 — core.** Profiles, header, RS, RaptorQ, FrameSpec, CLI that renders frames to PNG and decodes PNGs. Property tests green.
2. **Week 3–4 — vision on photos.** Anchor detection, homography, calibration, sampling, classification, tuned against the synthetic set and the first real photos. Target: > 99% symbol accuracy at profile M in office light.
3. **Week 5 — Flutter sender.** CustomPainter with the breathing wave and particles. First phone-to-laptop transfers via the CLI.
4. **Week 6–7 — Flutter receiver.** flutter\_rust\_bridge, camera plugin, progress ring, live decode. First phone-to-phone transfers; start the real corpus.
5. **Week 8 — Tauri vendor side.** Decide command-per-frame vs wasm by measurement; ship whichever holds 30 fps.
6. **Week 9+ — field tuning.** Corpus from Cameroon devices, retune thresholds, decide whether 8-hue palette is safe.

The prototype on cimbar discussed earlier is still worth two days at the very start: it tells you what your target phones can resolve before you commit the profile numbers above.
