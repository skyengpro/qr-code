//! Plain data types that cross the language boundary, and the palette.
//!
//! Every struct has public fields and owns its data, so `flutter_rust_bridge` can translate
//! it to a Dart class by value. Dot fields are structure-of-arrays ([`DotsDto`]) so Dart
//! receives a few typed lists instead of thousands of small objects.

use flutter_rust_bridge::frb;

use particlecode_core::FrameSpec;
use particlecode_core::layout::{SwatchColor, dims};
use particlecode_core::profile::{Profile, colors};
use particlecode_core::session::{DisplayInfo, SessionConfig, ShapeChoice};
use particlecode_core::{DecodeError, Progress};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------

/// Lattice density. Mirrors [`Profile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileDto {
    /// 48 dots across the field.
    S,
    /// 72 dots across the field (the default).
    M,
    /// 96 dots across the field.
    L,
}

impl From<Profile> for ProfileDto {
    fn from(p: Profile) -> Self {
        match p {
            Profile::S => ProfileDto::S,
            Profile::M => ProfileDto::M,
            Profile::L => ProfileDto::L,
        }
    }
}

impl From<ProfileDto> for Profile {
    fn from(p: ProfileDto) -> Self {
        match p {
            ProfileDto::S => Profile::S,
            ProfileDto::M => Profile::M,
            ProfileDto::L => Profile::L,
        }
    }
}

/// Which mask the dot field uses. Mirrors [`ShapeChoice`].
///
/// In JSON: `{"kind":"require","id":0}`, `{"kind":"prefer","id":28}`,
/// `{"kind":"auto","ids":[2,3]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShapeChoiceDto {
    /// Use exactly this shape id; fail if unknown or it does not fit.
    Require {
        /// Shape id.
        id: u8,
    },
    /// Try this shape first; fall back to the disc.
    Prefer {
        /// Shape id.
        id: u8,
    },
    /// Try these shape ids in order; the disc is the last resort.
    Auto {
        /// Shape ids, best first.
        ids: Vec<u8>,
    },
}

impl From<ShapeChoiceDto> for ShapeChoice {
    fn from(s: ShapeChoiceDto) -> Self {
        match s {
            ShapeChoiceDto::Require { id } => ShapeChoice::Require(id),
            ShapeChoiceDto::Prefer { id } => ShapeChoice::Prefer(id),
            ShapeChoiceDto::Auto { ids } => ShapeChoice::Auto(ids),
        }
    }
}

/// Physical display information used to pick a profile. Mirrors [`DisplayInfo`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayInfoDto {
    /// Width of the area available to the code, in pixels.
    pub width_px: u32,
    /// Height of the area available to the code, in pixels.
    pub height_px: u32,
    /// Physical width of that area in millimetres.
    pub width_mm: f32,
    /// Physical height of that area in millimetres.
    pub height_mm: f32,
}

impl Default for DisplayInfoDto {
    /// A typical phone: 1080 x 2400 px, 68 x 151 mm.
    fn default() -> Self {
        DisplayInfoDto {
            width_px: 1080,
            height_px: 2400,
            width_mm: 68.0,
            height_mm: 151.0,
        }
    }
}

impl From<DisplayInfoDto> for DisplayInfo {
    fn from(d: DisplayInfoDto) -> Self {
        DisplayInfo {
            width_px: d.width_px,
            height_px: d.height_px,
            width_mm: d.width_mm,
            height_mm: d.height_mm,
        }
    }
}

/// Everything [`ParticleEncoder::new`](super::encoder::ParticleEncoder::new) needs besides
/// the payload. Mirrors [`SessionConfig`] plus the session identity and display.
///
/// Every field has a default, so a JSON config may omit any of them (`{}` is a valid
/// config).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EncoderConfig {
    /// Segment size is `1024 << segment_kb_log2` bytes (`0..=15`). Default 3 (8 KiB).
    pub segment_kb_log2: u8,
    /// Fixed profile, or `None` to pick one from `display` and `min_pitch_mm`.
    /// Default `Some(M)`.
    pub profile: Option<ProfileDto>,
    /// Data frames per second; also the rate of the liveness clock in `frame`. Default 10.
    pub fps: u16,
    /// Emit a single self-contained frame (print, poster) with the stronger ECC.
    pub is_static: bool,
    /// Mask selection. Default `Require(0)` (the disc).
    pub shape: ShapeChoiceDto,
    /// Repair packets appended to each segment per pass. Default 2.
    pub repair_per_pass: u32,
    /// Smallest acceptable dot pitch in millimetres for automatic profile selection.
    /// Default 1.6.
    pub min_pitch_mm: f32,
    /// The display the code will be shown on.
    pub display: DisplayInfoDto,
    /// Fixed 16-bit session id, or `None` for a random one.
    pub session_id: Option<u16>,
    /// Fixed 32-byte session key, or `None` for a random one.
    pub key: Option<Vec<u8>>,
    /// Unix second at which frame 0 is shown (the liveness clock origin), or `None` for
    /// the current wall clock.
    pub clock_origin_unix: Option<u64>,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        let core = SessionConfig::default();
        EncoderConfig {
            segment_kb_log2: core.segment_kb_log2,
            profile: core.profile.map(ProfileDto::from),
            fps: core.fps,
            is_static: core.is_static,
            shape: ShapeChoiceDto::Require { id: 0 },
            repair_per_pass: core.repair_per_pass,
            min_pitch_mm: core.min_pitch_mm,
            display: DisplayInfoDto::default(),
            session_id: None,
            key: None,
            clock_origin_unix: None,
        }
    }
}

impl EncoderConfig {
    /// The core configuration (without identity and display).
    pub(crate) fn session_config(&self) -> SessionConfig {
        SessionConfig {
            segment_kb_log2: self.segment_kb_log2,
            profile: self.profile.map(Profile::from),
            fps: self.fps,
            is_static: self.is_static,
            shape: self.shape.clone().into(),
            repair_per_pass: self.repair_per_pass,
            min_pitch_mm: self.min_pitch_mm,
            ..SessionConfig::default()
        }
    }
}

/// Facts about an encoder session, for UI and diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncoderInfo {
    /// Profile of every frame.
    pub profile: ProfileDto,
    /// Shape id of every frame.
    pub shape_id: u8,
    /// Payload dots per frame.
    pub dot_count: u32,
    /// Payload bytes carried per frame (the fountain symbol size).
    pub block_size: u32,
    /// Number of segments.
    pub segment_count: u32,
    /// Frames in one pass over all segments.
    pub frames_per_pass: u32,
    /// Session id carried by every header.
    pub session_id: u16,
    /// Data frames per second the session was configured with.
    pub fps: u16,
    /// Frame half-side `R` in pitch units.
    pub frame_half_side: f64,
    /// Quiet zone around the frame, in pitch units.
    pub quiet_zone: f64,
}

// ---------------------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------------------

/// A ring-and-core anchor mark. Draw the ring (annulus `ring_in..ring_out`), then the
/// solid core, then the background-colored pupil if `pupil_r > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AnchorDto {
    /// Center x, pitch units.
    pub x: f64,
    /// Center y, pitch units, down.
    pub y: f64,
    /// Solid core radius.
    pub core_r: f64,
    /// Ring inner radius.
    pub ring_in: f64,
    /// Ring outer radius.
    pub ring_out: f64,
    /// Background-colored pupil radius inside the core (0 = none).
    pub pupil_r: f64,
}

/// What a calibration swatch shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SwatchColorDto {
    /// A data hue: index `0..4` into [`PaletteDto::hues`].
    Hue {
        /// Hue index.
        index: u8,
    },
    /// The white reference ([`PaletteDto::white`]).
    White,
    /// The black reference ([`PaletteDto::black`]).
    Black,
}

/// A calibration swatch (a plain disc of radius [`FrameDto::swatch_r`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SwatchDto {
    /// Center x, pitch units.
    pub x: f64,
    /// Center y, pitch units, down.
    pub y: f64,
    /// Color.
    pub color: SwatchColorDto,
    /// Which hero anchor's halo it belongs to (`0..3`: top, lower right, lower left).
    pub anchor: u8,
    /// Slot on the halo (`0..16`); slot 0 points away from the frame center.
    pub slot: u8,
}

/// A particle speck at its rest angle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ParticleDto {
    /// Encoded sector `0..16` on this particle's arc.
    pub sector: u8,
    /// Rest angle in radians (sector center), measured from +x towards +y (y down).
    pub angle: f64,
}

/// Dots as parallel arrays: dot `i` is at `(xs[i], ys[i])` (lattice cell `(qs[i], rs[i])`)
/// with hue `colors[i]` (`0..4`) and size class `sizes[i]` (0 small, 1 large). All six
/// vectors have equal length.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DotsDto {
    /// Axial lattice column of each dot (0 for header dots). Painters match dots across
    /// frames by `(q, r)` for the cross-fade.
    pub qs: Vec<i32>,
    /// Axial lattice row of each dot (0 for header dots).
    pub rs: Vec<i32>,
    /// Rest x of each dot, pitch units.
    pub xs: Vec<f64>,
    /// Rest y of each dot, pitch units, down.
    pub ys: Vec<f64>,
    /// Hue index `0..4` into [`PaletteDto::hues`].
    pub colors: Vec<u8>,
    /// Size class: 0 small ([`FrameDto::dot_r_small`]), 1 large ([`FrameDto::dot_r_large`]).
    pub sizes: Vec<u8>,
}

impl DotsDto {
    fn from_dots(dots: &[particlecode_core::frame::Dot]) -> DotsDto {
        DotsDto {
            qs: dots.iter().map(|d| d.q).collect(),
            rs: dots.iter().map(|d| d.r).collect(),
            xs: dots.iter().map(|d| d.at.x).collect(),
            ys: dots.iter().map(|d| d.at.y).collect(),
            colors: dots.iter().map(|d| d.color).collect(),
            sizes: dots.iter().map(|d| d.size).collect(),
        }
    }
}

/// Everything a painter needs to draw one data frame. Coordinates are in pitch units with
/// the origin at the frame center and `y` down. Draw back to front: background, anchors,
/// swatches, header dots, data dots, particles (see the crate README).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameDto {
    /// Frame index within the session.
    pub index: u32,
    /// Session id of the header this frame carries.
    pub session_id: u16,
    /// Segment this frame belongs to.
    pub segment_index: u16,
    /// Packet seed (fountain symbol id) of this frame.
    pub packet_seed: u16,
    /// Payload length in bytes, from the header.
    pub payload_len: u32,
    /// Segment size is `1024 << segment_kb_log2` bytes, from the header.
    pub segment_kb_log2: u8,
    /// Shape table version, from the header.
    pub shape_table_version: u8,
    /// Hues in the palette: 4 (8 is reserved).
    pub palette_hues: u8,
    /// Profile.
    pub profile: ProfileDto,
    /// Shape id of the dot field.
    pub shape_id: u8,
    /// Whether this is a static (print) frame.
    pub is_static: bool,
    /// Frame half-side `R`; the quiet zone lies outside `[-R, R]^2`.
    pub frame_half_side: f64,
    /// Quiet zone width around the frame, in pitch units.
    pub quiet_zone: f64,
    /// Half-side of the whole drawing, `frame_half_side + quiet_zone`. A painter scales by
    /// `size_px / (2 * view_half_side)`.
    pub view_half_side: f64,
    /// Inner radius of the particle band.
    pub particle_band_inner: f64,
    /// Outer radius of the particle band.
    pub particle_band_outer: f64,
    /// Radius of a small data dot.
    pub dot_r_small: f64,
    /// Radius of a large data dot.
    pub dot_r_large: f64,
    /// Radius of a calibration swatch.
    pub swatch_r: f64,
    /// Radius of a particle speck.
    pub particle_r: f64,
    /// Three hero anchors (top, lower right, lower left) then the center anchor.
    pub anchors: Vec<AnchorDto>,
    /// The 48 calibration swatches.
    pub swatches: Vec<SwatchDto>,
    /// The 48 header dots.
    pub header_dots: DotsDto,
    /// The payload dots, in symbol order.
    pub dots: DotsDto,
    /// The six particles.
    pub particles: Vec<ParticleDto>,
}

impl From<&FrameSpec> for FrameDto {
    fn from(spec: &FrameSpec) -> Self {
        let h = &spec.header;
        FrameDto {
            index: spec.index,
            session_id: h.session_id,
            segment_index: h.segment_index,
            packet_seed: h.packet_seed,
            payload_len: h.payload_len,
            segment_kb_log2: h.segment_kb_log2,
            shape_table_version: h.shape_table_version,
            palette_hues: match h.palette {
                particlecode_core::Palette::Hues4 => 4,
                particlecode_core::Palette::Hues8 => 8,
            },
            profile: spec.profile.into(),
            shape_id: h.shape_id,
            is_static: h.is_static,
            frame_half_side: spec.frame_half_side,
            quiet_zone: dims::QUIET_ZONE,
            view_half_side: spec.frame_half_side + dims::QUIET_ZONE,
            particle_band_inner: spec.particle_band.inner,
            particle_band_outer: spec.particle_band.outer,
            dot_r_small: dims::DOT_R_SMALL,
            dot_r_large: dims::DOT_R_LARGE,
            swatch_r: dims::SWATCH_DOT_R,
            particle_r: dims::PARTICLE_R,
            anchors: spec
                .anchors
                .iter()
                .map(|a| AnchorDto {
                    x: a.at.x,
                    y: a.at.y,
                    core_r: a.core_r,
                    ring_in: a.ring_in,
                    ring_out: a.ring_out,
                    pupil_r: a.pupil_r,
                })
                .collect(),
            swatches: spec
                .swatches
                .iter()
                .map(|s| SwatchDto {
                    x: s.at.x,
                    y: s.at.y,
                    color: match s.color {
                        SwatchColor::Hue(index) => SwatchColorDto::Hue { index },
                        SwatchColor::White => SwatchColorDto::White,
                        SwatchColor::Black => SwatchColorDto::Black,
                    },
                    anchor: s.anchor,
                    slot: s.slot,
                })
                .collect(),
            header_dots: DotsDto::from_dots(&spec.header_dots),
            dots: DotsDto::from_dots(&spec.dots),
            particles: spec
                .particles
                .iter()
                .map(|p| ParticleDto {
                    sector: p.sector,
                    angle: p.angle,
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------------------

/// An sRGB color. Painters must use these exact values: no color management, no filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbDto {
    /// Red, 0..=255.
    pub r: u8,
    /// Green, 0..=255.
    pub g: u8,
    /// Blue, 0..=255.
    pub b: u8,
}

impl From<particlecode_core::profile::Rgb> for RgbDto {
    fn from(c: particlecode_core::profile::Rgb) -> Self {
        RgbDto {
            r: c.0,
            g: c.1,
            b: c.2,
        }
    }
}

/// The exact palette of [`particlecode_core::profile::colors`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteDto {
    /// The four data hues, indexed by dot color: blue, green, orange, magenta.
    pub hues: Vec<RgbDto>,
    /// Quiet zone and field background.
    pub background: RgbDto,
    /// Anchor rings and cores.
    pub anchor: RgbDto,
    /// White calibration swatch.
    pub white: RgbDto,
    /// Black calibration swatch.
    pub black: RgbDto,
    /// Particle specks.
    pub particle: RgbDto,
}

/// The palette every painter must draw with.
#[frb(sync)]
pub fn palette() -> PaletteDto {
    PaletteDto {
        hues: colors::HUES4.iter().map(|&c| c.into()).collect(),
        background: colors::BACKGROUND.into(),
        anchor: colors::ANCHOR.into(),
        white: colors::WHITE.into(),
        black: colors::BLACK.into(),
        particle: colors::PARTICLE.into(),
    }
}

// ---------------------------------------------------------------------------------------
// Decoder progress
// ---------------------------------------------------------------------------------------

/// What happened to one fed frame. Mirrors [`Progress`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ProgressDto {
    /// The frame was not used (bad header, uncorrectable body, other session, ...).
    Rejected {
        /// Why, in words.
        reason: String,
    },
    /// The frame was accepted (possibly as a duplicate).
    Accepted {
        /// Distinct symbols held, summed over segments (a decoded segment counts fully).
        have: u32,
        /// Symbols needed if nothing is lost.
        need: u32,
        /// Segments decoded and verified so far.
        segments_done: u32,
        /// Segments in the transfer.
        segments_total: u32,
    },
    /// The whole payload arrived and passed its CRC-32.
    Done {
        /// The payload.
        payload: Vec<u8>,
    },
}

impl From<Progress> for ProgressDto {
    fn from(p: Progress) -> Self {
        match p {
            Progress::Rejected(e) => ProgressDto::rejected(&e),
            Progress::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            } => ProgressDto::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            },
            Progress::Done(payload) => ProgressDto::Done { payload },
        }
    }
}

impl ProgressDto {
    /// A rejection with the core error's message.
    pub(crate) fn rejected(e: &DecodeError) -> ProgressDto {
        ProgressDto::Rejected {
            reason: e.to_string(),
        }
    }

    /// A rejection that did not come from the core (bad lengths from the host).
    pub(crate) fn rejected_because(reason: &str) -> ProgressDto {
        ProgressDto::Rejected {
            reason: reason.into(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Camera frames
// ---------------------------------------------------------------------------------------

/// A point in camera-image pixels (origin top-left, `y` down).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ImagePointDto {
    /// Horizontal, rightwards.
    pub x: f64,
    /// Vertical, downwards.
    pub y: f64,
}

/// The particle liveness read from a camera frame. Mirrors the vision crate's `Liveness`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LivenessDto {
    /// The particles could not be read.
    Unknown,
    /// The particles were read.
    Read {
        /// Low byte of the packet seed, as shown by the particles.
        seed_low: u8,
        /// The 16-bit rolling HMAC value, as shown by the particles; check it with
        /// `liveness_verify`.
        rolling: u16,
        /// Whether `seed_low` matches the decoded header's packet seed.
        consistent: bool,
    },
}

impl From<particlecode_vision::Liveness> for LivenessDto {
    fn from(l: particlecode_vision::Liveness) -> Self {
        match l {
            particlecode_vision::Liveness::Unknown => LivenessDto::Unknown,
            particlecode_vision::Liveness::Read {
                seed_low,
                rolling,
                consistent,
            } => LivenessDto::Read {
                seed_low,
                rolling,
                consistent,
            },
        }
    }
}

/// A camera frame handed over by address: the planes stay in the camera's own memory and
/// the decoder reads them in place (see `ParticleDecoder::feed_camera_frame` for the
/// contract). Layout as in `ParticleDecoder::feed_yuv`: YUV 4:2:0, chroma at half resolution,
/// chroma sample `(col, row)` at `row * uv_stride + col * uv_pixel_stride`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraFrameDto {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Address of the first luma byte.
    pub y_address: usize,
    /// Readable bytes at `y_address`.
    pub y_len: usize,
    /// Address of the first Cb sample.
    pub u_address: usize,
    /// Readable bytes at `u_address`.
    pub u_len: usize,
    /// Address of the first Cr sample.
    pub v_address: usize,
    /// Readable bytes at `v_address`.
    pub v_len: usize,
    /// Bytes between luma rows.
    pub y_stride: u32,
    /// Bytes between chroma rows.
    pub uv_stride: u32,
    /// Bytes between chroma samples in a row: 1 planar, 2 interleaved.
    pub uv_pixel_stride: u32,
}

/// What happened to one camera frame (`feed_yuv`, `feed_camera_frame`, `feed_rgba`).
///
/// A frame that cannot be located or read is a normal outcome: `progress` is then
/// `Rejected`, `reject_reason` says why and `hint` (if any) is text for the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameResultDto {
    /// Whether the three anchors were found (the overlay may be drawn).
    pub locked: bool,
    /// Profile, once the frame's header was read.
    pub profile: Option<ProfileDto>,
    /// Shape id, once the frame's header was read.
    pub shape_id: Option<u8>,
    /// Anchor positions in image pixels for a UI overlay: top, lower right, lower left,
    /// then the center mark if it was found. Empty when not locked.
    pub anchors: Vec<ImagePointDto>,
    /// Sharpness score (higher is sharper; 0 when not locked). Comparable between frames of
    /// the same code at a similar scale: keep the sharpest of the captures of each data
    /// frame.
    pub sharpness: f64,
    /// Particle liveness, `Unknown` unless the frame was read.
    pub liveness: LivenessDto,
    /// What the receiver did with the frame.
    pub progress: ProgressDto,
    /// Technical reason when the frame was rejected, else `None`.
    pub reject_reason: Option<String>,
    /// Short user-facing advice ("too dark or glare", "move back"), else `None`.
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_is_exactly_the_core_palette() {
        let p = palette();
        assert_eq!(p.hues.len(), 4);
        assert_eq!(
            p.hues[0],
            RgbDto {
                r: 20,
                g: 110,
                b: 245
            }
        );
        assert_eq!(
            p.background,
            RgbDto {
                r: 246,
                g: 244,
                b: 239
            }
        );
        assert_eq!(
            p.black,
            RgbDto {
                r: 12,
                g: 12,
                b: 16
            }
        );
    }

    #[test]
    fn config_json_defaults_and_overrides() {
        let c: EncoderConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c, EncoderConfig::default());
        let c: EncoderConfig = serde_json::from_str(
            r#"{"profile":"L","shape":{"kind":"prefer","id":28},"session_id":7,
                "display":{"width_mm":100.0}}"#,
        )
        .unwrap();
        assert_eq!(c.profile, Some(ProfileDto::L));
        assert_eq!(c.shape, ShapeChoiceDto::Prefer { id: 28 });
        assert_eq!(c.session_id, Some(7));
        assert!((c.display.width_mm - 100.0).abs() < f32::EPSILON);
        assert_eq!(c.display.height_px, 2400);
    }
}
