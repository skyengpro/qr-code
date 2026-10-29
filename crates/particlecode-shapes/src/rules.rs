//! The shape rules of spec section "Shapes" that can be checked without a decoder
//! (rules 2 to 6), evaluated per profile.
//!
//! * **Rule 2, ink coverage:** at least [`MIN_COVERAGE`] of the lattice points of the design
//!   box carry ink.
//! * **Rule 3, stroke width:** the *stroke width* of the shape is at least
//!   [`MIN_STROKE_PITCHES`] pitches. See [`stroke_width_pitches`] for the method.
//! * **Rule 4, islands:** every connected component of usable dots (hex adjacency) has at
//!   least [`MIN_ISLAND_DOTS`] dots.
//! * **Rule 5, keep-outs:** informational. The anchor keep-outs are punched out by
//!   [`DotOrder`]; the report says how many ink dots they took.
//! * **Rule 6, connectivity:** informational. Connected shapes decode better; the report
//!   counts the parts (rules 2 to 4 are checked for every part).
//!
//! Rule 7 (`validate`) needs the vision decoder and lives in [`crate::validate`].

use particlecode_core::body::BodyCodec;
use particlecode_core::layout::{DotOrder, Lattice, LatticePoint, to_design_box};
use particlecode_core::profile::{EccLevel, Profile};

use crate::path::DESIGN_SIZE;
use crate::raster::ShapeInput;
use crate::thickness::Thickness;

/// Rule 2: minimum fraction of lattice points that carry ink.
pub const MIN_COVERAGE: f64 = 0.25;
/// Rule 3: minimum stroke width in pitches.
pub const MIN_STROKE_PITCHES: f64 = 4.0;
/// Rule 4: minimum dots per island.
pub const MIN_ISLAND_DOTS: usize = 60;
/// Share of ink dots that rule 3 ignores when it reports the thinnest stroke. Tapering
/// tips and rounded corners are always thin; a shape whose thin parts stay under this share
/// of its ink is judged by the rest.
pub const THIN_TOLERANCE: f64 = 0.03;

/// Outcome of one rule for one profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// Rule number in the spec (2, 3 or 4).
    pub rule: u8,
    /// Short name.
    pub name: &'static str,
    /// Whether the rule holds.
    pub passed: bool,
    /// Human-readable measurement and requirement.
    pub detail: String,
}

/// Measurements and verdicts for one profile.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileReport {
    /// Profile (S, M or L).
    pub profile: Profile,
    /// Lattice points of the profile.
    pub lattice_points: usize,
    /// Mask points with ink, before keep-outs.
    pub ink_dots: usize,
    /// `ink_dots / lattice_points`.
    pub coverage: f64,
    /// Payload dots after keep-outs.
    pub usable_dots: usize,
    /// Ink dots removed by the anchor keep-outs.
    pub keepout_dots: usize,
    /// Stroke width in pitches (see [`stroke_width_pitches`]).
    pub stroke_pitches: f64,
    /// Island sizes in dots, largest first.
    pub islands: Vec<usize>,
    /// Data bytes per frame at [`EccLevel::Normal`].
    pub bytes_normal: usize,
    /// Data bytes per frame at [`EccLevel::High`].
    pub bytes_high: usize,
    /// Rules 2, 3 and 4.
    pub checks: Vec<Check>,
}

impl ProfileReport {
    /// Whether rules 2 to 4 all hold.
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
}

/// Report for all three profiles.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// What was checked.
    pub label: String,
    /// S, M, L.
    pub profiles: Vec<ProfileReport>,
    /// Warnings from reading the shape.
    pub notes: Vec<String>,
}

impl Report {
    /// Whether rules 2 to 4 hold on every profile.
    pub fn passed(&self) -> bool {
        self.profiles.iter().all(ProfileReport::passed)
    }

    /// Report for `profile`.
    pub fn profile(&self, profile: Profile) -> Option<&ProfileReport> {
        self.profiles.iter().find(|p| p.profile == profile)
    }
}

/// Check a shape against rules 2 to 6 on all profiles.
pub fn check(input: &ShapeInput) -> Report {
    let mut thickness = Thickness::new(&input.raster);
    let profiles = Profile::ALL
        .iter()
        .zip(&input.masks)
        .map(|(&profile, mask)| check_profile(profile, mask, &mut thickness))
        .collect();
    Report {
        label: input.label.clone(),
        profiles,
        notes: input.notes.clone(),
    }
}

/// Check one profile's mask. `thickness` describes the shape's raster.
pub fn check_profile(profile: Profile, mask: &[bool], thickness: &mut Thickness) -> ProfileReport {
    let lattice = Lattice::new(profile);
    let order = DotOrder::new(&lattice, mask);
    let ink_dots = mask.iter().filter(|&&b| b).count();
    let usable_dots = order.len();
    let coverage = ink_dots as f64 / lattice.len().max(1) as f64;

    let stroke_pitches = stroke_width_pitches(profile, order.points(), thickness);
    let islands = island_sizes(order.points());

    let bytes_normal = BodyCodec::new(usable_dots, EccLevel::Normal).block_size();
    let bytes_high = BodyCodec::new(usable_dots, EccLevel::High).block_size();

    let small: Vec<usize> = islands
        .iter()
        .copied()
        .filter(|&n| n < MIN_ISLAND_DOTS)
        .collect();
    let checks = vec![
        Check {
            rule: 2,
            name: "ink coverage",
            passed: coverage >= MIN_COVERAGE,
            detail: format!(
                "{:.1}% of the box ({ink_dots} of {} dots), need >= {:.0}%",
                coverage * 100.0,
                lattice.len(),
                MIN_COVERAGE * 100.0
            ),
        },
        Check {
            rule: 3,
            name: "stroke width",
            passed: stroke_pitches >= MIN_STROKE_PITCHES,
            detail: format!(
                "{stroke_pitches:.1} p, need >= {MIN_STROKE_PITCHES:.0} p \
                 (ignoring the thinnest {:.0}% of the ink: tips, corners)",
                THIN_TOLERANCE * 100.0
            ),
        },
        Check {
            rule: 4,
            name: "islands",
            passed: small.is_empty(),
            detail: if small.is_empty() {
                format!(
                    "{} part(s), smallest {} dots, need >= {MIN_ISLAND_DOTS}",
                    islands.len(),
                    islands.last().copied().unwrap_or(0)
                )
            } else {
                format!(
                    "{} island(s) under {MIN_ISLAND_DOTS} dots: {}",
                    small.len(),
                    join_sizes(&small)
                )
            },
        },
    ];
    ProfileReport {
        profile,
        lattice_points: lattice.len(),
        ink_dots,
        coverage,
        usable_dots,
        keepout_dots: ink_dots - usable_dots.min(ink_dots),
        stroke_pitches,
        islands,
        bytes_normal,
        bytes_high,
        checks,
    }
}

fn join_sizes(sizes: &[usize]) -> String {
    let shown: Vec<String> = sizes.iter().take(6).map(ToString::to_string).collect();
    let more = if sizes.len() > 6 { ", ..." } else { "" };
    format!("{}{more}", shown.join(", "))
}

/// The shape's stroke width in pitches at `profile`.
///
/// **Method.** The local width at a dot is the diameter of the largest disc that lies
/// inside the shape (before keep-outs) and covers the dot: the "local thickness" of the
/// shape. A bar of width `w` has local width `w` everywhere, a blob its inscribed
/// diameter. "Covered by a disc of radius `rho`" is the morphological opening of the
/// 1000 x 1000 raster by `rho`, computed with exact Euclidean distance transforms (see
/// [`crate::thickness`]); the width is found by bisection on `rho`.
///
/// Tips that taper to a point and rounded corners are thin by construction, so the thinnest
/// [`THIN_TOLERANCE`] of the usable dots (by count) are ignored: the stroke width is the
/// largest `w` such that at most that share of the dots has local width below `w`. A shape
/// whose thin parts (tips, corners) cover less than 3% of its ink is judged by the rest;
/// any thin stroke, bridge or serif bigger than that pulls the figure below the limit.
/// Returns 0 for a shape without usable dots.
pub fn stroke_width_pitches(
    profile: Profile,
    dots: &[LatticePoint],
    thickness: &mut Thickness,
) -> f64 {
    if dots.is_empty() {
        return 0.0;
    }
    let unit_per_pitch = f64::from(DESIGN_SIZE) / f64::from(profile.dots_across());
    let points: Vec<(f64, f64)> = dots.iter().map(|d| to_design_box(profile, d.at)).collect();
    let allowed = (points.len() as f64 * THIN_TOLERANCE) as usize;
    let floor = MIN_STROKE_PITCHES * unit_per_pitch;
    thickness.width_quantile(&points, allowed, floor) / unit_per_pitch
}

/// Axial offsets of the six hex neighbours.
const NEIGHBOURS: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

/// Sizes of the connected components (hex adjacency) of `dots`, largest first.
pub fn island_sizes(dots: &[LatticePoint]) -> Vec<usize> {
    if dots.is_empty() {
        return Vec::new();
    }
    let q_min = dots.iter().map(|d| d.q).min().unwrap_or(0) - 1;
    let r_min = dots.iter().map(|d| d.r).min().unwrap_or(0) - 1;
    let q_max = dots.iter().map(|d| d.q).max().unwrap_or(0) + 1;
    let r_max = dots.iter().map(|d| d.r).max().unwrap_or(0) + 1;
    let width = (q_max - q_min + 1) as usize;
    let height = (r_max - r_min + 1) as usize;
    let index = |q: i32, r: i32| (r - r_min) as usize * width + (q - q_min) as usize;
    let mut grid = vec![false; width * height];
    for d in dots {
        grid[index(d.q, d.r)] = true;
    }
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for d in dots {
        if !grid[index(d.q, d.r)] {
            continue;
        }
        grid[index(d.q, d.r)] = false;
        stack.push((d.q, d.r));
        let mut size = 0usize;
        while let Some((q, r)) = stack.pop() {
            size += 1;
            for (dq, dr) in NEIGHBOURS {
                let (nq, nr) = (q + dq, r + dr);
                if grid[index(nq, nr)] {
                    grid[index(nq, nr)] = false;
                    stack.push((nq, nr));
                }
            }
        }
        sizes.push(size);
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::ViewBox;
    use particlecode_core::shapes::ShapeTable;

    fn from_d(d: &str, stroke: Option<f32>) -> ShapeInput {
        ShapeInput::from_path_data(d, &ViewBox::DESIGN, stroke, "test").unwrap()
    }

    #[test]
    fn builtin_disc_passes_everything() {
        let input = ShapeInput::from_table(&ShapeTable::builtin(), 0).unwrap();
        let report = check(&input);
        assert!(report.passed(), "{report:#?}");
        let m = report.profile(Profile::M).unwrap();
        assert!((4200..4700).contains(&m.usable_dots), "{}", m.usable_dots);
        assert_eq!(m.islands.len(), 1);
        // The disc is 72 p wide.
        assert!(m.stroke_pitches > 30.0, "{}", m.stroke_pitches);
        assert!(m.bytes_normal > m.bytes_high);
    }

    fn from_table(table: &ShapeTable, id: u8) -> ShapeInput {
        ShapeInput::from_table(table, id).unwrap()
    }

    fn failing_rules(report: &Report) -> Vec<(Profile, u8)> {
        report
            .profiles
            .iter()
            .flat_map(|p| {
                p.checks
                    .iter()
                    .filter(|c| !c.passed)
                    .map(move |c| (p.profile, c.rule))
            })
            .collect()
    }

    #[test]
    fn some_builtins_pass() {
        let table = ShapeTable::builtin();
        for id in [1, 2, 16, 28, 29, 30] {
            let input = from_table(&table, id);
            assert!(check(&input).passed(), "{}", input.label);
        }
    }

    /// Every built-in against rules 2-4. Slow (about a third of a second per shape in a
    /// release build): `cargo test -p particlecode-shapes --release -- --ignored`.
    #[test]
    #[ignore = "slow; run with --release -- --ignored"]
    fn all_builtins_meet_the_rules() {
        let table = ShapeTable::builtin();
        for id in table.ids() {
            let input = from_table(&table, id);
            let failing = failing_rules(&check(&input));
            assert!(failing.is_empty(), "{}: {failing:?}", input.label);
        }
    }

    #[test]
    fn thin_stroke_is_flagged() {
        // A 20-unit-wide ring (about 1 p at M): lots of ink-free area and a hairline.
        let input = from_d(
            "M 100 500 A 400 400 0 1 1 900 500 A 400 400 0 1 1 100 500 Z",
            Some(20.0),
        );
        let report = check(&input);
        let m = report.profile(Profile::M).unwrap();
        let rule3 = m.checks.iter().find(|c| c.rule == 3).unwrap();
        assert!(!rule3.passed, "{rule3:?}");
        assert!(m.stroke_pitches < 2.0, "{}", m.stroke_pitches);
        assert!(!report.passed());
    }

    #[test]
    fn heavy_stroke_passes_where_thin_fails() {
        let d = "M 120 500 A 380 380 0 1 1 880 500 A 380 380 0 1 1 120 500 Z";
        let thick = check(&from_d(d, Some(180.0)));
        let m = thick.profile(Profile::M).unwrap();
        assert!(
            m.checks.iter().find(|c| c.rule == 3).unwrap().passed,
            "{m:#?}"
        );
        // 180 units = 13 p at M.
        assert!(
            (m.stroke_pitches - 13.0).abs() < 1.5,
            "{}",
            m.stroke_pitches
        );
    }

    #[test]
    fn tiny_island_is_flagged() {
        // A big square plus a detached 60-unit square far from it: a handful of dots.
        let d =
            "M 100 300 L 700 300 L 700 900 L 100 900 Z M 820 100 L 880 100 L 880 160 L 820 160 Z";
        let report = check(&from_d(d, None));
        let m = report.profile(Profile::M).unwrap();
        let rule4 = m.checks.iter().find(|c| c.rule == 4).unwrap();
        assert!(!rule4.passed, "{rule4:?}");
        assert_eq!(m.islands.len(), 2);
        assert!(m.islands[1] < MIN_ISLAND_DOTS);
    }

    #[test]
    fn island_sizes_use_hex_adjacency() {
        use particlecode_core::layout::axial_to_point;
        let pt = |q, r| LatticePoint {
            q,
            r,
            at: axial_to_point(q, r),
        };
        // (0,0)-(1,0)-(0,1) connected; (5,5) alone; (1,-1) touches (0,0) diagonally.
        let dots = [pt(0, 0), pt(1, 0), pt(0, 1), pt(5, 5), pt(1, -1)];
        assert_eq!(island_sizes(&dots), vec![4, 1]);
        // (1,1) is not adjacent to (0,0) on a hex lattice.
        assert_eq!(island_sizes(&[pt(0, 0), pt(1, 1)]), vec![1, 1]);
        assert!(island_sizes(&[]).is_empty());
    }

    #[test]
    fn low_coverage_is_flagged() {
        // A single 100 x 100 square: 1% of the box.
        let report = check(&from_d("M 450 450 L 550 450 L 550 550 L 450 550 Z", None));
        let rule2 = report.profile(Profile::M).unwrap().checks[0].clone();
        assert_eq!(rule2.rule, 2);
        assert!(!rule2.passed);
    }

    #[test]
    fn outside_box_is_noted() {
        let input = from_d("M -100 -100 L 1100 -100 L 1100 1100 L -100 1100 Z", None);
        assert!(!input.notes.is_empty());
    }
}
