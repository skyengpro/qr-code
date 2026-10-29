use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::layout::{Lattice, in_keepout};
use crate::profile::Profile;

/// Usable (keep-outs removed) dot count.
fn usable(table: &ShapeTable, id: u8, profile: Profile) -> usize {
    table.dot_order(id, profile).expect("known id").len()
}

/// Sizes of the 8-connected (hex adjacency) components of the usable dots.
fn island_sizes(table: &ShapeTable, id: u8, profile: Profile) -> Vec<usize> {
    let order = table.dot_order(id, profile).expect("known id");
    let mut cells: BTreeMap<(i32, i32), bool> =
        order.points().iter().map(|p| ((p.q, p.r), false)).collect();
    let keys: Vec<(i32, i32)> = cells.keys().copied().collect();
    let mut sizes = Vec::new();
    for start in keys {
        if cells[&start] {
            continue;
        }
        let mut size = 0;
        let mut stack = vec![start];
        cells.insert(start, true);
        while let Some((q, r)) = stack.pop() {
            size += 1;
            for (dq, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)] {
                let n = (q + dq, r + dr);
                if cells.get(&n) == Some(&false) {
                    cells.insert(n, true);
                    stack.push(n);
                }
            }
        }
        sizes.push(size);
    }
    sizes
}

#[test]
fn disc_mask_equals_norm_test() {
    let table = ShapeTable::builtin();
    for profile in Profile::ALL {
        let lattice = Lattice::new(profile);
        let f = profile.field_half();
        let mask = table.mask(DISC, profile).unwrap();
        for (p, &m) in lattice.points().iter().zip(&mask) {
            assert_eq!(
                m,
                p.at.norm() <= f,
                "{profile:?} q={} r={} at {:?}",
                p.q,
                p.r,
                p.at
            );
        }
    }
}

#[test]
fn ids_and_names() {
    let table = ShapeTable::builtin();
    let ids: Vec<u8> = table.ids().collect();
    let mut expected: Vec<u8> = (0..=30).collect();
    assert_eq!(ids, expected);
    expected.sort_unstable();
    assert_eq!(ids, expected);
    assert!(!table.contains(31));
    assert!(!table.contains(32));
    assert_eq!(table.name(31), None);
    assert_eq!(table.name(DISC), Some("disc"));
    assert_eq!(table.name(ROUNDED_SQUARE), Some("rounded-square"));
    assert_eq!(table.name(HEART), Some("heart"));
    assert_eq!(table.name(HEXAGON), Some("hexagon"));
    assert_eq!(table.name(RING), Some("ring"));
    for (i, c) in ('A'..='Z').enumerate() {
        let id = letter(c).unwrap();
        assert_eq!(usize::from(id), i + 2);
        assert_eq!(letter(c.to_ascii_lowercase()), Some(id));
        let mut buf = [0u8; 4];
        assert_eq!(table.name(id), Some(&*c.encode_utf8(&mut buf)));
    }
    assert_eq!(letter('1'), None);
    assert_eq!(letter('é'), None);
    assert_eq!(table.mask(31, Profile::S), None);
    assert!(table.dot_order(200, Profile::M).is_none());
    assert_eq!(builtin_contains(31, 500.0, 500.0), None);
    assert_eq!(builtin_contains(0, 500.0, 500.0), Some(true));
    assert_eq!(builtin_contains(0, 0.0, 0.0), Some(false));
}

#[test]
fn capacity_targets_at_m() {
    let table = ShapeTable::builtin();
    let disc = usable(&table, DISC, Profile::M);
    assert!((4400..=4600).contains(&disc), "disc {disc}");
    let square = usable(&table, ROUNDED_SQUARE, Profile::M);
    assert!(square > disc, "rounded square {square}");
    let heart = usable(&table, HEART, Profile::M);
    assert!((3000..=3600).contains(&heart), "heart {heart}");
    let hexagon = usable(&table, HEXAGON, Profile::M);
    assert!(hexagon > 3500, "hexagon {hexagon}");
    let ring = usable(&table, RING, Profile::M);
    assert!(ring >= 1400, "ring {ring}");
    for c in 'A'..='Z' {
        let n = usable(&table, letter(c).unwrap(), Profile::M);
        assert!(n >= 1250, "letter {c}: {n}");
    }
}

#[test]
fn no_small_islands() {
    let table = ShapeTable::builtin();
    for id in table.ids() {
        for (profile, min) in [(Profile::S, 25), (Profile::M, 60), (Profile::L, 60)] {
            let sizes = island_sizes(&table, id, profile);
            let smallest = sizes.iter().copied().min().unwrap_or(0);
            assert!(
                smallest >= min,
                "{} at {profile:?}: islands {sizes:?}",
                table.name(id).unwrap()
            );
        }
    }
}

#[test]
fn counters_hold_the_center_anchor() {
    // The center anchor lands in the counter of these letters.
    for c in ['O', 'D', 'Q'] {
        let id = letter(c).unwrap();
        assert_eq!(builtin_contains(id, 500.0, 500.0), Some(false), "{c}");
    }
    // ... and a ring around the counter exists.
    for c in ['O', 'D', 'Q'] {
        let id = letter(c).unwrap();
        assert_eq!(builtin_contains(id, 500.0, 70.0 + 10.0), Some(true), "{c}");
    }
}

/// Opening test: every ink pixel of a letter must lie in a disc of radius 2 pitches at S
/// that is entirely ink (i.e. strokes are at least 4 pitches wide), except for a small
/// fraction of pixels in acute corners.
#[test]
fn letters_have_thick_strokes() {
    const N: usize = 125; // 8-unit cells
    const CELL: f64 = 1000.0 / N as f64;
    let pitch_s = 1000.0 / f64::from(Profile::S.dots_across());
    let radius = 2.0 * pitch_s;
    let reach = (radius / CELL) as i32;
    let offsets: Vec<(i32, i32)> = (-reach..=reach)
        .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| {
            libm::hypot(f64::from(dx), f64::from(dy)) * CELL <= radius - CELL * 0.75
        })
        .collect();
    for c in 'A'..='Z' {
        let id = letter(c).unwrap();
        let ink: Vec<bool> = (0..N * N)
            .map(|k| {
                let (x, y) = ((k % N) as f64 + 0.5, (k / N) as f64 + 0.5);
                builtin_contains(id, x * CELL, y * CELL).unwrap()
            })
            .collect();
        let at = |x: i32, y: i32| {
            (0..N as i32).contains(&x)
                && (0..N as i32).contains(&y)
                && ink[y as usize * N + x as usize]
        };
        // Erode, then dilate.
        let eroded: Vec<bool> = (0..N * N)
            .map(|k| {
                let (x, y) = ((k % N) as i32, (k / N) as i32);
                offsets.iter().all(|&(dx, dy)| at(x + dx, y + dy))
            })
            .collect();
        let mut covered = vec![false; N * N];
        for (k, _) in eroded.iter().enumerate().filter(|&(_, &e)| e) {
            let (x, y) = ((k % N) as i32, (k / N) as i32);
            for &(dx, dy) in &offsets {
                let (nx, ny) = (x + dx, y + dy);
                if (0..N as i32).contains(&nx) && (0..N as i32).contains(&ny) {
                    covered[ny as usize * N + nx as usize] = true;
                }
            }
        }
        let total = ink.iter().filter(|&&b| b).count();
        let lost = ink.iter().zip(&covered).filter(|&(&i, &c)| i && !c).count();
        assert!(
            lost * 100 <= total * 3,
            "letter {c}: {lost} of {total} ink cells too thin"
        );
    }
}

fn custom_masks(mut f: impl FnMut(Profile, usize) -> bool) -> [Vec<bool>; 3] {
    Profile::ALL.map(|profile| {
        (0..Lattice::new(profile).len())
            .map(|k| f(profile, k))
            .collect::<Vec<bool>>()
    })
}

#[test]
fn bytes_round_trip_with_custom_shapes() {
    let mut table = ShapeTable::builtin();
    table.set_version(2).unwrap();
    let stripes = custom_masks(|_, k| k % 7 < 3);
    let checker = custom_masks(|p, k| (k + usize::from(p.code())) % 2 == 0);
    table.insert(200, "stripes", stripes.clone()).unwrap();
    table.insert(32, "checker ✓", checker.clone()).unwrap();
    assert_eq!(table.ids().last(), Some(200));
    assert_eq!(
        table.ids().filter(|&i| i >= 32).collect::<Vec<_>>(),
        [32, 200]
    );
    assert_eq!(table.name(32), Some("checker ✓"));
    assert_eq!(table.mask(200, Profile::M).unwrap(), stripes[1]);
    assert!(table.dot_order(32, Profile::S).unwrap().len() > 100);

    let bytes = table.to_bytes();
    assert_eq!(&bytes[..4], b"PCSH");
    let back = ShapeTable::from_bytes(&bytes).unwrap();
    assert_eq!(back, table);
    assert_eq!(back.version(), 2);
    for profile in Profile::ALL {
        assert_eq!(back.mask(32, profile), table.mask(32, profile));
    }
    assert_eq!(back.to_bytes(), bytes);

    // A builtin-only table round-trips too and stays tiny.
    let plain = ShapeTable::builtin().to_bytes();
    assert_eq!(plain.len(), 12);
    assert_eq!(
        ShapeTable::from_bytes(&plain).unwrap(),
        ShapeTable::builtin()
    );
}

#[test]
fn insert_validates() {
    let mut table = ShapeTable::builtin();
    let good = custom_masks(|_, _| true);
    assert_eq!(
        table.insert(31, "x", good.clone()),
        Err(ShapeError::ReservedId(31))
    );
    assert_eq!(
        table.insert(0, "x", good.clone()),
        Err(ShapeError::ReservedId(0))
    );
    let mut bad = good.clone();
    bad[1].pop();
    assert!(matches!(
        table.insert(40, "x", bad),
        Err(ShapeError::MaskLength {
            profile: Profile::M,
            ..
        })
    ));
    let long = String::from_utf8(vec![b'a'; 256]).unwrap();
    assert_eq!(
        table.insert(40, &long, good.clone()),
        Err(ShapeError::NameTooLong(256))
    );
    table.insert(40, "ok", good.clone()).unwrap();
    assert_eq!(
        table.insert(40, "again", good),
        Err(ShapeError::DuplicateId(40))
    );
    assert_eq!(table.set_version(4), Err(ShapeError::BadTableVersion(4)));
}

#[test]
fn from_bytes_rejects_garbage() {
    assert_eq!(ShapeTable::from_bytes(&[]), Err(ShapeError::BadMagic));
    assert_eq!(
        ShapeTable::from_bytes(b"nope, not a table"),
        Err(ShapeError::BadMagic)
    );
    assert_eq!(ShapeTable::from_bytes(b"PCSH"), Err(ShapeError::Truncated));

    let mut table = ShapeTable::builtin();
    table
        .insert(77, "x", custom_masks(|_, k| k % 3 == 0))
        .unwrap();
    let good = table.to_bytes();
    // Every truncation fails cleanly.
    for len in 0..good.len() {
        assert!(ShapeTable::from_bytes(&good[..len]).is_err(), "len {len}");
    }
    // Every single-byte corruption is caught by the checksum or a structural check.
    for at in (0..good.len()).step_by(7) {
        let mut bad = good.clone();
        bad[at] ^= 0x55;
        assert!(ShapeTable::from_bytes(&bad).is_err(), "byte {at}");
    }
    let mut extra = good.clone();
    extra.push(0);
    assert!(ShapeTable::from_bytes(&extra).is_err());
    // Pseudo-random garbage with a valid header prefix.
    let mut state = 0x1234_5678_9abc_def0_u64;
    for round in 0..200 {
        let mut junk: Vec<u8> = (0..(round * 5))
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 56) as u8
            })
            .collect();
        if round % 2 == 0 && junk.len() >= 6 {
            junk[..4].copy_from_slice(b"PCSH");
            junk[4] = 1;
        }
        assert!(ShapeTable::from_bytes(&junk).is_err());
    }
}

#[test]
fn from_bytes_rejects_structural_errors_behind_a_valid_checksum() {
    fn seal(mut body: Vec<u8>) -> Vec<u8> {
        let crc = crate::crc::crc32(&body);
        body.extend_from_slice(&crc.to_le_bytes());
        body
    }
    let header = |count: u16| {
        let mut v = b"PCSH".to_vec();
        v.extend_from_slice(&[1, 0]);
        v.extend_from_slice(&count.to_le_bytes());
        v
    };
    // Count claims an entry that is not there.
    assert_eq!(
        ShapeTable::from_bytes(&seal(header(1))),
        Err(ShapeError::Truncated)
    );
    // Reserved id.
    let mut v = header(1);
    v.extend_from_slice(&[5, 0]);
    assert_eq!(
        ShapeTable::from_bytes(&seal(v)),
        Err(ShapeError::ReservedId(5))
    );
    // Unsupported table version.
    let mut v = b"PCSH".to_vec();
    v.extend_from_slice(&[1, 9, 0, 0]);
    assert_eq!(
        ShapeTable::from_bytes(&seal(v)),
        Err(ShapeError::BadTableVersion(9))
    );
    // Unsupported format version.
    let mut v = b"PCSH".to_vec();
    v.extend_from_slice(&[2, 0, 0, 0]);
    assert_eq!(
        ShapeTable::from_bytes(&seal(v)),
        Err(ShapeError::UnsupportedFormat(2))
    );
    // Bad UTF-8 name.
    let mut v = header(1);
    v.extend_from_slice(&[40, 1, 0xFF]);
    assert_eq!(ShapeTable::from_bytes(&seal(v)), Err(ShapeError::BadName));
    // Padding bits set: build a valid entry, then flip a padding bit of the last mask.
    let mut table = ShapeTable::builtin();
    table.insert(40, "p", custom_masks(|_, _| false)).unwrap();
    let mut body = table.to_bytes();
    body.truncate(body.len() - 4);
    let last = body.len() - 1;
    let l_len = Lattice::new(Profile::L).len();
    if !l_len.is_multiple_of(8) {
        body[last] |= 0x80;
        assert_eq!(
            ShapeTable::from_bytes(&seal(body)),
            Err(ShapeError::BadPadding)
        );
    }
    // Unsorted / duplicate ids.
    let mut table = ShapeTable::builtin();
    table.insert(40, "a", custom_masks(|_, _| false)).unwrap();
    table.insert(41, "b", custom_masks(|_, _| false)).unwrap();
    let mut body = table.to_bytes();
    body.truncate(body.len() - 4);
    let first_id = 8;
    let entry_len = (body.len() - 8) / 2;
    body[first_id + entry_len] = 40;
    assert_eq!(
        ShapeTable::from_bytes(&seal(body)),
        Err(ShapeError::Unsorted)
    );
}

// ---------------------------------------------------------------------------------------
// Visual checks. Run with
//   cargo test -p particlecode-core --lib shapes::tests::preview -- --ignored --nocapture
// ---------------------------------------------------------------------------------------

/// ASCII rendering of the usable dots; `@` ink, `o` keep-out, `.` outside the shape.
#[cfg(feature = "std")]
fn ascii(table: &ShapeTable, id: u8, profile: Profile) -> String {
    let lattice = Lattice::new(profile);
    let mask = table.mask(id, profile).unwrap();
    let f = profile.field_half();
    let mut rows: BTreeMap<i32, Vec<(usize, char)>> = BTreeMap::new();
    for (p, &m) in lattice.points().iter().zip(&mask) {
        let col = libm::round((p.at.x + f) * 2.0) as usize;
        let ch = if in_keepout(profile, p.at) {
            if m { 'o' } else { ' ' }
        } else if m {
            '@'
        } else {
            ' '
        };
        rows.entry(p.r).or_default().push((col, ch));
    }
    let mut out = String::new();
    for cells in rows.values() {
        let mut line = vec![' '; (4.0 * f) as usize + 3];
        for &(col, ch) in cells {
            line[col] = ch;
        }
        out.extend(line.iter());
        out.push('\n');
    }
    out
}

#[cfg(feature = "std")]
#[test]
#[ignore = "visual check; run with --ignored --nocapture"]
#[allow(clippy::print_stdout)]
fn preview_all() {
    let table = ShapeTable::builtin();
    let which = std::env::var("PC_PREVIEW").unwrap_or_else(|_| "ABCDEFGHIJKLMNOPQRSTUVWXYZ".into());
    for c in which.chars() {
        let id = match c {
            '*' => DISC,
            '#' => ROUNDED_SQUARE,
            '+' => HEART,
            '=' => HEXAGON,
            '0' => RING,
            _ => letter(c).unwrap(),
        };
        println!(
            "--- {} (id {id}) S: {} dots",
            table.name(id).unwrap(),
            usable(&table, id, Profile::S)
        );
        println!("{}", ascii(&table, id, Profile::S));
    }
}

#[cfg(feature = "std")]
#[test]
#[ignore = "prints the capacity table; run with --ignored --nocapture"]
#[allow(clippy::print_stdout)]
fn preview_capacity_table() {
    let table = ShapeTable::builtin();
    for id in table.ids() {
        let n = Profile::ALL.map(|p| usable(&table, id, p));
        let sizes = island_sizes(&table, id, Profile::M);
        println!(
            "{id:>2} {:<15} S {:>5} M {:>5} L {:>5}   smallest island at M: {}",
            table.name(id).unwrap(),
            n[0],
            n[1],
            n[2],
            sizes.iter().min().unwrap()
        );
    }
}

/// Dumps 250 × 250 analytic bitmaps (one `0`/`1` text file per shape) into the directory
/// named by `PC_SHAPE_DUMP`, for rendering contact sheets.
#[cfg(feature = "std")]
#[test]
#[ignore = "writes files; set PC_SHAPE_DUMP=<dir>"]
fn dump_bitmaps() {
    let Ok(dir) = std::env::var("PC_SHAPE_DUMP") else {
        return;
    };
    let table = ShapeTable::builtin();
    for id in table.ids() {
        let mut text = String::new();
        for j in 0..250 {
            for i in 0..250 {
                let (x, y) = ((f64::from(i) + 0.5) * 4.0, (f64::from(j) + 0.5) * 4.0);
                text.push(if builtin_contains(id, x, y).unwrap() {
                    '1'
                } else {
                    '0'
                });
            }
            text.push('\n');
        }
        std::fs::write(format!("{dir}/{id:02}.txt"), text).unwrap();
    }
}
