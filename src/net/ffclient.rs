use std::{
    io::{Read, Write},
    mem::size_of,
    net::{SocketAddr, TcpStream},
    time::SystemTime,
};

use num_traits::FromPrimitive;

use crate::{
    error::{log, FFError, FFResult, Severity},
    net::{struct_to_bytes, PACKET_BUFFER_SIZE, SILENCED_PACKETS},
};

use super::{
    bytes_to_struct,
    crypto::{decrypt_payload, encrypt_payload, EncryptionMode, CRYPTO_KEY_SIZE, DEFAULT_KEY},
    packet::{FFPacket, PacketID},
};

#[derive(Debug, Clone)]
pub enum ClientType {
    Unknown,
    GameClient {
        serial_key: i64,    // iEnterSerialKey
        pc_id: Option<i32>, // iPC_ID
    },
    LoginServer,
    ShardServer(i64), // iConn_UID
}

pub struct FFClient {
    sock: TcpStream,
    addr: SocketAddr,
    in_buf: [u8; PACKET_BUFFER_SIZE],
    in_buf_ptr: usize,
    in_data_len: usize,
    out_buf: [u8; PACKET_BUFFER_SIZE],
    out_buf_ptr: usize,
    pub e_key: [u8; CRYPTO_KEY_SIZE],
    pub fe_key: [u8; CRYPTO_KEY_SIZE],
    pub enc_mode: EncryptionMode,
    pub client_type: ClientType,
    last_heartbeat: SystemTime,
}

impl FFClient {
    pub fn new(conn_data: (TcpStream, SocketAddr)) -> Self {
        let default_key: [u8; CRYPTO_KEY_SIZE] = DEFAULT_KEY.try_into().unwrap();
        Self {
            sock: conn_data.0,
            addr: conn_data.1,
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
        }
    }

    pub fn get_sock(&self) -> &TcpStream {
        &self.sock
    }

    pub fn get_addr(&self) -> String {
        self.addr.to_string()
    }

    pub fn get_fe_key_uint(&self) -> u64 {
        u64::from_le_bytes(self.fe_key)
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

    pub fn get_last_heartbeat(&self) -> SystemTime {
        self.last_heartbeat
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

        let id = u32::from_le_bytes(self.in_buf[from..to].try_into().unwrap());
        PacketID::from_u32(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Bad packet ID {id}"),
        ))
    }

    pub fn get_packet<T: FFPacket>(&mut self, pkt_id: PacketID) -> FFResult<&T> {
        let buffered_pkt_id = self.peek_packet_id()?;
        assert_eq!(
            buffered_pkt_id, pkt_id,
            "Tried to fetch packet {:?} != buffered {:?}",
            pkt_id, buffered_pkt_id
        );
        self.in_buf_ptr += 4;
        let pkt = self.get_struct()?;

        if !SILENCED_PACKETS.contains(&pkt_id) {
            log(Severity::Debug, &format!("{:?}", pkt));
        }

        Ok(pkt)
    }

    pub fn get_struct<T: FFPacket>(&mut self) -> FFResult<&T> {
        let sz: usize = size_of::<T>();
        let from = self.in_buf_ptr;
        let to = from + sz;
        if to > self.in_data_len {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Couldn't read struct; not enough bytes came in: {} > {}",
                    to, self.in_data_len
                ),
            ));
        }

        let buf: &[u8] = &self.in_buf[from..to];
        let s = unsafe { bytes_to_struct(buf) };
        self.in_buf_ptr += sz;
        Ok(s)
    }

    pub fn read_payload(&mut self) -> FFResult<()> {
        self.last_heartbeat = SystemTime::now();

        // read the size
        let mut sz_buf: [u8; 4] = [0; 4];
        self.sock
            .read_exact(&mut sz_buf)
            .map_err(FFError::from_io_err)?;
        let sz: usize = u32::from_le_bytes(sz_buf) as usize;

        // read the packet
        let buf: &mut [u8] = &mut self.in_buf[..sz];
        self.sock.read_exact(buf).map_err(FFError::from_io_err)?;
        self.in_buf_ptr = 0;
        self.in_data_len = sz;

        // decrypt the packet (client always encrypts with E key)
        decrypt_payload(buf, &self.e_key);

        let id = self.peek_packet_id()?;

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
        self.queue_packet(pkt_id, pkt)?;
        self.flush()
    }

    pub fn queue_packet<T: FFPacket>(&mut self, pkt_id: PacketID, pkt: &T) -> FFResult<()> {
        // add the packet ID and contents
        let id_buf = (pkt_id as u32).to_le_bytes();
        self.copy_to_buf(&id_buf)?;
        self.queue_struct(pkt)
    }

    pub fn queue_struct<T: FFPacket>(&mut self, s: &T) -> FFResult<()> {
        let struct_buf = unsafe { struct_to_bytes(s) };
        self.copy_to_buf(struct_buf)
    }

    fn copy_to_buf(&mut self, dat: &[u8]) -> FFResult<()> {
        let sz = dat.len();
        let from = self.out_buf_ptr;
        let to = from + sz;
        if to > PACKET_BUFFER_SIZE {
            return Err(FFError::build_dc(
                Severity::Warning,
                format!(
                    "Payload too big for output ({} > {})",
                    sz, PACKET_BUFFER_SIZE
                ),
            ));
        }

        self.out_buf[from..to].copy_from_slice(dat);
        self.out_buf_ptr = to;
        Ok(())
    }
}
