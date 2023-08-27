use std::{result, error::Error};

pub type Result<T> = result::Result<T, Box<dyn Error>>;

pub const CN_PACKET_BUFFER_SIZE: usize = 4096;

pub mod net;

pub mod util {
    pub fn parse_utf16(chars: &[u16]) -> String {
        let end_pos: usize = chars.iter()
            .position(|&c| c == 0)
            .unwrap_or(chars.len());
        String::from_utf16_lossy(&chars[..end_pos])
    }
}
