use std::{net::TcpStream, io::Read, time::Duration};
use crate::Result;

pub trait CNServer {
    fn new(poll_timeout: Option<Duration>) -> Result<Self> where Self: Sized;
    fn poll(&mut self) -> Result<()>;
}

pub fn sock_read(sock: &mut TcpStream) -> Result<()> {
    let mut buf: [u8; 4] = [0; 4];
    sock.read_exact(&mut buf)?; // uh oh, this blocks
    let sz: u32 = u32::from_le_bytes(buf);
    println!("size {sz}");
    let mut buf: Vec<u8> = Vec::new();
    sock.read_to_end(&mut buf)?;
    Ok(())
}
