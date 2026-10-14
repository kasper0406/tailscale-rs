# Draft (rewrite before filing): tcp: a fast retransmit is lost if emitting it fails

On main (87b463f), when `dispatch` sends a fast retransmit and the emit callback returns an error (for example
because the device is briefly out of TX buffers), the repair is never retried, and nothing arms a timer for it.
The flag is cleared before the emit. If the peer has nothing more to send and there's no new data or keepalive,
the connection stays stalled.

`src/socket/tcp.rs` 2566–2585 set `pending_fast_retransmit = true` and make the timer idle; 2712 clears the flag;
the `emit(...)?` at 2864 then returns early on error, skipping the bookkeeping that would arm the RTO.

**Repro** (in `src/socket/tcp.rs` tests; fails on main with `left: 0, right: 1`,
`timer=Idle { keep_alive_at: None }`):

```rust
fn flight() -> TestSocket {
    let mut s = socket_established();
    #[cfg(feature = "socket-tcp-reno")]
    s.set_congestion_control(CongestionControl::Reno);
    s.remote_mss = 6;
    send!(s, time 0, TcpRepr { seq_number: REMOTE_SEQ + 1, ack_number: Some(LOCAL_SEQ + 1), ..SEND_TEMPL });
    s.send_slice(b"aaaaaabbbbbbccccccdddddd").unwrap();
    for offset in [0, 6, 12, 18] {
        recv!(s, time 0, Ok(TcpRepr {
            seq_number: LOCAL_SEQ + 1 + offset,
            ack_number: Some(REMOTE_SEQ + 1),
            payload: &b"aaaaaabbbbbbccccccdddddd"[offset..offset + 6],
            ..RECV_TEMPL
        }));
    }
    for _ in 0..3 {
        send!(s, time 10, TcpRepr { seq_number: REMOTE_SEQ + 1, ack_number: Some(LOCAL_SEQ + 1), ..SEND_TEMPL });
    }
    assert_eq!(s.timer, Timer::FastRetransmit);
    s
}

#[test]
fn fast_retransmit_survives_failed_emit() {
    let mut s = flight();
    s.cx.set_now(Instant::from_millis(10));
    assert_eq!(s.socket.dispatch(&mut s.cx, |_, _, (_, repr)| {
        assert_eq!(repr.payload, b"aaaaaa");
        Err::<(), _>(())
    }), Err(()));
    let mut repairs = 0;
    s.cx.set_now(Instant::from_millis(11));
    s.socket.dispatch(&mut s.cx, |_, _, (_, repr)| {
        assert_eq!(repr.seq_number, LOCAL_SEQ + 1);
        repairs += 1;
        Ok::<(), ()>(())
    }).unwrap();
    assert_eq!(repairs, 1, "failed emit lost the repair; timer={:?}", s.timer);
}
```

**Possible fix:** clear `pending_fast_retransmit` only after the emit succeeds (or restore it on error), so the
next `dispatch` retries the repair, without reducing cwnd a second time; and make sure a retransmission
deadline is armed whenever data is in flight.

_Found with AI assistance (analysis and repro test); the fix above is a sketch._
