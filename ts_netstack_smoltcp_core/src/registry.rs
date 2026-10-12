//! Socket metadata paired with SocketSet by the add/remove wrappers.
use smoltcp::socket::{AnySocket, icmp, raw, tcp, udp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SocketKind {
    Tcp,
    Udp,
    Raw,
    Icmp,
}

pub(crate) trait RegisteredSocket: AnySocket<'static> {
    const KIND: SocketKind;
}

macro_rules! register_kind {
    ($ty:ty, $kind:ident) => {
        impl RegisteredSocket for $ty {
            const KIND: SocketKind = SocketKind::$kind;
        }
    };
}
register_kind!(tcp::Socket<'static>, Tcp);
register_kind!(udp::Socket<'static>, Udp);
register_kind!(raw::Socket<'static>, Raw);
register_kind!(icmp::Socket<'static>, Icmp);

pub(crate) struct SocketEntry {
    pub generation: u64,
    pub kind: SocketKind,
}

#[cfg(test)]
mod tests {
    use smoltcp::{
        time::Instant,
        wire::{IpProtocol, IpVersion},
    };

    use super::*;
    use crate::{Config, Netstack};

    #[test]
    fn wrappers_keep_kinds_and_generations_in_sync_on_slot_reuse() {
        let mut stack = Netstack::new(Config::default(), Instant::ZERO);
        let tcp = tcp::Socket::new(
            tcp::SocketBuffer::new(alloc::vec![0; 64]),
            tcp::SocketBuffer::new(alloc::vec![0; 64]),
        );
        let handle = stack.add_socket(tcp);
        let generation = stack.handle_gen(handle).unwrap();
        assert!(stack.socket_matches::<tcp::Socket>(handle));
        assert!(!stack.socket_matches::<udp::Socket>(handle));
        stack.pending_tcp_closes.insert(handle);
        stack.remove_socket(handle);
        assert_eq!(stack.handle_gen(handle), None);
        assert!(!stack.socket_matches::<tcp::Socket>(handle));
        assert!(stack.pending_tcp_closes.is_empty());

        let udp = udp::Socket::new(
            udp::PacketBuffer::new(
                alloc::vec![udp::PacketMetadata::EMPTY; 1],
                alloc::vec![0; 64],
            ),
            udp::PacketBuffer::new(
                alloc::vec![udp::PacketMetadata::EMPTY; 1],
                alloc::vec![0; 64],
            ),
        );
        let recycled = stack.add_socket(udp);
        assert_eq!(recycled, handle);
        assert_ne!(stack.handle_gen(recycled), Some(generation));
        assert!(stack.socket_matches::<udp::Socket>(recycled));
        assert!(!stack.socket_matches::<tcp::Socket>(recycled));

        let raw = raw::Socket::new(
            Some(IpVersion::Ipv4),
            Some(IpProtocol::Icmp),
            raw::PacketBuffer::new(alloc::vec![], alloc::vec![]),
            raw::PacketBuffer::new(alloc::vec![], alloc::vec![]),
        );
        let raw = stack.add_socket(raw);
        assert!(stack.socket_matches::<raw::Socket>(raw));
        assert!(!stack.socket_matches::<icmp::Socket>(raw));
        let icmp = icmp::Socket::new(
            icmp::PacketBuffer::new(alloc::vec![], alloc::vec![]),
            icmp::PacketBuffer::new(alloc::vec![], alloc::vec![]),
        );
        let icmp = stack.add_socket(icmp);
        assert!(stack.socket_matches::<icmp::Socket>(icmp));
        assert!(!stack.socket_matches::<raw::Socket>(icmp));
    }
}
