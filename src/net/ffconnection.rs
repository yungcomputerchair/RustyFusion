use std::{
    collections::HashMap,
    io::IoSlice,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        Mutex, RwLock,
    },
};

use crate::{
    error::{log, log_error, FFError, FFResult, Severity},
    net::{
        crypto::{self, EncryptionMode, DEFAULT_KEY},
        packet::{
            Packet, PacketID, PACKET_MASK_CL2FE, PACKET_MASK_CL2LS, PACKET_MASK_FE2LS,
            PACKET_MASK_LS2FE,
        },
        ClientType, FFClient, LiveCheckCallback, PacketBuffer, PacketCallback, PACKET_BUFFER_SIZE,
        PACKET_LENGTH_SIZE, SILENCED_PACKETS, UNKNOWN_CT_ALLOWED_PACKETS,
    },
};

pub enum ClientMessage {
    SendPacket(Packet),
    UpdateEncryption {
        new_e_key: Option<u64>,
        new_fe_key: Option<u64>,
        new_mode: Option<EncryptionMode>,
    },
    ClearLiveCheck,
    Shutdown,
}

pub enum ServerMessage {
    ClientDisconnected(usize),
    Shutdown,
}

pub struct FFConnection<S: Send + 'static> {
    key: usize,
    sock: TcpStream,
    in_buf: Arc<PacketBuffer>,
    out_buf: PacketBuffer,
    e_key: u64,
    fe_key: u64,
    enc_mode: EncryptionMode,
    live_check_time: Option<Instant>,
    ignore_packets: bool,
    //
    pkt_handler: PacketCallback<S>,
    live_check: Option<(Duration, LiveCheckCallback)>,
    //
    client: FFClient,
    clients: Arc<RwLock<HashMap<usize, FFClient>>>,
    state: Arc<Mutex<S>>,
}
impl<S: Send + 'static> FFConnection<S> {
    pub fn new(
        key: usize,
        sock: TcpStream,
        client: FFClient,
        pkt_handler: PacketCallback<S>,
        live_check: Option<(Duration, LiveCheckCallback)>,
        clients: Arc<RwLock<HashMap<usize, FFClient>>>,
        state: Arc<Mutex<S>>,
    ) -> Self {
        Self {
            key,
            sock,
            in_buf: Arc::new(PacketBuffer::default()),
            out_buf: PacketBuffer::default(),
            e_key: DEFAULT_KEY,
            fe_key: DEFAULT_KEY,
            enc_mode: EncryptionMode::EKey,
            live_check_time: None,
            ignore_packets: false,
            //
            pkt_handler,
            live_check,
            //
            client,
            clients,
            state,
        }
    }

    pub async fn run(
        &mut self,
        mut rx: UnboundedReceiver<ClientMessage>,
        tx: UnboundedSender<ServerMessage>,
    ) {
        let mut lc_interval = self.live_check.map(|(dur, _)| tokio::time::interval(dur));

        loop {
            enum Event {
                Message(ClientMessage),
                PacketReady(FFResult<Packet>),
                LiveCheck,
            }

            let event = tokio::select! {
                biased;
                Some(msg) = rx.recv() => Event::Message(msg),
                res = self.read_next_packet() => Event::PacketReady(res),
                _ = async { lc_interval.as_mut().unwrap().tick().await }, if lc_interval.is_some() => Event::LiveCheck,
            };

            match event {
                Event::Message(msg) => match msg {
                    ClientMessage::SendPacket(pkt) => {
                        if let Err(e) = self.send_payload(pkt).await {
                            log_error(e);
                            break;
                        }
                    }
                    ClientMessage::ClearLiveCheck => {
                        self.clear_live_check();
                    }
                    ClientMessage::Shutdown => {
                        break;
                    }
                    ClientMessage::UpdateEncryption {
                        new_e_key,
                        new_fe_key,
                        new_mode,
                    } => {
                        if let Some(e_key) = new_e_key {
                            self.e_key = e_key;
                        }
                        if let Some(fe_key) = new_fe_key {
                            self.fe_key = fe_key;
                        }
                        if let Some(enc_mode) = new_mode {
                            self.enc_mode = enc_mode;
                        }
                    }
                },
                Event::PacketReady(Err(e)) => {
                    let should_dc = e.should_dc();
                    log_error(e);
                    if should_dc {
                        break;
                    }
                }
                Event::PacketReady(Ok(pkt)) => {
                    let clients = self.clients.read().await;
                    if let Err(e) =
                        (self.pkt_handler)(pkt, self.key, &clients, self.state.clone()).await
                    {
                        let fatal = e.get_severity() == Severity::Fatal;
                        let should_dc = e.should_dc();
                        log_error(e);

                        if fatal {
                            let _ = tx.send(ServerMessage::Shutdown);
                        }

                        if should_dc {
                            break;
                        }
                    }
                }
                Event::LiveCheck => {
                    if self.supports_live_check() {
                        self.do_live_check();
                    }
                }
            }
        }

        let _ = tx.send(ServerMessage::ClientDisconnected(self.key));
    }

    fn can_send_packet(&self, pkt_id: PacketID) -> bool {
        let pkt_id_raw = pkt_id as u32;
        let meta = self.client.meta.read();
        match meta.client_type {
            ClientType::Unknown => UNKNOWN_CT_ALLOWED_PACKETS.contains(&pkt_id),
            ClientType::UnauthedClient { .. } | ClientType::GameClient { .. } => {
                PACKET_MASK_CL2FE & pkt_id_raw != 0 || PACKET_MASK_CL2LS & pkt_id_raw != 0
            }
            ClientType::LoginServer => PACKET_MASK_LS2FE & pkt_id_raw != 0,
            ClientType::UnauthedShardServer(_) => {
                pkt_id == PacketID::P_FE2LS_REQ_CONNECT
                    || pkt_id == PacketID::P_FE2LS_REQ_LIVE_CHECK
            }
            ClientType::ShardServer(_) => PACKET_MASK_FE2LS & pkt_id_raw != 0,
        }
    }

    pub fn set_ignore_packets(&mut self, ignore: bool) -> FFResult<()> {
        if self.ignore_packets == ignore {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Tried to set ignore_packets to {} when it's already {}",
                    ignore, self.ignore_packets
                ),
            ));
        }

        self.ignore_packets = ignore;
        Ok(())
    }

    fn supports_live_check(&self) -> bool {
        let meta = self.client.meta.read();
        matches!(
            meta.client_type,
            ClientType::GameClient { .. } | ClientType::ShardServer(_) | ClientType::LoginServer
        )
    }

    fn do_live_check(&mut self) {
        let (_, lc_callback) = self.live_check.as_ref().unwrap();
        if self.live_check_time.is_some() {
            log(
                Severity::Info,
                &format!(
                    "Client {} didn't respond to live check; disconnecting",
                    self.client.get_addr()
                ),
            );
            self.client.disconnect();
        } else {
            log(
                Severity::Debug,
                &format!("Sending live check to client {}", self.client.get_addr()),
            );
            lc_callback(&self.client);
            self.live_check_time = Some(Instant::now());
        }
    }

    fn clear_live_check(&mut self) {
        let Some(time_lc) = self.live_check_time.take() else {
            // spurious live check response; ignore
            return;
        };

        let ping_ms = time_lc.elapsed().as_millis() as u64;

        log(
            Severity::Debug,
            &format!(
                "Client {} responded to live check in {} ms",
                self.client.get_addr(),
                ping_ms,
            ),
        );

        let meta = self.client.meta.read();
        if let Some(ping) = meta.ping_ms.as_ref() {
            ping.store(ping_ms, Ordering::Relaxed);
        } else {
            drop(meta);
            let mut meta = self.client.meta.write();
            meta.ping_ms = Some(AtomicU64::new(ping_ms));
        }
    }

    async fn read_next_packet(&mut self) -> FFResult<Packet> {
        // read the size
        let mut sz_buf: [u8; PACKET_LENGTH_SIZE] = [0; PACKET_LENGTH_SIZE];
        self.sock.read_exact(&mut sz_buf).await?;
        let sz = u32::from_le_bytes(sz_buf) as usize;
        if sz > PACKET_BUFFER_SIZE {
            return Err(FFError::build_dc(
                Severity::Warning,
                format!(
                    "Payload bigger than input buffer ({} > {}); disconnecting client",
                    sz, PACKET_BUFFER_SIZE
                ),
            ));
        }

        // reuse the buffer if refcount is 1 (common case),
        // otherwise clone-on-write
        let id = {
            let in_buf = Arc::make_mut(&mut self.in_buf);
            in_buf.reset();
            self.sock.read_exact(&mut in_buf.buf[..sz]).await?;
            in_buf.cursor = sz;
            crypto::decrypt_payload(&mut in_buf.buf[..sz], self.e_key);
            in_buf.peek_packet_id()?
        };

        // discard packet if we're ignoring them for this client,
        // or if the packet ID is not allowed for this client type.
        if self.ignore_packets || !self.can_send_packet(id) {
            return Err(FFError::build(
                Severity::Warning,
                format!("Ignoring {:?} from {:?}", id, self.client.get_client_type()),
            ));
        }

        if !SILENCED_PACKETS.contains(&id) {
            log(
                Severity::Debug,
                &format!("{} sent {:?}", self.client.get_addr(), id),
            );
        }

        // Zero-copy: just bumps the refcount
        let pkt = Packet::_from_arc(Arc::clone(&self.in_buf));
        Ok(pkt)
    }

    async fn flush(&mut self) -> FFResult<()> {
        let sz: usize = self.out_buf.len(); // everything buffered
        self.flush_exact(sz).await
    }

    async fn flush_exact(&mut self, sz: usize) -> FFResult<()> {
        // send the size
        assert!(sz <= PACKET_BUFFER_SIZE);

        // prepare buffers
        let sz_buf: [u8; 4] = u32::to_le_bytes(sz as u32);
        let send_buf = &mut self.out_buf.buf[..sz];

        // encrypt the payload (client decrypts with either E or FE key)
        match self.enc_mode {
            EncryptionMode::EKey => crypto::encrypt_payload(send_buf, self.e_key),
            EncryptionMode::FEKey => crypto::encrypt_payload(send_buf, self.fe_key),
        }

        // send size + payload in a single syscall (writev)
        let mut slices: &mut [IoSlice] = &mut [IoSlice::new(&sz_buf), IoSlice::new(send_buf)];
        let total = sz_buf.len() + sz;
        let mut written = 0;
        while written < total {
            let n = self.sock.write_vectored(slices).await?;
            if n == 0 {
                return Err(FFError::from(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "write_vectored wrote 0 bytes",
                )));
            }
            written += n;
            IoSlice::advance_slices(&mut slices, n);
        }

        self.out_buf.reset();
        Ok(())
    }

    async fn send_payload(&mut self, pkt: Packet) -> FFResult<()> {
        let bytes = pkt.read_bytes();
        self.out_buf.push_bytes(bytes)?;
        self.flush().await
    }
}
