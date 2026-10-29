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
`version` = format version (2 high bits, currently 0) ‖ shape table version (2 low bits).

**Spec correction:** RS(48,32) over GF(8) does not exist (GF(8) RS codes have length ≤ 7).
We use **RS(24,16) over GF(2⁶)** — same 48 dots, same 96 data bits. Code symbol `i` →
dots `2i` (high 3 bits) and `2i+1` (low 3 bits), both on the same halo; a whole lost halo
is 8 erasures, which the code still corrects. Guaranteed: 4 symbol errors (4–8 dots) or
8 erasures.

## Body

* One RaptorQ encoding symbol of `T` bytes per frame, `T` = `BodyCodec::block_size()`,
  a pure function of (dot count, ECC level).
* **Whitening (addition to the spec):** the `T` bytes are XORed with a keystream seeded
  by (session id, segment index, packet seed) before RS. Without it, CBOR zero runs and
  RaptorQ's systematic source symbols make large single-color regions, which both look bad
  and starve the neighbour-relative size classifier.
* `B = ⌊3·dots / 8⌋` coded bytes are split into `C = ⌈B / 255⌉` codewords of near-equal
  length; each codeword of length `n` carries `⌈n · parity₂₅₅ / 255⌉` parity bytes
  (64 → RS(255,191) for animated frames, 96 → RS(255,159) for static frames).
* Interleaving: byte `j` of codeword `c` goes to stream position `j·C + c` (holes from
  shorter codewords are skipped). Stream bytes → 3-bit symbols → dot `k` in serpentine order.
  Leftover dots (< 3) get filler symbols.
* Low-confidence symbols become byte erasures.

## Data layer

* Framed payload = `payload ‖ CRC-32(payload)`; header `payload length` is the bare
  payload length.
* Segment `s` = framed bytes `[s·S, (s+1)·S)`. Each segment is one RaptorQ source block
  over `segment ‖ CRC-32(segment)` with symbol size `T` and alignment 1, so a completed
  segment is verified on its own before delivery.
* Schedule: pass `k`, segment `s` emits ESIs `k·(K_s + r) .. (k+1)·(K_s + r)` mod 4096,
  `r = 2` by default. Pass 0 starts with the `K_s` systematic source symbols.
* The receiver locks onto the first session id it sees and drops other sessions.

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

## Not implemented in v1

* 8-hue palette (header bit reserved; encoder and receiver reject it).
* MP4/WebM packaging (pipe the PNG sequence into ffmpeg; see the README).
* Resuming a transfer across sessions.
