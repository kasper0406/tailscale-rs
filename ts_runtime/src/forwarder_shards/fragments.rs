//! Bounded pre-dispatch IPv4 reassembly. No first-fragment association or unbounded ID cache.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use ts_packet::PacketMut;
use ts_transport::PeerId;
const MAX_BYTES: usize = 1 << 20;
const MAX_DATAGRAMS: usize = 128;
const MAX_PIECES: usize = 64;
const TIMEOUT: Duration = Duration::from_secs(30);
#[derive(Hash, Eq, PartialEq)]
struct Key(PeerId, [u8; 4], [u8; 4], u8, u16);
struct Datagram {
    created: Instant,
    head: Option<Vec<u8>>,
    pieces: Vec<(usize, Vec<u8>)>,
    end: Option<usize>,
    bytes: usize,
}
#[derive(Default)]
pub(super) struct Reassembly {
    entries: HashMap<Key, Datagram>,
    bytes: usize,
}
impl Reassembly {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub(super) fn push(&mut self, peer: PeerId, packet: PacketMut) -> Option<PacketMut> {
        let now = Instant::now();
        self.entries.retain(|_, d| {
            if now.duration_since(d.created) >= TIMEOUT {
                self.bytes -= d.bytes;
                false
            } else {
                true
            }
        });
        let b = packet.as_ref();
        super::ipv4(b)?;
        let ihl = usize::from(b[0] & 15) * 4;
        let total = usize::from(u16::from_be_bytes([b[2], b[3]]));
        if total < ihl || total > b.len() {
            return None;
        }
        let flags = u16::from_be_bytes([b[6], b[7]]);
        let start = usize::from(flags & 0x1fff) * 8;
        let more = flags & 0x2000 != 0;
        let payload = &b[ihl..total];
        let end = start.checked_add(payload.len())?;
        let key = Key(
            peer,
            b[12..16].try_into().ok()?,
            b[16..20].try_into().ok()?,
            b[9],
            u16::from_be_bytes([b[4], b[5]]),
        );
        if payload.is_empty()
            || (more && payload.len() % 8 != 0)
            || end > 65515
            || flags & 0xc000 != 0
        {
            self.remove(&key);
            return None;
        }
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_DATAGRAMS {
            return None;
        }
        let added = payload.len() + if start == 0 { ihl } else { 0 };
        if self.bytes + added > MAX_BYTES {
            self.remove(&key);
            return None;
        }
        let d = self.entries.entry(key).or_insert_with(|| Datagram {
            created: now,
            head: None,
            pieces: Vec::new(),
            end: None,
            bytes: 0,
        });
        // Drop the entire datagram on any overlap, duplicate or inconsistent tail.
        if d.pieces.len() >= MAX_PIECES
            || d.pieces
                .iter()
                .any(|(s, p)| start < s + p.len() && *s < end)
            || d.end
                .is_some_and(|last| end > last || (!more && last != end))
            || (!more && d.pieces.iter().any(|(s, p)| s + p.len() > end))
        {
            // Entry is removed below via its reconstructed key, keeping accounting exact.
            let key = Key(
                peer,
                b[12..16].try_into().ok()?,
                b[16..20].try_into().ok()?,
                b[9],
                u16::from_be_bytes([b[4], b[5]]),
            );
            self.remove(&key);
            return None;
        }
        if start == 0 {
            d.head = Some(b[..ihl].to_vec());
        }
        if !more {
            d.end = Some(end);
        }
        d.pieces.push((start, payload.to_vec()));
        d.bytes += added;
        self.bytes += added;
        let (Some(head), Some(end)) = (&d.head, d.end) else {
            return None;
        };
        d.pieces.sort_unstable_by_key(|p| p.0);
        let mut next = 0;
        for (start, p) in &d.pieces {
            if *start != next {
                return None;
            }
            next += p.len();
        }
        if next != end || head.len() + end > 65535 {
            return None;
        }
        let header_len = head.len();
        let mut assembled = head.clone();
        for (_, p) in &d.pieces {
            assembled.extend_from_slice(p);
        }
        let total = (assembled.len() as u16).to_be_bytes();
        assembled[2..4].copy_from_slice(&total);
        assembled[6..8].fill(0);
        assembled[10..12].fill(0);
        let mut sum: u32 = assembled[..header_len]
            .chunks_exact(2)
            .map(|p| u32::from(u16::from_be_bytes([p[0], p[1]])))
            .sum();
        while sum >> 16 != 0 {
            sum = (sum & 65535) + (sum >> 16);
        }
        assembled[10..12].copy_from_slice(&(!(sum as u16)).to_be_bytes());
        let key = Key(
            peer,
            b[12..16].try_into().ok()?,
            b[16..20].try_into().ok()?,
            b[9],
            u16::from_be_bytes([b[4], b[5]]),
        );
        self.remove(&key);
        Some(assembled.into())
    }
    fn remove(&mut self, key: &Key) {
        if let Some(d) = self.entries.remove(key) {
            self.bytes -= d.bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fragment(peer_id: u16, start: usize, more: bool, payload: &[u8]) -> PacketMut {
        let mut b = vec![0; 20];
        b[0] = 0x45;
        b[8] = 64;
        b[9] = 17;
        b[4..6].copy_from_slice(&peer_id.to_be_bytes());
        b[6..8]
            .copy_from_slice(&((start as u16 / 8) | if more { 0x2000 } else { 0 }).to_be_bytes());
        b[12..16].copy_from_slice(&[100, 64, 0, 1]);
        b[16..20].copy_from_slice(&[8, 8, 8, 8]);
        b.extend_from_slice(payload);
        let len = b.len() as u16;
        b[2..4].copy_from_slice(&len.to_be_bytes());
        b.into()
    }
    #[test]
    fn all_fragments_share_reassembly_then_canonical_owner_in_either_arrival_order() {
        let selector = crate::forwarder_shards::Selector::new(8);
        let mut payload = vec![0; 64];
        payload[..4].copy_from_slice(&[0x12, 0x34, 0, 53]);
        for later_first in [false, true] {
            let mut r = Reassembly::default();
            let mut pieces = vec![
                fragment(1, 0, true, &payload[..32]),
                fragment(1, 32, false, &payload[32..]),
            ];
            if later_first {
                pieces.reverse();
            }
            assert!(r.push(PeerId(1), pieces.remove(0)).is_none());
            let full = r.push(PeerId(1), pieces.remove(0)).unwrap();
            assert_eq!(&full.as_ref()[20..], &payload);
            assert_eq!(
                selector.owner(PeerId(1), full.as_ref()),
                selector.owner(PeerId(1), fragment(1, 0, false, &payload).as_ref())
            );
            assert_eq!(r.bytes, 0);
            assert!(r.entries.is_empty());
        }
    }
    #[test]
    fn peer_and_datagram_id_namespace_fragments() {
        let mut r = Reassembly::default();
        assert!(r.push(PeerId(1), fragment(1, 0, true, &[1; 32])).is_none());
        assert!(r.push(PeerId(2), fragment(1, 32, false, &[2; 8])).is_none());
        assert!(r.push(PeerId(1), fragment(2, 32, false, &[3; 8])).is_none());
        let full = r.push(PeerId(1), fragment(1, 32, false, &[4; 8])).unwrap();
        assert_eq!(&full.as_ref()[20..52], &[1; 32]);
        assert_eq!(r.entries.len(), 2);
    }
    #[test]
    fn overlaps_duplicates_and_conflicting_tails_drop_datagram() {
        for overlap in [0, 8, 24] {
            let mut r = Reassembly::default();
            r.push(PeerId(1), fragment(1, 0, true, &[1; 32]));
            assert!(
                r.push(PeerId(1), fragment(1, overlap, false, &[2; 16]))
                    .is_none()
            );
            assert_eq!(r.bytes, 0);
            assert!(r.entries.is_empty());
        }
        let mut r = Reassembly::default();
        r.push(PeerId(1), fragment(1, 32, false, &[1; 8]));
        assert!(r.push(PeerId(1), fragment(1, 48, false, &[1; 8])).is_none());
        assert_eq!(r.bytes, 0);
    }
    #[test]
    fn memory_entry_piece_and_time_limits_are_bounded() {
        let mut r = Reassembly::default();
        for id in 0..=MAX_DATAGRAMS as u16 {
            r.push(PeerId(1), fragment(id, 0, true, &[0; 8]));
        }
        assert_eq!(r.entries.len(), MAX_DATAGRAMS);
        for d in r.entries.values_mut() {
            d.created = Instant::now() - TIMEOUT;
        }
        r.push(PeerId(1), fragment(200, 0, true, &[0; 8]));
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.bytes, 28);
        r.clear();
        for id in 0..128 {
            r.push(PeerId(1), fragment(id, 0, true, &[0; 16384]));
            assert!(r.bytes <= MAX_BYTES);
        }
        r.clear();
        for i in 0..=MAX_PIECES {
            r.push(PeerId(1), fragment(1, i * 8, true, &[0; 8]));
        }
        assert_eq!(r.bytes, 0);
        assert!(r.entries.is_empty());
    }
    #[test]
    fn invalid_sizes_and_nonaligned_fragments_are_refused() {
        let mut r = Reassembly::default();
        assert!(r.push(PeerId(1), fragment(1, 0, true, &[0; 7])).is_none());
        assert!(
            r.push(PeerId(1), fragment(1, 65512, false, &[0; 8]))
                .is_none()
        );
        assert!(r.entries.is_empty());
    }
}
