//! Microbenchmark of macOS UDP send paths relevant to a WireGuard underlay socket:
//! unconnected sendto (what magicsock does today), connected send, connected + IP_DONTFRAG,
//! and connected + IP_DONTFRAG + sendmsg_x batches. Reports packets/s and CPU us/packet
//! (getrusage) for a fixed payload, sending to `dst` (a host:port on the LAN that discards).
//!
//! usage: udp-send <dst ip:port | sink> [secs=2] [payload=1240] [batch=32]
//! `sink` (default) sends to a discard socket on loopback.

use std::{
    io,
    mem::{size_of, zeroed},
    net::{SocketAddr, UdpSocket},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

// From xnu bsd/sys/socket_private.h (private SPI, exported by libSystem since macOS 10.11).
#[repr(C)]
struct MsghdrX {
    msg_name: *mut libc::c_void,
    msg_namelen: libc::socklen_t,
    msg_iov: *mut libc::iovec,
    msg_iovlen: libc::c_int,
    msg_control: *mut libc::c_void,
    msg_controllen: libc::socklen_t,
    msg_flags: libc::c_int,
    msg_datalen: libc::size_t,
}

extern "C" {
    fn sendmsg_x(s: libc::c_int, msgp: *const MsghdrX, cnt: libc::c_uint, flags: libc::c_int) -> isize;
}

const IP_DONTFRAG: libc::c_int = 28; // netinet/in.h on darwin

fn cpu_us() -> f64 {
    unsafe {
        let mut ru: libc::rusage = zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let t = |tv: libc::timeval| tv.tv_sec as f64 * 1e6 + tv.tv_usec as f64;
        t(ru.ru_utime) + t(ru.ru_stime)
    }
}

fn set_df(s: &UdpSocket) -> io::Result<()> {
    let one: libc::c_int = 1;
    let r = unsafe {
        libc::setsockopt(s.as_raw_fd(), libc::IPPROTO_IP, IP_DONTFRAG, &one as *const _ as *const _, size_of::<libc::c_int>() as _)
    };
    if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

struct Result { pkts: u64, errs: u64, secs: f64, cpu: f64 }

fn run(secs: f64, mut send: impl FnMut() -> io::Result<usize>) -> Result {
    let (t0, c0) = (Instant::now(), cpu_us());
    let deadline = t0 + Duration::from_secs_f64(secs);
    let (mut pkts, mut errs) = (0u64, 0u64);
    while Instant::now() < deadline {
        for _ in 0..64 {
            match send() {
                Ok(n) => pkts += n as u64,
                Err(_) => errs += 1, // ENOBUFS when the interface queue is full
            }
        }
    }
    Result { pkts, errs, secs: t0.elapsed().as_secs_f64(), cpu: cpu_us() - c0 }
}

fn report(name: &str, r: Result, payload: usize) {
    let pps = r.pkts as f64 / r.secs;
    println!(
        "{name:<28} pps={:>9.0} mbps={:>7.0} cpu_us_per_pkt={:>5.2} enobufs_or_err={}",
        pps, pps * payload as f64 * 8.0 / 1e6, r.cpu / r.pkts.max(1) as f64, r.errs
    );
}

fn main() -> io::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dst: SocketAddr = match a.first().map(String::as_str) {
        Some("sink") | None => {
            // Local discard sink on loopback (keeps the LAN quiet).
            let sink = UdpSocket::bind("127.0.0.1:0")?;
            let addr = sink.local_addr()?;
            std::thread::spawn(move || {
                let mut b = [0u8; 65536];
                while sink.recv(&mut b).is_ok() {}
            });
            addr
        }
        Some(d) => d.parse().unwrap(),
    };
    let secs: f64 = a.get(1).map_or(2.0, |s| s.parse().unwrap());
    let payload: usize = a.get(2).map_or(1240, |s| s.parse().unwrap());
    let batch: usize = a.get(3).map_or(32, |s| s.parse().unwrap());
    let buf = vec![0xabu8; payload];

    let s = UdpSocket::bind("0.0.0.0:0")?;
    report("unconnected sendto", run(secs, || s.send_to(&buf, dst).map(|_| 1)), payload);

    let s = UdpSocket::bind("0.0.0.0:0")?;
    set_df(&s)?;
    report("unconnected sendto + DF", run(secs, || s.send_to(&buf, dst).map(|_| 1)), payload);

    let s = UdpSocket::bind("0.0.0.0:0")?;
    s.connect(dst)?;
    report("connected send", run(secs, || s.send(&buf).map(|_| 1)), payload);

    let s = UdpSocket::bind("0.0.0.0:0")?;
    s.connect(dst)?;
    set_df(&s)?;
    report("connected send + DF", run(secs, || s.send(&buf).map(|_| 1)), payload);

    let fd = s.as_raw_fd();
    let mut iovs: Vec<libc::iovec> = (0..batch)
        .map(|_| libc::iovec { iov_base: buf.as_ptr() as *mut _, iov_len: payload })
        .collect();
    let hdrs: Vec<MsghdrX> = iovs
        .iter_mut()
        .map(|iov| MsghdrX {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: iov,
            msg_iovlen: 1,
            msg_control: std::ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
            msg_datalen: 0,
        })
        .collect();
    let name = format!("connected sendmsg_x x{batch} + DF");
    report(
        &name,
        run(secs / 1.0, || {
            // SAFETY: hdrs/iovs/buf outlive the call; the socket is connected so msg_name is null.
            let n = unsafe { sendmsg_x(fd, hdrs.as_ptr(), batch as _, 0) };
            if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
        }),
        payload,
    );
    Ok(())
}
