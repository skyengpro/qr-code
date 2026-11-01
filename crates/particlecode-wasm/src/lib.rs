//! # particlecode-wasm
//!
//! The Particle Code sender for the browser: payload bytes in, animated RGBA frames out, ready
//! for `ctx.putImageData` on a `<canvas>`. It is the Rust half of what the Flutter
//! `ParticleCodeView` does on a phone, built from the same crates:
//!
//! * the frames come from [`particlecode_core::session::Session`];
//! * the pixels come from [`particlecode_render::render_animated`], so geometry and palette
//!   have exactly one source of truth;
//! * the frame schedule (data index from elapsed time, the 40 ms cross-fade, the liveness
//!   second) is a port of `flutter/particle_code/lib/src/view.dart`'s `_onTick`.
//!
//! **Rust owns the clocks, JavaScript owns the timer.** The page calls
//! [`ParticleCanvas::tick`] from `requestAnimationFrame` with the elapsed time and the wall
//! clock and blits the buffer when it returns `true`. Nothing in here reads a clock
//! (`SystemTime::now()` panics on `wasm32-unknown-unknown`), so everything is a pure function
//! of the arguments, and the same inputs give the same pixels on every platform.
//!
//! ```text
//! const code = new ParticleCanvas(payload, "s", 10, Date.now() / 1000, true);
//! // every animation frame:
//! if (code.tick(performance.now() - t0, Date.now(), size)) {
//!   const px = new Uint8ClampedArray(wasm_memory().buffer, code.pixels_ptr(), code.pixels_len());
//!   ctx.putImageData(new ImageData(px, size, size), 0, 0);
//! }
//! ```

use particlecode_core::FrameSpec;
use particlecode_core::error::EncodeError;
use particlecode_core::motion::{MotionConfig, seed_from_session};
use particlecode_core::profile::Profile;
use particlecode_core::session::{DisplayInfo, Session, SessionConfig, random_identity};
use particlecode_render::{RenderConfig, render, render_animated};
use wasm_bindgen::prelude::*;

/// Largest data frame rate accepted; far above anything a display can show.
pub const MAX_FPS: u32 = 240;

/// Why a [`ParticleCanvas`] could not be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CanvasError {
    /// The profile string was not `"s"`, `"m"` or `"l"`.
    #[error("unknown profile {0:?} (expected \"s\", \"m\" or \"l\")")]
    Profile(String),
    /// The frame rate was zero or above [`MAX_FPS`].
    #[error("fps must be 1..={MAX_FPS}, got {0}")]
    Fps(u32),
    /// The clock origin was negative, NaN or infinite.
    #[error("clock origin must be a finite, non-negative number of seconds, got {0}")]
    ClockOrigin(f64),
    /// The core refused the payload or configuration.
    #[error(transparent)]
    Encode(#[from] EncodeError),
}

/// What the pixel buffer currently shows, to skip a repaint that cannot change anything.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drawn {
    index: u32,
    size_px: u32,
    /// `elapsed_ms` of the paint; `None` for a static code, which only depends on the frame.
    elapsed_ms: Option<f64>,
}

/// An animated (or static) Particle Code painter for a `<canvas>`.
#[wasm_bindgen]
#[derive(Debug)]
pub struct ParticleCanvas {
    session: Session,
    fps: u32,
    /// `None` paints the static code (no drift, no cross-fade), like `ParticleCodeView` with
    /// `motion: null`.
    motion: Option<MotionConfig>,
    seed: u64,
    /// The data frame on screen, its index and the one before it (for the cross-fade).
    current: Option<FrameSpec>,
    previous: Option<FrameSpec>,
    index: u32,
    drawn: Option<Drawn>,
    size_px: u32,
    /// Straight RGBA, `size_px * size_px * 4` bytes. The background is opaque and the painter
    /// never draws translucent pixels onto it, so premultiplied and straight alpha coincide.
    pixels: Vec<u8>,
}

impl ParticleCanvas {
    /// [`ParticleCanvas::new`] without the JavaScript error type: a random session id and key
    /// from the OS (`crypto.getRandomValues` in a browser).
    pub fn try_new(
        payload: &[u8],
        profile: &str,
        fps: u32,
        clock_origin_unix: f64,
        motion: bool,
    ) -> Result<ParticleCanvas, CanvasError> {
        let (session_id, key) = random_identity()?;
        ParticleCanvas::with_identity(
            payload,
            profile,
            fps,
            clock_origin_unix,
            motion,
            session_id,
            key,
        )
    }

    /// Build a canvas with an explicit session identity. The frames are then a pure function
    /// of the arguments, which is what the tests rely on.
    pub fn with_identity(
        payload: &[u8],
        profile: &str,
        fps: u32,
        clock_origin_unix: f64,
        motion: bool,
        session_id: u16,
        key: [u8; 32],
    ) -> Result<ParticleCanvas, CanvasError> {
        let profile = parse_profile(profile)?;
        let rate = u16::try_from(fps)
            .ok()
            .filter(|f| *f >= 1 && u32::from(*f) <= MAX_FPS)
            .ok_or(CanvasError::Fps(fps))?;
        if !clock_origin_unix.is_finite() || clock_origin_unix < 0.0 {
            return Err(CanvasError::ClockOrigin(clock_origin_unix));
        }
        // The profile is fixed, so the display only has to be well-formed.
        let display = DisplayInfo {
            width_px: 1080,
            height_px: 1080,
            width_mm: 100.0,
            height_mm: 100.0,
        };
        let config = SessionConfig {
            profile: Some(profile),
            fps: rate,
            ..SessionConfig::default()
        };
        let session = Session::with_identity(
            payload,
            display,
            &config,
            session_id,
            key,
            clock_origin_unix.floor() as u64,
        )?;
        Ok(ParticleCanvas {
            seed: seed_from_session(session.session_id()),
            session,
            fps: u32::from(rate),
            motion: motion.then(MotionConfig::default),
            current: None,
            previous: None,
            index: 0,
            drawn: None,
            size_px: 0,
            pixels: Vec::new(),
        })
    }

    /// The straight-RGBA pixels of the last [`tick`](ParticleCanvas::tick) that returned `true`.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Index of the data frame on screen (`floor(elapsed * fps / 1000)` at the last tick).
    pub fn frame_index(&self) -> u32 {
        self.index
    }

    /// Bring `current`/`previous` to the data frame due at `elapsed_ms`. Returns whether a new
    /// frame was fetched.
    ///
    /// Like the Flutter view, the index comes from elapsed time, never from the number of
    /// ticks, so a 120 Hz display repaints one frame several times. A frame is built with the
    /// wall-clock second at which it is fetched, so the particles carry the live liveness
    /// value. Time running backwards (a restarted clock) drops the previous frame.
    fn advance(&mut self, elapsed_ms: f64, now_unix_ms: f64) -> bool {
        let due = data_index(elapsed_ms, self.fps);
        if self.current.is_some() && due == self.index {
            return false;
        }
        let unix = unix_seconds(now_unix_ms);
        let next = self.session.frame_at(due, unix);
        self.previous = if due > self.index {
            self.current.take()
        } else {
            None
        };
        self.current = Some(next);
        self.index = due;
        true
    }

    /// Paint the current frame into `pixels`.
    fn paint(&mut self, elapsed_ms: f64, size_px: u32) {
        let Some(frame) = self.current.as_ref() else {
            return;
        };
        let cfg = RenderConfig {
            size_px,
            fps: self.fps,
            ..RenderConfig::default()
        };
        let pixmap = match &self.motion {
            Some(motion) => {
                let t = elapsed_ms.max(0.0) / 1000.0;
                // Seconds since the switch to this frame; unbounded when there is no previous
                // frame so the cross-fade is skipped.
                let since_switch = match self.previous {
                    Some(_) => t - f64::from(self.index) / f64::from(self.fps),
                    None => f64::INFINITY,
                };
                render_animated(
                    frame,
                    self.previous.as_ref(),
                    &cfg,
                    motion,
                    self.seed,
                    t,
                    since_switch,
                )
            }
            None => render(frame, &cfg),
        };
        // The painter allocates its own pixmap (tiny-skia owns that buffer); copy into the
        // persistent one so JavaScript can keep a single view over the same bytes. Same
        // length at the same size, so after the first paint this reuses the capacity.
        self.pixels.clear();
        self.pixels.extend_from_slice(pixmap.data());
        self.size_px = pixmap.width();
    }
}

#[wasm_bindgen]
impl ParticleCanvas {
    /// Start a session for `payload`.
    ///
    /// * `profile` - lattice density: `"s"` (48 dots across), `"m"` (72) or `"l"` (96).
    /// * `fps` - data frames per second (`1..=240`); the receiver needs a few distinct frames
    ///   per payload, so 10 is the usual rate.
    /// * `clock_origin_unix` - the Unix time in **seconds** at which frame 0 is shown
    ///   (`Date.now() / 1000`).
    /// * `motion` - `true` for the animated code, `false` for a static one that only
    ///   repaints when the data frame changes (for `prefers-reduced-motion`).
    ///
    /// The session id and the 32-byte key are random (`crypto.getRandomValues`).
    ///
    /// # Errors
    ///
    /// Throws when `profile` or `fps` is out of range, the clock origin is not a finite
    /// non-negative number, or the payload does not fit (empty, or over 16 MiB).
    #[wasm_bindgen(constructor)]
    pub fn new(
        payload: &[u8],
        profile: &str,
        fps: u32,
        clock_origin_unix: f64,
        motion: bool,
    ) -> Result<ParticleCanvas, JsError> {
        Ok(ParticleCanvas::try_new(
            payload,
            profile,
            fps,
            clock_origin_unix,
            motion,
        )?)
    }

    /// Paint the code for `elapsed_ms` milliseconds after the animation started.
    ///
    /// * `elapsed_ms` - a monotonic clock since the start, e.g. `performance.now() - t0`. It
    ///   picks the data frame (`floor(elapsed * fps / 1000)`) and drives drift, cross-fade
    ///   and ripple. Negative or NaN counts as 0.
    /// * `now_unix_ms` - the wall clock, `Date.now()`; its second is the liveness value the
    ///   particles show, so a verifier with a synchronised clock agrees.
    /// * `size_px` - side of the square image in pixels (clamped to `1..=8192`). Changing it
    ///   repaints.
    ///
    /// Returns `true` when [`pixels_ptr`](Self::pixels_ptr) now holds different pixels than
    /// after the previous call. An animated code changes on every new `elapsed_ms`; a static
    /// one only when the data frame or the size changes.
    pub fn tick(&mut self, elapsed_ms: f64, now_unix_ms: f64, size_px: u32) -> bool {
        let elapsed_ms = if elapsed_ms.is_finite() {
            elapsed_ms.max(0.0)
        } else {
            0.0
        };
        let size_px = size_px.clamp(1, RenderConfig::MAX_SIZE_PX);
        self.advance(elapsed_ms, now_unix_ms);
        let wanted = Drawn {
            index: self.index,
            size_px,
            elapsed_ms: self.motion.is_some().then_some(elapsed_ms),
        };
        if self.drawn == Some(wanted) {
            return false;
        }
        self.paint(elapsed_ms, size_px);
        self.drawn = Some(wanted);
        true
    }

    /// Forget the schedule so the next [`tick`](Self::tick) starts over, for a clock that
    /// restarts (the page was hidden and `elapsed_ms` begins again at 0). The session, and so
    /// the code's identity, stays.
    pub fn reset(&mut self) {
        self.current = None;
        self.previous = None;
        self.index = 0;
        self.drawn = None;
    }

    /// Address of the pixel buffer in the module's linear memory, for a zero-copy
    /// `new Uint8ClampedArray(wasm_memory().buffer, ptr, len)`.
    ///
    /// Reading the address is safe Rust (`as_ptr`), and the view is created on the JavaScript
    /// side, so this keeps the hot path free of copies and of `unsafe`. Read it again after
    /// every `tick` that returned `true`: a size change can move the buffer, and growing the
    /// module's memory detaches every older view.
    pub fn pixels_ptr(&self) -> usize {
        self.pixels.as_ptr() as usize
    }

    /// Length of the pixel buffer in bytes: `size_px * size_px * 4`.
    pub fn pixels_len(&self) -> usize {
        self.pixels.len()
    }

    /// A copy of the pixels, for callers that cannot reach the module's memory. Allocates
    /// on every call; prefer [`pixels_ptr`](Self::pixels_ptr).
    pub fn pixels_copy(&self) -> Vec<u8> {
        self.pixels.clone()
    }

    /// Side of the painted image in pixels (0 before the first `tick`).
    pub fn size_px(&self) -> u32 {
        self.size_px
    }

    /// The 16-bit session id carried by every frame header.
    pub fn session_id(&self) -> u16 {
        self.session.session_id()
    }

    /// Data frames per second.
    pub fn fps(&self) -> u32 {
        self.fps
    }
}

/// The module's linear memory (a `WebAssembly.Memory`), so a page can build the zero-copy view
/// `new Uint8ClampedArray(wasm_memory().buffer, code.pixels_ptr(), code.pixels_len())` without
/// keeping the object `initSync`/`init` returned.
#[wasm_bindgen]
pub fn wasm_memory() -> JsValue {
    wasm_bindgen::memory()
}

/// `floor(elapsed_ms * fps / 1000)`, the data frame due at `elapsed_ms` (0 for negative or
/// non-finite time, saturating at `u32::MAX`).
pub fn data_index(elapsed_ms: f64, fps: u32) -> u32 {
    if !elapsed_ms.is_finite() || elapsed_ms <= 0.0 {
        return 0;
    }
    (elapsed_ms * f64::from(fps) / 1000.0).floor() as u32
}

/// Whole Unix seconds of a millisecond wall-clock reading (0 for negative or non-finite).
fn unix_seconds(now_unix_ms: f64) -> u64 {
    if now_unix_ms.is_finite() && now_unix_ms > 0.0 {
        (now_unix_ms / 1000.0).floor() as u64
    } else {
        0
    }
}

fn parse_profile(profile: &str) -> Result<Profile, CanvasError> {
    match profile.to_ascii_lowercase().as_str() {
        "s" => Ok(Profile::S),
        "m" => Ok(Profile::M),
        "l" => Ok(Profile::L),
        _ => Err(CanvasError::Profile(profile.to_owned())),
    }
}

#[cfg(test)]
mod tests;
