use polling::{Event, PollMode, Poller};

use std::{
    collections::HashMap,
    io::{ErrorKind, Result},
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, SystemTime},
};

use crate::{
    error::{log, log_if_failed, FFError, Severity},
    state::ServerState,
};

use super::{
    ClientMap, ClientType, DisconnectCallback, FFClient, LiveCheckCallback, PacketCallback,
};

const EPOLL_KEY_SELF: usize = 0;

pub struct FFServer {
    poll_timeout: Option<Duration>,
    sock: TcpListener,
    poller: Poller,
    next_epoll_key: usize,
    pkt_handler: PacketCallback,
    dc_handler: Option<DisconnectCallback>,
    live_check_handler: Option<LiveCheckCallback>,
    clients: HashMap<usize, FFClient>,
}

impl FFServer {
    pub fn new(
        addr: &str,
        pkt_handler: PacketCallback,
        dc_handler: Option<DisconnectCallback>,
        live_check_handler: Option<LiveCheckCallback>,
        poll_timeout: Option<Duration>,
    ) -> Result<Self> {
        let server: Self = Self {
            poll_timeout,
            sock: TcpListener::bind(addr)?,
            poller: Poller::new()?,
            next_epoll_key: EPOLL_KEY_SELF + 1,
            pkt_handler,
            dc_handler,
            live_check_handler,
            clients: HashMap::new(),
        };
        server.sock.set_nonblocking(true)?;
        server.poller.add_with_mode(
            &server.sock,
            Event::readable(EPOLL_KEY_SELF),
            PollMode::Level,
        )?;
        Ok(server)
    }

    pub fn connect(&mut self, addr: &str, cltype: ClientType) -> Option<&mut FFClient> {
        let addr: SocketAddr = addr.parse().expect("Bad address");
        let stream = TcpStream::connect(addr);
        if let Ok(stream) = stream {
            let conn_data: (TcpStream, SocketAddr) = (stream, addr);
            let key: usize = self.register_client(conn_data).unwrap();
            let client: &mut FFClient = self.clients.get_mut(&key).unwrap();
            client.client_type = cltype;
            return Some(client);
        }
        None
    }

    pub fn poll(&mut self, state: &mut ServerState, live_check_interval: Duration) -> Result<()> {
        let time_now = SystemTime::now();
        let client_keys: Vec<usize> = self.clients.keys().copied().collect();
        for key in client_keys {
            let client = self.clients.get_mut(&key).unwrap();
            // live check
            if let Some(lc_callback) = self.live_check_handler {
                match client.live_check_time {
                    Some(dc_time) => {
                        if dc_time < time_now {
                            log(
                                Severity::Info,
                                &format!(
                                    "Client {} didn't respond to live check; disconnecting",
                                    client.get_addr()
                                ),
                            );
                            client.disconnect();
                        }
                    }
                    None => {
                        let time_since_last_heartbeat =
                            time_now.duration_since(client.last_heartbeat).unwrap();
                        if time_since_last_heartbeat > live_check_interval {
                            log(
                                Severity::Debug,
                                &format!("Sending live check to client {}", client.get_addr()),
                            );
                            log_if_failed(lc_callback(client));
                            client.live_check_time = Some(time_now + live_check_interval);
                        }
                    }
                }
            }

            if client.should_dc() {
                self.disconnect_client(key, state)?;
            }
        }

        let mut events: Vec<Event> = Vec::new();
        if let Err(e) = self.poller.wait(&mut events, self.poll_timeout) {
            match e.kind() {
                ErrorKind::Interrupted => return Ok(()), // this is fine
                _ => {
                    return Err(e);
                }
            }
        }

        for ev in events.iter() {
            if ev.key == EPOLL_KEY_SELF {
                let conn_data: (TcpStream, SocketAddr) = self.sock.accept()?;
                log(
                    Severity::Debug,
                    &format!("New connection from {}", conn_data.1),
                );
                self.register_client(conn_data)?;
            } else {
                let client = match self.clients.get_mut(&ev.key) {
                    Some(client) => client,
                    None => {
                        continue;
                    }
                };
                let addr = client.get_addr();

                let res = (|clients: &mut HashMap<usize, FFClient>| {
                    let client = clients.get_mut(&ev.key).unwrap();
                    client.read_payload()?;
                    let pkt_id = client.peek_packet_id()?;
                    (self.pkt_handler)(ev.key, clients, pkt_id, state, time_now).map_err(|e| {
                        FFError::build(e.get_severity(), format!("<{:?}> {}", pkt_id, e.get_msg()))
                    })
                })(&mut self.clients);

                if let Err(e) = res {
                    log(e.get_severity(), &format!("{} ({})", e.get_msg(), addr));
                    if e.should_dc() {
                        self.disconnect_client(ev.key, state)?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_endpoint(&self) -> String {
        self.sock.local_addr().unwrap().to_string()
    }

    pub fn get_clients(&mut self) -> impl Iterator<Item = (&usize, &mut FFClient)> + '_ {
        self.clients.iter_mut()
    }

    pub fn get_client_map(&mut self) -> ClientMap {
        ClientMap::new(0, &mut self.clients)
    }

    pub fn disconnect_client(&mut self, client_key: usize, state: &mut ServerState) -> Result<()> {
        if let Some(callback) = self.dc_handler {
            callback(client_key, &mut self.clients, state);
        };
        self.unregister_client(client_key)
    }

    fn get_next_epoll_key(&mut self) -> usize {
        let key: usize = self.next_epoll_key;
        self.next_epoll_key += 1;
        key
    }

    fn register_client(&mut self, conn_data: (TcpStream, SocketAddr)) -> Result<usize> {
        let key: usize = self.get_next_epoll_key();
        self.poller
            .add_with_mode(&conn_data.0, Event::readable(key), PollMode::Level)?;
        self.clients.insert(key, FFClient::new(conn_data));
        Ok(key)
    }

    fn unregister_client(&mut self, key: usize) -> Result<()> {
        let client = self.clients.remove(&key).unwrap();
        self.poller.delete(&client.sock)?;
        Ok(()) // client is dropped
    }
}
