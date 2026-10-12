//! Seal+open throughput of ChaCha20-Poly1305 for WireGuard-sized packets (1280-byte inner MTU +
//! 32-byte overhead): RustCrypto `chacha20poly1305` (what ts_tunnel uses) vs `ring`.
use std::time::Instant;

const PKT: usize = 1312;
const N: usize = 400_000;

fn main() {
    let key = [7u8; 32];
    let mut buf = vec![0u8; PKT];

    // RustCrypto
    {
        use chacha20poly1305::{aead::AeadInPlace, ChaCha20Poly1305, KeyInit, Nonce};
        let c = ChaCha20Poly1305::new(&key.into());
        let t = Instant::now();
        for i in 0..N {
            let mut n = [0u8; 12];
            n[4..].copy_from_slice(&(i as u64).to_le_bytes());
            let tag = c.encrypt_in_place_detached(Nonce::from_slice(&n), &[], &mut buf).unwrap();
            c.decrypt_in_place_detached(Nonce::from_slice(&n), &[], &mut buf, &tag).unwrap();
        }
        let s = t.elapsed().as_secs_f64();
        println!("rustcrypto: {:.0} ns/packet (seal+open), {:.2} Gbit/s", s / N as f64 * 1e9, (N * PKT * 8) as f64 / s / 1e9);
    }
    // ring
    {
        use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
        let k = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &key).unwrap());
        let t = Instant::now();
        for i in 0..N {
            let mut n = [0u8; 12];
            n[4..].copy_from_slice(&(i as u64).to_le_bytes());
            let tag = k.seal_in_place_separate_tag(Nonce::assume_unique_for_key(n), Aad::empty(), &mut buf).unwrap();
            let mut ct = buf.clone();
            ct.extend_from_slice(tag.as_ref());
            k.open_in_place(Nonce::assume_unique_for_key(n), Aad::empty(), &mut ct).unwrap();
        }
        let s = t.elapsed().as_secs_f64();
        println!("ring:       {:.0} ns/packet (seal+open, incl. one 1.3 KB copy), {:.2} Gbit/s", s / N as f64 * 1e9, (N * PKT * 8) as f64 / s / 1e9);
    }
}
