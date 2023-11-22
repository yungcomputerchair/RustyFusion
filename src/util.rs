use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn parse_utf16(chars: &[u16]) -> String {
    let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf16_lossy(&chars[..end_pos])
}

pub fn parse_utf8(chars: &[u8]) -> String {
    let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf8_lossy(&chars[..end_pos]).to_string()
}

pub fn encode_utf16<const SIZE: usize>(chars: &str) -> [u16; SIZE] {
    let mut str_vec: Vec<u16> = chars.encode_utf16().collect();
    str_vec.push(0);
    if str_vec.len() > SIZE {
        panic!("Buffer too small for encoded string");
    }
    str_vec.resize(SIZE, 0);
    str_vec.try_into().unwrap()
}

pub fn get_time() -> u64 {
    let now: SystemTime = SystemTime::now();
    let diff: Duration = now.duration_since(UNIX_EPOCH).unwrap();
    diff.as_millis() as u64
}
