//! Which socket receives a unicast datagram on macOS when a wildcard socket and a socket
//! connected to the sender share one port via SO_REUSEPORT? (Assumption behind connected
//! per-peer underlay sockets.) Also checks that a third, unrelated sender still reaches the
//! wildcard socket, and the order in which the sockets are created doesn't matter.
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

fn reuse_sock(bind: SocketAddr) -> UdpSocket {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
    s.set_reuse_address(true).unwrap();
    s.set_reuse_port(true).unwrap();
    s.bind(&bind.into()).unwrap();
    let s: UdpSocket = s.into();
    s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    s
}

fn got(s: &UdpSocket) -> Option<String> {
    let mut b = [0u8; 64];
    s.recv_from(&mut b).ok().map(|(n, _)| String::from_utf8_lossy(&b[..n]).into_owned())
}

fn main() {
    for ip in ["127.0.0.1", "192.168.50.76"] {
        let peer = UdpSocket::bind(format!("{ip}:0")).unwrap();
        let other = UdpSocket::bind(format!("{ip}:0")).unwrap();
        let wild = reuse_sock("0.0.0.0:0".parse().unwrap());
        let port = wild.local_addr().unwrap().port();
        let conn = reuse_sock(format!("0.0.0.0:{port}").parse().unwrap());
        conn.connect(peer.local_addr().unwrap()).unwrap();
        let dst: SocketAddr = format!("{ip}:{port}").parse().unwrap();
        for i in 0..3 {
            peer.send_to(format!("from-peer-{i}").as_bytes(), dst).unwrap();
        }
        other.send_to(b"from-other", dst).unwrap();
        let (mut c, mut w) = (vec![], vec![]);
        while let Some(m) = got(&conn) { c.push(m) }
        while let Some(m) = got(&wild) { w.push(m) }
        println!("{ip}: connected socket got {c:?}; wildcard socket got {w:?}");
        // Reply path: the connected socket sends from the shared port.
        conn.send(b"reply").unwrap();
        let mut b = [0u8; 16];
        let (_, from) = peer.recv_from(&mut b).unwrap();
        println!("{ip}: reply from {from} (shared port {port}: {})", from.port() == port);
    }
}
