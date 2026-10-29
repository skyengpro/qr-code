//! Receiver side: header-checked packets → per-segment `RaptorQ` decoders → in-order bytes.
//!
//! Two layers, mirroring [`crate::session`]:
//!
//! * [`PacketReceiver`] — the *packet layer*: takes `(Header, symbol bytes)` and does all
//!   the data-layer work (session lock, header consistency, de-duplication, `RaptorQ`, CRC
//!   checks, in-order delivery).
//! * [`Receiver`] — the *symbol layer*: takes dot symbols, decodes the header and body
//!   (Reed-Solomon) and hands the result to a [`PacketReceiver`].
//!
//! The receiver never panics on hostile input: every packet is either accepted or rejected
//! with a [`DecodeError`].

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use raptorq::{EncodingPacket, PayloadId, SourceBlockDecoder};

use crate::error::DecodeError;
use crate::header::{HEADER_SYMBOLS, Header};
use crate::profile::Palette;
use crate::segment::{
    CRC_LEN, ESI_SPACE, MAX_BLOCK_SIZE, MAX_PAYLOAD, MAX_SEGMENT_KB_LOG2, MAX_SEGMENTS,
    crc_matches, framed_is_valid, source_symbols, transmission_info,
};
use crate::symbol::Sym3;

/// What happened to one packet or frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The packet was not used; the transfer state is unchanged, except after
    /// [`DecodeError::SegmentCrc`], which discards that segment's collected symbols so it
    /// can be received afresh.
    Rejected(DecodeError),
    /// The packet was accepted (possibly as a duplicate of one already held).
    Accepted {
        /// `Σ min(distinct symbols received, K_s)` over all segments; a decoded segment
        /// counts `K_s`.
        have: u32,
        /// `Σ K_s`: symbols needed if nothing is lost.
        need: u32,
        /// Segments decoded and CRC-verified so far.
        segments_done: u32,
        /// Segments in the transfer.
        segments_total: u32,
    },
    /// The whole payload arrived and passed its CRC-32. Repeated for every later packet.
    Done(Vec<u8>),
}

/// Decoding state of one segment.
#[derive(Debug)]
enum Seg {
    /// Symbols are still arriving.
    Collecting {
        /// Created on the first symbol.
        decoder: Option<Box<SourceBlockDecoder>>,
        /// Packet seeds seen so far.
        seen: BTreeSet<u16>,
    },
    /// Decoded and verified, waiting for earlier segments. Holds the segment's framed bytes
    /// (without the segment CRC).
    Decoded(Vec<u8>),
    /// Appended to the assembled payload.
    Delivered,
}

impl Seg {
    fn fresh() -> Seg {
        Seg::Collecting {
            decoder: None,
            seen: BTreeSet::new(),
        }
    }
}

/// State fixed by the first valid packet.
#[derive(Debug)]
struct Locked {
    /// The first header; every later one must agree with it on everything but the segment
    /// index and packet seed.
    first: Header,
    /// Symbol size `T`.
    block_size: usize,
    /// `K_s` per segment.
    ks: Vec<u32>,
    need: u32,
    segs: Vec<Seg>,
    /// First segment not yet appended to `assembled`.
    next: usize,
    /// Framed bytes (`payload ‖ CRC-32`) of all delivered segments, in order.
    assembled: Vec<u8>,
    /// Bytes of the payload already returned by [`PacketReceiver::take_ready`].
    taken: usize,
    done: bool,
}

/// Packet-layer receiver. See the [module docs](self).
#[derive(Debug, Default)]
pub struct PacketReceiver {
    state: Option<Locked>,
}

impl PacketReceiver {
    /// A receiver that has not seen any packet yet.
    pub fn new() -> PacketReceiver {
        PacketReceiver { state: None }
    }

    /// Forget everything, including the session lock.
    pub fn reset(&mut self) {
        self.state = None;
    }

    /// Session id of the transfer being received, once locked.
    pub fn session_id(&self) -> Option<u16> {
        self.state.as_ref().map(|s| s.first.session_id)
    }

    /// The first header seen (the transfer's parameters), once locked.
    pub fn first_header(&self) -> Option<&Header> {
        self.state.as_ref().map(|s| &s.first)
    }

    /// Payload bytes newly delivered in order since the last call (never the trailing
    /// CRC-32). Bytes appear as soon as their segment and all earlier segments are
    /// verified. `None` if nothing new.
    pub fn take_ready(&mut self) -> Option<Vec<u8>> {
        let st = self.state.as_mut()?;
        let end = st.assembled.len().min(st.first.payload_len as usize);
        if end <= st.taken {
            return None;
        }
        let out = st.assembled[st.taken..end].to_vec();
        st.taken = end;
        Some(out)
    }

    /// Feed one packet: the decoded `header` and the `symbol` bytes (exactly one `RaptorQ`
    /// encoding symbol of `T` bytes; `T` is learned from the first packet).
    pub fn push(&mut self, header: &Header, symbol: &[u8]) -> Progress {
        match self.try_push(header, symbol) {
            Ok(progress) => progress,
            Err(e) => Progress::Rejected(e),
        }
    }

    fn try_push(&mut self, header: &Header, symbol: &[u8]) -> Result<Progress, DecodeError> {
        check_header(header)?;
        match &self.state {
            None => self.state = Some(Locked::lock(header, symbol.len())?),
            Some(st) => st.check(header, symbol.len())?,
        }
        let st = self.state.as_mut().expect("locked above");
        if st.done {
            return Ok(Progress::Done(st.payload()));
        }
        match st.absorb(header, symbol) {
            Ok(()) => {}
            Err(e @ DecodeError::PayloadCrc) => {
                // Every segment verified yet the whole did not: nothing here can be
                // trusted, so start over.
                self.state = None;
                return Err(e);
            }
            Err(e) => return Err(e),
        }
        let st = self.state.as_ref().expect("still locked");
        Ok(if st.done {
            Progress::Done(st.payload())
        } else {
            st.progress()
        })
    }
}

/// Header checks that need no history.
fn check_header(h: &Header) -> Result<(), DecodeError> {
    if h.palette == Palette::Hues8 {
        return Err(DecodeError::UnsupportedPalette);
    }
    if h.payload_len as usize > MAX_PAYLOAD {
        return Err(DecodeError::Inconsistent("payload length beyond 24 bits"));
    }
    if h.segment_kb_log2 > MAX_SEGMENT_KB_LOG2 {
        return Err(DecodeError::Inconsistent("segment size beyond 32 MiB"));
    }
    let segments = h.segment_count();
    if segments > MAX_SEGMENTS {
        return Err(DecodeError::Inconsistent("more than 1024 segments"));
    }
    if usize::from(h.segment_index) >= segments {
        return Err(DecodeError::Inconsistent(
            "segment index beyond segment count",
        ));
    }
    if u32::from(h.packet_seed) >= ESI_SPACE {
        return Err(DecodeError::Inconsistent("packet seed beyond 12 bits"));
    }
    Ok(())
}

impl Locked {
    /// Build the state from the first valid header and the symbol size it came with.
    fn lock(first: &Header, block_size: usize) -> Result<Locked, DecodeError> {
        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            return Err(DecodeError::Inconsistent(
                "symbol size must be 1..=65535 bytes",
            ));
        }
        let count = first.segment_count();
        let mut ks = Vec::with_capacity(count);
        let mut need = 0u32;
        for s in 0..count {
            let k = source_symbols(first.segment_len(s), block_size);
            if k > ESI_SPACE as usize {
                return Err(DecodeError::Inconsistent(
                    "segment needs more than 4096 symbols",
                ));
            }
            ks.push(k as u32);
            need += k as u32;
        }
        if first.is_static && (count != 1 || ks[0] != 1) {
            return Err(DecodeError::Inconsistent(
                "static frame does not hold the payload",
            ));
        }
        Ok(Locked {
            first: *first,
            block_size,
            ks,
            need,
            segs: (0..count).map(|_| Seg::fresh()).collect(),
            next: 0,
            assembled: Vec::new(),
            taken: 0,
            done: false,
        })
    }

    /// Compare a later packet with the locked transfer.
    fn check(&self, h: &Header, symbol_len: usize) -> Result<(), DecodeError> {
        let f = &self.first;
        if h.session_id != f.session_id {
            return Err(DecodeError::OtherSession);
        }
        let same = h.payload_len == f.payload_len
            && h.segment_kb_log2 == f.segment_kb_log2
            && h.profile == f.profile
            && h.shape_id == f.shape_id
            && h.is_static == f.is_static
            && h.palette == f.palette
            && h.shape_table_version == f.shape_table_version;
        if !same {
            return Err(DecodeError::Inconsistent(
                "header differs from the session's first header",
            ));
        }
        if symbol_len != self.block_size {
            return Err(DecodeError::WrongSymbolCount {
                expected: self.block_size,
                got: symbol_len,
            });
        }
        Ok(())
    }

    /// Take one (already validated) packet into its segment.
    fn absorb(&mut self, h: &Header, symbol: &[u8]) -> Result<(), DecodeError> {
        let s = usize::from(h.segment_index);
        let k = self.ks[s];
        let Seg::Collecting { decoder, seen } = &mut self.segs[s] else {
            return Ok(()); // segment already decoded
        };
        if !seen.insert(h.packet_seed) {
            return Ok(()); // duplicate
        }
        let decoder = decoder.get_or_insert_with(|| {
            let len = u64::from(k) * self.block_size as u64;
            Box::new(SourceBlockDecoder::new(
                0,
                &transmission_info(k as usize, self.block_size),
                len,
            ))
        });
        let packet =
            EncodingPacket::new(PayloadId::new(0, u32::from(h.packet_seed)), symbol.to_vec());
        let Some(block) = decoder.decode([packet]) else {
            return Ok(());
        };

        let seg_len = self.first.segment_len(s);
        if !crc_matches(&block, seg_len) {
            self.segs[s] = Seg::fresh();
            return Err(DecodeError::SegmentCrc(s as u32));
        }
        let mut bytes = block;
        bytes.truncate(seg_len);
        self.segs[s] = Seg::Decoded(bytes);
        self.deliver()
    }

    /// Append every consecutive decoded segment to the assembled payload; finish when all
    /// are in.
    fn deliver(&mut self) -> Result<(), DecodeError> {
        while self.next < self.segs.len() && matches!(self.segs[self.next], Seg::Decoded(_)) {
            if let Seg::Decoded(bytes) =
                core::mem::replace(&mut self.segs[self.next], Seg::Delivered)
            {
                self.assembled.extend_from_slice(&bytes);
            }
            self.next += 1;
        }
        if self.next == self.segs.len() {
            if !framed_is_valid(&self.assembled) {
                return Err(DecodeError::PayloadCrc);
            }
            self.done = true;
        }
        Ok(())
    }

    fn payload(&self) -> Vec<u8> {
        self.assembled[..self.assembled.len() - CRC_LEN].to_vec()
    }

    fn progress(&self) -> Progress {
        let mut have = 0u32;
        let mut done = 0u32;
        for (seg, &k) in self.segs.iter().zip(&self.ks) {
            match seg {
                Seg::Collecting { seen, .. } => have += (seen.len() as u32).min(k),
                Seg::Decoded(_) | Seg::Delivered => {
                    have += k;
                    done += 1;
                }
            }
        }
        Progress::Accepted {
            have,
            need: self.need,
            segments_done: done,
            segments_total: self.segs.len() as u32,
        }
    }
}

/// Symbol-layer receiver: dot symbols → header → body → [`PacketReceiver`].
#[derive(Debug, Default)]
pub struct Receiver {
    packets: PacketReceiver,
}

impl Receiver {
    /// A receiver that has not seen any frame yet.
    pub fn new() -> Receiver {
        Receiver {
            packets: PacketReceiver::new(),
        }
    }

    /// Feed one frame's symbols: [`HEADER_SYMBOLS`] header dots followed by the body dots
    /// of the frame's profile and shape. Frames whose header does not decode are rejected.
    pub fn feed(&mut self, symbols: &[Sym3]) -> Progress {
        if symbols.len() < HEADER_SYMBOLS {
            return Progress::Rejected(DecodeError::WrongSymbolCount {
                expected: HEADER_SYMBOLS,
                got: symbols.len(),
            });
        }
        let (head, body) = symbols.split_at(HEADER_SYMBOLS);
        match Header::decode(head) {
            Ok(header) => self.feed_decoded(&header, body),
            Err(e) => Progress::Rejected(e),
        }
    }

    /// Feed a frame whose header the caller has already decoded (e.g. the vision stage read
    /// and checked it) together with its body dots.
    ///
    /// Wiring still to do (needs `shapes` and `body`): reject an unknown shape id or shape
    /// table mismatch, build the `DotOrder` for the header's (profile, shape) (cache it per
    /// session), check `body.len() == order.len()`, run `BodyCodec::decode` with
    /// `WhitenKey { session_id, segment_index, packet_seed }` (rejecting on
    /// [`DecodeError::Uncorrectable`]) and pass the resulting `T` bytes to
    /// [`PacketReceiver::push`].
    #[allow(clippy::todo, clippy::unused_self)]
    pub fn feed_decoded(&mut self, header: &Header, body: &[Sym3]) -> Progress {
        let _ = (header, body, &mut self.packets);
        todo!("wire BodyCodec::decode and the shape table")
    }

    /// Payload bytes newly delivered in order since the last call; see
    /// [`PacketReceiver::take_ready`].
    pub fn take_ready(&mut self) -> Option<Vec<u8>> {
        self.packets.take_ready()
    }

    /// Session id of the transfer being received, once locked.
    pub fn session_id(&self) -> Option<u16> {
        self.packets.session_id()
    }

    /// The packet layer, e.g. to read the locked header.
    pub fn packets(&self) -> &PacketReceiver {
        &self.packets
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::profile::Profile;
    use crate::session::{PacketSession, SessionConfig};

    fn header(session_id: u16) -> Header {
        Header {
            shape_table_version: 0,
            profile: Profile::M,
            palette: Palette::Hues4,
            is_static: false,
            shape_id: 0,
            session_id,
            payload_len: 0,
            segment_kb_log2: 3,
            segment_index: 0,
            packet_seed: 0,
        }
    }

    fn payload(n: usize) -> Vec<u8> {
        let mut x = 0x9E37_79B9u32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8
            })
            .collect()
    }

    fn session(data: &[u8], t: usize, id: u16) -> PacketSession {
        PacketSession::new(data, t, header(id), &SessionConfig::default()).unwrap()
    }

    /// Deterministic shuffle + loss; returns the received packet order for `passes` passes.
    fn lossy_order(total: u32, loss_percent: u32, seed: u64) -> Vec<u32> {
        let mut x = seed | 1;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut v: Vec<u32> = (0..total)
            .filter(|_| next() % 100 >= u64::from(loss_percent))
            .collect();
        for i in (1..v.len()).rev() {
            v.swap(i, (next() % (i as u64 + 1)) as usize);
        }
        v
    }

    /// Feed packets until Done; returns (payload, packets used).
    fn run(s: &PacketSession, order: &[u32]) -> Option<(Vec<u8>, usize)> {
        let mut rx = PacketReceiver::new();
        for (n, &i) in order.iter().enumerate() {
            let (h, sym) = s.packet(i);
            if let Progress::Done(p) = rx.push(&h, &sym) {
                return Some((p, n + 1));
            }
        }
        None
    }

    #[test]
    fn roundtrip_sizes_clean() {
        for n in [0usize, 1, 100, 6_000, 20_000] {
            for t in [64usize, 333, 1250, 2000] {
                let data = payload(n);
                let s = session(&data, t, 7);
                let order: Vec<u32> = (0..s.frames_per_pass()).collect();
                let (got, used) = run(&s, &order).unwrap_or_else(|| panic!("n={n} t={t}"));
                assert_eq!(got, data, "n={n} t={t}");
                // Clean in-order read: done at the last segment's K-th (systematic) packet,
                // i.e. just before its r = 2 repair packets.
                assert_eq!(used as u32, s.frames_per_pass() - 2, "n={n} t={t}");
            }
        }
    }

    #[test]
    fn roundtrip_large_lossy_shuffled() {
        let data = payload(200_000);
        let s = session(&data, 1250, 0x1234);
        let fpp = s.frames_per_pass();
        // 30 % loss over six passes, shuffled across passes.
        let order = lossy_order(6 * fpp, 30, 42);
        let (got, _) = run(&s, &order).expect("should decode within six lossy passes");
        assert_eq!(got, data);
    }

    #[test]
    fn roundtrip_lossy_small_sizes() {
        for (n, t, seed) in [
            (0, 1250, 1),
            (1, 1250, 2),
            (100, 1250, 3),
            (6_000, 1250, 4),
            (20_000, 700, 5),
        ] {
            let data = payload(n);
            let s = session(&data, t, 99);
            let order = lossy_order(12 * s.frames_per_pass(), 30, seed);
            let (got, _) = run(&s, &order).unwrap_or_else(|| panic!("n={n}"));
            assert_eq!(got, data);
        }
    }

    #[test]
    fn progressive_delivery_in_order() {
        let data = payload(20_000);
        let s = session(&data, 1250, 5);
        let mut rx = PacketReceiver::new();
        let mut collected = Vec::new();
        // Deliver segment 2's packets first, then 1's, then 0's.
        let mut seg_packets: Vec<Vec<u32>> = vec![vec![]; 3];
        for i in 0..s.frames_per_pass() {
            seg_packets[usize::from(s.packet(i).0.segment_index)].push(i);
        }
        for seg in [2usize, 1, 0] {
            for &i in &seg_packets[seg] {
                let (h, sym) = s.packet(i);
                let before = collected.len();
                let p = rx.push(&h, &sym);
                if let Some(b) = rx.take_ready() {
                    collected.extend(b);
                }
                if seg > 0 {
                    // Nothing can be delivered before segment 0 is complete.
                    assert_eq!(collected.len(), before, "delivered out of order");
                    assert!(!matches!(p, Progress::Done(_)));
                }
            }
        }
        assert_eq!(collected, data);
        assert!(rx.take_ready().is_none());
    }

    #[test]
    fn segment_delivered_as_soon_as_complete() {
        let data = payload(20_000);
        let s = session(&data, 1250, 5);
        let mut rx = PacketReceiver::new();
        let mut got = Vec::new();
        let mut first_delivery_at = None;
        for i in 0..s.frames_per_pass() {
            let (h, sym) = s.packet(i);
            rx.push(&h, &sym);
            if let Some(b) = rx.take_ready() {
                first_delivery_at.get_or_insert(i + 1);
                got.extend(b);
            }
        }
        // Segment 0 has K = 7: its bytes are available after exactly 7 packets.
        assert_eq!(first_delivery_at, Some(7));
        assert_eq!(got, data);
    }

    #[test]
    fn progress_counts() {
        let data = payload(20_000);
        let s = session(&data, 1250, 5);
        let mut rx = PacketReceiver::new();
        let (h, sym) = s.packet(0);
        assert_eq!(
            rx.push(&h, &sym),
            Progress::Accepted {
                have: 1,
                need: 17,
                segments_done: 0,
                segments_total: 3
            }
        );
        // Duplicate does not count.
        assert_eq!(
            rx.push(&h, &sym),
            Progress::Accepted {
                have: 1,
                need: 17,
                segments_done: 0,
                segments_total: 3
            }
        );
        for i in 1..7 {
            let (h, sym) = s.packet(i);
            rx.push(&h, &sym);
        }
        let (h, sym) = s.packet(7); // repair symbol of a finished segment
        assert_eq!(
            rx.push(&h, &sym),
            Progress::Accepted {
                have: 7,
                need: 17,
                segments_done: 1,
                segments_total: 3
            }
        );
    }

    #[test]
    fn done_is_sticky_and_take_ready_flushes() {
        let data = payload(300);
        let s = session(&data, 100, 5);
        let mut rx = PacketReceiver::new();
        let mut last = None;
        for i in 0..s.frames_per_pass() {
            let (h, sym) = s.packet(i);
            last = Some(rx.push(&h, &sym));
        }
        assert_eq!(last, Some(Progress::Done(data.clone())));
        let (h, sym) = s.packet(0);
        assert_eq!(rx.push(&h, &sym), Progress::Done(data.clone()));
        assert_eq!(rx.take_ready(), Some(data));
        assert_eq!(rx.take_ready(), None);
    }

    #[test]
    fn crc_split_across_segments() {
        // 8189 payload bytes + 4 CRC bytes = 8193: the last segment holds only 1 CRC byte.
        let data = payload(8189);
        let s = session(&data, 1250, 5);
        assert_eq!(s.segment_count(), 2);
        let order: Vec<u32> = (0..s.frames_per_pass()).collect();
        let mut rx = PacketReceiver::new();
        let mut got = Vec::new();
        let mut done = None;
        for &i in &order {
            let (h, sym) = s.packet(i);
            if let Progress::Done(p) = rx.push(&h, &sym) {
                done = Some(p);
            }
            if let Some(b) = rx.take_ready() {
                got.extend(b);
            }
        }
        assert_eq!(done.as_deref(), Some(&data[..]));
        assert_eq!(got, data);
    }

    #[test]
    fn other_session_rejected() {
        let a = session(&payload(500), 100, 1);
        let b = session(&payload(500), 100, 2);
        let mut rx = PacketReceiver::new();
        let (h, sym) = a.packet(0);
        assert!(matches!(rx.push(&h, &sym), Progress::Accepted { .. }));
        assert_eq!(rx.session_id(), Some(1));
        let (h, sym) = b.packet(0);
        assert_eq!(
            rx.push(&h, &sym),
            Progress::Rejected(DecodeError::OtherSession)
        );
        // Session A still completes.
        let order: Vec<u32> = (0..a.frames_per_pass()).collect();
        let mut done = false;
        for i in order {
            let (h, sym) = a.packet(i);
            done |= matches!(rx.push(&h, &sym), Progress::Done(_));
        }
        assert!(done);
    }

    #[test]
    fn inconsistent_headers_rejected() {
        let s = session(&payload(500), 100, 1);
        let mut rx = PacketReceiver::new();
        let (h, sym) = s.packet(0);
        rx.push(&h, &sym);
        let (h1, sym1) = s.packet(1);
        let bad = [
            Header {
                payload_len: 501,
                ..h1
            },
            Header {
                segment_kb_log2: 4,
                ..h1
            },
            Header {
                profile: Profile::L,
                ..h1
            },
            Header { shape_id: 3, ..h1 },
            Header {
                is_static: true,
                ..h1
            },
            Header {
                shape_table_version: 1,
                ..h1
            },
            Header {
                segment_index: 1,
                ..h1
            },
            Header {
                packet_seed: 4096,
                ..h1
            },
            Header {
                palette: Palette::Hues8,
                ..h1
            },
        ];
        for b in bad {
            assert!(matches!(rx.push(&b, &sym1), Progress::Rejected(_)), "{b:?}");
        }
        assert_eq!(
            rx.push(&h1, &sym1[..50]),
            Progress::Rejected(DecodeError::WrongSymbolCount {
                expected: 100,
                got: 50
            })
        );
        assert!(matches!(
            rx.push(&h1, &sym1),
            Progress::Accepted { have: 2, .. }
        ));
    }

    #[test]
    fn bad_first_packets_do_not_lock() {
        let mut rx = PacketReceiver::new();
        let h = Header {
            payload_len: 500,
            ..header(1)
        };
        assert!(matches!(rx.push(&h, &[]), Progress::Rejected(_)));
        assert!(matches!(
            rx.push(&h, &vec![0; 70_000]),
            Progress::Rejected(_)
        ));
        // Needs more than 4096 symbols for a 4 KiB segment of 1-byte symbols.
        let big = Header {
            payload_len: 8000,
            ..header(1)
        };
        assert!(matches!(rx.push(&big, &[0]), Progress::Rejected(_)));
        let st = Header {
            payload_len: 500,
            is_static: true,
            ..header(1)
        };
        assert!(matches!(rx.push(&st, &[0; 10]), Progress::Rejected(_)));
        assert_eq!(rx.session_id(), None);
        let huge = Header {
            payload_len: u32::MAX,
            ..header(1)
        };
        assert!(matches!(rx.push(&huge, &[0; 10]), Progress::Rejected(_)));
        let kb = Header {
            segment_kb_log2: 15,
            payload_len: 1 << 20,
            ..header(1)
        };
        let _ = rx.push(&kb, &[0; 10]);
        assert!(matches!(
            rx.push(
                &Header {
                    segment_kb_log2: 16,
                    ..header(1)
                },
                &[0; 10]
            ),
            Progress::Rejected(_)
        ));
    }

    #[test]
    fn corrupted_symbols_never_give_wrong_done() {
        let data = payload(3000);
        let s = session(&data, 500, 3);
        let fpp = s.frames_per_pass();
        for corrupt_at in 0..fpp {
            let mut rx = PacketReceiver::new();
            let mut saw_crc_error = false;
            for pass in 0..4 {
                for i in 0..fpp {
                    let idx = pass * fpp + i;
                    let (h, mut sym) = s.packet(idx);
                    if pass == 0 && i == corrupt_at {
                        sym[0] ^= 0x5A;
                        let last = sym.len() - 1;
                        sym[last] ^= 0xFF;
                    }
                    match rx.push(&h, &sym) {
                        Progress::Done(p) => {
                            assert_eq!(p, data, "wrong Done for corruption at {corrupt_at}");
                        }
                        Progress::Rejected(DecodeError::SegmentCrc(_)) => saw_crc_error = true,
                        _ => {}
                    }
                }
            }
            // The corruption is either caught by a CRC or never mattered (a repair symbol
            // that was not needed); a wrong Done is excluded by the assertion above.
            let _ = saw_crc_error;
        }
    }

    #[test]
    fn corrupted_systematic_symbol_is_caught_and_recovered() {
        let data = payload(1000);
        let s = session(&data, 500, 3); // K = ceil(1008/500) = 3
        let mut rx = PacketReceiver::new();
        let mut result = None;
        for i in 0..3 {
            let (h, mut sym) = s.packet(i);
            if i == 1 {
                sym[10] ^= 1;
            }
            result = Some(rx.push(&h, &sym));
        }
        assert_eq!(result, Some(Progress::Rejected(DecodeError::SegmentCrc(0))));
        assert_eq!(rx.take_ready(), None);
        // The segment restarted from scratch; a clean pass completes it.
        let mut done = None;
        for i in 0..s.frames_per_pass() * 2 {
            let (h, sym) = s.packet(i);
            if let Progress::Done(p) = rx.push(&h, &sym) {
                done = Some(p);
                break;
            }
        }
        assert_eq!(done, Some(data));
    }

    #[test]
    fn random_garbage_never_panics_or_decodes() {
        let data = payload(12_000);
        let s = session(&data, 400, 3);
        let mut rx = PacketReceiver::new();
        let (h0, sym0) = s.packet(0);
        rx.push(&h0, &sym0);
        let mut x = 12345u64;
        for n in 0..300u32 {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let sym: Vec<u8> = (0..400).map(|k| (x >> (k % 56)) as u8 ^ k as u8).collect();
            let h = Header {
                segment_index: (n % 2) as u16,
                packet_seed: (x >> 40) as u16 & 0xFFF,
                ..h0
            };
            match rx.push(&h, &sym) {
                Progress::Done(p) => assert_eq!(p, data),
                Progress::Accepted { .. } | Progress::Rejected(_) => {}
            }
        }
    }

    #[test]
    fn symbol_layer_rejects_short_frames() {
        let mut rx = Receiver::new();
        assert_eq!(
            rx.feed(&[Sym3::new(0); 10]),
            Progress::Rejected(DecodeError::WrongSymbolCount {
                expected: 48,
                got: 10
            })
        );
        assert_eq!(rx.session_id(), None);
        assert_eq!(rx.take_ready(), None);
    }

    #[test]
    fn k_equals_one_segments() {
        // Tiny payload, big T: K = 1; repair-only reception must still decode.
        let data = payload(10);
        let s = session(&data, 1250, 8);
        assert_eq!(s.source_symbols(0), Some(1));
        let mut rx = PacketReceiver::new();
        let (h, sym) = s.packet(2); // seed 2 = repair
        assert!(h.packet_seed >= 1);
        assert_eq!(rx.push(&h, &sym), Progress::Done(data));
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(24))]

        #[test]
        fn lossy_shuffled_roundtrip(
            n in 0usize..30_000,
            t in 16usize..2_500,
            kb in 0u8..5,
            repair in 0u32..4,
            loss in 0u32..40,
            seed in 1u64..u64::MAX,
        ) {
            let data = payload(n);
            let cfg = SessionConfig {
                segment_kb_log2: kb,
                repair_per_pass: repair,
                ..SessionConfig::default()
            };
            // Skip combinations the sender legitimately refuses.
            let Ok(s) = PacketSession::new(&data, t, header(0x4242), &cfg) else {
                return Ok(());
            };
            // Enough passes for 40 % loss with huge margin.
            let order = lossy_order(30 * s.frames_per_pass(), loss, seed);
            let (got, _) = run(&s, &order).expect("30 passes must be enough");
            proptest::prop_assert_eq!(got, data);
        }

        #[test]
        fn corruption_never_yields_wrong_done(
            n in 1usize..4_000,
            t in 16usize..800,
            flips in proptest::collection::vec((0u32..400, 0usize..800, 1u8..=255), 1..8),
        ) {
            let data = payload(n);
            let s = session(&data, t, 9);
            let mut rx = PacketReceiver::new();
            for pass in 0..6u32 {
                for i in 0..s.frames_per_pass() {
                    let idx = pass * s.frames_per_pass() + i;
                    let (h, mut sym) = s.packet(idx);
                    for &(at, byte, mask) in &flips {
                        if at == idx {
                            let len = sym.len();
                            sym[byte % len] ^= mask;
                        }
                    }
                    if let Progress::Done(p) = rx.push(&h, &sym) {
                        proptest::prop_assert_eq!(&p, &data);
                    }
                }
            }
        }
    }
}
