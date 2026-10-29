//! Shared arithmetic of the fountain layer (private): payload framing, per-segment source
//! blocks, `RaptorQ` parameters and the packet-seed schedule. Sender and receiver both go
//! through here so they cannot disagree.

use alloc::vec::Vec;

use raptorq::ObjectTransmissionInformation;

use crate::crc::crc32;

/// The packet seed is a 12-bit field: encoding symbol ids wrap at this value.
pub(crate) const ESI_SPACE: u32 = 4096;
/// The segment index is a 10-bit field.
pub(crate) const MAX_SEGMENTS: usize = 1024;
/// The segment size field is 4 bits, but `n = 15` is the largest value the format uses.
pub(crate) const MAX_SEGMENT_KB_LOG2: u8 = 15;
/// The payload length is a 24-bit field.
pub(crate) const MAX_PAYLOAD: usize = (1 << 24) - 1;
/// `RaptorQ` symbol sizes are 16-bit.
pub(crate) const MAX_BLOCK_SIZE: usize = u16::MAX as usize;
/// Bytes of CRC-32 appended to the payload and to every segment.
pub(crate) const CRC_LEN: usize = 4;

/// `payload ‖ CRC-32(payload)` with the CRC in big-endian byte order.
pub(crate) fn frame_payload(payload: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(payload.len() + CRC_LEN);
    framed.extend_from_slice(payload);
    framed.extend_from_slice(&crc32(payload).to_be_bytes());
    framed
}

/// Whether `framed` (`payload ‖ CRC-32`) is internally consistent.
pub(crate) fn framed_is_valid(framed: &[u8]) -> bool {
    framed.len() >= CRC_LEN && crc_matches(framed, framed.len() - CRC_LEN)
}

/// Whether `data[len..len + 4]` is the big-endian CRC-32 of `data[..len]`.
pub(crate) fn crc_matches(data: &[u8], len: usize) -> bool {
    data.get(len..len + CRC_LEN)
        .is_some_and(|tail| tail == crc32(&data[..len]).to_be_bytes().as_slice())
}

/// Number of source symbols `K` of a segment of `seg_len` framed bytes at symbol size `t`.
pub(crate) fn source_symbols(seg_len: usize, t: usize) -> usize {
    (seg_len + CRC_LEN).div_ceil(t)
}

/// The `RaptorQ` source block of a segment: `segment ‖ CRC-32(segment)`, zero-padded to a
/// whole number of symbols.
pub(crate) fn source_block(segment: &[u8], t: usize) -> Vec<u8> {
    let k = source_symbols(segment.len(), t);
    let mut block = Vec::with_capacity(k * t);
    block.extend_from_slice(segment);
    block.extend_from_slice(&crc32(segment).to_be_bytes());
    block.resize(k * t, 0);
    block
}

/// `RaptorQ` parameters for one segment: one source block, one sub-block, alignment 1.
pub(crate) fn transmission_info(k: usize, t: usize) -> ObjectTransmissionInformation {
    ObjectTransmissionInformation::new((k * t) as u64, t as u16, 1, 1, 1)
}

/// Packet seed (encoding symbol id) of the `j`-th packet of a segment in pass `pass`, when
/// the segment emits `per_pass` packets per pass.
pub(crate) fn packet_seed(pass: u32, per_pass: u32, j: u32) -> u16 {
    ((u64::from(pass) * u64::from(per_pass) + u64::from(j)) % u64::from(ESI_SPACE)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_layout() {
        let b = source_block(&[1, 2, 3], 4);
        assert_eq!(b.len(), 8);
        assert_eq!(&b[..3], &[1, 2, 3]);
        assert!(crc_matches(&b, 3));
        assert_eq!(source_symbols(3, 4), 2);
        assert_eq!(source_symbols(4, 4), 2);
        assert_eq!(source_symbols(0, 4), 1);
    }

    #[test]
    fn framing() {
        assert!(framed_is_valid(&frame_payload(b"hello")));
        assert!(framed_is_valid(&frame_payload(b"")));
        let mut f = frame_payload(b"hello");
        f[0] ^= 1;
        assert!(!framed_is_valid(&f));
        assert!(!framed_is_valid(&[1, 2]));
    }

    #[test]
    fn seeds_wrap() {
        assert_eq!(packet_seed(0, 9, 3), 3);
        assert_eq!(packet_seed(1, 9, 0), 9);
        assert_eq!(packet_seed(455, 9, 0), 4095);
        assert_eq!(packet_seed(456, 9, 0), 8);
        assert_eq!(packet_seed(u32::MAX, 4096, 4095), 4095);
    }
}
