use num_traits::FromPrimitive;
use polling::{Event, PollMode, Poller};

use crate::{
    error::BadPacketID,
    net::{crypto, packet::*},
    util::{get_time, parse_utf16},
    Result, CN_PACKET_BUFFER_SIZE,
};
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    mem::size_of,
    net::{SocketAddr, TcpListener, TcpStream},
    slice::from_raw_parts,
    time::Duration,
};

const EPOLL_KEY_SELF: usize = 0;

pub struct CNServer {
    poll_timeout: Option<Duration>,
    sock: TcpListener,
    poller: Poller,
    events: Vec<Event>,
    next_epoll_key: usize,
    clients: HashMap<usize, (TcpStream, SocketAddr)>,
}

impl CNServer {
    pub fn new(poll_timeout: Option<Duration>) -> Result<Self> {
        let server: Self = Self {
            poll_timeout,
            sock: TcpListener::bind("127.0.0.1:23000")?,
            poller: Poller::new()?,
            events: Vec::new(),
            next_epoll_key: EPOLL_KEY_SELF + 1,
            clients: HashMap::new(),
        };
        server.sock.set_nonblocking(true)?;
        server
            .poller
            .add_with_mode(&server.sock, Event::all(EPOLL_KEY_SELF), PollMode::Edge)?;
        Ok(server)
    }

    pub fn poll(&mut self) -> Result<()> {
        let sock: &mut TcpListener = &mut self.sock;
        let poller: &mut Poller = &mut self.poller;
        self.events.clear();
        //println!("Waiting...");
        let res = poller.wait(&mut self.events, self.poll_timeout);
        if let Err(e) = res {
            match e.kind() {
                ErrorKind::Interrupted => return Ok(()), // this is fine
                _ => {
                    return Err(Box::new(e));
                }
            }
        }
        for ev in &self.events {
            //dbg!(ev);
            if ev.key == EPOLL_KEY_SELF {
                let conn_data: (TcpStream, SocketAddr) = sock.accept()?;
                println!("New connection from {}", conn_data.1);
                let new_sock_key: usize = self.next_epoll_key;
                self.next_epoll_key += 1;
                self.clients.insert(new_sock_key, conn_data);
                let new_sock: &TcpStream = &self.clients.get(&new_sock_key).unwrap().0;
                poller.add_with_mode(new_sock, Event::all(new_sock_key), PollMode::Edge)?;
            } else {
                let sock: &mut TcpStream = &mut self.clients.get_mut(&ev.key).unwrap().0;
                if !ev.readable || !ev.writable {
                    continue;
                };
                if let Err(e) = sock_read(sock) {
                    println!("err {e}");
                    poller.delete(&*sock)?;
                    self.clients.remove(&ev.key);
                }
            }
        }
        Ok(())
    }
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
    let id: PacketID = match PacketID::from_u32(id) {
        Some(id) => id,
        None => {
            return Err(Box::new(BadPacketID::new(id)));
        }
    };
    println!("{:?}", id);

    let pack: &sP_CL2LS_REQ_LOGIN = unsafe { bytes_to_struct(&body[4..]) };
    println!(
        "login request from {} ({})",
        parse_utf16(&pack.szID),
        parse_utf16(&pack.szPassword)
    );

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
