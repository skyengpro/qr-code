//! Sender side: segmented RaptorQ fountain coding into a deterministic stream of frames.
//!
//! STUB — implemented by the session work package.

/// Physical display information used to pick a profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayInfo {
    /// Width of the area available to the code, in pixels.
    pub width_px: u32,
    /// Height of the area available to the code, in pixels.
    pub height_px: u32,
    /// Physical width of that area in millimetres.
    pub width_mm: f32,
    /// Physical height of that area in millimetres.
    pub height_mm: f32,
}

/// STUB.
#[derive(Debug, Clone, Default)]
pub struct SessionConfig;

/// STUB.
#[derive(Debug, Clone)]
pub enum ShapeChoice {
    /// Use exactly this shape.
    Require(u8),
}

/// STUB.
#[derive(Debug)]
pub struct Session;
