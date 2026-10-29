//! Sender side: segmented `RaptorQ` fountain coding into a deterministic stream of frames.
//!
//! Two layers:
//!
//! * [`PacketSession`] — the *packet layer*. Pure data-layer logic: framing, segmenting,
//!   `RaptorQ` and the packet schedule. A packet is a [`Header`] plus exactly one encoding
//!   symbol of `T` bytes (`T` is the body codec's block size).
//! * [`Session`] — the *symbol layer*. Wraps a [`PacketSession`] with the session key, the
//!   dot order, the body codec (whitening, Reed-Solomon, interleaving) and the particle
//!   liveness, and turns packets into [`FrameSpec`]s.
//!
//! # Data layer recap
//!
//! `framed = payload ‖ CRC-32(payload)`. Segment `s` is `framed[s·S ..]` (at most
//! `S = 1024 << segment_kb_log2` bytes); each segment is one `RaptorQ` source block over
//! `segment ‖ CRC-32(segment)` with symbol size `T`, so a segment has
//! `K_s = ⌈(len + 4) / T⌉` source symbols. Pass `k` visits every segment in order and emits
//! `K_s + r` packets per segment with packet seeds `(k·(K_s + r) + j) mod 4096` for
//! `j = 0..K_s + r`. A seed `< K_s` is the systematic source symbol with that index; a seed
//! `≥ K_s` is repair symbol number `seed − K_s` (`RaptorQ` encoding symbol id `seed`, because
//! the encoder's ids count from the start of the *unpadded* source block; the encoder maps
//! them past the padding symbols internally).
//!
//! CRC-32 values are appended in big-endian byte order.

use alloc::vec::Vec;

use raptorq::SourceBlockEncoder;

use crate::body::{BodyCodec, WhitenKey};
use crate::error::EncodeError;
use crate::frame::FrameSpec;
use crate::header::Header;
use crate::layout::DotOrder;
use crate::liveness;
use crate::profile::{EccLevel, Palette, Profile};
use crate::segment::{
    CRC_LEN, MAX_BLOCK_SIZE, MAX_PAYLOAD, MAX_SEGMENT_KB_LOG2, MAX_SEGMENTS, packet_seed,
    source_block, source_symbols, transmission_info,
};
use crate::shapes::{self, ShapeTable};

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

/// Which mask the dot field uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeChoice {
    /// Use exactly this shape id; fail if it is unknown or does not fit the payload.
    Require(u8),
    /// Try this shape first; fall back to the disc (id 0) if it is unknown or too small.
    Prefer(u8),
    /// Try these shape ids in order and use the first that works; the disc is the last
    /// resort.
    Auto(Vec<u8>),
}

/// Sender configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionConfig {
    /// Segment size is `1024 << segment_kb_log2` bytes (`0..=15`). Default 3 (8 KiB).
    pub segment_kb_log2: u8,
    /// Fixed profile, or `None` to pick one from the [`DisplayInfo`] with
    /// [`choose_profile`]. Default `Some(Profile::M)`.
    pub profile: Option<Profile>,
    /// Palette. Only [`Palette::Hues4`] is implemented.
    pub palette: Palette,
    /// Data frames per second; also the rate at which the liveness clock advances in
    /// [`Session::frame`]. Default 10.
    pub fps: u16,
    /// Emit a single self-contained frame (print, poster) with the stronger ECC level.
    pub is_static: bool,
    /// Mask selection. Default `Require(0)` (the disc).
    pub shape: ShapeChoice,
    /// Repair packets `r` appended to each segment per pass. Default 2.
    pub repair_per_pass: u32,
    /// Smallest acceptable dot pitch in millimetres for automatic profile selection.
    /// Default 1.6.
    pub min_pitch_mm: f32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            segment_kb_log2: 3,
            profile: Some(Profile::M),
            palette: Palette::Hues4,
            fps: 10,
            is_static: false,
            shape: ShapeChoice::Require(0),
            repair_per_pass: 2,
            min_pitch_mm: 1.6,
        }
    }
}

/// Pick the densest profile whose dot pitch on `display` is at least `min_pitch_mm`.
///
/// The frame spans `1.25·N + 3` pitches across the shorter physical display side (the
/// field is `N` dots wide, the frame is 1.25 × that plus the quiet zone and margins). The
/// result is the largest of L, M, S that satisfies the pitch limit, or S if none does
/// (including for non-positive or NaN sizes).
pub fn choose_profile(display: &DisplayInfo, min_pitch_mm: f32) -> Profile {
    let shorter = f64::from(display.width_mm.min(display.height_mm));
    let min_pitch = f64::from(min_pitch_mm);
    [Profile::L, Profile::M]
        .into_iter()
        .find(|p| shorter / (1.25 * f64::from(p.dots_across()) + 3.0) >= min_pitch)
        .unwrap_or(Profile::S)
}

/// One `RaptorQ` source block with its precomputed encoder.
#[derive(Debug, Clone)]
struct SegmentEncoder {
    /// Source symbols `K_s`.
    k: u32,
    /// Packets per pass: `K_s + r`.
    per_pass: u32,
    /// `K_s · T` bytes: the padded source block (the systematic symbols).
    block: Vec<u8>,
    encoder: SourceBlockEncoder,
}

/// Packet-layer sender: payload → deterministic stream of `(Header, symbol)` packets.
///
/// Construction does all the heavy work (one `RaptorQ` encoder per segment); [`packet`]
/// is `&self` and cheap.
///
/// [`packet`]: PacketSession::packet
#[derive(Debug, Clone)]
pub struct PacketSession {
    base: Header,
    block_size: usize,
    segments: Vec<SegmentEncoder>,
    /// `starts[s]` = index within a pass of segment `s`'s first packet.
    starts: Vec<u32>,
    frames_per_pass: u32,
}

impl PacketSession {
    /// Build the packet schedule for `payload` with symbols of `block_size` bytes.
    ///
    /// `base_header` supplies `profile`, `shape_id`, `palette`, `is_static`, `session_id`
    /// and `shape_table_version`; `payload_len` and `segment_kb_log2` (taken from
    /// `config`), `segment_index` and `packet_seed` are overwritten. `config.is_static`
    /// and `config.profile` are *not* read here: the header is authoritative.
    ///
    /// A static session must fit one segment with `K = 1` and uses `r = 0`, i.e.
    /// `payload.len() + 8 <= block_size`; otherwise [`EncodeError::DoesNotFit`].
    pub fn new(
        payload: &[u8],
        block_size: usize,
        base_header: Header,
        config: &SessionConfig,
    ) -> Result<Self, EncodeError> {
        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            return Err(EncodeError::InvalidConfig(
                "block size must be 1..=65535 bytes",
            ));
        }
        if config.segment_kb_log2 > MAX_SEGMENT_KB_LOG2 {
            return Err(EncodeError::InvalidConfig(
                "segment_kb_log2 must be at most 15",
            ));
        }
        if base_header.palette == Palette::Hues8 || config.palette == Palette::Hues8 {
            return Err(EncodeError::UnsupportedPalette);
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(EncodeError::PayloadTooLarge(payload.len()));
        }

        let mut base = base_header;
        base.payload_len = payload.len() as u32;
        base.segment_kb_log2 = config.segment_kb_log2;
        base.segment_index = 0;
        base.packet_seed = 0;

        let segment_count = base.segment_count();
        let repair = if base.is_static {
            0
        } else {
            config.repair_per_pass
        };
        if base.is_static {
            let fits = segment_count == 1 && source_symbols(base.segment_len(0), block_size) == 1;
            if !fits {
                return Err(EncodeError::DoesNotFit {
                    need: payload.len() + 2 * CRC_LEN,
                    capacity: block_size,
                });
            }
        }
        if segment_count > MAX_SEGMENTS {
            return Err(EncodeError::TooManySegments(segment_count));
        }

        let framed = crate::segment::frame_payload(payload);
        let mut segments = Vec::with_capacity(segment_count);
        let mut starts = Vec::with_capacity(segment_count);
        let mut frames_per_pass = 0u32;
        for s in 0..segment_count {
            let start = s * base.segment_size();
            let chunk = &framed[start..start + base.segment_len(s)];
            let k = source_symbols(chunk.len(), block_size);
            let per_pass = k as u64 + u64::from(repair);
            if per_pass > u64::from(crate::segment::ESI_SPACE) {
                return Err(EncodeError::SegmentTooLarge(
                    per_pass.min(usize::MAX as u64) as usize,
                ));
            }
            let block = source_block(chunk, block_size);
            let encoder = SourceBlockEncoder::new(0, &transmission_info(k, block_size), &block);
            starts.push(frames_per_pass);
            frames_per_pass += per_pass as u32;
            segments.push(SegmentEncoder {
                k: k as u32,
                per_pass: per_pass as u32,
                block,
                encoder,
            });
        }

        Ok(PacketSession {
            base,
            block_size,
            segments,
            starts,
            frames_per_pass,
        })
    }

    /// Packets in one pass over all segments: `Σ (K_s + r)`.
    pub fn frames_per_pass(&self) -> u32 {
        self.frames_per_pass
    }

    /// Number of segments.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Symbol size `T` in bytes.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Source symbols `K_s` of segment `s`, or `None` if out of range.
    pub fn source_symbols(&self, segment: usize) -> Option<u32> {
        self.segments.get(segment).map(|s| s.k)
    }

    /// Session id carried by every header.
    pub fn session_id(&self) -> u16 {
        self.base.session_id
    }

    /// Frame `i`: its header and exactly `block_size` symbol bytes. `i` counts packets
    /// across passes, so `packet(i + frames_per_pass())` is the same segment and position
    /// in the next pass, with the next batch of packet seeds.
    pub fn packet(&self, i: u32) -> (Header, Vec<u8>) {
        let pass = i / self.frames_per_pass;
        let pos = i % self.frames_per_pass;
        let s = self.starts.partition_point(|&start| start <= pos) - 1;
        let seg = &self.segments[s];
        let j = pos - self.starts[s];
        let seed = packet_seed(pass, seg.per_pass, j);

        let header = Header {
            segment_index: s as u16,
            packet_seed: seed,
            ..self.base
        };
        let esi = u32::from(seed);
        let symbol = if esi < seg.k {
            let at = esi as usize * self.block_size;
            seg.block[at..at + self.block_size].to_vec()
        } else {
            let mut packets = seg.encoder.repair_packets(esi - seg.k, 1);
            packets.swap_remove(0).split().1
        };
        (header, symbol)
    }
}

/// Symbol-layer sender: payload → [`FrameSpec`]s.
///
/// Wraps a [`PacketSession`] with everything frame rendering needs besides the body codec:
/// the dot order (shape × profile), the session key and the liveness clock.
#[derive(Debug, Clone)]
pub struct Session {
    packets: PacketSession,
    order: DotOrder,
    codec: BodyCodec,
    key: [u8; 32],
    clock_origin_unix: u64,
    fps: u16,
}

impl Session {
    /// Assemble a session from its parts. Used by [`Session::with_identity`] and by tests;
    /// `order` must be the dot order the packets' block size was derived from.
    pub fn from_parts(
        packets: PacketSession,
        order: DotOrder,
        key: [u8; 32],
        clock_origin_unix: u64,
        fps: u16,
    ) -> Session {
        let ecc = EccLevel::for_static(packets.packet(0).0.is_static);
        let codec = BodyCodec::new(order.len(), ecc);
        assert_eq!(
            codec.block_size(),
            packets.block_size(),
            "order does not match packets"
        );
        Session {
            packets,
            order,
            codec,
            key,
            clock_origin_unix,
            fps: fps.max(1),
        }
    }

    /// Start a session with a fresh random 16-bit session id, a random 32-byte key and the
    /// current wall clock as the liveness origin (`std` only; this is the only place the
    /// clock is read).
    #[cfg(feature = "std")]
    pub fn new(
        payload: &[u8],
        display: DisplayInfo,
        config: &SessionConfig,
    ) -> Result<Session, EncodeError> {
        let (session_id, key) = random_identity()?;
        let origin = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Session::with_identity(payload, display, config, session_id, key, origin)
    }

    /// Start a session with an explicit identity (deterministic: the same arguments always
    /// give the same frames).
    ///
    /// `clock_origin_unix` is the Unix second at which frame 0 is shown; frame `i` carries
    /// the rolling liveness value of second `clock_origin_unix + i / fps`.
    ///
    /// Uses the built-in shape table; see [`Session::with_shapes`] for custom shapes.
    pub fn with_identity(
        payload: &[u8],
        display: DisplayInfo,
        config: &SessionConfig,
        session_id: u16,
        key: [u8; 32],
        clock_origin_unix: u64,
    ) -> Result<Session, EncodeError> {
        let table = ShapeTable::builtin();
        Session::with_shapes(
            payload,
            display,
            config,
            &table,
            session_id,
            key,
            clock_origin_unix,
        )
    }

    /// [`Session::with_identity`] with an explicit shape table (custom shapes 32–255).
    ///
    /// Shape resolution follows [`ShapeChoice`]: `Require(id)` fails with
    /// [`EncodeError::UnknownShape`] (or [`EncodeError::DoesNotFit`] for a static session
    /// whose payload does not fit one frame); `Prefer(id)` falls back to the disc;
    /// `Auto(ids)` takes the first listed shape that exists and fits, else the disc.
    pub fn with_shapes(
        payload: &[u8],
        display: DisplayInfo,
        config: &SessionConfig,
        table: &ShapeTable,
        session_id: u16,
        key: [u8; 32],
        clock_origin_unix: u64,
    ) -> Result<Session, EncodeError> {
        let profile = config
            .profile
            .unwrap_or_else(|| choose_profile(&display, config.min_pitch_mm));
        let ecc = EccLevel::for_static(config.is_static);
        // A static frame holds payload ‖ CRC-32 ‖ segment CRC-32 in one symbol.
        let capacity = |order: &DotOrder| BodyCodec::new(order.len(), ecc).block_size();
        let fits = |order: &DotOrder| !config.is_static || payload.len() + 8 <= capacity(order);
        let candidates: Vec<u8> = match &config.shape {
            ShapeChoice::Require(id) => alloc::vec![*id],
            ShapeChoice::Prefer(id) => alloc::vec![*id, shapes::DISC],
            ShapeChoice::Auto(ids) => ids.iter().copied().chain([shapes::DISC]).collect(),
        };
        let chosen = candidates
            .iter()
            .find_map(|&id| table.dot_order(id, profile).filter(fits).map(|o| (id, o)));
        let Some((shape_id, order)) = chosen else {
            // Nothing usable: unknown shape, or nothing fits one static frame.
            let order = candidates
                .iter()
                .find_map(|&id| table.dot_order(id, profile))
                .ok_or(EncodeError::UnknownShape(candidates[0]))?;
            return Err(EncodeError::DoesNotFit {
                need: payload.len() + 8,
                capacity: capacity(&order),
            });
        };
        let codec = BodyCodec::new(order.len(), ecc);
        let base_header = Header {
            shape_table_version: table.version(),
            profile,
            palette: config.palette,
            is_static: config.is_static,
            shape_id,
            session_id,
            payload_len: 0,
            segment_kb_log2: config.segment_kb_log2,
            segment_index: 0,
            packet_seed: 0,
        };
        let packets = PacketSession::new(payload, codec.block_size(), base_header, config)?;
        Ok(Session::from_parts(
            packets,
            order,
            key,
            clock_origin_unix,
            config.fps,
        ))
    }

    /// Profile of every frame.
    pub fn profile(&self) -> Profile {
        self.order.profile()
    }

    /// Payload bytes carried per frame (the `RaptorQ` symbol size `T`).
    pub fn block_size(&self) -> usize {
        self.codec.block_size()
    }

    /// The dot order (profile × shape) every frame uses.
    pub fn dot_order(&self) -> &DotOrder {
        &self.order
    }

    /// Session id carried by every header.
    pub fn session_id(&self) -> u16 {
        self.packets.session_id()
    }

    /// The session key (shared with verifiers out of band; see [`crate::liveness`]).
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    /// Packets in one pass; see [`PacketSession::frames_per_pass`].
    pub fn frames_per_pass(&self) -> u32 {
        self.packets.frames_per_pass()
    }

    /// The underlying packet layer.
    pub fn packets(&self) -> &PacketSession {
        &self.packets
    }

    /// Frame `i` with the liveness clock derived from the frame index:
    /// second `clock_origin_unix + i / fps`. Deterministic.
    pub fn frame(&self, i: u32) -> FrameSpec {
        self.frame_at(
            i,
            self.clock_origin_unix + u64::from(i / u32::from(self.fps)),
        )
    }

    /// Frame `i` with the particles showing the liveness value of `unix_seconds` (use the
    /// real wall clock when displaying live).
    pub fn frame_at(&self, i: u32, unix_seconds: u64) -> FrameSpec {
        let (header, block) = self.packets.packet(i);
        let body = self.encode_body(&header, &block);
        let sectors = liveness::particle_sectors(
            (header.packet_seed & 0xFF) as u8,
            liveness::rolling_value(&self.key, unix_seconds),
        );
        FrameSpec::assemble(i, header, &header.encode(), &self.order, &body, sectors)
    }

    /// Whiten, Reed-Solomon protect, interleave and map one symbol to `order.len()` dot
    /// symbols (`BodyCodec::encode` with `WhitenKey { session_id, segment_index,
    /// packet_seed }`).
    fn encode_body(&self, header: &Header, block: &[u8]) -> Vec<u8> {
        self.codec.encode(block, whiten_key(header))
    }
}

/// The whitening key of a frame.
pub(crate) fn whiten_key(header: &Header) -> WhitenKey {
    WhitenKey {
        session_id: header.session_id,
        segment_index: header.segment_index,
        packet_seed: header.packet_seed,
    }
}

/// Fresh random session id and key from the OS (`std` only).
#[cfg(feature = "std")]
pub fn random_identity() -> Result<(u16, [u8; 32]), EncodeError> {
    let mut id = [0u8; 2];
    let mut key = [0u8; 32];
    getrandom::fill(&mut id).map_err(|_| EncodeError::Rng)?;
    getrandom::fill(&mut key).map_err(|_| EncodeError::Rng)?;
    Ok((u16::from_be_bytes(id), key))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::receiver::{PacketReceiver, Progress};

    fn header(is_static: bool) -> Header {
        Header {
            shape_table_version: 0,
            profile: Profile::M,
            palette: Palette::Hues4,
            is_static,
            shape_id: 0,
            session_id: 0xBEEF,
            payload_len: 0,
            segment_kb_log2: 3,
            segment_index: 0,
            packet_seed: 0,
        }
    }

    fn payload(n: usize) -> Vec<u8> {
        // Deterministic, incompressible-looking bytes.
        let mut x = 0x1234_5678u32;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect()
    }

    fn session(n: usize, t: usize) -> PacketSession {
        PacketSession::new(&payload(n), t, header(false), &SessionConfig::default()).unwrap()
    }

    #[test]
    fn frames_per_pass_math() {
        // 6000 + 4 = 6004 framed bytes in one segment; + 4 CRC = 6008; T = 1250 -> K = 5.
        let s = session(6000, 1250);
        assert_eq!(s.segment_count(), 1);
        assert_eq!(s.source_symbols(0), Some(5));
        assert_eq!(s.frames_per_pass(), 5 + 2);

        // 20 000 + 4 framed = 8192 + 8192 + 3624 -> K = ceil(8196/1250)=7, 7, ceil(3628/1250)=3.
        let s = session(20_000, 1250);
        assert_eq!(s.segment_count(), 3);
        assert_eq!(s.source_symbols(0), Some(7));
        assert_eq!(s.source_symbols(1), Some(7));
        assert_eq!(s.source_symbols(2), Some(3));
        assert_eq!(s.frames_per_pass(), (7 + 2) + (7 + 2) + (3 + 2));

        let s = session(0, 1250);
        assert_eq!((s.segment_count(), s.frames_per_pass()), (1, 1 + 2));
        assert_eq!(s.source_symbols(1), None);
    }

    #[test]
    fn packet_schedule_and_headers() {
        let s = session(20_000, 1250);
        let fpp = s.frames_per_pass();
        for i in 0..3 * fpp {
            let (h, sym) = s.packet(i);
            assert_eq!(sym.len(), 1250);
            assert_eq!(h.payload_len, 20_000);
            assert_eq!(h.session_id, 0xBEEF);
            assert_eq!(h.segment_kb_log2, 3);
        }
        // Pass 0: segment 0 seeds 0..9, segment 1 seeds 0..9, segment 2 seeds 0..5.
        let seeds: Vec<(u16, u16)> = (0..fpp)
            .map(|i| s.packet(i))
            .map(|(h, _)| (h.segment_index, h.packet_seed))
            .collect();
        assert_eq!(&seeds[..9], &(0..9).map(|j| (0, j)).collect::<Vec<_>>()[..]);
        assert_eq!(
            &seeds[9..18],
            &(0..9).map(|j| (1, j)).collect::<Vec<_>>()[..]
        );
        assert_eq!(
            &seeds[18..],
            &(0..5).map(|j| (2, j)).collect::<Vec<_>>()[..]
        );
        // Pass 1 continues the numbering per segment.
        assert_eq!(s.packet(fpp).0.packet_seed, 9);
        assert_eq!(s.packet(fpp + 9).0.packet_seed, 9);
        assert_eq!(s.packet(fpp + 18).0.packet_seed, 5);
        // Deterministic.
        assert_eq!(s.packet(17), s.packet(17));
    }

    #[test]
    fn systematic_symbols_are_the_data() {
        let data = payload(2000);
        let s = PacketSession::new(&data, 500, header(false), &SessionConfig::default()).unwrap();
        // framed = data ‖ crc; first K symbols concatenated start with data.
        let mut cat = Vec::new();
        for i in 0..s.source_symbols(0).unwrap() {
            cat.extend(s.packet(i).1);
        }
        assert_eq!(&cat[..2000], &data[..]);
    }

    #[test]
    fn esi_wraps_at_4096() {
        // 1 segment, K = 5, r = 2 -> 7 per pass; 4096 / 7 is not an integer so seeds wrap
        // mid-pass.
        let s = session(6000, 1250);
        let fpp = s.frames_per_pass();
        let (h, _) = s.packet(585 * fpp); // 585 * 7 = 4095
        assert_eq!(h.packet_seed, 4095);
        let (h, _) = s.packet(585 * fpp + 1);
        assert_eq!(h.packet_seed, 0);
        // Wrapped seeds below K are source symbols again, and equal the first pass's.
        assert_eq!(s.packet(585 * fpp + 1).1, s.packet(0).1);
        // Seeds never leave the 12-bit field even for huge frame indices.
        assert!(s.packet(u32::MAX).0.packet_seed < 4096);
    }

    #[test]
    fn repair_only_decodes() {
        // Feed only repair symbols (seeds >= K) of every segment.
        let data = payload(20_000);
        let s = PacketSession::new(&data, 1250, header(false), &SessionConfig::default()).unwrap();
        let mut rx = PacketReceiver::new();
        let fpp = s.frames_per_pass();
        let mut done = None;
        'outer: for i in 0..40 * fpp {
            let (h, sym) = s.packet(i);
            let k = s.source_symbols(usize::from(h.segment_index)).unwrap();
            if u32::from(h.packet_seed) < k {
                continue;
            }
            if let Progress::Done(p) = rx.push(&h, &sym) {
                done = Some(p);
                break 'outer;
            }
        }
        assert_eq!(done.as_deref(), Some(&data[..]));
    }

    #[test]
    fn static_session() {
        let cfg = SessionConfig {
            is_static: true,
            ..SessionConfig::default()
        };
        let data = payload(100);
        let s = PacketSession::new(&data, 200, header(true), &cfg).unwrap();
        assert_eq!((s.segment_count(), s.frames_per_pass()), (1, 1));
        let (h, sym) = s.packet(0);
        assert!(h.is_static);
        let mut rx = PacketReceiver::new();
        assert_eq!(rx.push(&h, &sym), Progress::Done(data));

        // Exactly full: payload + 8 == T.
        assert!(PacketSession::new(&payload(192), 200, header(true), &cfg).is_ok());
        // One byte too many.
        assert_eq!(
            PacketSession::new(&payload(193), 200, header(true), &cfg).unwrap_err(),
            EncodeError::DoesNotFit {
                need: 201,
                capacity: 200
            }
        );
        // Several segments.
        assert!(matches!(
            PacketSession::new(&payload(9000), 60_000, header(true), &cfg),
            Err(EncodeError::DoesNotFit { .. })
        ));
        // Empty payload.
        let s = PacketSession::new(&[], 8, header(true), &cfg).unwrap();
        let (h, sym) = s.packet(0);
        assert_eq!(PacketReceiver::new().push(&h, &sym), Progress::Done(vec![]));
    }

    #[test]
    fn validation() {
        let cfg = SessionConfig::default();
        let new = |p: &[u8], t, h, c: &SessionConfig| PacketSession::new(p, t, h, c).map(|_| ());
        assert_eq!(
            new(&vec![0; (1 << 24) - 1 + 1], 1250, header(false), &cfg),
            Err(EncodeError::PayloadTooLarge(1 << 24))
        );
        let hues8 = Header {
            palette: Palette::Hues8,
            ..header(false)
        };
        assert_eq!(
            new(b"x", 1250, hues8, &cfg),
            Err(EncodeError::UnsupportedPalette)
        );
        let cfg8 = SessionConfig {
            palette: Palette::Hues8,
            ..SessionConfig::default()
        };
        assert_eq!(
            new(b"x", 1250, header(false), &cfg8),
            Err(EncodeError::UnsupportedPalette)
        );
        // 1 KiB segments: 1025 segments need > 1 MiB.
        let small = SessionConfig {
            segment_kb_log2: 0,
            ..SessionConfig::default()
        };
        assert_eq!(
            new(&vec![0; 1024 * 1024], 60_000, header(false), &small),
            Err(EncodeError::TooManySegments(1025))
        );
        assert!(matches!(
            new(
                b"x",
                1250,
                header(false),
                &SessionConfig {
                    segment_kb_log2: 16,
                    ..cfg.clone()
                }
            ),
            Err(EncodeError::InvalidConfig(_))
        ));
        assert!(matches!(
            new(b"x", 0, header(false), &cfg),
            Err(EncodeError::InvalidConfig(_))
        ));
        assert!(matches!(
            new(b"x", 70_000, header(false), &cfg),
            Err(EncodeError::InvalidConfig(_))
        ));
        // 32 KiB segment with T = 4 bytes -> K = 8193 > 4096.
        let big = SessionConfig {
            segment_kb_log2: 5,
            ..SessionConfig::default()
        };
        assert!(matches!(
            new(&vec![0; 40_000], 4, header(false), &big),
            Err(EncodeError::SegmentTooLarge(_))
        ));
        // K + r over the limit although K alone fits.
        let r = SessionConfig {
            repair_per_pass: 4092,
            ..SessionConfig::default()
        };
        assert!(matches!(
            new(&vec![0; 6000], 1250, header(false), &r),
            Err(EncodeError::SegmentTooLarge(4097))
        ));
        let r = SessionConfig {
            repair_per_pass: u32::MAX,
            ..SessionConfig::default()
        };
        assert!(matches!(
            new(b"x", 1250, header(false), &r),
            Err(EncodeError::SegmentTooLarge(_))
        ));
    }

    #[test]
    fn zero_repair_is_allowed() {
        let cfg = SessionConfig {
            repair_per_pass: 0,
            ..SessionConfig::default()
        };
        let s = PacketSession::new(&payload(6000), 1250, header(false), &cfg).unwrap();
        assert_eq!(s.frames_per_pass(), 5);
    }

    #[test]
    fn profile_choice() {
        let d = |mm: f32| DisplayInfo {
            width_px: 1000,
            height_px: 2000,
            width_mm: mm,
            height_mm: 2.0 * mm,
        };
        // L needs 1.6 * 123 = 196.8 mm, M 1.6 * 93 = 148.8 mm, S 1.6 * 63 = 100.8 mm.
        assert_eq!(choose_profile(&d(200.0), 1.6), Profile::L);
        assert_eq!(choose_profile(&d(196.7), 1.6), Profile::M);
        assert_eq!(choose_profile(&d(150.0), 1.6), Profile::M);
        assert_eq!(choose_profile(&d(148.7), 1.6), Profile::S);
        assert_eq!(choose_profile(&d(120.0), 1.6), Profile::S);
        assert_eq!(choose_profile(&d(10.0), 1.6), Profile::S);
        // The shorter side decides, whichever it is.
        let wide = DisplayInfo {
            width_px: 1,
            height_px: 1,
            width_mm: 500.0,
            height_mm: 100.0,
        };
        assert_eq!(choose_profile(&wide, 1.6), Profile::S);
        // A looser pitch limit admits denser profiles.
        assert_eq!(choose_profile(&d(100.0), 0.8), Profile::L);
        // Junk in, S out.
        assert_eq!(choose_profile(&d(f32::NAN), 1.6), Profile::S);
        assert_eq!(choose_profile(&d(-5.0), 1.6), Profile::S);
    }

    #[test]
    fn config_defaults() {
        let c = SessionConfig::default();
        assert_eq!(c.segment_kb_log2, 3);
        assert_eq!(c.profile, Some(Profile::M));
        assert_eq!((c.fps, c.repair_per_pass), (10, 2));
        assert!(!c.is_static);
        assert_eq!(c.shape, ShapeChoice::Require(0));
        assert!((c.min_pitch_mm - 1.6).abs() < f32::EPSILON);
    }

    #[cfg(feature = "std")]
    #[test]
    fn random_identity_varies() {
        let (a, ka) = random_identity().unwrap();
        let (b, kb) = random_identity().unwrap();
        assert_ne!(ka, kb);
        let _ = (a, b);
    }
}
