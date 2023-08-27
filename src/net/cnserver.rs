use std::{net::TcpStream, io::{Read, Write}, time::Duration, slice::from_raw_parts, mem::size_of};
use crate::{Result, net::{crypto, packet::*}, CN_PACKET_BUFFER_SIZE, util::{parse_utf16, get_time}};

pub trait CNServer {
    fn new(poll_timeout: Option<Duration>) -> Result<Self> where Self: Sized;
    fn poll(&mut self) -> Result<()>;
}

unsafe fn bytes_to_struct<T>(bytes: &[u8]) -> &T {
    // haters will call this "undefined behavior"
    let struct_ptr: *const T = bytes.as_ptr().cast();
    &*struct_ptr
}

unsafe fn struct_to_bytes<T>(pack: &T) -> &[u8] {
    let n: usize = size_of::<T>();
    let struct_ptr: *const T = pack;
    let buf_ptr: *const u8 = struct_ptr.cast();
    from_raw_parts(buf_ptr, n)
}

pub fn sock_read(sock: &mut TcpStream) -> Result<()> {
    let mut buf: [u8; CN_PACKET_BUFFER_SIZE] = [0; CN_PACKET_BUFFER_SIZE];

    //let peeked: usize = sock.peek(&mut buf)?;
    //println!("peeked {} bytes", peeked);

    let mut sz_buf: [u8; 4] = [0; 4];
    sock.read_exact(&mut sz_buf)?;

    let sz: usize = u32::from_le_bytes(sz_buf) as usize;
    let body: &mut [u8] = &mut buf[0..sz];
    sock.read_exact(body)?;
    crypto::decrypt_packet(body, crypto::DEFAULT_KEY);

    let id: u32 = u32::from_le_bytes(body[0..4].try_into().unwrap());
    println!("packet id {id}");

    let pack: &sP_CL2LS_REQ_LOGIN = unsafe { bytes_to_struct(&body[4..]) };
    println!("login request from {} ({})", parse_utf16(&pack.szID), parse_utf16(&pack.szPassword));

    let pack = sP_LS2CL_REP_LOGIN_SUCC {
        iCharCount: 0,
        iSlotNum: 0,
        iPaymentFlag: 1,
        iTempForPacking4: 69,
        uiSvrTime: get_time() as u64,
        szID: pack.szID.clone(),
        iOpenBetaFlag: 0,
    };

    let buf: &[u8] = unsafe { struct_to_bytes(&pack) };
    sz_buf = ((buf.len() + 4) as u32).to_le_bytes();
    sock.write_all(&sz_buf)?;
    let id: u32 = 0x21000001;
    let mut out_buf = id.to_le_bytes().to_vec();
    out_buf.append(&mut buf.to_vec());

    crypto::encrypt_packet(&mut out_buf, crypto::DEFAULT_KEY);
    sock.write_all(&out_buf)?;

    Ok(())
}
