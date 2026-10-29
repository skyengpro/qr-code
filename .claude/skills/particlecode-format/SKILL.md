---
name: particlecode-format
description: Invariants of the Particle Code optical format (geometry, header bits, RS codes, whitening, interleaving, fountain schedule, particles) and which files must change together. Use when touching particlecode-core's layout/header/body/session/receiver, the renderer, or the vision decoder.
---

# Particle Code format invariants

Read `docs/spec.md` (product spec) and `docs/design.md` (implementation decisions and spec
corrections) first. Then:

1. **Geometry has one source of truth:** `crates/particlecode-core/src/layout.rs`. The
   renderer and vision decoder call into it. Changing a radius/angle there is a format
   change.
2. **Format changes are versioned.** Anything that changes what a decoder must do (header
   layout, codes, whitening, interleaving, dot order, palette, geometry) bumps
   `header::FORMAT_VERSION`; anything that changes built-in shape bitmaps bumps
   `shapes::SHAPE_TABLE_VERSION`. Update `docs/design.md` in the same commit.
3. **Encoder output is deterministic.** `Session::frame(i)` is a pure function of the
   payload, config, session id/key and `i`. Tests pin this; no `HashMap` iteration,
   no wall-clock reads inside core.
4. **Decoder never panics** on any image or symbol stream; it returns `Rejected`.
5. **Timing invariance:** nothing in the decoder uses frame order or timing; frames are
   identified by the header's packet seed.
6. **Animation stays inside the sampling budget:** drift ≤ 0.12 p (hard limit 0.15),
   radius pulse shared with neighbours, no hue shift > 10°, nothing drawn over the field.
   `motion` functions are pure in (t, seed, progress).
7. **Palette values are exact sRGB** (`profile::colors`). Never JPEG 4:2:0 a frame.

## Change checklist

| You changed | Also update / run |
| --- | --- |
| `layout.rs` | renderer, vision `locate`/`read`, `docs/design.md`, synthetic vision tests |
| header bits | `header.rs` tests, fuzz target, `docs/design.md` |
| body codec | proptests, `BodyCodec::block_size` callers, design doc |
| shapes | `SHAPE_TABLE_VERSION`, `particlecode-shapes list` output in README |
| palette | design doc table, vision calibration thresholds |
