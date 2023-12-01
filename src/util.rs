use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::{thread_rng, Rng};

use crate::player::TEST_ACC_UID_START;

pub fn parse_utf16(chars: &[u16]) -> String {
    let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    if let Ok(val) = String::from_utf16(&chars[..end_pos]) {
        val
    } else {
        String::new()
    }
}

pub fn parse_utf8(chars: &[u8]) -> String {
    let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    if let Ok(val) = std::str::from_utf8(&chars[..end_pos]) {
        val.to_string()
    } else {
        String::new()
    }
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

pub fn get_uid() -> i64 {
    let uid: i64 = thread_rng().gen_range(i64::MIN..TEST_ACC_UID_START);
    uid
}
