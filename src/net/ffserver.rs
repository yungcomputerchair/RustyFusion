use polling::{Event, PollMode, Poller};

use crate::Result;
use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{SocketAddr, TcpListener, TcpStream},
    time::Duration,
};

use super::{
    ffclient::{ClientType, FFClient},
    packet::PacketID,
};

const EPOLL_KEY_SELF: usize = 0;

pub struct FFServer {
    poll_timeout: Option<Duration>,
    sock: TcpListener,
    poller: Poller,
    next_epoll_key: usize,
    clients: HashMap<usize, FFClient>,
}

impl FFServer {
    pub fn new(addr: &str, poll_timeout: Option<Duration>) -> Result<Self> {
        let server: Self = Self {
            poll_timeout,
            sock: TcpListener::bind(addr)?,
            poller: Poller::new()?,
            next_epoll_key: EPOLL_KEY_SELF + 1,
            clients: HashMap::new(),
        };
        server.sock.set_nonblocking(true)?;
        server
            .poller
            .add_with_mode(&server.sock, Event::all(EPOLL_KEY_SELF), PollMode::Level)?;
        Ok(server)
    }

    pub fn connect(&mut self, addr: &str, cltype: ClientType) -> &mut FFClient {
        let addr: SocketAddr = addr.parse().expect("Bad address");
        let stream: TcpStream = TcpStream::connect(addr).expect("Failed to connect");
        let conn_data: (TcpStream, SocketAddr) = (stream, addr);
        let key: usize = self
            .register_client(conn_data)
            .expect("Couldn't register client");
        let client: &mut FFClient = self.clients.get_mut(&key).unwrap();
        client.set_client_type(cltype);
        client
    }

    pub fn poll(
        &mut self,
        handler: &dyn Fn(&usize, &mut HashMap<usize, FFClient>, PacketID) -> Result<()>,
    ) -> Result<()> {
        let mut events: Vec<Event> = Vec::new();
        //println!("Waiting...");
        if let Err(e) = self.poller.wait(&mut events, self.poll_timeout) {
            match e.kind() {
                ErrorKind::Interrupted => return Ok(()), // this is fine
                _ => {
                    return Err(Box::new(e));
                }
            }
        }
        for ev in events.iter() {
            //dbg!(ev);
            if ev.key == EPOLL_KEY_SELF {
                let conn_data: (TcpStream, SocketAddr) = self.sock.accept()?;
                println!("New connection from {}", conn_data.1);
                self.register_client(conn_data)?;
            } else {
                if !ev.readable || !ev.writable {
                    continue;
                };
                let clients: &mut HashMap<usize, FFClient> = &mut self.clients;
                let client: &mut FFClient = &mut clients.get_mut(&ev.key).unwrap();
                match client.read_packet() {
                    Ok(pkt) => {
                        handler(&ev.key, clients, pkt)?;
                    }
                    Err(e) => {
                        println!("err on socket {}: {}", ev.key, e);
                        self.unregister_client(ev.key)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_endpoint(&self) -> String {
        self.sock.local_addr().unwrap().to_string()
    }

    fn get_next_epoll_key(&mut self) -> usize {
        let key: usize = self.next_epoll_key;
        self.next_epoll_key += 1;
        key
    }

    fn register_client(&mut self, conn_data: (TcpStream, SocketAddr)) -> Result<usize> {
        let key: usize = self.get_next_epoll_key();
        self.poller
            .add_with_mode(&conn_data.0, Event::all(key), PollMode::Level)?;
        self.clients.insert(key, FFClient::new(conn_data));
        Ok(key)
    }

    fn unregister_client(&mut self, key: usize) -> Result<()> {
        let client: &FFClient = self.clients.get(&key).unwrap();
        match client.get_client_type() {
            ClientType::LoginServer => panic!("Lost connection to login server"),
            _ => {}
        }
        self.poller.delete(client.get_sock())?;
        self.clients.remove(&key);
        Ok(())
    }
}
