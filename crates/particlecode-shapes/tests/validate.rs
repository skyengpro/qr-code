//! Rule 7 end to end: synthetic camera frames of a shape and of the disc through the vision
//! decoder. Small frame counts keep debug runs short; the 200-frame runs are `#[ignore]`d.

use std::fmt::Write as _;

use particlecode_core::profile::Profile;
use particlecode_core::shapes::ShapeTable;
use particlecode_shapes::ShapeInput;
use particlecode_shapes::build::add_shape;
use particlecode_shapes::path::ViewBox;
use particlecode_shapes::validate::{Comparison, ValidateConfig, compare_with_disc};

const STAR_SVG: &str = include_str!("../assets/star.svg");
const HEART: u8 = 28;

fn config(frames: usize) -> ValidateConfig {
    ValidateConfig {
        profile: Profile::M,
        frames,
        ..ValidateConfig::default()
    }
}

fn run(table: &ShapeTable, id: u8, cfg: &ValidateConfig) -> Comparison {
    let order = table.dot_order(id, cfg.profile).expect("shape in table");
    compare_with_disc(&order, id, table, cfg, &|_, _, _| {}).expect("validation runs")
}

fn table_with_star() -> ShapeTable {
    let mut table = ShapeTable::builtin();
    let star = ShapeInput::from_svg(STAR_SVG, None, "star.svg").unwrap();
    add_shape(&mut table, 32, "star", &star, false).unwrap();
    table
}

/// An Archimedean spiral of thin line: four turns, 30 units wide (about 2 p at M). It fails
/// rules 2 and 3 and is only accepted with `force`.
fn spiral_table() -> ShapeTable {
    let mut d = String::new();
    let steps = 400;
    for i in 0..=steps {
        let t = f64::from(i) / f64::from(steps);
        let angle = t * 4.0 * std::f64::consts::TAU;
        let r = 40.0 + 420.0 * t;
        let (x, y) = (500.0 + r * angle.cos(), 500.0 + r * angle.sin());
        let cmd = if i == 0 { "M" } else { "L" };
        write!(d, "{cmd} {x:.1} {y:.1} ").unwrap();
    }
    let input = ShapeInput::from_path_data(&d, &ViewBox::DESIGN, Some(30.0), "spiral").unwrap();
    let mut table = ShapeTable::builtin();
    assert!(
        add_shape(&mut table, 40, "spiral", &input, false).is_err(),
        "the spiral must fail the rules"
    );
    add_shape(&mut table, 40, "spiral", &input, true).unwrap();
    table
}

#[test]
fn heart_and_star_pass_rule_7() {
    let table = table_with_star();
    for id in [HEART, 32] {
        let c = run(&table, id, &config(12));
        assert!(
            c.verdict.passed,
            "{} fails: {c:#?}",
            table.name(id).unwrap_or("?")
        );
        assert_eq!(c.shape.frames, 12);
        assert!(c.shape.lock_rate() > 0.9, "{c:#?}");
        // The same cameras decode the disc, so the comparison means something.
        assert!(c.disc.fully_decoded >= 9, "{c:#?}");
    }
}

#[test]
fn thin_spiral_is_measured_against_the_disc() {
    let table = spiral_table();
    let c = run(&table, 40, &config(12));
    assert_eq!(c.shape.frames, 12);
    assert_eq!(c.disc.frames, 12);
    assert!(c.shape.read > 0 && c.disc.read > 0, "{c:#?}");
    assert!((c.verdict.ratio - c.shape.ser() / c.disc.ser()).abs() < 1e-9);
}

/// The numbers quoted in the tool's documentation: 200 frames of the heart and the star.
/// Slow in debug builds: `cargo test -p particlecode-shapes --release -- --ignored`.
#[test]
#[ignore = "200 frames per shape; run with --release -- --ignored"]
fn heart_and_star_pass_200_frames() {
    let table = table_with_star();
    for id in [HEART, 32] {
        let c = run(&table, id, &config(200));
        assert!(c.verdict.passed, "{id}: {c:#?}");
    }
}
