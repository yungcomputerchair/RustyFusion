use std::{
    io::{Read, Write},
    mem::size_of,
    net::{SocketAddr, TcpStream},
};

use num_traits::{FromPrimitive, ToPrimitive};

use crate::{
    error::{BadPacketID, BadRequest},
    net::{struct_to_bytes, PACKET_BUFFER_SIZE},
    util::get_time,
    Result,
};

use super::{
    bytes_to_struct,
    crypto::{decrypt_packet, encrypt_packet, EncryptionMode, CRYPTO_KEY_SIZE, DEFAULT_KEY},
    packet::{FFPacket, PacketID},
};

#[derive(Debug, Clone)]
pub enum ClientType {
    Unknown,
    GameClient {
        serial_key: i64,     // iEnterSerialKey
        pc_uid: Option<i64>, // iPC_UID
    },
    LoginServer,
    ShardServer(i64), // iConn_UID
}

pub struct FFClient {
    sock: TcpStream,
    addr: SocketAddr,
    buf: [u8; PACKET_BUFFER_SIZE],
    last_pkt_id: PacketID,
    last_pkt_sz: usize,
    e_key: [u8; CRYPTO_KEY_SIZE],
    fe_key: [u8; CRYPTO_KEY_SIZE],
    enc_mode: EncryptionMode,
    client_type: ClientType,
    last_heartbeat: u64,
}

impl FFClient {
    pub fn new(conn_data: (TcpStream, SocketAddr)) -> Self {
        let default_key: [u8; CRYPTO_KEY_SIZE] = DEFAULT_KEY.try_into().unwrap();
        Self {
            sock: conn_data.0,
            addr: conn_data.1,
            buf: [0; PACKET_BUFFER_SIZE],
            last_pkt_id: PacketID::P_NULL,
            last_pkt_sz: 0,
            e_key: default_key,
            fe_key: default_key,
            enc_mode: EncryptionMode::EKey,
            client_type: ClientType::Unknown,
            last_heartbeat: get_time(),
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

    pub fn set_e_key(&mut self, key: [u8; CRYPTO_KEY_SIZE]) {
        self.e_key = key;
    }

    pub fn set_fe_key(&mut self, key: [u8; CRYPTO_KEY_SIZE]) {
        self.fe_key = key;
    }

    pub fn set_enc_mode(&mut self, mode: EncryptionMode) {
        self.enc_mode = mode;
    }

    pub fn get_client_type(&self) -> ClientType {
        self.client_type.clone()
    }

    pub fn set_client_type(&mut self, cltype: ClientType) {
        self.client_type = cltype;
    }

    pub fn get_player_id(&mut self) -> Result<i64> {
        if let ClientType::GameClient {
            pc_uid: Some(pc_uid),
            ..
        } = self.client_type
        {
            Ok(pc_uid)
        } else {
            Err(Box::new(BadRequest::new(self)))
        }
    }

    pub fn get_packet_id(&self) -> PacketID {
        self.last_pkt_id
    }

    pub fn get_packet<T: FFPacket>(&self) -> &T {
        let pkt_buf: &[u8] = &self.buf[4..self.last_pkt_sz];
        unsafe { bytes_to_struct(pkt_buf) }
    }

    pub fn read_packet(&mut self) -> Result<PacketID> {
        self.last_heartbeat = get_time();

        // read the size
        let mut sz_buf: [u8; 4] = [0; 4];
        self.sock.read_exact(&mut sz_buf)?;
        let sz: usize = u32::from_le_bytes(sz_buf) as usize;

        // read the packet
        let buf: &mut [u8] = &mut self.buf[..sz];
        self.sock.read_exact(buf)?;

        // decrypt the packet (client always encrypts with E key)
        decrypt_packet(buf, &self.e_key);

        let id: u32 = u32::from_le_bytes(buf[..4].try_into().unwrap());
        let id: PacketID = match PacketID::from_u32(id) {
            Some(id) => id,
            None => {
                return Err(Box::new(BadPacketID::new(id)));
            }
        };

        self.last_pkt_id = id;
        self.last_pkt_sz = sz;
        Ok(id)
    }

    pub fn send_packet<T: FFPacket>(&mut self, pkt_id: PacketID, pkt: &T) -> Result<()> {
        // send the size
        let sz: usize = 4 + size_of::<T>();
        let mut sz_buf: [u8; 4] = u32::to_le_bytes(sz as u32);
        self.sock.write_all(&sz_buf)?;

        // prepare the packet (reuse sz_buf for id)
        sz_buf = PacketID::to_u32(&pkt_id).unwrap().to_le_bytes();
        let pkt_buf: &[u8] = unsafe { struct_to_bytes(pkt) };
        let mut out_buf: Vec<u8> = [&sz_buf, pkt_buf].concat();

        // encrypt the packet (client decrypts with either E or FE key)
        match self.enc_mode {
            EncryptionMode::EKey => encrypt_packet(&mut out_buf, &self.e_key),
            EncryptionMode::FEKey => encrypt_packet(&mut out_buf, &self.fe_key),
        }

        // send the packet
        self.sock.write_all(&out_buf)?;
        Ok(())
    }
}
