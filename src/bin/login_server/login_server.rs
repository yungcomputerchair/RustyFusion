use std::{time::Duration, net::{TcpListener, TcpStream, SocketAddr}, collections::HashMap};
use polling::{Poller, Event, PollMode};
use rusty_fusion::{Result, net::cnserver::{CNServer, sock_read}};

const EPOLL_KEY_SELF: usize = 0;

pub struct LoginServer {
    poll_timeout: Option<Duration>,
    sock: Option<TcpListener>,
    poller: Option<Poller>,
    events: Vec<Event>,
    next_epoll_key: usize,
    clients: HashMap<usize, (TcpStream, SocketAddr)>,
}

impl LoginServer {
    pub fn new(poll_timeout: Option<Duration>) -> LoginServer {
        LoginServer {
            poll_timeout,
            sock: None,
            poller: None,
            events: Vec::new(),
            next_epoll_key: EPOLL_KEY_SELF + 1,
            clients: HashMap::new()
        }
    }
}

impl CNServer for LoginServer {
    fn init(&mut self) -> Result<()> {
        self.sock = Some(TcpListener::bind("127.0.0.1:23000")?);
        self.sock.as_mut().unwrap().set_nonblocking(true)?;
        self.poller = Some(Poller::new()?);
        self.poller.as_mut().unwrap().add_with_mode(
            self.sock.as_ref().unwrap(), Event::all(EPOLL_KEY_SELF), PollMode::Edge)?;
        Ok(())
    }

    fn poll(&mut self) -> Result<()> {
        self.events.clear();
        self.poller.as_mut().unwrap().wait(&mut self.events, self.poll_timeout)?;
        for ev in &self.events {
            if ev.key == EPOLL_KEY_SELF {
                let sock: (TcpStream, SocketAddr) = self.sock.as_mut().unwrap().accept()?;
                println!("New connection from {}", sock.1);
                self.clients.insert(self.next_epoll_key, sock);
                let sock: &TcpStream = &self.clients.get(&self.next_epoll_key).unwrap().0;
                self.poller.as_mut().unwrap().add_with_mode(
                    sock, Event::all(self.next_epoll_key), PollMode::Edge)?;
                self.next_epoll_key += 1;
            } else {
                let sock: &mut TcpStream = &mut self.clients.get_mut(&ev.key).unwrap().0;
                if let Err(e) = sock_read(sock) {
                    println!("err {e}");
                }
            }
        }
        Ok(())
    }
}