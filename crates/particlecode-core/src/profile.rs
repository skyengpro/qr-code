//! Profiles (lattice density), palettes and ECC levels.

/// Lattice density announced in the header. The receiver never guesses the lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Profile {
    /// 48 dots across the field. Fallback for weak cameras.
    S,
    /// 72 dots across the field. The default.
    M,
    /// 96 dots across the field. 1080p-or-better cameras held close.
    L,
}

impl Profile {
    /// All profiles, smallest first.
    pub const ALL: [Profile; 3] = [Profile::S, Profile::M, Profile::L];

    /// Dots across the field (`N` in the spec).
    pub const fn dots_across(self) -> u32 {
        match self {
            Profile::S => 48,
            Profile::M => 72,
            Profile::L => 96,
        }
    }

    /// Half-side of the field square in pitch units (`F = N / 2`).
    pub const fn field_half(self) -> f64 {
        self.dots_across() as f64 / 2.0
    }

    /// 2-bit header code.
    pub const fn code(self) -> u8 {
        match self {
            Profile::S => 0,
            Profile::M => 1,
            Profile::L => 2,
        }
    }

    /// Inverse of [`Profile::code`]. Code 3 is reserved.
    pub const fn from_code(code: u8) -> Option<Profile> {
        match code {
            0 => Some(Profile::S),
            1 => Some(Profile::M),
            2 => Some(Profile::L),
            _ => None,
        }
    }
}

/// Number of hues used for dot colors. Only [`Palette::Hues4`] is implemented in v1;
/// [`Palette::Hues8`] is reserved in the header until field data shows the margin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Palette {
    /// Blue, green, orange, magenta — 2 bits per dot.
    #[default]
    Hues4,
    /// Reserved: 8 hues, 3 bits of color per dot.
    Hues8,
}

/// Reed-Solomon strength of the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EccLevel {
    /// RS(255,191): 64 parity bytes per full codeword (~25%). Animated frames.
    #[default]
    Normal,
    /// RS(255,159): 96 parity bytes per full codeword (~38%). Static / print frames.
    High,
}

impl EccLevel {
    /// Parity bytes of a full 255-byte codeword.
    pub const fn parity_per_255(self) -> usize {
        match self {
            EccLevel::Normal => 64,
            EccLevel::High => 96,
        }
    }

    /// ECC level implied by the header's `static` flag.
    pub const fn for_static(is_static: bool) -> EccLevel {
        if is_static { EccLevel::High } else { EccLevel::Normal }
    }
}

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The exact sRGB colors every painter must use. Never color-managed.
pub mod colors {
    use super::Rgb;

    /// The four data hues, indexed by the 2-bit dot color. Similar luma (~98–129 BT.601)
    /// so size classification is not fooled by brightness; ≥ 68° apart in CbCr.
    pub const HUES4: [Rgb; 4] = [
        Rgb(20, 110, 245), // 0 blue
        Rgb(40, 165, 30),  // 1 green
        Rgb(235, 100, 0),  // 2 orange
        Rgb(215, 40, 175), // 3 magenta
    ];
    /// Quiet zone and field background (warm off-white).
    pub const BACKGROUND: Rgb = Rgb(246, 244, 239);
    /// Anchor rings and cores: much darker than any data hue, so a luma threshold
    /// separates anchors from dots.
    pub const ANCHOR: Rgb = Rgb(24, 28, 52);
    /// White calibration swatch.
    pub const WHITE: Rgb = Rgb(255, 255, 255);
    /// Black calibration swatch.
    pub const BLACK: Rgb = Rgb(12, 12, 16);
    /// Particle specks: neutral grey, no chroma.
    pub const PARTICLE: Rgb = Rgb(120, 122, 128);
}
