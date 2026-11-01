// Smoke test for the built browser module (scripts/build-wasm.sh), run under Node >= 18:
//
//   scripts/build-wasm.sh out && node crates/particlecode-wasm/tests/smoke.mjs out
//
// It loads the `--target web` output the way a bundler-free page would (initSync with the
// bytes), draws a code and checks the buffer. Decoding is covered by the native round-trip
// tests; this proves the wasm build itself runs and exposes the documented API.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const dir = resolve(process.argv[2] ?? "target/wasm-pkg");
const glue = await import(pathToFileURL(resolve(dir, "particlecode_wasm.js")).href);
glue.initSync({ module: readFileSync(resolve(dir, "particlecode_wasm_bg.wasm")) });

const { ParticleCanvas, wasm_memory } = glue;
const nowMs = Date.now();
const SIZE = 480;

const json = new TextEncoder().encode(
  JSON.stringify({ v: 1, k: "B".padEnd(87, "k"), t: 1760000000, l: "Chrome on macOS", s: "s".repeat(86) }),
);
assert.ok(json.length > 200 && json.length < 300, `payload is ${json.length} bytes`);

for (const [name, payload, profile, motion] of [
  ["digits, animated", new TextEncoder().encode("482915"), "s", true],
  ["json, animated", json, "s", true],
  ["json, static", json, "s", false],
  ["digits, profile m", new TextEncoder().encode("482915"), "m", true],
]) {
  const code = new ParticleCanvas(payload, profile, 10, nowMs / 1000, motion);
  assert.equal(code.fps(), 10, name);
  assert.ok(Number.isInteger(code.session_id()), name);
  assert.equal(code.pixels_len(), 0, `${name}: nothing painted yet`);

  assert.equal(code.tick(0, nowMs, SIZE), true, `${name}: first tick paints`);
  assert.equal(code.pixels_len(), SIZE * SIZE * 4, `${name}: length is size^2 * 4`);
  assert.equal(code.size_px(), SIZE, name);

  // Zero-copy view over the module's memory equals the copy.
  const view = new Uint8ClampedArray(wasm_memory().buffer, code.pixels_ptr(), code.pixels_len());
  const copy = code.pixels_copy();
  assert.equal(copy.length, view.length, name);
  assert.ok(Buffer.from(copy.buffer, copy.byteOffset, copy.length).equals(Buffer.from(view.buffer, view.byteOffset, view.length)), `${name}: view == copy`);

  let opaque = true;
  let distinct = new Set();
  for (let i = 0; i < view.length; i += 4) {
    if (view[i + 3] !== 255) opaque = false;
    distinct.add((view[i] << 16) | (view[i + 1] << 8) | view[i + 2]);
  }
  assert.ok(opaque, `${name}: opaque`);
  assert.ok(distinct.size > 4, `${name}: more than a flat image (${distinct.size} colours)`);

  const second = code.tick(33, nowMs + 33, SIZE);
  assert.equal(second, motion, `${name}: a later tick in the same data frame repaints only when animated`);
  assert.equal(code.tick(100, nowMs + 100, SIZE), true, `${name}: next data frame`);
  assert.equal(code.tick(100, nowMs + 100, SIZE), false, `${name}: same time again, nothing to do`);
  code.free();
}

// Errors arrive as exceptions, not aborts.
assert.throws(() => new ParticleCanvas(json, "x", 10, nowMs / 1000, true), /profile/);
assert.throws(() => new ParticleCanvas(json, "s", 0, nowMs / 1000, true), /fps/);
assert.throws(() => new ParticleCanvas(json, "s", 10, Number.NaN, true), /clock/);

// Timing, informational: ticks of an animated code at a typical console size.
const code = new ParticleCanvas(json, "s", 10, nowMs / 1000, true);
code.tick(0, nowMs, 656);
const N = 30;
const t0 = performance.now();
for (let k = 1; k <= N; k++) code.tick(k * 16.7, nowMs + k * 17, 656);
console.log(`ok: ${((performance.now() - t0) / N).toFixed(1)} ms per tick at 656 px`);
