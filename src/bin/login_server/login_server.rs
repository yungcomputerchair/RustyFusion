use std::{time::Duration, net::{TcpListener, TcpStream, SocketAddr}, collections::HashMap};
use polling::{Poller, Event, PollMode};
use rusty_fusion::{Result, net::cnserver::{CNServer, sock_read}};

const EPOLL_KEY_SELF: usize = 0;

pub struct LoginServer {
    poll_timeout: Option<Duration>,
    sock: TcpListener,
    poller: Poller,
    events: Vec<Event>,
    next_epoll_key: usize,
    clients: HashMap<usize, (TcpStream, SocketAddr)>,
}

impl CNServer for LoginServer {
    fn new(poll_timeout: Option<Duration>) -> Result<LoginServer> {
        let ls: LoginServer = LoginServer {
            poll_timeout,
            sock: TcpListener::bind("127.0.0.1:23000")?,
            poller: Poller::new()?,
            events: Vec::new(),
            next_epoll_key: EPOLL_KEY_SELF + 1,
            clients: HashMap::new()
        };
        ls.sock.set_nonblocking(true)?;
        ls.poller.add_with_mode(
            &ls.sock, Event::all(EPOLL_KEY_SELF), PollMode::Edge)?;
        Ok(ls)
    }

    fn poll(&mut self) -> Result<()> {
        let sock: &mut TcpListener = &mut self.sock;
        let poller: &mut Poller = &mut self.poller;
        self.events.clear();
        //println!("Waiting...")
        poller.wait(&mut self.events, self.poll_timeout)?;
        for ev in &self.events {
            //dbg!(ev);
            if ev.key == EPOLL_KEY_SELF {
                let conn_data: (TcpStream, SocketAddr) = sock.accept()?;
                println!("New connection from {}", conn_data.1);
                let new_sock_key: usize = self.next_epoll_key;
                self.next_epoll_key += 1;
                self.clients.insert(new_sock_key, conn_data);
                let new_sock: &TcpStream = &self.clients.get(&new_sock_key).unwrap().0;
                poller.add_with_mode(
                    new_sock, Event::all(new_sock_key), PollMode::Edge)?;
            } else {
                let sock: &mut TcpStream = &mut self.clients.get_mut(&ev.key).unwrap().0;
                if !ev.readable { continue };
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