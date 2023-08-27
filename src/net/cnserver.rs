use std::{net::TcpStream, io::Read, time::Duration};
use crate::{Result, net::{crypto, packet::*}, CN_PACKET_BUFFER_SIZE, util::parse_utf16};

pub trait CNServer {
    fn new(poll_timeout: Option<Duration>) -> Result<Self> where Self: Sized;
    fn poll(&mut self) -> Result<()>;
}

unsafe fn bytes_to_struct<T>(bytes: &[u8]) -> &T {
    // haters will call this "undefined behavior"
    let struct_ptr: *const T = bytes.as_ptr().cast();
    &*struct_ptr
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
    Ok(())
}
