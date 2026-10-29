//! Sample payloads for the examples and tests: small hand-built COSE_Sign1-style CBOR
//! documents. No CBOR library is needed; the encoder below covers the few item types used.
//! The signature bytes are pseudo-random, not a real signature.

use particlecode_render::synth::Rng;

/// Append a CBOR initial byte plus argument for `major` (0..=7) and value `n`.
fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let m = major << 5;
    if n < 24 {
        out.push(m | n as u8);
    } else if n <= 0xFF {
        out.extend([m | 0x18, n as u8]);
    } else if n <= 0xFFFF {
        out.push(m | 0x19);
        out.extend((n as u16).to_be_bytes());
    } else if n <= 0xFFFF_FFFF {
        out.push(m | 0x1A);
        out.extend((n as u32).to_be_bytes());
    } else {
        out.push(m | 0x1B);
        out.extend(n.to_be_bytes());
    }
}

fn uint(out: &mut Vec<u8>, n: u64) {
    head(out, 0, n);
}

fn text(out: &mut Vec<u8>, s: &str) {
    head(out, 3, s.len() as u64);
    out.extend(s.as_bytes());
}

fn bytes(out: &mut Vec<u8>, b: &[u8]) {
    head(out, 2, b.len() as u64);
    out.extend(b);
}

fn array(out: &mut Vec<u8>, len: u64) {
    head(out, 4, len);
}

fn map(out: &mut Vec<u8>, len: u64) {
    head(out, 5, len);
}

/// Deterministic pseudo-random bytes.
fn noise(rng: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| (rng.next_u64() >> 24) as u8).collect()
}

/// Wrap `claims` (a CBOR document) in a `COSE_Sign1` envelope: tag 18, protected header
/// `{1: -7}` (ES256), empty unprotected header, the claims as payload and a 64-byte
/// stand-in signature.
fn cose_sign1(claims: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = vec![0xD2]; // tag(18)
    array(&mut out, 4);
    bytes(&mut out, &[0xA1, 0x01, 0x26]);
    map(&mut out, 0);
    bytes(&mut out, claims);
    bytes(&mut out, &noise(rng, 64));
    out
}

/// A small event ticket (under 200 bytes).
pub fn ticket() -> Vec<u8> {
    let mut rng = Rng::new(0x71C4);
    let mut claims = Vec::new();
    map(&mut claims, 8);
    text(&mut claims, "v");
    uint(&mut claims, 1);
    text(&mut claims, "typ");
    text(&mut claims, "ticket");
    text(&mut claims, "iss");
    text(&mut claims, "https://tickets.example.org");
    text(&mut claims, "sub");
    text(&mut claims, "Ada Lovelace");
    text(&mut claims, "evt");
    text(&mut claims, "Particle Code Meetup");
    text(&mut claims, "seat");
    text(&mut claims, "B-17");
    text(&mut claims, "iat");
    uint(&mut claims, 1_780_000_000);
    text(&mut claims, "exp");
    uint(&mut claims, 1_780_086_400);
    cose_sign1(&claims, &mut rng)
}

/// A credential-sized document of at least `target_len` bytes: a signed claims map with a
/// list of attachments (names, media types, SHA-256-sized digests and a little data).
pub fn credential(target_len: usize) -> Vec<u8> {
    let mut records: u64 = 0;
    loop {
        let mut claims = Vec::new();
        map(&mut claims, 4);
        text(&mut claims, "v");
        uint(&mut claims, 1);
        text(&mut claims, "typ");
        text(&mut claims, "credential");
        text(&mut claims, "sub");
        text(&mut claims, "did:example:123456789abcdefghi");
        text(&mut claims, "attachments");
        array(&mut claims, records);
        let mut rec_rng = Rng::new(0xA77A);
        for i in 0..records {
            map(&mut claims, 4);
            text(&mut claims, "name");
            text(&mut claims, &format!("document-{i:03}.pdf"));
            text(&mut claims, "type");
            text(&mut claims, "application/pdf");
            text(&mut claims, "sha256");
            bytes(&mut claims, &noise(&mut rec_rng, 32));
            text(&mut claims, "note");
            bytes(&mut claims, &noise(&mut rec_rng, 48));
        }
        let doc = cose_sign1(&claims, &mut Rng::new(0xC4ED));
        if doc.len() >= target_len {
            return doc;
        }
        records += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_encoding_matches_rfc_8949() {
        let enc = |major, n| {
            let mut v = Vec::new();
            head(&mut v, major, n);
            v
        };
        assert_eq!(enc(0, 10), [0x0A]);
        assert_eq!(enc(0, 24), [0x18, 0x18]);
        assert_eq!(enc(0, 500), [0x19, 0x01, 0xF4]);
        assert_eq!(enc(0, 1_000_000), [0x1A, 0x00, 0x0F, 0x42, 0x40]);
        assert_eq!(enc(0, 1 << 40)[0], 0x1B);
        assert_eq!(enc(3, 4), [0x64]);
    }

    #[test]
    fn documents_are_deterministic_and_sized() {
        assert_eq!(ticket(), ticket());
        let t = ticket();
        assert!((150..400).contains(&t.len()), "{}", t.len());
        assert_eq!(t[0], 0xD2, "COSE_Sign1 tag");
        let c = credential(6000);
        assert!((6000..6200).contains(&c.len()), "{}", c.len());
        assert_eq!(c, credential(6000));
    }

    #[test]
    fn committed_sample_files_match_the_generators() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/data");
        if std::env::var_os("PARTICLECODE_WRITE_SAMPLES").is_some() {
            // Regenerate with: PARTICLECODE_WRITE_SAMPLES=1 cargo test -p particlecode-cli sample
            std::fs::create_dir_all(&dir).expect("create examples/data");
            std::fs::write(dir.join("ticket.cbor"), ticket()).expect("write ticket");
            std::fs::write(dir.join("credential.cbor"), credential(6000))
                .expect("write credential");
        }
        let read = |name: &str| std::fs::read(dir.join(name)).expect("sample file is committed");
        assert_eq!(read("ticket.cbor"), ticket());
        assert_eq!(read("credential.cbor"), credential(6000));
    }
}
