use std::num::Wrapping;

use rand::{thread_rng, Rng};

use super::PACKET_BUFFER_SIZE;

pub const DEFAULT_KEY: &[u8] = b"m@rQn~W#";
pub const CRYPTO_KEY_SIZE: usize = DEFAULT_KEY.len();
pub const AUTH_CHALLENGE_SIZE: usize = PACKET_BUFFER_SIZE - 4;

pub type CryptoKey = [u8; CRYPTO_KEY_SIZE];
pub type AuthChallenge = [u8; AUTH_CHALLENGE_SIZE];

pub enum EncryptionMode {
    EKey,
    FEKey,
}

fn xor(buf: &mut [u8], key: &[u8], size: usize) {
    for i in 0..size {
        let b: &mut u8 = &mut buf[i];
        *b ^= key[i % key.len()];
    }
}

fn byte_swap(er_size: usize, buf: &mut [u8], size: usize) -> usize {
    let mut num: usize = 0;
    let mut num3: usize = 0;

    while num + er_size <= size {
        let num4: usize = num + num3;
        let num5: usize = num + (er_size - 1 - num3);
        buf.swap(num4, num5);

        num += er_size;
        num3 += 1;
        if num3 > er_size / 2 {
            num3 = 0;
        }
    }
    let num2: usize = er_size - (num + er_size - size);
    num + num2
}

pub fn decrypt_payload(buf: &mut [u8], key: &[u8]) {
    let key_size = key.len();
    let er_size: usize = buf.len() % (key_size / 2 + 1) * 2 + key_size;
    let xor_size: usize = byte_swap(er_size, buf, buf.len());
    xor(buf, key, xor_size);
}

pub fn encrypt_payload(buf: &mut [u8], key: &[u8]) {
    let key_size = key.len();
    let er_size: usize = buf.len() % (key_size / 2 + 1) * 2 + key_size;
    xor(buf, key, buf.len());
    byte_swap(er_size, buf, buf.len());
}

pub fn gen_key(time: u64, iv1: i32, iv2: i32) -> CryptoKey {
    let time = Wrapping(time);
    let num = Wrapping((iv1 + 1) as u64);
    let num2 = Wrapping((iv2 + 1) as u64);
    let default_key = Wrapping(u64::from_le_bytes(DEFAULT_KEY.try_into().unwrap()));
    let result: u64 = (default_key * (time * num * num2)).0;
    result.to_le_bytes()
}

pub fn gen_auth_challenge() -> AuthChallenge {
    let mut chall: AuthChallenge = [0; AUTH_CHALLENGE_SIZE];
    thread_rng().fill(&mut chall[..]);
    chall
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use crate::net::bytes_to_struct;
    use crate::net::packet::*;
    use crate::net::struct_to_bytes;
    use crate::util;

    use super::{decrypt_payload, encrypt_payload, CRYPTO_KEY_SIZE};

    #[test]
    fn test_enc_dec() {
        let pkt = sP_LS2CL_REP_LOGIN_SUCC {
            iCharCount: 1,
            iSlotNum: 2,
            iPaymentFlag: 3,
            iTempForPacking4: 4,
            uiSvrTime: util::get_timestamp_ms(SystemTime::now()),
            szID: [6; 33],
            iOpenBetaFlag: 7,
        };
        let bytes: &[u8] = unsafe { struct_to_bytes(&pkt) };
        let mut buf: Vec<u8> = bytes.to_vec();

        let key: [u8; CRYPTO_KEY_SIZE] = 4382366871217075016_u64.to_le_bytes();
        encrypt_payload(&mut buf, &key);
        assert_ne!(buf.as_slice(), bytes);
        decrypt_payload(&mut buf, &key);
        assert_eq!(buf.as_slice(), bytes);

        let pkt_dec: sP_LS2CL_REP_LOGIN_SUCC = unsafe { *bytes_to_struct(&buf) };
        //dbg!(pkt_dec);
        assert_eq!({ pkt.uiSvrTime }, { pkt_dec.uiSvrTime });
    }
}
