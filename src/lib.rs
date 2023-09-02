#[macro_use]
extern crate num_derive;

use std::{error::Error, result};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub const CN_PACKET_BUFFER_SIZE: usize = 4096;

pub mod net;

pub mod util {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn parse_utf16(chars: &[u16]) -> String {
        let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
        String::from_utf16_lossy(&chars[..end_pos])
    }

    pub fn get_time() -> u64 {
        let now: SystemTime = SystemTime::now();
        let diff: Duration = now.duration_since(UNIX_EPOCH).unwrap();
        diff.as_millis() as u64
    }
}

pub mod error {
    use std::{error::Error, fmt::Display};

    use crate::net::{ffclient::ClientType, packet::PacketID};

    #[derive(Debug)]
    pub struct BadPacketID {
        packet_id: u32,
    }
    impl BadPacketID {
        pub fn new(packet_id: u32) -> Self {
            Self { packet_id }
        }
    }
    impl Error for BadPacketID {}
    impl Display for BadPacketID {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Bad packet ID {}", self.packet_id)
        }
    }

    #[derive(Debug)]
    pub struct BadRequest {
        addr: String,
        packet_id: PacketID,
        client_type: ClientType,
    }
    impl BadRequest {
        pub fn new(addr: String, packet_id: PacketID, client_type: ClientType) -> Self {
            Self {
                addr,
                packet_id,
                client_type,
            }
        }
    }
    impl Error for BadRequest {}
    impl Display for BadRequest {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "Bad request from {} (client type {:?}): {:?}",
                self.addr, self.client_type, self.packet_id
            )
        }
    }
}
