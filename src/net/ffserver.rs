use polling::{Event, PollMode, Poller};
use tokio::sync::Mutex;

use std::{
    collections::HashMap,
    io::{ErrorKind, Result},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    error::{log, log_if_failed, FFError, Severity},
    state::ServerState,
};

use super::{
    ClientMap, ClientType, DisconnectCallback, FFClient, FFClientHandle, LiveCheckCallback,
    PacketCallback,
};

const EPOLL_KEY_SELF: usize = 0;

pub struct FFServer {
    poll_timeout: Option<Duration>,
    sock: TcpListener,
    poller: Poller,
    next_epoll_key: usize,
    pkt_handler: PacketCallback,
    dc_handler: Option<DisconnectCallback>,
    live_check: Option<(Duration, LiveCheckCallback)>,
    clients: HashMap<usize, FFClient>,
    handles: HashMap<usize, FFClientHandle>,
}

impl FFServer {
    pub fn new(
        addr: SocketAddr,
        pkt_handler: PacketCallback,
        dc_handler: Option<DisconnectCallback>,
        live_check: Option<(Duration, LiveCheckCallback)>,
        poll_timeout: Option<Duration>,
    ) -> Result<Self> {
        let server: Self = Self {
            poll_timeout,
            sock: TcpListener::bind(addr)?,
            poller: Poller::new()?,
            next_epoll_key: EPOLL_KEY_SELF + 1,
            pkt_handler,
            dc_handler,
            live_check,
            clients: HashMap::new(),
            handles: HashMap::new(),
        };
        server.sock.set_nonblocking(true)?;
        server.poller.add_with_mode(
            &server.sock,
            Event::readable(EPOLL_KEY_SELF),
            PollMode::Level,
        )?;
        Ok(server)
    }

    pub fn connect(&mut self, addr: SocketAddr, cltype: ClientType) -> Option<&mut FFClient> {
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

    pub async fn poll(&mut self, state: &Arc<Mutex<ServerState>>) -> Result<()> {
        let time_now = Instant::now();
        let client_keys: Vec<usize> = self.clients.keys().copied().collect();
        for key in client_keys {
            let client = self.clients.get_mut(&key).unwrap();

            // live check
            if client.supports_live_check() {
                if let Some((lc_interval, lc_callback)) = self.live_check {
                    match client.live_check_time {
                        Some(live_check_time) => {
                            let dc_time = live_check_time + lc_interval;
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
                            let time_since_last_ping =
                                time_now.duration_since(client.last_ping_time);
                            if client.ping.is_none() || time_since_last_ping > lc_interval {
                                log(
                                    Severity::Debug,
                                    &format!("Sending live check to client {}", client.get_addr()),
                                );
                                log_if_failed(lc_callback(client));
                                client.live_check_time = Some(time_now);
                            }
                        }
                    }
                }
            }

            if client.should_dc() {
                let mut state = state.lock().await;
                self.disconnect_client(key, &mut state)?;
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
                let mut state = state.lock().await;
                let client = match self.clients.get_mut(&ev.key) {
                    Some(client) => client,
                    None => {
                        continue; // client was disconnected
                    }
                };
                let addr = client.get_addr();

                let pkt_handler = self.pkt_handler;
                let res = (|clients: &mut HashMap<usize, FFClient>,
                            handles: &HashMap<usize, FFClientHandle>,
                            state: &mut ServerState| {
                    let client = clients.get_mut(&ev.key).unwrap();
                    client.read_payload()?;
                    let pkt_id = client.peek_packet_id()?;
                    pkt_handler(ev.key, clients, handles, pkt_id, state).map_err(|e| {
                        FFError::build(e.get_severity(), format!("<{:?}> {}", pkt_id, e.get_msg()))
                    })
                })(&mut self.clients, &self.handles, &mut state);

                if let Err(e) = res {
                    log(e.get_severity(), &format!("{} ({})", e.get_msg(), addr));
                    if e.should_dc() {
                        self.disconnect_client(ev.key, &mut state)?;
                    }
                }
            }
        }

        // Drain pending channel messages for all clients
        for client in self.clients.values_mut() {
            client.drain_messages();
        }

        Ok(())
    }

    pub fn get_endpoint(&self) -> String {
        self.sock.local_addr().unwrap().to_string()
    }

    pub fn get_clients(&mut self) -> &mut HashMap<usize, FFClient> {
        &mut self.clients
    }

    pub fn get_handles(&self) -> &HashMap<usize, FFClientHandle> {
        &self.handles
    }

    pub fn get_client_map(&mut self) -> ClientMap<'_> {
        ClientMap::new(0, &mut self.clients, &self.handles)
    }

    pub fn disconnect_client(&mut self, client_key: usize, state: &mut ServerState) -> Result<()> {
        if let Some(callback) = self.dc_handler {
            callback(client_key, &mut self.clients, &self.handles, state);
        };
        self.unregister_client(client_key)
    }

    fn get_next_epoll_key(&mut self) -> usize {
        let key: usize = self.next_epoll_key;
        self.next_epoll_key += 1;
        key
    }

    fn register_client(&mut self, conn_data: (TcpStream, SocketAddr)) -> Result<usize> {
        log_if_failed(conn_data.0.set_nodelay(true).map_err(|e| {
            FFError::build(Severity::Debug, format!("Failed to set TCP_NODELAY: {}", e))
        }));

        let key: usize = self.get_next_epoll_key();
        self.poller
            .add_with_mode(&conn_data.0, Event::readable(key), PollMode::Level)?;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.clients.insert(key, FFClient::new(conn_data, rx));
        self.handles.insert(key, FFClientHandle::new(tx));
        Ok(key)
    }

    fn unregister_client(&mut self, key: usize) -> Result<()> {
        let client = self.clients.remove(&key).unwrap();
        self.handles.remove(&key);
        self.poller.delete(&client.sock)?;
        Ok(()) // client is dropped
    }
}
