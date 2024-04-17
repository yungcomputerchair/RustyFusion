use std::{
    io::{Read, Write},
    mem::size_of,
    net::{SocketAddr, TcpStream},
    time::SystemTime,
};

use crate::{
    error::{log, panic_log, FFError, FFResult, Severity},
    net::{struct_to_bytes, PACKET_BUFFER_SIZE, SILENCED_PACKETS},
};

use super::{
    bytes_to_struct,
    crypto::{
        decrypt_payload, encrypt_payload, AuthChallenge, EncryptionMode, CRYPTO_KEY_SIZE,
        DEFAULT_KEY,
    },
    packet::{
        FFPacket, PacketID, PACKET_MASK_CL2FE, PACKET_MASK_CL2LS, PACKET_MASK_FE2LS,
        PACKET_MASK_LS2FE,
    },
    UNKNOWN_CT_ALLOWED_PACKETS,
};

#[derive(Debug, Clone)]
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
    UnauthedShardServer(Box<AuthChallenge>),
    ShardServer(i32),
}

pub struct FFClient {
    pub sock: TcpStream,
    addr: SocketAddr,
    waiting_data_len: Option<usize>,
    in_buf: [u8; PACKET_BUFFER_SIZE],
    in_buf_ptr: usize,
    in_data_len: usize,
    out_buf: [u8; PACKET_BUFFER_SIZE],
    out_buf_ptr: usize,
    pub e_key: [u8; CRYPTO_KEY_SIZE],
    pub fe_key: [u8; CRYPTO_KEY_SIZE],
    pub enc_mode: EncryptionMode,
    pub client_type: ClientType,
    pub last_heartbeat: SystemTime,
    pub live_check_time: Option<SystemTime>,
    should_dc: bool,
    ignore_packets: bool,
}

impl FFClient {
    pub fn new(conn_data: (TcpStream, SocketAddr)) -> Self {
        let default_key: [u8; CRYPTO_KEY_SIZE] = DEFAULT_KEY.try_into().unwrap();
        Self {
            sock: conn_data.0,
            addr: conn_data.1,
            waiting_data_len: None,
            in_buf: [0; PACKET_BUFFER_SIZE],
            in_buf_ptr: 0,
            in_data_len: 0,
            out_buf: [0; PACKET_BUFFER_SIZE],
            out_buf_ptr: 0,
            e_key: default_key,
            fe_key: default_key,
            enc_mode: EncryptionMode::EKey,
            client_type: ClientType::Unknown,
            last_heartbeat: SystemTime::now(),
            live_check_time: None,
            should_dc: false,
            ignore_packets: false,
        }
    }

    pub fn should_dc(&self) -> bool {
        self.should_dc
    }

    pub fn disconnect(&mut self) {
        self.should_dc = true;
    }

    pub fn get_addr(&self) -> String {
        self.addr.to_string()
    }

    pub fn get_fe_key_uint(&self) -> u64 {
        u64::from_le_bytes(self.fe_key)
    }

    pub fn get_account_id(&self) -> FFResult<i64> {
        if let ClientType::GameClient { account_id, .. } = self.client_type {
            Ok(account_id)
        } else {
            Err(FFError::build(
                Severity::Warning,
                "Couldn't get account ID for client".to_string(),
            ))
        }
    }

    pub fn get_player_id(&self) -> FFResult<i32> {
        if let ClientType::GameClient {
            pc_id: Some(pc_id), ..
        } = self.client_type
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
        if let ClientType::ShardServer(shard_id) = self.client_type {
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
        if let ClientType::GameClient { pc_id, .. } = &mut self.client_type {
            *pc_id = None;
        }
        Ok(pc_id)
    }

    pub fn can_send_packet(&self, pkt_id: PacketID) -> bool {
        let pkt_id_raw = pkt_id as u32;
        match self.client_type {
            ClientType::Unknown => UNKNOWN_CT_ALLOWED_PACKETS.contains(&pkt_id),
            ClientType::UnauthedClient { .. } | ClientType::GameClient { .. } => {
                PACKET_MASK_CL2FE & pkt_id_raw != 0 || PACKET_MASK_CL2LS & pkt_id_raw != 0
            }
            ClientType::LoginServer => PACKET_MASK_LS2FE & pkt_id_raw != 0,
            ClientType::UnauthedShardServer(_) => pkt_id == PacketID::P_FE2LS_REQ_CONNECT,
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

    pub fn peek_packet_id(&self) -> FFResult<PacketID> {
        let from = self.in_buf_ptr;
        let to = from + 4;
        if to > self.in_data_len {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Couldn't peek packet ID; not enough bytes came in: {} > {}",
                    to, self.in_data_len
                ),
            ));
        }

        let id_ord = u32::from_le_bytes(self.in_buf[from..to].try_into().unwrap());
        let id: FFResult<PacketID> = id_ord.try_into();
        id.map_err(|_| FFError::build(Severity::Warning, format!("Bad packet ID: {}", id_ord)))
    }

    pub fn get_packet<T: FFPacket>(&mut self, pkt_id: PacketID) -> FFResult<&T> {
        let buffered_pkt_id = self.peek_packet_id()?;
        assert_eq!(
            buffered_pkt_id, pkt_id,
            "Tried to fetch packet {:?} != buffered {:?}",
            pkt_id, buffered_pkt_id
        );
        self.in_buf_ptr += 4;
        let pkt = self.get_struct_internal(!SILENCED_PACKETS.contains(&pkt_id))?;
        Ok(pkt)
    }

    pub fn get_struct<T: FFPacket>(&mut self) -> FFResult<&T> {
        self.get_struct_internal(true)
    }

    fn get_struct_internal<T: FFPacket>(&mut self, log_struct: bool) -> FFResult<&T> {
        let sz: usize = size_of::<T>();
        let from = self.in_buf_ptr;
        let to = from + sz;
        if to > self.in_data_len {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Bad struct read; not enough bytes came in: {} < {}",
                    self.in_data_len, to
                ),
            ));
        }

        let buf: &[u8] = &self.in_buf[from..to];
        let s = unsafe { bytes_to_struct(buf) };
        self.in_buf_ptr += sz;

        if log_struct {
            log(Severity::Debug, &format!("{:#?}", s));
        }

        Ok(s)
    }

    pub fn read_payload(&mut self) -> FFResult<()> {
        self.last_heartbeat = SystemTime::now();
        self.live_check_time = None;

        if self.waiting_data_len.is_none() {
            // read the size
            let mut sz_buf: [u8; 4] = [0; 4];
            self.sock
                .read_exact(&mut sz_buf)
                .map_err(FFError::from_io_err)?;
            let sz: usize = u32::from_le_bytes(sz_buf) as usize;
            self.waiting_data_len = Some(sz);
        }

        let sz = self.waiting_data_len.unwrap();
        if sz > PACKET_BUFFER_SIZE {
            return Err(FFError::build_dc(
                Severity::Warning,
                format!(
                    "Payload bigger than input buffer ({} > {}); disconnecting client",
                    sz, PACKET_BUFFER_SIZE
                ),
            ));
        }

        // read the packet
        let buf: &mut [u8] = &mut self.in_buf[..sz];
        self.sock.read_exact(buf).map_err(FFError::from_io_err)?;
        self.waiting_data_len = None;
        self.in_buf_ptr = 0;
        self.in_data_len = sz;

        // decrypt the packet (client always encrypts with E key)
        decrypt_payload(buf, &self.e_key);

        let id = self.peek_packet_id()?;

        // discard packet if we're ignoring them for this client,
        // or if the packet ID is not allowed for this client type.
        // we need to set the data length to 0 to "empty" the buffer.
        // we also need to return an error so the caller doesn't fire the packet handler.
        if self.ignore_packets || !self.can_send_packet(id) {
            self.in_data_len = 0;
            return Err(FFError::build(
                Severity::Warning,
                format!("Ignoring {:?} from {:?}", id, self.client_type),
            ));
        }

        if !SILENCED_PACKETS.contains(&id) {
            log(
                Severity::Debug,
                &format!("{} sent {:?}", self.get_addr(), id),
            );
        }
        Ok(())
    }

    pub fn flush(&mut self) -> FFResult<()> {
        let sz: usize = self.out_buf_ptr; // everything buffered
        self.flush_exact(sz)
    }

    pub fn flush_exact(&mut self, sz: usize) -> FFResult<()> {
        // send the size
        assert!(sz <= PACKET_BUFFER_SIZE);

        // send the size unencrypted
        let sz_buf: [u8; 4] = u32::to_le_bytes(sz as u32);
        self.sock.write_all(&sz_buf).map_err(FFError::from_io_err)?;

        let send_buf = &mut self.out_buf[..sz];

        // encrypt the payload (client decrypts with either E or FE key)
        match self.enc_mode {
            EncryptionMode::EKey => encrypt_payload(send_buf, &self.e_key),
            EncryptionMode::FEKey => encrypt_payload(send_buf, &self.fe_key),
        }

        // send the payload
        self.sock
            .write_all(send_buf)
            .map_err(FFError::from_io_err)?;

        self.out_buf.fill(0);
        self.out_buf_ptr = 0;
        Ok(())
    }

    pub fn send_packet<T: FFPacket>(&mut self, pkt_id: PacketID, pkt: &T) -> FFResult<()> {
        self.queue_packet(pkt_id, pkt);
        self.flush()
    }

    pub fn queue_packet<T: FFPacket>(&mut self, pkt_id: PacketID, pkt: &T) {
        // add the packet ID and contents
        let id_buf = (pkt_id as u32).to_le_bytes();
        self.copy_to_buf(&id_buf);
        self.queue_struct(pkt);
    }

    pub fn queue_struct<T: FFPacket>(&mut self, s: &T) {
        let struct_buf = unsafe { struct_to_bytes(s) };
        self.copy_to_buf(struct_buf);
    }

    fn copy_to_buf(&mut self, dat: &[u8]) {
        let sz = dat.len();
        let from = self.out_buf_ptr;
        let to = from + sz;
        if to > PACKET_BUFFER_SIZE {
            panic_log(&format!(
                "Payload too big for output ({} > {})",
                sz, PACKET_BUFFER_SIZE
            ));
        }

        self.out_buf[from..to].copy_from_slice(dat);
        self.out_buf_ptr = to;
    }
}
