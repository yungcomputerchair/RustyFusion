use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
};

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use crate::{
    error::{log, FFError, FFResult, Severity},
    net::{ClientMetadata, FFConnection, ServerMessage},
    state::ServerState,
};

use super::{ClientType, DisconnectCallback, FFClient, LiveCheckCallback, PacketCallback};

pub struct FFServer {
    sock: TcpListener,
    next_client_key: usize,
    pkt_handler: PacketCallback,
    dc_handler: Option<DisconnectCallback>,
    live_check: Option<(Duration, LiveCheckCallback)>,
    clients: Arc<RwLock<HashMap<usize, FFClient>>>,
    state: Arc<Mutex<ServerState>>,
    event_tx: UnboundedSender<ServerMessage>,
    event_rx: UnboundedReceiver<ServerMessage>,
}

impl FFServer {
    pub async fn new(
        addr: SocketAddr,
        pkt_handler: PacketCallback,
        dc_handler: Option<DisconnectCallback>,
        live_check: Option<(Duration, LiveCheckCallback)>,
        state: Arc<Mutex<ServerState>>,
    ) -> FFResult<Self> {
        let sock = TcpListener::bind(addr).await?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            sock,
            next_client_key: 1,
            pkt_handler,
            dc_handler,
            live_check,
            clients: Arc::new(RwLock::new(HashMap::new())),
            state,
            event_tx,
            event_rx,
        })
    }

    pub async fn connect(&mut self, addr: SocketAddr, cltype: ClientType) -> Option<FFClient> {
        let stream = TcpStream::connect(addr).await.ok()?;
        let conn_data = (stream, addr);
        let key = self.register_client(conn_data, Some(cltype)).await.ok()?;
        let clients = self.clients.read().await;
        clients.get(&key).cloned()
    }

    pub async fn poll(&mut self) -> FFResult<()> {
        tokio::select! {
            result = self.sock.accept() => {
                let conn_data = result?;

                log(
                    Severity::Debug,
                    &format!("New connection from {}", conn_data.1),
                );

                self.register_client(conn_data, None).await?;
                Ok(())
            }
            Some(event) = self.event_rx.recv() => {
                match event {
                    ServerMessage::ClientDisconnected(key) => {
                        let client = self.disconnect_client(key).await?;

                        log(
                            Severity::Debug,
                            &format!("Client {} disconnected", client.get_addr()),
                        );
                    }
                }
                Ok(())
            }
        }
    }

    pub fn get_endpoint(&self) -> String {
        self.sock.local_addr().unwrap().to_string()
    }

    pub async fn get_clients(&self) -> RwLockReadGuard<'_, HashMap<usize, FFClient>> {
        self.clients.read().await
    }

    pub async fn get_clients_mut(&self) -> RwLockWriteGuard<'_, HashMap<usize, FFClient>> {
        self.clients.write().await
    }

    pub async fn disconnect_client(&mut self, key: usize) -> FFResult<FFClient> {
        if let Some(callback) = self.dc_handler {
            let clients = self.clients.read().await;
            let mut state = self.state.lock().await;
            callback(key, &clients, &mut state);
        };

        self.unregister_client(key).await
    }

    fn get_next_client_key(&mut self) -> usize {
        let key = self.next_client_key;
        self.next_client_key += 1;
        key
    }

    async fn register_client(
        &mut self,
        conn_data: (TcpStream, SocketAddr),
        client_type: Option<ClientType>,
    ) -> FFResult<usize> {
        let (sock, addr) = conn_data;
        if let Err(e) = sock.set_nodelay(true) {
            log(
                Severity::Debug,
                &format!("Failed to set TCP_NODELAY for {}: {}", addr, e),
            );
        }

        let meta = ClientMetadata::new(addr, client_type);
        let (tx, rx) = mpsc::unbounded_channel();
        let client = FFClient::new(tx, meta);

        let key: usize = self.get_next_client_key();
        {
            let mut clients = self.clients.write().await;
            clients.insert(key, client.clone());
        }

        let mut conn = FFConnection::new(
            key,
            sock,
            client,
            self.pkt_handler,
            self.live_check,
            self.clients.clone(),
            self.state.clone(),
        );

        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            conn.run(rx).await;
            let _ = event_tx.send(ServerMessage::ClientDisconnected(key));
        });

        Ok(key)
    }

    async fn unregister_client(&mut self, key: usize) -> FFResult<FFClient> {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.remove(&key) {
            Ok(client)
        } else {
            Err(FFError::build(
                Severity::Warning,
                format!(
                    "Attempted to unregister non-existent client with key {}",
                    key
                ),
            ))
        }
    }
}
