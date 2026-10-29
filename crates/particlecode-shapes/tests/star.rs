//! The example shape `assets/star.svg` end to end: SVG, rule report, `shapes.bin` round
//! trip, preview.

use particlecode_core::profile::Profile;
use particlecode_core::shapes::ShapeTable;
use particlecode_shapes::build::add_shape;
use particlecode_shapes::preview::{PreviewOptions, preview_png};
use particlecode_shapes::report::{to_json, to_text};
use particlecode_shapes::rules::{MIN_COVERAGE, MIN_ISLAND_DOTS, MIN_STROKE_PITCHES};
use particlecode_shapes::{ShapeInput, check};

const STAR_SVG: &str = include_str!("../assets/star.svg");

fn star() -> ShapeInput {
    ShapeInput::from_svg(STAR_SVG, None, "star.svg").expect("star.svg parses")
}

#[test]
fn star_passes_every_rule_on_every_profile() {
    let report = check(&star());
    assert!(report.passed(), "{}", to_text(&report));
    for p in &report.profiles {
        assert!(
            p.coverage >= MIN_COVERAGE,
            "{:?}: {}",
            p.profile,
            p.coverage
        );
        assert!(
            p.stroke_pitches >= MIN_STROKE_PITCHES,
            "{:?}: {}",
            p.profile,
            p.stroke_pitches
        );
        assert_eq!(p.islands.len(), 1, "{:?}: {:?}", p.profile, p.islands);
        assert!(p.islands[0] >= MIN_ISLAND_DOTS);
        assert!(p.bytes_normal > p.bytes_high);
    }
    // Capacity is in the heart-to-letter range of the spec's table at M.
    let m = report.profile(Profile::M).unwrap();
    assert!((1800..3300).contains(&m.usable_dots), "{}", m.usable_dots);
    assert!(to_json(&report).contains("\"passed\":true"));
}

#[test]
fn star_survives_a_shapes_bin_round_trip() {
    let input = star();
    let mut table = ShapeTable::builtin();
    add_shape(&mut table, 32, "star", &input, false).expect("star is accepted");
    table.set_version(1).unwrap();

    let bytes = table.to_bytes();
    let back = ShapeTable::from_bytes(&bytes).expect("shapes.bin parses");
    assert_eq!(back, table);
    assert_eq!(back.version(), 1);
    assert_eq!(back.name(32), Some("star"));
    for (profile, mask) in Profile::ALL.into_iter().zip(&input.masks) {
        assert_eq!(&back.mask(32, profile).unwrap(), mask, "{profile:?}");
    }

    // The table-based check agrees with the SVG-based one on the rules that only look at
    // the masks (ink, islands, capacity).
    let svg_report = check(&input);
    let table_report = check(&ShapeInput::from_table(&back, 32).unwrap());
    assert!(table_report.passed(), "{}", to_text(&table_report));
    for (a, b) in svg_report.profiles.iter().zip(&table_report.profiles) {
        assert_eq!(
            (a.ink_dots, a.usable_dots, &a.islands, a.bytes_normal),
            (b.ink_dots, b.usable_dots, &b.islands, b.bytes_normal)
        );
    }
}

#[test]
fn star_previews_as_a_png() {
    let mut table = ShapeTable::builtin();
    add_shape(&mut table, 32, "star", &star(), false).unwrap();
    let order = table.dot_order(32, Profile::S).unwrap();
    let opts = PreviewOptions {
        profile: Profile::S,
        size_px: 240,
        seed: 5,
    };
    let png = preview_png(&order, 32, table.version(), &opts).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}
