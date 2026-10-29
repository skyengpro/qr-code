//! [`RenderConfig`]: the scale and look shared by every packager.

use particlecode_core::FrameSpec;
use particlecode_core::layout::dims;
use particlecode_core::profile::{Rgb, colors};

use crate::error::RenderError;

/// Output scale and look.
///
/// The square image covers `[-(R + margin), R + margin]²` in pitch units, where `R` is
/// [`FrameSpec::frame_half_side`], so one config renders every profile and shape at the
/// same pixel size.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderConfig {
    /// Image side in pixels (`1..=`[`RenderConfig::MAX_SIZE_PX`]).
    pub size_px: u32,
    /// Background and quiet-zone colour. Also the colour of the top anchor's pupil.
    pub background: Rgb,
    /// Quiet zone around `[-R, R]²`, in pitches. Values below
    /// [`dims::QUIET_ZONE`] (1.5) are raised to it.
    pub margin: f64,
    /// Data-frame rate used by the GIF and sequence packagers.
    pub fps: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            size_px: 800,
            background: colors::BACKGROUND,
            margin: 2.5,
            fps: 10,
        }
    }
}

impl RenderConfig {
    /// Largest supported image side.
    pub const MAX_SIZE_PX: u32 = 8192;

    /// Check the values a packager relies on.
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.size_px == 0 || self.size_px > Self::MAX_SIZE_PX {
            return Err(RenderError::BadSize(self.size_px));
        }
        if self.fps == 0 {
            return Err(RenderError::BadConfig("fps must be at least 1"));
        }
        if !self.margin.is_finite() {
            return Err(RenderError::BadConfig("margin must be finite"));
        }
        Ok(())
    }

    /// Quiet zone actually used, in pitches (never below the format minimum).
    pub fn margin_pitches(&self) -> f64 {
        if self.margin.is_finite() {
            self.margin.max(dims::QUIET_ZONE)
        } else {
            dims::QUIET_ZONE
        }
    }

    /// Half-side of the visible square in pitches: `R + margin`.
    pub fn half_extent(&self, frame: &FrameSpec) -> f64 {
        frame.frame_half_side + self.margin_pitches()
    }

    /// Pixels per dot pitch for `frame`.
    pub fn pitch_px(&self, frame: &FrameSpec) -> f64 {
        f64::from(self.size_px.clamp(1, Self::MAX_SIZE_PX)) / (2.0 * self.half_extent(frame))
    }

    /// Duration of one data frame in GIF delay units (centiseconds), at least 1.
    pub fn frame_delay_cs(&self) -> u16 {
        let cs = (100.0 / f64::from(self.fps.max(1))).round();
        cs.clamp(1.0, f64::from(u16::MAX)) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let c = RenderConfig::default();
        assert!(c.validate().is_ok());
        assert_eq!(c.size_px, 800);
        assert_eq!(c.frame_delay_cs(), 10);
        assert!(c.margin >= dims::QUIET_ZONE);
    }

    #[test]
    fn bad_values_are_rejected() {
        assert!(
            RenderConfig {
                size_px: 0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            RenderConfig {
                fps: 0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            RenderConfig {
                margin: f64::NAN,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        let low = RenderConfig {
            margin: 0.1,
            ..Default::default()
        };
        assert!((low.margin_pitches() - dims::QUIET_ZONE).abs() < f64::EPSILON);
    }
}
