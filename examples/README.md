# Examples

* [`flutter-demo/`](flutter-demo/README.md): the Flutter demo app (Send and Scan tabs) over the
  `particle_code` plugin. `cd examples/flutter-demo && flutter run` on a phone.
* The Rust examples below.

## Rust

Runnable end-to-end examples of the encoder side. They are Cargo examples of the
`particlecode-cli` crate, so they share its dependencies and the helpers in
`particlecode_cli` (sample payloads, session builder, contact sheets).

Run them from the repository root, in release mode (RaptorQ and the renderer are slow
unoptimised):

```sh
cargo run --release -p particlecode-cli --example <name>
```

Output goes to `out/` (git-ignored) unless stated otherwise.

| Example | What it shows | Output |
| --- | --- | --- |
| `encode_png` | A small COSE-style CBOR ticket (`data/ticket.cbor`) to a `Session`, first frame to PNG. The shortest path from bytes to pixels. | `out/encode_png.png` |
| `animated_gif` | The 6 KB `data/credential.cbor` as a looping GIF with the breathing mood and particle trails. The loop is one breathing period (25 frames at 10 fps) so it wraps seamlessly. | `out/animated_gif.gif` |
| `shapes_gallery` | One frame per shape family (disc, rounded square, heart, hexagon, ring, letters P A R T) rendered at 360 px and composed into a 3 x 3 contact sheet. | `out/shapes_gallery.png` |
| `symbol_roundtrip` | `Session` to `Receiver` at the symbol level for a 30 KB payload in 4 KiB segments, with every 4th frame dropped and 1% of the symbols corrupted. Prints progress (symbols held / needed, segments verified) frame by frame. | terminal only |
| `camera_roundtrip` | The whole pipeline in one process: 6 KB payload, `Session`, animated render (breathing mood), synthetic phone camera (tilt, colour cast, blur, noise from `particlecode_render::synth`), `particlecode-vision` locate/read, `Receiver`. Every 3rd frame is lost. Prints per-frame progress and liveness (including an HMAC check with the session key) and ends with the bytes verified. | terminal only |
| `print_poster` | A static, high-ECC frame for paper: an SVG sized in millimetres and a 600 dpi PNG (with `pHYs` metadata), both at a 1.3 mm dot pitch. | `out/print_poster.svg`, `out/print_poster.png` |
| `readme_images` | Regenerates the pictures in `docs/images/` (hero, shapes, letter, animated GIF, and a synthetic camera view built with `particlecode_render::synth`). Takes an optional output directory. | `docs/images/*` |

## Sample data

`data/ticket.cbor` (193 bytes) and `data/credential.cbor` (about 6 KB) are COSE_Sign1-shaped
CBOR documents with a pseudo-random stand-in signature, built by hand in
`particlecode_cli::sample` (no CBOR dependency). A test checks that the committed files match
the generators; regenerate them with

```sh
PARTICLECODE_WRITE_SAMPLES=1 cargo test -p particlecode-cli sample
```

## The same things from the command line

```sh
cargo pc encode examples/data/ticket.cbor -o out/ticket.png
cargo pc encode examples/data/credential.cbor -o out/credential.gif --all-moods
cargo pc encode examples/data/ticket.cbor -o out/poster.svg --static --prefer-shape heart
cargo pc encode examples/data/credential.cbor -o out/frames          # PNG sequence + manifest.json
cargo pc info --shape disc --shape heart
cargo pc roundtrip examples/data/credential.cbor --drop-every 4 --error-rate 0.01
cargo pc roundtrip examples/data/credential.cbor --camera --drop-every 3 --blur 1.2 --noise 6 --tilt 20 -v
cargo pc decode out/frames -o out/decoded.cbor                          # read the PNG sequence back
cargo pc decode out/frames --json > /dev/null                           # one JSON object per image on stderr
```
