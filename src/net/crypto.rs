use std::num::Wrapping;

pub const DEFAULT_KEY: &[u8] = b"m@rQn~W#";
pub const CRYPTO_KEY_SIZE: usize = DEFAULT_KEY.len();

pub enum EncryptionMode {
    EKey,
    FEKey,
}

fn xor(buf: &mut [u8], key: &[u8], size: usize) {
    for i in 0..size {
        let b: &mut u8 = &mut buf[i];
        *b = *b ^ key[i % CRYPTO_KEY_SIZE];
    }
}

fn byte_swap(er_size: usize, buf: &mut [u8], size: usize) -> usize {
    let mut num: usize = 0;
    let mut num3: usize = 0;

    while num + er_size <= size {
        let num4: usize = num + num3;
        let num5: usize = num + (er_size - 1 - num3);

        let tmp: u8 = buf[num4];
        buf[num4] = buf[num5];
        buf[num5] = tmp;

        num += er_size;
        num3 += 1;
        if num3 > er_size / 2 {
            num3 = 0;
        }
    }
    let num2: usize = er_size - (num + er_size - size);
    num + num2
}

pub fn decrypt_packet(buf: &mut [u8], key: &[u8]) {
    debug_assert!(key.len() == CRYPTO_KEY_SIZE);
    let er_size: usize = buf.len() % (CRYPTO_KEY_SIZE / 2 + 1) * 2 + CRYPTO_KEY_SIZE;
    let xor_size: usize = byte_swap(er_size, buf, buf.len());
    xor(buf, key, xor_size);
}

pub fn encrypt_packet(buf: &mut [u8], key: &[u8]) {
    debug_assert!(key.len() == CRYPTO_KEY_SIZE);
    let er_size: usize = buf.len() % (CRYPTO_KEY_SIZE / 2 + 1) * 2 + CRYPTO_KEY_SIZE;
    xor(buf, key, buf.len());
    byte_swap(er_size, buf, buf.len());
}

pub fn gen_key(time: u64, iv1: i32, iv2: i32) -> [u8; CRYPTO_KEY_SIZE] {
    let time = Wrapping(time);
    let num = Wrapping((iv1 + 1) as u64);
    let num2 = Wrapping((iv2 + 1) as u64);
    let default_key = Wrapping(u64::from_le_bytes(DEFAULT_KEY.try_into().unwrap()));
    let result: u64 = (default_key * (time * num * num2)).0;
    result.to_le_bytes()
}

#[cfg(test)]
mod tests {
    use crate::net::bytes_to_struct;
    use crate::net::packet::*;
    use crate::net::struct_to_bytes;
    use crate::util::get_time;

    use super::{decrypt_packet, encrypt_packet, CRYPTO_KEY_SIZE};

    #[test]
    fn test_enc_dec() {
        let pkt = sP_LS2CL_REP_LOGIN_SUCC {
            iCharCount: 1,
            iSlotNum: 2,
            iPaymentFlag: 3,
            iTempForPacking4: 4,
            uiSvrTime: get_time(),
            szID: [6; 33],
            iOpenBetaFlag: 7,
        };
        let bytes: &[u8] = unsafe { struct_to_bytes(&pkt) };
        let mut buf: Vec<u8> = bytes.to_vec();

        let key: [u8; CRYPTO_KEY_SIZE] = (4382366871217075016 as u64).to_le_bytes();
        encrypt_packet(&mut buf, &key);
        assert_ne!(buf.as_slice(), bytes);
        decrypt_packet(&mut buf, &key);
        assert_eq!(buf.as_slice(), bytes);

        let pkt_dec: sP_LS2CL_REP_LOGIN_SUCC = unsafe { *bytes_to_struct(&buf) };
        //dbg!(pkt_dec);
        assert_eq!({ pkt.uiSvrTime }, { pkt_dec.uiSvrTime });
    }
}
