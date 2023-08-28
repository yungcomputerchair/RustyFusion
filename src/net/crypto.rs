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
