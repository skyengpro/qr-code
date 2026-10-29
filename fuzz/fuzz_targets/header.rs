//! Feeds arbitrary (value, confidence) pairs into `Header::decode`; it must never panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use particlecode_core::{Header, Sym3};

fuzz_target!(|data: &[u8]| {
    // Two bytes per dot: raw value (deliberately not masked to 3 bits) and confidence.
    let symbols: Vec<Sym3> = data
        .chunks_exact(2)
        .take(64)
        .map(|c| Sym3 { value: c[0], confidence: c[1] })
        .collect();
    let _ = Header::decode(&symbols);

    // Exactly 48 dots exercises the Reed-Solomon path rather than the length check.
    if data.len() >= 96 {
        let exact: Vec<Sym3> =
            data.chunks_exact(2).take(48).map(|c| Sym3 { value: c[0], confidence: c[1] }).collect();
        let _ = Header::decode(&exact);
    }

    // Raw bytes as values at full confidence.
    let plain: Vec<Sym3> = data.iter().take(48).map(|&b| Sym3::new(b)).collect();
    let _ = Header::decode(&plain);
});
