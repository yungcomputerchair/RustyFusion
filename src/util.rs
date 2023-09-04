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
