//! PNG sequence + `manifest.json` packager.

use std::fs;
use std::path::Path;

use particlecode_core::FrameSpec;
use particlecode_core::profile::{Profile, colors};
use serde::{Deserialize, Serialize};

use crate::color::hex;
use crate::config::RenderConfig;
use crate::error::RenderError;
use crate::paint::render;
use crate::png_out::encode_png;

/// One frame of a [`Manifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFrame {
    /// File name inside the sequence directory (no path separators).
    pub file: String,
    /// Position in the sequence (0-based).
    pub index: u32,
    /// Segment this frame's packet belongs to.
    pub segment_index: u16,
    /// `RaptorQ` encoding symbol id of the packet.
    pub packet_seed: u16,
}

/// Description of a PNG sequence, written next to the frames as `manifest.json` so a
/// designer or a later job can pick frames, re-time them or build a video.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Session id from the frame headers.
    pub session_id: u16,
    /// Lattice profile.
    pub profile: Profile,
    /// Shape (mask) id.
    pub shape_id: u8,
    /// Payload length in bytes.
    pub payload_len: u32,
    /// Segment size is `2^n` KiB.
    pub segment_kb_log2: u8,
    /// Number of segments (derived from the header, never transmitted).
    pub segment_count: usize,
    /// Data frames per second.
    pub fps: u32,
    /// Side of every PNG in pixels.
    pub frame_size_px: u32,
    /// `#rrggbb` colours in this order: the four data hues, background, anchor, white,
    /// black, particle.
    pub palette: Vec<String>,
    /// The frames, in display order.
    pub frames: Vec<ManifestFrame>,
}

impl Manifest {
    /// Derive a manifest from the frames' headers and the render config. Session-level
    /// fields come from the first frame; frame file names are `frame_00000.png`, ...
    pub fn from_frames(frames: &[FrameSpec], cfg: &RenderConfig) -> Result<Manifest, RenderError> {
        let first = frames.first().ok_or(RenderError::NoFrames)?;
        let h = &first.header;
        let palette = colors::HUES4
            .into_iter()
            .chain([
                cfg.background,
                colors::ANCHOR,
                colors::WHITE,
                colors::BLACK,
                colors::PARTICLE,
            ])
            .map(hex)
            .collect();
        Ok(Manifest {
            session_id: h.session_id,
            profile: h.profile,
            shape_id: h.shape_id,
            payload_len: h.payload_len,
            segment_kb_log2: h.segment_kb_log2,
            segment_count: h.segment_count(),
            fps: cfg.fps,
            frame_size_px: cfg.size_px,
            palette,
            frames: frames
                .iter()
                .enumerate()
                .map(|(i, f)| ManifestFrame {
                    file: format!("frame_{i:05}.png"),
                    index: i as u32,
                    segment_index: f.header.segment_index,
                    packet_seed: f.header.packet_seed,
                })
                .collect(),
        })
    }
}

/// A manifest file name must stay inside the output directory.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// Write every frame as a PNG (named by `manifest.frames[i].file`) plus `manifest.json`
/// into `dir`, creating it if needed. Never JPEG: it would smear the colours.
///
/// `manifest.frames` must have one entry per frame and file names without path separators
/// (use [`Manifest::from_frames`] to build one).
pub fn write_sequence(
    dir: &Path,
    frames: &[FrameSpec],
    cfg: &RenderConfig,
    manifest: &Manifest,
) -> Result<(), RenderError> {
    cfg.validate()?;
    if manifest.frames.len() != frames.len() {
        return Err(RenderError::ManifestMismatch(format!(
            "{} manifest entries for {} frames",
            manifest.frames.len(),
            frames.len()
        )));
    }
    if let Some(bad) = manifest.frames.iter().find(|m| !safe_name(&m.file)) {
        return Err(RenderError::ManifestMismatch(format!(
            "unsafe file name {:?}",
            bad.file
        )));
    }
    fs::create_dir_all(dir)?;
    for (frame, entry) in frames.iter().zip(&manifest.frames) {
        fs::write(dir.join(&entry.file), encode_png(&render(frame, cfg))?)?;
    }
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(manifest)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        assert!(safe_name("frame_00000.png"));
        assert!(!safe_name("../x.png"));
        assert!(!safe_name("a/b.png"));
        assert!(!safe_name(".."));
        assert!(!safe_name(""));
    }
}
