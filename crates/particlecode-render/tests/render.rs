//! Pixel-level and packaging tests for the painter.

use std::io::Cursor;

use particlecode_core::frame::FrameSpec;
use particlecode_core::layout::{Point, dims};
use particlecode_core::motion::{self, Mood, MotionConfig};
use particlecode_core::profile::{Profile, Rgb, colors};
use particlecode_render::demo::demo_frame;
use particlecode_render::{
    GifOptions, Manifest, RenderConfig, encode_png, render, render_animated, render_svg, write_gif,
    write_gif_with, write_sequence,
};
use tiny_skia::Pixmap;

fn cfg() -> RenderConfig {
    RenderConfig {
        size_px: 800,
        ..RenderConfig::default()
    }
}

/// Pixel under the frame point `p`.
fn pixel(pm: &Pixmap, frame: &FrameSpec, cfg: &RenderConfig, p: Point) -> Rgb {
    let s = cfg.pitch_px(frame);
    let h = cfg.half_extent(frame);
    let x = ((p.x + h) * s).floor() as u32;
    let y = ((p.y + h) * s).floor() as u32;
    let c = pm.pixel(x, y).expect("inside the image").demultiply();
    Rgb(c.red(), c.green(), c.blue())
}

fn frames(n: u32) -> Vec<FrameSpec> {
    (0..n).map(|i| demo_frame(Profile::S, 42, i)).collect()
}

#[test]
fn flat_colours_are_exact() {
    let frame = demo_frame(Profile::M, 7, 0);
    let cfg = cfg();
    let pm = render(&frame, &cfg);
    assert_eq!((pm.width(), pm.height()), (800, 800));

    // Corner: quiet zone.
    assert_eq!(
        pm.pixel(0, 0).expect("pixel").demultiply().red(),
        colors::BACKGROUND.0
    );
    assert_eq!(
        pixel(
            &pm,
            &frame,
            &cfg,
            Point::new(-frame.frame_half_side, -frame.frame_half_side)
        ),
        colors::BACKGROUND
    );

    // Every data dot's centre has its exact palette colour.
    for d in frame.dots.iter().step_by(37) {
        assert_eq!(
            pixel(&pm, &frame, &cfg, d.at),
            colors::HUES4[usize::from(d.color)]
        );
    }
    for d in &frame.header_dots {
        assert_eq!(
            pixel(&pm, &frame, &cfg, d.at),
            colors::HUES4[usize::from(d.color)]
        );
    }

    // Anchor cores, ring and the background-coloured pupil of the top anchor.
    for a in &frame.anchors {
        let core_px = if a.pupil_r > 0.0 {
            Point::new(a.at.x + 0.5 * (a.core_r + a.pupil_r), a.at.y)
        } else {
            a.at
        };
        assert_eq!(pixel(&pm, &frame, &cfg, core_px), colors::ANCHOR);
        let ring_mid = Point::new(a.at.x + 0.5 * (a.ring_in + a.ring_out), a.at.y);
        assert_eq!(pixel(&pm, &frame, &cfg, ring_mid), colors::ANCHOR);
        let gap = Point::new(a.at.x + 0.5 * (a.core_r + a.ring_in), a.at.y);
        assert_eq!(
            pixel(&pm, &frame, &cfg, gap),
            colors::BACKGROUND,
            "gap between core and ring"
        );
    }
    assert_eq!(
        pixel(&pm, &frame, &cfg, frame.anchors[0].at),
        colors::BACKGROUND,
        "pupil"
    );

    // Swatches: hue, white, black.
    let white = frame
        .swatches
        .iter()
        .find(|s| s.slot == 0 && s.anchor == 0)
        .expect("white");
    let black = frame
        .swatches
        .iter()
        .find(|s| s.slot == 8 && s.anchor == 0)
        .expect("black");
    assert_eq!(pixel(&pm, &frame, &cfg, white.at), colors::WHITE);
    assert_eq!(pixel(&pm, &frame, &cfg, black.at), colors::BLACK);

    // Particles.
    let band = frame.particle_band;
    for p in &frame.particles {
        let at = Point::default().polar(band.mid(), p.angle);
        assert_eq!(pixel(&pm, &frame, &cfg, at), colors::PARTICLE);
    }
}

#[test]
fn size_and_background_follow_the_config() {
    let frame = demo_frame(Profile::S, 1, 0);
    let cfg = RenderConfig {
        size_px: 321,
        background: Rgb(1, 2, 3),
        margin: 0.0,
        fps: 10,
    };
    let pm = render(&frame, &cfg);
    assert_eq!(pm.width(), 321);
    // Margin is raised to the 1.5 p quiet zone.
    assert!((cfg.margin_pitches() - dims::QUIET_ZONE).abs() < f64::EPSILON);
    let c = pm.pixel(0, 0).expect("pixel").demultiply();
    assert_eq!((c.red(), c.green(), c.blue()), (1, 2, 3));
    // Out-of-range sizes are clamped, not panics.
    assert_eq!(
        render(
            &frame,
            &RenderConfig {
                size_px: 0,
                ..cfg.clone()
            }
        )
        .width(),
        1
    );
}

#[test]
fn png_decodes_to_the_same_pixels() {
    let frame = demo_frame(Profile::S, 3, 0);
    let cfg = RenderConfig {
        size_px: 300,
        ..RenderConfig::default()
    };
    let pm = render(&frame, &cfg);
    let bytes = encode_png(&pm).expect("encodes");
    let mut reader = png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .expect("valid png");
    let mut buf = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut buf).expect("frame");
    assert_eq!((info.width, info.height), (300, 300));
    for (i, p) in pm.pixels().iter().enumerate() {
        let c = p.demultiply();
        assert_eq!(&buf[i * 3..i * 3 + 3], &[c.red(), c.green(), c.blue()]);
    }
}

#[test]
fn svg_has_the_expected_circles() {
    let frame = demo_frame(Profile::S, 5, 0);
    let svg = render_svg(&frame, &cfg());
    let expected = frame.anchors.len() // cores
        + frame.anchors.iter().filter(|a| a.pupil_r > 0.0).count()
        + frame.swatches.len()
        + frame.header_dots.len()
        + frame.dots.len()
        + frame.particles.len();
    assert_eq!(svg.matches("<circle").count(), expected);
    assert_eq!(svg.matches("evenodd").count(), 4);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(svg.contains(&format!("fill=\"#{:02x}{:02x}{:02x}\"", 20, 110, 245)));
    assert!(!svg.contains("stroke"));
}

#[test]
fn gif_decodes_with_the_right_frame_count_and_exact_colours() {
    let fs = frames(3);
    let cfg = RenderConfig {
        size_px: 200,
        ..RenderConfig::default()
    };
    let bytes = write_gif(&fs, &cfg, &MotionConfig::default()).expect("encodes");
    assert_eq!(&bytes[..6], b"GIF89a");

    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(Cursor::new(bytes)).expect("valid gif");
    assert_eq!((dec.width(), dec.height()), (200, 200));
    let mut count = 0;
    let mut first_corner = None;
    while let Some(f) = dec.read_next_frame().expect("frame") {
        assert_eq!(f.delay, 10);
        if first_corner.is_none() {
            first_corner = Some([f.buffer[0], f.buffer[1], f.buffer[2]]);
        }
        count += 1;
    }
    assert_eq!(count, 3);
    assert_eq!(first_corner, Some([246, 244, 239]));
}

#[test]
fn gif_options_and_reveal_change_the_frame_count() {
    let fs = frames(4);
    let cfg = RenderConfig {
        size_px: 120,
        ..RenderConfig::default()
    };
    let count = |bytes: Vec<u8>| {
        let mut dec = gif::DecodeOptions::new()
            .read_info(Cursor::new(bytes))
            .expect("valid gif");
        let mut n = 0;
        let mut total_cs = 0u32;
        while let Some(f) = dec.read_next_frame().expect("frame") {
            n += 1;
            total_cs += u32::from(f.delay);
        }
        (n, total_cs)
    };
    let sub = GifOptions {
        subframes: 2,
        crossfade: false,
    };
    assert_eq!(
        count(write_gif_with(&fs, &cfg, &MotionConfig::default(), &sub).expect("ok")),
        (8, 40)
    );
    let fade = GifOptions {
        subframes: 1,
        crossfade: true,
    };
    assert_eq!(
        count(write_gif_with(&fs, &cfg, &MotionConfig::default(), &fade).expect("ok")),
        (7, 40)
    );
    let reveal = MotionConfig {
        reveal: true,
        ..MotionConfig::default()
    };
    // 6 reveal frames precede the 4 data frames.
    assert_eq!(count(write_gif(&fs, &cfg, &reveal).expect("ok")), (10, 100));
    assert!(write_gif(&[], &cfg, &reveal).is_err());
}

#[test]
fn sequence_writes_pngs_and_manifest() {
    let fs = frames(3);
    let cfg = RenderConfig {
        size_px: 100,
        fps: 8,
        ..RenderConfig::default()
    };
    let dir = std::env::temp_dir().join(format!("pc-seq-{}", std::process::id()));
    let manifest = Manifest::from_frames(&fs, &cfg).expect("manifest");
    write_sequence(&dir, &fs, &cfg, &manifest).expect("writes");
    for i in 0..3 {
        let bytes = std::fs::read(dir.join(format!("frame_{i:05}.png"))).expect("png");
        assert_eq!(&bytes[1..4], b"PNG");
    }
    let text = std::fs::read_to_string(dir.join("manifest.json")).expect("manifest");
    let back: Manifest = serde_json::from_str(&text).expect("parses");
    assert_eq!(back, manifest);
    assert_eq!(back.frames.len(), 3);
    assert_eq!(back.fps, 8);
    assert_eq!(back.frame_size_px, 100);
    assert_eq!(back.session_id, 0x5A17);
    assert_eq!(back.palette.len(), 9);
    assert_eq!(back.palette[0], "#146ef5");
    assert_eq!(back.frames[2].packet_seed, 2);

    // A manifest for the wrong number of frames is refused.
    let short = Manifest::from_frames(&fs[..2], &cfg).expect("manifest");
    assert!(write_sequence(&dir, &fs, &cfg, &short).is_err());
    let mut evil = manifest.clone();
    evil.frames[0].file = "../escape.png".into();
    assert!(write_sequence(&dir, &fs, &cfg, &evil).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn animation_is_deterministic_and_keeps_data_dots_readable() {
    let frame = demo_frame(Profile::M, 9, 0);
    let cfg = cfg();
    let mc = MotionConfig::default();
    let a = render_animated(&frame, None, &cfg, &mc, 77, 1.3, 1.0);
    let b = render_animated(&frame, None, &cfg, &mc, 77, 1.3, 1.0);
    assert_eq!(a.data(), b.data());
    let c = render_animated(&frame, None, &cfg, &mc, 77, 1.9, 1.0);
    assert_ne!(a.data(), c.data(), "the wave moves");

    // Every sampled dot centre (lattice point) is still its exact colour: drift is far
    // smaller than the radius, so the lattice point stays inside the dot.
    let r_half = frame.frame_half_side;
    for d in frame.dots.iter().step_by(23) {
        let want = colors::HUES4[usize::from(d.color)];
        let off = motion::dot_offset(&mc, 77, 1.3, d.at, r_half);
        assert!(off.norm() <= 0.12);
        assert_eq!(pixel(&a, &frame, &cfg, d.at), want);
    }
}

#[test]
fn crossfade_blends_changed_dots_and_finishes_on_the_new_frame() {
    let prev = demo_frame(Profile::S, 1, 0);
    let mut next = prev.clone();
    next.index = 1;
    // Make dot 0 differ in colour only.
    next.dots[0].color = (prev.dots[0].color + 1) % 4;
    next.dots[0].size = prev.dots[0].size;
    let cfg = cfg();
    let mc = MotionConfig::default();
    let at = next.dots[0].at;
    let at_switch = render_animated(&next, Some(&prev), &cfg, &mc, 1, 0.0, 0.0);
    let mid = render_animated(&next, Some(&prev), &cfg, &mc, 1, 0.0, 0.02);
    let done = render_animated(&next, Some(&prev), &cfg, &mc, 1, 0.0, 0.05);
    let (c0, c1) = (
        colors::HUES4[usize::from(prev.dots[0].color)],
        colors::HUES4[usize::from(next.dots[0].color)],
    );
    assert_eq!(pixel(&at_switch, &next, &cfg, at), c0);
    assert_eq!(pixel(&done, &next, &cfg, at), c1);
    let m = pixel(&mid, &next, &cfg, at);
    assert_ne!(m, c0);
    assert_ne!(m, c1);
    let shimmer = MotionConfig {
        shimmer: true,
        ..mc
    };
    let m2 = pixel(
        &render_animated(&next, Some(&prev), &cfg, &shimmer, 1, 0.0, 0.02),
        &next,
        &cfg,
        at,
    );
    assert_ne!(m2, m, "shimmer rotates hue instead of mixing in RGB");
}

#[test]
fn optional_layers_draw_something() {
    let frame = demo_frame(Profile::S, 2, 0);
    let cfg = RenderConfig {
        size_px: 400,
        ..RenderConfig::default()
    };
    let base = MotionConfig {
        mood: Mood::FlowField,
        ..MotionConfig::default()
    };
    let still = render_animated(&frame, None, &cfg, &base, 3, 1.5, 1.0);

    // Half a period in, anchor 0 is at the peak of its swell.
    let heart = MotionConfig {
        anchor_heartbeat: true,
        ..base
    };
    let beat = render_animated(&frame, None, &cfg, &heart, 3, 1.5, 1.0);
    assert_ne!(still.data(), beat.data());
    let swollen = motion::anchor_scale(&heart, 1.5, 0);
    assert!(swollen > 1.09);
    // The ring grows: a point just outside the resting ring becomes ink.
    let a = frame.anchors[0];
    let outside = Point::new(a.at.x + 0.5 * (a.ring_out + a.ring_out * swollen), a.at.y);
    assert_eq!(pixel(&still, &frame, &cfg, outside), colors::BACKGROUND);
    assert_eq!(pixel(&beat, &frame, &cfg, outside), colors::ANCHOR);

    let trails = MotionConfig {
        particle_trails: true,
        ..base
    };
    assert_ne!(
        still.data(),
        render_animated(&frame, None, &cfg, &trails, 3, 1.5, 1.0).data()
    );

    // Reveal: at t = 0 the dot field is invisible, but anchors are there.
    let reveal = MotionConfig {
        reveal: true,
        ..base
    };
    let hidden = render_animated(&frame, None, &cfg, &reveal, 3, 0.0, 1.0);
    let d = frame.dots[frame.dots.len() / 2];
    assert_eq!(pixel(&hidden, &frame, &cfg, d.at), colors::BACKGROUND);
    assert_eq!(
        pixel(&hidden, &frame, &cfg, frame.anchors[1].at),
        colors::ANCHOR
    );
    let shown = render_animated(&frame, None, &cfg, &reveal, 3, 0.7, 1.0);
    assert_eq!(
        pixel(&shown, &frame, &cfg, d.at),
        colors::HUES4[usize::from(d.color)]
    );

    // Ripple: a segment change lightens dots on the ring but keeps their hue family.
    let mut next = frame.clone();
    next.header.segment_index += 1;
    let ripple = MotionConfig {
        ripple: true,
        ..base
    };
    let ts = motion::RIPPLE_DURATION / 2.0;
    let ring_r = 0.525 * frame.frame_half_side;
    let on_ring = frame
        .dots
        .iter()
        .min_by(|a, b| {
            (a.at.norm() - ring_r)
                .abs()
                .total_cmp(&(b.at.norm() - ring_r).abs())
        })
        .expect("dots");
    let lit = render_animated(&next, Some(&frame), &cfg, &ripple, 3, 1.0, ts.max(0.05));
    let c = pixel(&lit, &next, &cfg, on_ring.at);
    let base_c = colors::HUES4[usize::from(on_ring.color)];
    assert!(
        u32::from(c.0) + u32::from(c.1) + u32::from(c.2)
            > u32::from(base_c.0) + u32::from(base_c.1) + u32::from(base_c.2)
    );
}

/// Writes sample images to `target/render-samples/` for eyeballing: static renders of
/// every profile, an animated frame with every layer on, an SVG and a camera-degraded
/// render. Run with
/// `cargo test -p particlecode-render --test render -- --ignored write_samples`.
#[test]
#[ignore = "writes sample images into target/ for visual inspection"]
fn write_samples() {
    use particlecode_render::synth::{CameraPose, Degradation, RgbImage, apply_all};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/render-samples");
    std::fs::create_dir_all(&dir).expect("dir");
    let save = |name: &str, pm: &Pixmap| {
        std::fs::write(dir.join(name), encode_png(pm).expect("png")).expect("write");
    };
    for profile in Profile::ALL {
        let frame = demo_frame(profile, 11, 0);
        save(
            &format!("static_{profile:?}.png"),
            &render(&frame, &RenderConfig::default()),
        );
    }
    let prev = demo_frame(Profile::M, 11, 0);
    let frame = demo_frame(Profile::M, 11, 1);
    let all = MotionConfig::all();
    let pm = render_animated(
        &frame,
        Some(&prev),
        &RenderConfig::default(),
        &all,
        5,
        3.3,
        0.5,
    );
    save("animated_M.png", &pm);
    std::fs::write(
        dir.join("static_M.svg"),
        render_svg(&prev, &RenderConfig::default()),
    )
    .expect("svg");

    let cfg = RenderConfig {
        size_px: 600,
        ..RenderConfig::default()
    };
    let img = RgbImage::from_pixmap(&render(&prev, &cfg));
    let chain = [
        Degradation::Tilt {
            pose: CameraPose {
                rotate_deg: 10.0,
                tilt_x_deg: 25.0,
                tilt_y_deg: -15.0,
                scale: 0.85,
            },
            out_w: 600,
            out_h: 600,
            background: Rgb(40, 44, 52),
        },
        Degradation::Blur { sigma: 1.2 },
        Degradation::Glare {
            center: (420.0, 170.0),
            radius: 70.0,
            strength: 0.6,
        },
        Degradation::Exposure {
            gain: 0.92,
            offset: -4.0,
        },
        Degradation::ColorCast {
            r: 1.06,
            g: 1.0,
            b: 0.9,
        },
        Degradation::Noise {
            sigma: 5.0,
            seed: 1,
        },
    ];
    std::fs::write(
        dir.join("degraded_M.png"),
        apply_all(&img, &chain).to_png().expect("png"),
    )
    .expect("write");
}
