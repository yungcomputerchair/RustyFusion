use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::{thread_rng, Rng};

use crate::{
    defines::*,
    enums::ItemLocation,
    error::{FFError, FFResult, Severity},
    player::TEST_ACC_UID_START,
};

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

pub fn get_timestamp_ms(time: SystemTime) -> u64 {
    let diff = time.duration_since(UNIX_EPOCH).unwrap();
    diff.as_millis() as u64
}

pub fn get_timestamp_sec(time: SystemTime) -> u32 {
    let diff = time.duration_since(UNIX_EPOCH).unwrap();
    diff.as_secs() as u32
}

pub fn get_systime_from_ms(timestamp_ms: u64) -> SystemTime {
    let diff = Duration::from_millis(timestamp_ms);
    SystemTime::UNIX_EPOCH + diff
}

pub fn get_systime_from_sec(timestamp_sec: u64) -> SystemTime {
    let diff = Duration::from_secs(timestamp_sec);
    SystemTime::UNIX_EPOCH + diff
}

pub fn get_uid() -> i64 {
    let uid: i64 = thread_rng().gen_range(i64::MIN..TEST_ACC_UID_START);
    uid
}

pub fn slot_num_to_loc_and_slot_num(mut slot_num: usize) -> FFResult<(ItemLocation, usize)> {
    if slot_num < SIZEOF_EQUIP_SLOT as usize {
        return Ok((ItemLocation::Equip, slot_num));
    }

    slot_num -= SIZEOF_EQUIP_SLOT as usize;
    if slot_num < SIZEOF_INVEN_SLOT as usize {
        return Ok((ItemLocation::Inven, slot_num));
    }

    slot_num -= SIZEOF_INVEN_SLOT as usize;
    if slot_num < SIZEOF_QINVEN_SLOT as usize {
        return Ok((ItemLocation::QInven, slot_num));
    }

    slot_num -= SIZEOF_QINVEN_SLOT as usize;
    if slot_num < SIZEOF_BANK_SLOT as usize {
        return Ok((ItemLocation::Bank, slot_num));
    }

    Err(FFError::build(
        Severity::Warning,
        format!("Bad slot number: {slot_num}"),
    ))
}

pub fn weighted_rand(weights: &[i32]) -> usize {
    let sum: i32 = weights.iter().sum();
    let mut roll = rand::thread_rng().gen_range(0..=sum);
    for (idx, limit) in weights.iter().enumerate() {
        if roll < *limit {
            return idx;
        } else {
            roll -= *limit;
        }
    }
    weights.len() - 1
}
