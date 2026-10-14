# Draft (rewrite before filing): tcp: the retransmit timer is lost when a zero window reopens with data in flight

On main (87b463f): a partial ACK that also advertises a zero window replaces the running retransmit timer with
the zero-window-probe timer. When a later ACK reopens the window without acknowledging the rest, the timer is set
to idle even though data is still in flight. That data is never retransmitted unless the peer sends something
else.

`src/socket/tcp.rs` 2265–2277: the probe-start condition allows replacing the RTO after a partial ACK, and the
window-reopen branch calls `set_for_idle` unconditionally. RFC 6298 §5 requires a running retransmission timer
while data is outstanding.

**Repro** (fails on main: `outstanding data has no timer`):

```rust
#[test]
fn reopened_window_retains_retransmit_timer() {
    let mut s = socket_established();
    #[cfg(feature = "socket-tcp-reno")]
    s.set_congestion_control(CongestionControl::Reno);
    s.remote_mss = 6;
    s.send_slice(b"aaaaaabbbbbb").unwrap();
    for (offset, payload) in [(0, &b"aaaaaa"[..]), (6, &b"bbbbbb"[..])] {
        recv!(s, time 0, Ok(TcpRepr {
            seq_number: LOCAL_SEQ + 1 + offset,
            ack_number: Some(REMOTE_SEQ + 1),
            payload,
            ..RECV_TEMPL
        }));
    }
    // ACK the first segment and close the window.
    send!(s, time 10, TcpRepr {
        seq_number: REMOTE_SEQ + 1, ack_number: Some(LOCAL_SEQ + 1 + 6), window_len: 0, ..SEND_TEMPL
    });
    assert!(s.timer.is_zero_window_probe());
    // Reopen the window without acknowledging the second segment.
    send!(s, time 20, TcpRepr { seq_number: REMOTE_SEQ + 1, ack_number: Some(LOCAL_SEQ + 1 + 6), ..SEND_TEMPL });
    assert_eq!(s.flight_size(), 6);
    recv_nothing!(s, time 3_600_020); // nothing is retransmitted within an hour
    assert!(s.timer.is_retransmit(), "outstanding data has no timer: {:?}", s.timer);
}
```

**Possible fix:** don't swap the retransmit timer for the probe timer while data is in flight, and when the window
reopens with data outstanding, re-arm the retransmit timer instead of going idle.

_Found with AI assistance (analysis and repro test); the fix above is a sketch._
