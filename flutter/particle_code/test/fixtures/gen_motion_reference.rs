use particlecode_core::layout::{ParticleBand, Point};
use particlecode_core::motion::*;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() {
    let mut rng = Lcg(2024);
    let mut out = String::new();
    out.push_str("{\n");

    let ids: Vec<u16> = vec![0, 1, 7, 255, 4660, 40000, 65535];
    out.push_str("\"seeds\": [");
    out.push_str(
        &ids.iter()
            .map(|&i| format!("{{\"session\":{},\"seed\":\"{:016x}\"}}", i, seed_from_session(i)))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push_str("],\n");

    let seeds: Vec<u64> = ids
        .iter()
        .map(|&i| seed_from_session(i))
        .chain([0u64, 1, 0xFFFF_FFFF_FFFF_FFFF, 0x8000_0000_0000_0000, 123456789])
        .collect();

    let mut rows = vec![];
    for mood in [Mood::Breathing, Mood::FlowField] {
        let cfg = MotionConfig { mood, ..MotionConfig::all() };
        for n in 0..400 {
            let seed = seeds[n % seeds.len()];
            let r = [26.0, 38.0, 51.0, 0.5, 45.0][n % 5];
            let t = if n % 7 == 0 { 0.0 } else { rng.next() * 900.0 };
            let at = Point::new((rng.next() - 0.5) * 2.0 * r, (rng.next() - 0.5) * 2.0 * r);
            let o = dot_offset(&cfg, seed, t, at, r);
            let s = dot_radius_scale(&cfg, seed, t, at, r);
            rows.push(format!(
                "{{\"mood\":\"{:?}\",\"seed\":\"{:016x}\",\"t\":{:?},\"x\":{:?},\"y\":{:?},\"half\":{:?},\"ox\":{:?},\"oy\":{:?},\"scale\":{:?}}}",
                mood, seed, t, at.x, at.y, r, o.x, o.y, s
            ));
        }
    }
    out.push_str(&format!("\"dots\": [\n{}\n],\n", rows.join(",\n")));

    let mut rows = vec![];
    for n in 0..60 {
        let t = rng.next() * 30.0;
        let a = n % 4;
        for hb in [true, false] {
            let cfg = MotionConfig { anchor_heartbeat: hb, ..MotionConfig::default() };
            rows.push(format!(
                "{{\"heartbeat\":{},\"t\":{:?},\"index\":{},\"v\":{:?}}}",
                hb,
                t,
                a,
                anchor_scale(&cfg, t, a)
            ));
        }
    }
    out.push_str(&format!("\"anchor_scale\": [\n{}\n],\n", rows.join(",\n")));

    let mut rows = vec![];
    for t in [-1.0, 0.0, 0.01, 0.02, 0.03, 0.04, 0.5] {
        rows.push(format!("{{\"t\":{:?},\"v\":{:?}}}", t, crossfade_alpha(t)));
    }
    out.push_str(&format!("\"crossfade\": [{}],\n", rows.join(",")));

    let mut rows = vec![];
    for _ in 0..200 {
        let r = 38.0;
        let t = rng.next() * 0.8 - 0.05;
        let at = Point::new((rng.next() - 0.5) * 2.0 * r, (rng.next() - 0.5) * 2.0 * r);
        rows.push(format!(
            "{{\"t\":{:?},\"x\":{:?},\"y\":{:?},\"half\":{:?},\"reveal\":{:?},\"ripple\":{:?}}}",
            t,
            at.x,
            at.y,
            r,
            reveal_opacity(t, at, r),
            ripple_lighten(t, at, r)
        ));
    }
    out.push_str(&format!("\"reveal_ripple\": [\n{}\n],\n", rows.join(",\n")));

    let mut rows = vec![];
    for n in 0..300 {
        let seed = seeds[n % seeds.len()];
        let k = n % 6;
        let from = (rng.next() - 0.5) * 6.28;
        let to = (rng.next() - 0.5) * 6.28;
        let t = rng.next() * 500.0;
        let ts = if n % 3 == 0 { rng.next() * 0.08 } else { 1.0 };
        let inner = 30.0 + rng.next() * 10.0;
        let band = ParticleBand { inner, outer: inner + 4.5 };
        let (a, r) = particle_pose(seed, t, k, from, to, ts, &band);
        rows.push(format!(
            "{{\"seed\":\"{:016x}\",\"t\":{:?},\"k\":{},\"from\":{:?},\"to\":{:?},\"since\":{:?},\"inner\":{:?},\"outer\":{:?},\"angle\":{:?},\"radius\":{:?}}}",
            seed, t, k, from, to, ts, band.inner, band.outer, a, r
        ));
    }
    out.push_str(&format!("\"particles\": [\n{}\n]\n", rows.join(",\n")));
    out.push_str("}\n");
    print!("{}", out);
}
