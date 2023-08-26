use std::{net::TcpStream, io::Read, time::Duration};
use crate::*;

pub trait CNServer {
    fn new(poll_timeout: Option<Duration>) -> Result<Self> where Self: Sized;
    fn poll(&mut self) -> Result<()>;
}

pub fn sock_read(sock: &mut TcpStream) -> Result<()> {
    let mut buf: [u8; CN_PACKET_BUFFER_SIZE as usize] = [0; CN_PACKET_BUFFER_SIZE as usize];

    //let peeked: usize = sock.peek(&mut buf)?;
    //println!("peeked {} bytes", peeked);

    let mut sz_buf: [u8; 4] = [0; 4];
    sock.read_exact(&mut sz_buf)?;

    let sz: usize = u32::from_le_bytes(sz_buf) as usize;
    sock.read_exact(&mut buf[0..sz])?;

    Ok(())
}
