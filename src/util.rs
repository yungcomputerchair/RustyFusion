use std::{
    ops::{BitAnd, BitAndAssign, BitOrAssign, Not, Shl, Shr},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Local};
use rand::{distributions::uniform::SampleUniform, Rng};

use crate::{
    defines::*,
    enums::{ItemLocation, ItemType},
    error::{panic_log, FFError, FFResult, Severity},
    item::Item,
};

pub trait AsBytes {
    fn to_bytes(&self) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Self;
}
impl AsBytes for i32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut bytes_array = [0; 4];
        bytes_array.copy_from_slice(&bytes[..4]);
        Self::from_le_bytes(bytes_array)
    }
}
impl AsBytes for i64 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut bytes_array = [0; 8];
        bytes_array.copy_from_slice(&bytes[..8]);
        Self::from_le_bytes(bytes_array)
    }
}

#[derive(Debug, Clone)]
pub struct Bitfield<T> {
    chunks: Vec<T>,
}
impl<
        T: Clone
            + Copy
            + From<bool>
            + AsBytes
            + Shl<usize, Output = T>
            + Shr<usize, Output = T>
            + Not<Output = T>
            + PartialEq
            + BitOrAssign
            + BitAndAssign
            + BitAnd<Output = T>,
    > Bitfield<T>
{
    pub fn new(num_chunks: usize) -> Self {
        Self {
            chunks: vec![Self::false_value(); num_chunks],
        }
    }

    pub fn from_single(val: T) -> Self {
        Self { chunks: vec![val] }
    }

    const fn chunk_size_bytes() -> usize {
        std::mem::size_of::<T>()
    }

    const fn chunk_size_bits() -> usize {
        Self::chunk_size_bytes() * 8
    }

    fn false_value() -> T {
        T::from(false)
    }

    fn true_value() -> T {
        T::from(true)
    }

    pub fn set(&mut self, idx: usize, val: bool) -> FFResult<bool> {
        let chunk_idx = idx / Self::chunk_size_bits();
        if chunk_idx >= self.chunks.len() {
            let max_idx = self.chunks.len() * Self::chunk_size_bits() - 1;
            return Err(FFError::build(
                Severity::Warning,
                format!("Bitfield index {} out of bounds, max {}", idx, max_idx),
            ));
        }

        let bit_idx = idx % Self::chunk_size_bits();
        let mask = Self::true_value() << bit_idx;
        let old_val = self.chunks[chunk_idx];
        if val {
            self.chunks[chunk_idx] |= mask;
        } else {
            self.chunks[chunk_idx] &= !mask;
        }
        Ok(old_val != self.chunks[chunk_idx])
    }

    pub fn get(&self, idx: usize) -> FFResult<bool> {
        let chunk_idx = idx / Self::chunk_size_bits();
        if chunk_idx >= self.chunks.len() {
            let max_idx = self.chunks.len() * Self::chunk_size_bits() - 1;
            return Err(FFError::build(
                Severity::Warning,
                format!("Bitfield index {} out of bounds, max {}", idx, max_idx),
            ));
        }

        let bit_idx = idx % Self::chunk_size_bits();
        let mask = Self::true_value() << bit_idx;
        Ok((self.chunks[chunk_idx] & mask) != Self::false_value())
    }

    pub fn get_chunk(&self, idx: usize) -> FFResult<T> {
        if idx >= self.chunks.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Bitfield chunk index {} out of bounds, max {}",
                    idx,
                    self.chunks.len() - 1
                ),
            ));
        }
        Ok(self.chunks[idx])
    }

    pub fn set_chunk(&mut self, idx: usize, val: T) -> FFResult<()> {
        if idx >= self.chunks.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Bitfield chunk index {} out of bounds, max {}",
                    idx,
                    self.chunks.len() - 1
                ),
            ));
        }
        self.chunks[idx] = val;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in &self.chunks {
            let chunk_bytes = chunk.to_bytes();
            for byte in &chunk_bytes {
                bytes.push(*byte);
            }
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8], num_chunks: usize) -> FFResult<Self> {
        let mut chunks = Vec::new();
        let chunk_size_bytes = Self::chunk_size_bytes();
        for i in 0..num_chunks {
            let start = i * chunk_size_bytes;
            let end = start + chunk_size_bytes;
            if start >= bytes.len() || end > bytes.len() {
                chunks.push(Self::false_value());
            } else {
                let chunk_bytes = &bytes[start..end];
                chunks.push(T::from_bytes(chunk_bytes));
            }
        }
        Ok(Self { chunks })
    }

    pub fn to_array<const SIZE: usize>(&self) -> FFResult<[T; SIZE]> {
        if SIZE > self.chunks.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Bitfield has {} chunks, but array size is {}",
                    self.chunks.len(),
                    SIZE
                ),
            ));
        }
        let mut array = [Self::false_value(); SIZE];
        for (i, chunk) in self.chunks.iter().enumerate() {
            array[i] = *chunk;
        }
        Ok(array)
    }
}

pub fn clamp<T: Ord>(val: T, min: T, max: T) -> T {
    if val < min {
        min
    } else if val > max {
        max
    } else {
        val
    }
}

pub fn clamp_min<T: Ord>(val: T, min: T) -> T {
    if val < min {
        min
    } else {
        val
    }
}

pub fn clamp_max<T: Ord>(val: T, max: T) -> T {
    if val > max {
        max
    } else {
        val
    }
}

pub fn rotation_to_angle(rotation_deg: i32) -> i32 {
    (270 - rotation_deg).rem_euclid(360)
}

pub fn angle_to_rotation(angle_deg: i32) -> i32 {
    (270 - angle_deg).rem_euclid(360)
}

pub fn parse_utf16(chars: &[u16]) -> FFResult<String> {
    let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    String::from_utf16(&chars[..end_pos]).map_err(|_| {
        FFError::build(
            Severity::Warning,
            format!("Bytes are not UTF-16: {:?}", chars),
        )
    })
}

pub fn parse_utf8(chars: &[u8]) -> FFResult<String> {
    let end_pos: usize = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    std::str::from_utf8(&chars[..end_pos])
        .map_err(|_| {
            FFError::build(
                Severity::Warning,
                format!("Bytes are not UTF-8: {:?}", chars),
            )
        })
        .map(|s| s.to_string())
}

pub fn encode_utf16<const SIZE: usize>(chars: &str) -> [u16; SIZE] {
    let mut str_vec: Vec<u16> = chars.encode_utf16().collect();
    str_vec.push(0);
    if str_vec.len() > SIZE {
        panic_log("Buffer too small for encoded string");
    }
    str_vec.resize(SIZE, 0);
    str_vec.try_into().unwrap()
}

pub fn get_timestamp_str(time: SystemTime) -> String {
    let date_time_local: DateTime<Local> = time.into();
    date_time_local.format("%Y-%m-%d %H:%M:%S").to_string()
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

pub fn hash_password(password: &str) -> FFResult<String> {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(FFError::from_bcrypt_err)
}

pub fn check_password(password: &str, hash: &str) -> FFResult<bool> {
    bcrypt::verify(password, hash).map_err(FFError::from_bcrypt_err)
}

pub fn make_duration(days: u64, hours: u64, mins: u64, secs: u64) -> Duration {
    let mut duration = Duration::from_secs(secs);
    duration += Duration::from_secs(mins * 60);
    duration += Duration::from_secs(hours * 60 * 60);
    duration += Duration::from_secs(days * 24 * 60 * 60);
    duration
}

pub fn get_duration_from_shorthand(shorthand: &str) -> FFResult<Duration> {
    let mut duration = Duration::from_secs(0);
    let mut num = 0;
    for c in shorthand.chars() {
        if c.is_ascii_digit() {
            num = num * 10 + c.to_digit(10).unwrap();
        } else {
            match c {
                'd' => duration += Duration::from_secs(num as u64 * 24 * 60 * 60),
                'h' => duration += Duration::from_secs(num as u64 * 60 * 60),
                'm' => duration += Duration::from_secs(num as u64 * 60),
                's' => duration += Duration::from_secs(num as u64),
                _ => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Invalid shorthand character: {}", c),
                    ))
                }
            }
            num = 0;
        }
    }
    Ok(duration)
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    format!(
        "{} day(s), {} hour(s), {} minute(s), {} second(s)",
        days,
        hours % 24,
        mins % 60,
        secs % 60
    )
}

pub fn get_uid() -> i64 {
    // player ids should really always be positive
    rand::thread_rng().gen_range(1..i64::MAX)
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

pub fn rand_range_inclusive<T: SampleUniform + Ord>(min: T, max: T) -> T {
    rand::thread_rng().gen_range(min..=max)
}

pub fn rand_range_exclusive<T: SampleUniform + Ord>(min: T, max: T) -> T {
    rand::thread_rng().gen_range(min..max)
}

pub fn get_text_file_contents(path: &str) -> FFResult<String> {
    std::fs::read_to_string(path).map_err(|e| {
        FFError::build(
            Severity::Warning,
            format!("Failed to read file '{}': {}", path, e),
        )
    })
}

pub fn get_random_gumball() -> Item {
    let gumballs = [
        Item::new(ItemType::General, ID_GUMBALL),
        Item::new(ItemType::General, ID_GUMBALL + 1),
        Item::new(ItemType::General, ID_GUMBALL + 2),
    ];
    let choice = rand_range_exclusive(0, gumballs.len());
    gumballs[choice]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_parsing() {
        assert_eq!(
            get_duration_from_shorthand("").unwrap(),
            make_duration(0, 0, 0, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1d").unwrap(),
            make_duration(1, 0, 0, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1h").unwrap(),
            make_duration(0, 1, 0, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1m").unwrap(),
            make_duration(0, 0, 1, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1s").unwrap(),
            make_duration(0, 0, 0, 1)
        );
        assert_eq!(
            get_duration_from_shorthand("1d1h1m1s").unwrap(),
            make_duration(1, 1, 1, 1)
        );
        assert_eq!(
            get_duration_from_shorthand("1d1h1m").unwrap(),
            make_duration(1, 1, 1, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1d1h").unwrap(),
            make_duration(1, 1, 0, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1d1m").unwrap(),
            make_duration(1, 0, 1, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1d1s").unwrap(),
            make_duration(1, 0, 0, 1)
        );
        assert_eq!(
            get_duration_from_shorthand("1h1m1s").unwrap(),
            make_duration(0, 1, 1, 1)
        );
        assert_eq!(
            get_duration_from_shorthand("1h1m").unwrap(),
            make_duration(0, 1, 1, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1h1s").unwrap(),
            make_duration(0, 1, 0, 1)
        );
        assert_eq!(
            get_duration_from_shorthand("1m1s").unwrap(),
            make_duration(0, 0, 1, 1)
        );
        assert_eq!(
            get_duration_from_shorthand("8h24m").unwrap(),
            make_duration(0, 8, 24, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1h1h").unwrap(),
            make_duration(0, 2, 0, 0)
        );
        assert_eq!(
            get_duration_from_shorthand("1h1d1h").unwrap(),
            make_duration(1, 2, 0, 0)
        );
    }
}
