use std::{
    fmt::Display,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

// We use parking_lot's RwLock instead of std's because it's more efficient and has a simpler API.
// On top of that, client metadata needs to be accessed from pure-sync context like the TUI,
// so we can't use tokio's async RwLock.
use parking_lot::RwLock;

use tokio::sync::mpsc::UnboundedSender;

use crate::{
    error::{log, panic_if_failed, FFError, FFResult, Severity},
    net::{
        packet::{FFPacket, Packet, PacketID},
        ClientMessage,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientType {
    Unknown,
    UnauthedClient {
        username: String,
        dup_pc_uid: Option<i64>,
    },
    GameClient {
        account_id: i64,
        serial_key: i64,    // iEnterSerialKey
        pc_id: Option<i32>, // iPC_ID
    },
    LoginServer,
    UnauthedShardServer(Vec<u8>), // auth challenge
    ShardServer(i32),             // shard ID
}
impl Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientType::Unknown => write!(f, "Unknown"),
            ClientType::UnauthedClient { username, .. } => {
                write!(f, "UnauthedClient({})", username)
            }
            ClientType::GameClient { account_id, .. } => write!(f, "GameClient({})", account_id),
            ClientType::LoginServer => write!(f, "LoginServer"),
            ClientType::UnauthedShardServer(_) => write!(f, "UnauthedShardServer"),
            ClientType::ShardServer(shard_id) => write!(f, "ShardServer({})", shard_id),
        }
    }
}

#[derive(Debug)]
pub struct ClientMetadata {
    pub addr: SocketAddr,
    pub client_type: ClientType,
    pub ping_ms: Option<AtomicU64>,
    pub live_check_time: Option<Instant>,
}
impl ClientMetadata {
    pub fn new(addr: SocketAddr, client_type: Option<ClientType>) -> Self {
        Self {
            addr,
            client_type: client_type.unwrap_or(ClientType::Unknown),
            ping_ms: None,
            live_check_time: None,
        }
    }
}

/// Handle to a connected client.
/// This is cheap to clone and safe to send across threads.
#[derive(Clone)]
pub struct FFClient {
    tx: UnboundedSender<ClientMessage>,
    pub meta: Arc<RwLock<ClientMetadata>>,
}
impl FFClient {
    pub fn new(tx: UnboundedSender<ClientMessage>, meta: ClientMetadata) -> Self {
        Self {
            tx,
            meta: Arc::new(RwLock::new(meta)),
        }
    }

    pub fn send_packet<T: FFPacket>(&self, pkt_id: PacketID, pkt: &T) {
        // It should be impossible for a single packet struct to be
        // bigger than the packet buffer, so fail fast if that happens.
        let pkt = panic_if_failed(Packet::new(pkt_id, pkt));
        self.send_payload(pkt);
    }

    pub fn send_payload(&self, pkt: Packet) {
        // it's okay to silently fail; if the channel is closed,
        // the client has already disconnected
        let _ = self.tx.send(ClientMessage::SendPacket(pkt));
    }

    pub fn disconnect(&self) {
        let _ = self.tx.send(ClientMessage::Shutdown);
        let mut meta = self.meta.write();
        meta.client_type = ClientType::Unknown;
    }

    pub fn get_ip(&self) -> IpAddr {
        let meta = self.meta.read();
        meta.addr.ip()
    }

    pub fn get_addr(&self) -> String {
        let meta = self.meta.read();
        meta.addr.to_string()
    }

    pub fn get_client_type(&self) -> ClientType {
        let meta = self.meta.read();
        meta.client_type.clone()
    }

    pub fn get_account_id(&self) -> FFResult<i64> {
        let meta = self.meta.read();
        if let ClientType::GameClient { account_id, .. } = meta.client_type {
            Ok(account_id)
        } else {
            Err(FFError::build(
                Severity::Warning,
                "Couldn't get account ID for client".to_string(),
            ))
        }
    }

    pub fn get_player_id(&self) -> FFResult<i32> {
        let meta = self.meta.read();
        if let ClientType::GameClient {
            pc_id: Some(pc_id), ..
        } = meta.client_type
        {
            Ok(pc_id)
        } else {
            Err(FFError::build(
                Severity::Warning,
                "Couldn't get player ID for client".to_string(),
            ))
        }
    }

    pub fn get_shard_id(&self) -> FFResult<i32> {
        let meta = self.meta.read();
        if let ClientType::ShardServer(shard_id) = meta.client_type {
            Ok(shard_id)
        } else {
            Err(FFError::build(
                Severity::Warning,
                "Couldn't get shard ID for client".to_string(),
            ))
        }
    }

    pub fn clear_player_id(&mut self) -> FFResult<i32> {
        let pc_id = self.get_player_id()?;
        let mut meta = self.meta.write();
        if let ClientType::GameClient { pc_id, .. } = &mut meta.client_type {
            *pc_id = None;
        }
        Ok(pc_id)
    }

    pub fn get_serial_key(&self) -> FFResult<i64> {
        let meta = self.meta.read();
        if let ClientType::GameClient { serial_key, .. } = meta.client_type {
            Ok(serial_key)
        } else {
            Err(FFError::build(
                Severity::Warning,
                "Couldn't get serial key for client".to_string(),
            ))
        }
    }

    pub fn clear_live_check(&self) {
        let meta = self.meta.read();
        let Some(time_lc) = meta.live_check_time else {
            // spurious live check response; ignore
            return;
        };
        drop(meta);

        let time_now = Instant::now();
        let ping = time_now.duration_since(time_lc);
        let ping_ms = ping.as_millis() as u64;

        log(
            Severity::Debug,
            &format!(
                "Client {} responded to live check in {} ms",
                self.get_addr(),
                ping_ms,
            ),
        );

        let mut meta = self.meta.write();
        meta.live_check_time = None;

        if let Some(ping) = meta.ping_ms.as_ref() {
            ping.store(ping_ms, Ordering::Relaxed);
        } else {
            meta.ping_ms = Some(AtomicU64::new(ping_ms));
        }
    }
}
