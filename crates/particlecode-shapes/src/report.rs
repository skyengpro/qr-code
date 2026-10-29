//! Text and JSON rendering of a [`Report`].

use std::fmt::Write as _;

use particlecode_core::profile::Profile;

use crate::rules::{MIN_COVERAGE, MIN_ISLAND_DOTS, MIN_STROKE_PITCHES, Report};

/// One-letter name of a profile.
pub fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::S => "S",
        Profile::M => "M",
        Profile::L => "L",
    }
}

/// Human-readable report: one block per profile, `ok` / `FAIL` per rule, then the verdict.
pub fn to_text(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "shape: {}", report.label);
    for note in &report.notes {
        let _ = writeln!(out, "  note: {note}");
    }
    for p in &report.profiles {
        let _ = writeln!(
            out,
            "profile {} ({} dots across, {} lattice points)",
            profile_name(p.profile),
            p.profile.dots_across(),
            p.lattice_points
        );
        let _ = writeln!(
            out,
            "  usable dots   {} ({} of the ink removed by anchor keep-outs, rule 5)",
            p.usable_dots, p.keepout_dots
        );
        let _ = writeln!(
            out,
            "  bytes/frame   {} at Normal ECC, {} at High ECC",
            p.bytes_normal, p.bytes_high
        );
        for c in &p.checks {
            let _ = writeln!(
                out,
                "  {}  rule {} {}: {}",
                if c.passed { "ok  " } else { "FAIL" },
                c.rule,
                c.name,
                c.detail
            );
        }
        let parts = p.islands.len();
        let _ = writeln!(
            out,
            "  info  rule 6: {}",
            match parts {
                0 => "no ink".to_string(),
                1 => "connected".to_string(),
                n => format!(
                    "{n} parts (rules 2-4 hold for each part: {})",
                    parts_hold(p)
                ),
            }
        );
    }
    let _ = writeln!(
        out,
        "rules 2-6: {}",
        if report.passed() { "PASS" } else { "FAIL" }
    );
    let _ = writeln!(
        out,
        "rule 7 (synthetic validation against the disc): not run, needs the vision decoder"
    );
    out
}

fn parts_hold(p: &crate::rules::ProfileReport) -> &'static str {
    if p.passed() { "yes" } else { "no" }
}

/// JSON report (hand-written, stable key order).
pub fn to_json(report: &Report) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "{{\"shape\":{},\"passed\":{},\"limits\":{{\"min_coverage\":{MIN_COVERAGE},\
         \"min_stroke_pitches\":{MIN_STROKE_PITCHES},\"min_island_dots\":{MIN_ISLAND_DOTS}}},\
         \"notes\":[{}],\"profiles\":[",
        json_str(&report.label),
        report.passed(),
        report
            .notes
            .iter()
            .map(|n| json_str(n))
            .collect::<Vec<_>>()
            .join(",")
    );
    for (i, p) in report.profiles.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"profile\":\"{}\",\"passed\":{},\"lattice_points\":{},\"ink_dots\":{},\
             \"coverage\":{:.4},\"usable_dots\":{},\"keepout_dots\":{},\
             \"stroke_pitches\":{:.3},\"islands\":[{}],\
             \"bytes_normal\":{},\"bytes_high\":{},\"checks\":[",
            profile_name(p.profile),
            p.passed(),
            p.lattice_points,
            p.ink_dots,
            p.coverage,
            p.usable_dots,
            p.keepout_dots,
            p.stroke_pitches,
            p.islands
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            p.bytes_normal,
            p.bytes_high
        );
        for (j, c) in p.checks.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"rule\":{},\"name\":{},\"passed\":{},\"detail\":{}}}",
                c.rule,
                json_str(c.name),
                c.passed,
                json_str(&c.detail)
            );
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

/// A JSON string literal.
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::ViewBox;
    use crate::raster::ShapeInput;
    use crate::rules::check;

    fn sample() -> Report {
        let input = ShapeInput::from_path_data(
            "M 100 100 L 900 100 L 900 900 L 100 900 Z",
            &ViewBox::DESIGN,
            None,
            "square \"x\"",
        )
        .unwrap();
        check(&input)
    }

    #[test]
    fn text_names_every_rule_and_profile() {
        let text = to_text(&sample());
        for needle in [
            "profile S",
            "profile M",
            "profile L",
            "rule 2",
            "rule 3",
            "rule 4",
        ] {
            assert!(text.contains(needle), "{needle} missing in\n{text}");
        }
        assert!(text.contains("rules 2-6: PASS"), "{text}");
    }

    #[test]
    fn json_is_balanced_and_escaped() {
        let json = to_json(&sample());
        assert!(
            json.starts_with("{\"shape\":\"square \\\"x\\\"\""),
            "{json}"
        );
        let opens = json.matches(['{', '[']).count();
        let closes = json.matches(['}', ']']).count();
        assert_eq!(opens, closes);
        assert!(json.contains("\"passed\":true"));
        assert_eq!(json.matches("\"profile\":").count(), 3);
    }
}
