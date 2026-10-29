//! `info`: capacity table for profiles and shapes.

use std::io::Write;

use anyhow::Result;
use particlecode_core::body::BodyCodec;
use particlecode_core::shapes::{self, ShapeTable};
use particlecode_core::{EccLevel, Profile};

use crate::spec::SessionSpec;
use particlecode_core::ShapeChoice;

/// Payload sizes the table reports frame counts for, in KiB.
pub const PAYLOAD_KB: [usize; 3] = [1, 6, 20];

/// Source frames and frames per pass for one payload size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameCount {
    /// Frames needed when nothing is lost (`Σ K_s`).
    pub needed: u32,
    /// Frames in one pass including the repair frames.
    pub per_pass: u32,
}

/// One line of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Lattice profile.
    pub profile: Profile,
    /// Shape id.
    pub shape_id: u8,
    /// Shape name.
    pub shape: String,
    /// Payload dots.
    pub dots: usize,
    /// Bytes per frame at normal (animated) ECC.
    pub bytes_normal: usize,
    /// Bytes per frame at high (static) ECC.
    pub bytes_high: usize,
    /// Frame counts for [`PAYLOAD_KB`] at normal ECC; `None` if the session cannot be built.
    pub frames: [Option<FrameCount>; 3],
}

/// Rows for the cross product of `profiles` and `shape_ids` (profile-major). Unknown shape
/// ids are skipped.
pub fn rows(profiles: &[Profile], shape_ids: &[u8]) -> Vec<Row> {
    let table = ShapeTable::builtin();
    let mut out = Vec::new();
    for &profile in profiles {
        for &id in shape_ids {
            let Some(order) = table.dot_order(id, profile) else {
                continue;
            };
            let bytes = |ecc| BodyCodec::new(order.len(), ecc).block_size();
            let frames = PAYLOAD_KB.map(|kb| frame_count(profile, id, kb));
            out.push(Row {
                profile,
                shape_id: id,
                shape: table.name(id).unwrap_or("?").to_owned(),
                dots: order.len(),
                bytes_normal: bytes(EccLevel::Normal),
                bytes_high: bytes(EccLevel::High),
                frames,
            });
        }
    }
    out
}

fn frame_count(profile: Profile, shape: u8, kb: usize) -> Option<FrameCount> {
    // Bytes of a fixed pseudo-pattern: only the length matters for the counts.
    let payload: Vec<u8> = (0..kb * 1024).map(|i| (i * 31 % 251) as u8).collect();
    let spec = SessionSpec {
        profile,
        shape: ShapeChoice::Require(shape),
        session_id: Some(0),
        key: Some([0; 32]),
        clock_origin: Some(0),
        ..SessionSpec::default()
    };
    let session = spec.build(&payload).ok()?;
    let packets = session.packets();
    let needed = (0..packets.segment_count())
        .filter_map(|s| packets.source_symbols(s))
        .sum();
    Some(FrameCount {
        needed,
        per_pass: session.frames_per_pass(),
    })
}

/// Every built-in shape id, ascending.
pub fn builtin_shape_ids() -> Vec<u8> {
    (0..=shapes::LAST_BUILTIN).collect()
}

/// Print `rows` as an aligned text table.
pub fn write_table(w: &mut impl Write, rows: &[Row]) -> Result<()> {
    writeln!(
        w,
        "{:<4} {:<15} {:>6} {:>19}   frames for a payload of",
        "", "", "", "bytes per frame"
    )?;
    writeln!(
        w,
        "{:<4} {:<15} {:>6} {:>9} {:>9}   {}",
        "prof",
        "shape",
        "dots",
        "normal",
        "high",
        PAYLOAD_KB
            .map(|kb| format!("{:>8}", format!("{kb} KB")))
            .join(" ")
    )?;
    for r in rows {
        let frames = r
            .frames
            .map(|f| match f {
                Some(f) => format!("{:>8}", format!("{}+{}", f.needed, f.per_pass - f.needed)),
                None => format!("{:>8}", "-"),
            })
            .join(" ");
        writeln!(
            w,
            "{:<4} {:<15} {:>6} {:>9} {:>9}   {}",
            format!("{:?}", r.profile),
            r.shape,
            r.dots,
            r.bytes_normal,
            r.bytes_high,
            frames
        )?;
    }
    writeln!(
        w,
        "\nframes = source frames needed + repair frames per pass (animated, normal ECC, 8 KiB segments)"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_rows_are_consistent() {
        let rows = rows(&Profile::ALL, &[0]);
        assert_eq!(rows.len(), 3);
        for pair in rows.windows(2) {
            assert!(pair[0].dots < pair[1].dots, "denser profile, more dots");
        }
        for r in &rows {
            assert!(r.bytes_high < r.bytes_normal, "high ECC carries less");
            let counts: Vec<_> = r.frames.iter().map(|f| f.expect("fits")).collect();
            assert!(counts[0].needed < counts[1].needed && counts[1].needed < counts[2].needed);
            assert!(counts.iter().all(|c| c.per_pass > c.needed));
            // 1 KB + framing needs ceil((1024 + 8) / T) frames.
            assert_eq!(
                counts[0].needed as usize,
                (1024 + 8usize).div_ceil(r.bytes_normal)
            );
        }
    }

    #[test]
    fn shapes_have_fewer_dots_than_the_disc_or_equal_frame_size() {
        let all = rows(&[Profile::M], &builtin_shape_ids());
        assert_eq!(all.len(), 31);
        let heart = all.iter().find(|r| r.shape == "heart").expect("heart");
        let disc = all.iter().find(|r| r.shape == "disc").expect("disc");
        assert!(heart.dots < disc.dots);
    }

    #[test]
    fn unknown_ids_are_skipped() {
        assert!(rows(&[Profile::S], &[31, 200]).is_empty());
    }

    #[test]
    fn table_prints_every_row() {
        let rows = rows(&[Profile::M], &[0, 28]);
        let mut buf = Vec::new();
        write_table(&mut buf, &rows).expect("write");
        let text = String::from_utf8(buf).expect("utf8");
        assert!(text.contains("disc") && text.contains("heart"));
        assert!(text.contains("1 KB") && text.contains("20 KB"));
        assert_eq!(text.lines().filter(|l| l.starts_with("M ")).count(), 2);
    }
}
