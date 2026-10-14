// Port of review26b-copy: same PRNG, seeds, hostile inputs, 400 steps and drain bound.
// Keep the unused pacing draw so disabling pacing does not change the input stream.
#[cfg(all(feature = "socket-tcp-reno", feature = "socket-tcp-cubic"))]
#[test]
#[ignore = "long-running randomized review-26 harness; configure R_SEED/R_ROUNDS"]
fn review26b_random_peer_inputs() {
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 16) as u32
        }
    }
    static SEGS: AtomicU64 = AtomicU64::new(0);
    static ACKS: AtomicU64 = AtomicU64::new(0);
    let env = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let rounds = env("R_ROUNDS", 200);
    let seed = env("R_SEED", 1);
    let steps = env("R_STEPS", 400);
    let mut wedges = 0u64;
    let mut worst_spin = 0u32;
    let mut panics = std::vec::Vec::new();
    let hook = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));
    for round in 0..rounds {
        let mut r = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (round + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
            | 1);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> (bool, u32) {
            let mut s = socket_established_with_buffer_sizes(65536, 64);
            s.set_congestion_control(if r.next() % 3 == 0 {
                CongestionControl::Reno
            } else {
                CongestionControl::Cubic
            });
            let _pacing_draw = r.next() % 4 != 0;
            s.set_sender_sack_enabled(true);
            s.set_min_rto(Duration::from_millis(200));
            s.set_nagle_enabled(r.next() % 2 == 0);
            s.remote_has_sack = r.next() % 5 != 0;
            let mss = [1usize, 6, 536, 1200, 1460, 9000, 65535][(r.next() % 7) as usize];
            s.remote_mss = mss;
            s.congestion_controller.inner_mut().set_mss(mss);
            s.remote_win_len = 65535;
            let mut now = Instant::from_millis(0);
            let mut spin = 0u32;
            let mut worst = 0u32;
            let inv = |s: &TestSocket| {
                assert!(
                    s.remote_last_seq >= s.local_seq_no,
                    "invariant snd_nxt >= snd_una"
                )
            };
            let peer =
                |s: &mut TestSocket, ack: TcpSeqNumber, sack: [Option<(u32, u32)>; 3], win: u16| {
                    let repr = TcpRepr {
                        seq_number: REMOTE_SEQ + 1,
                        ack_number: Some(ack),
                        window_len: win,
                        sack_ranges: sack,
                        ..SEND_TEMPL
                    };
                    let ip_repr = IpReprIpvX(IpvXRepr {
                        src_addr: REMOTE_ADDR,
                        dst_addr: LOCAL_ADDR,
                        next_header: IpProtocol::Tcp,
                        payload_len: repr.buffer_len(),
                        hop_limit: 64,
                    });
                    if s.socket.accepts(&mut s.cx, &ip_repr, &repr) {
                        let _ = s.socket.process(&mut s.cx, &ip_repr, &repr);
                        ACKS.fetch_add(1, Ordering::Relaxed);
                    }
                };
            for _ in 0..steps {
                now += Duration::from_micros(match r.next() % 5 {
                    0 => 0,
                    1 => (r.next() % 2000) as u64,
                    2 => (r.next() % 100_000) as u64,
                    3 => (r.next() % 2_000_000) as u64,
                    _ => 10_000,
                });
                s.cx.set_now(now);
                if r.next() % 3 == 0 {
                    let _ = s.socket.send_slice(&vec![0u8; (r.next() % 20000) as usize]);
                }
                for _ in 0..(r.next() % 64) {
                    let pa = s.socket.poll_at(&mut s.cx);
                    let mut emitted = false;
                    s.socket
                        .dispatch(&mut s.cx, |_, _, _| {
                            emitted = true;
                            Ok::<_, ()>(())
                        })
                        .unwrap();
                    if emitted {
                        SEGS.fetch_add(1, Ordering::Relaxed);
                    }
                    inv(&s);
                    if pa == PollAt::Now && !emitted {
                        spin += 1;
                        worst = worst.max(spin);
                    } else {
                        spin = 0;
                    }
                    assert!(spin < 2000, "poll_at Now without progress x{spin}");
                }
                let una = s.local_seq_no;
                let span = s.tx_buffer.len() as u32 + 1;
                let nxt = s.remote_last_seq;
                let ack = match r.next() % 5 {
                    0 => una,
                    1 => una + (r.next() % span) as usize,
                    2 => nxt.max(una),
                    3 => TcpSeqNumber(r.next() as i32),
                    _ => una + (r.next() % span.min(64)) as usize,
                };
                let mut sack = [None; 3];
                for b in sack.iter_mut() {
                    if r.next() % 2 == 0 {
                        let base = match r.next() % 4 {
                            0 => r.next(),
                            1 => (una + (r.next() % span) as usize).0 as u32,
                            2 => (nxt.0 as u32).wrapping_sub(r.next() % 3000),
                            _ => (una.0 as u32)
                                .wrapping_add(0x8000_0000)
                                .wrapping_add(r.next() % 4096),
                        };
                        let len = match r.next() % 4 {
                            0 => r.next(),
                            1 => r.next() % 6000,
                            2 => 0x8000_0000u32
                                .wrapping_add(r.next() % 4096)
                                .wrapping_sub(2048),
                            _ => (nxt.0 as u32).wrapping_sub(base),
                        };
                        *b = Some((base, base.wrapping_add(len)));
                    }
                }
                let win = match r.next() % 4 {
                    0 => 0,
                    1 => (r.next() % 65536) as u16,
                    _ => 65535,
                };
                peer(&mut s, ack, sack, win);
                inv(&s);
                // occasional duplicate-ACK bursts with in-flight SACKs
                if r.next() % 4 == 0 {
                    for _ in 0..(3 + r.next() % 4) {
                        let l = (s.local_seq_no + (r.next() % span) as usize).0 as u32;
                        let una2 = s.local_seq_no;
                        let l2 = l.wrapping_add(1 + r.next() % 3000);
                        peer(&mut s, una2, [Some((l, l2)), None, None], 65535);
                        inv(&s);
                    }
                }
            }
            // Liveness: cooperative peer ACKs the highest byte it sees, window open.
            let mut drained = false;
            for _ in 0..20_000 {
                now += Duration::from_millis(10);
                s.cx.set_now(now);
                let mut high: Option<TcpSeqNumber> = None;
                for _ in 0..64 {
                    let mut h = None;
                    s.socket
                        .dispatch(&mut s.cx, |_, _, (_, repr)| {
                            h = Some(repr.seq_number + repr.segment_len());
                            Ok::<_, ()>(())
                        })
                        .unwrap();
                    inv(&s);
                    match h {
                        Some(h) => {
                            SEGS.fetch_add(1, Ordering::Relaxed);
                            high = Some(high.map_or(h, |x: TcpSeqNumber| x.max(h)));
                        }
                        None => break,
                    }
                }
                if let Some(h) = high {
                    let ack = if h > s.local_seq_no {
                        h
                    } else {
                        s.local_seq_no
                    };
                    peer(&mut s, ack, [None; 3], 65535);
                    inv(&s);
                }
                if s.tx_buffer.is_empty() {
                    drained = true;
                    break;
                }
            }
            if !drained {
                std::println!(
                    "WEDGE round {round}: mss {mss} sack {} cc {:?} una {} nxt {} len {} timer {:?} win {} rec {:?} state {}",
                    s.remote_has_sack,
                    s.congestion_control(),
                    s.local_seq_no,
                    s.remote_last_seq,
                    s.tx_buffer.len(),
                    s.timer,
                    s.remote_win_len,
                    s.recovery_end,
                    s.state
                );
            }
            (drained, worst)
        }));
        match res {
            Ok((drained, w)) => {
                if !drained {
                    wedges += 1;
                }
                worst_spin = worst_spin.max(w);
            }
            Err(e) => {
                let msg = e
                    .downcast_ref::<std::string::String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                panics.push(std::format!("seed {seed} round {round}: {msg}"));
            }
        }
    }
    std::panic::set_hook(hook);
    std::println!(
        "SUMMARY seed={seed} rounds={rounds} segments={} acks={} wedges={wedges} panics={} worst_spin={worst_spin}",
        SEGS.load(Ordering::Relaxed),
        ACKS.load(Ordering::Relaxed),
        panics.len()
    );
    for p in panics.iter().take(10) {
        std::println!("PANIC {p}");
    }
    assert!(panics.is_empty() && wedges == 0);
}
