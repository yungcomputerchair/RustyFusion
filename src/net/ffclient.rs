use std::{
    io::{Read, Write},
    mem::size_of,
    net::{SocketAddr, TcpStream}, time::SystemTime,
};

use num_traits::{FromPrimitive, ToPrimitive};

use crate::{
    error::{log, FFError, FFResult, Severity},
    net::{struct_to_bytes, PACKET_BUFFER_SIZE, SILENCED_PACKETS},
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
        serial_key: i64,    // iEnterSerialKey
        pc_id: Option<i32>, // iPC_ID
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
            buf: [0; PACKET_BUFFER_SIZE],
            last_pkt_id: PacketID::P_NULL,
            last_pkt_sz: 0,
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

    pub fn get_player_id(&mut self) -> FFResult<i32> {
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

    pub fn get_packet_id(&self) -> PacketID {
        self.last_pkt_id
    }

    pub fn get_packet<T: FFPacket>(&self, pkt_id: PacketID) -> &T {
        assert_eq!(
            self.last_pkt_id, pkt_id,
            "Tried to fetch packet {:?} != buffered {:?}",
            pkt_id, self.last_pkt_id
        );

        let pkt_buf: &[u8] = &self.buf[4..self.last_pkt_sz];
        unsafe { bytes_to_struct(pkt_buf) }
    }

    pub fn read_packet(&mut self) -> FFResult<PacketID> {
        self.last_heartbeat = SystemTime::now();

        // read the size
        let mut sz_buf: [u8; 4] = [0; 4];
        self.sock
            .read_exact(&mut sz_buf)
            .map_err(FFError::from_io_err)?;
        let sz: usize = u32::from_le_bytes(sz_buf) as usize;

        // read the packet
        let buf: &mut [u8] = &mut self.buf[..sz];
        self.sock.read_exact(buf).map_err(FFError::from_io_err)?;

        // decrypt the packet (client always encrypts with E key)
        decrypt_packet(buf, &self.e_key);

        let id: u32 = u32::from_le_bytes(buf[..4].try_into().unwrap());
        let id: PacketID = PacketID::from_u32(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Bad packet ID {id}"),
        ))?;

        if !SILENCED_PACKETS.contains(&id) {
            log(
                Severity::Debug,
                &format!("{} sent {:?}", self.get_addr(), id),
            );
        }

        self.last_pkt_id = id;
        self.last_pkt_sz = sz;
        Ok(id)
    }

    pub fn send_packet<T: FFPacket>(&mut self, pkt_id: PacketID, pkt: &T) -> FFResult<()> {
        // send the size
        let sz: usize = 4 + size_of::<T>();
        let mut sz_buf: [u8; 4] = u32::to_le_bytes(sz as u32);
        self.sock.write_all(&sz_buf).map_err(FFError::from_io_err)?;

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
        self.sock
            .write_all(&out_buf)
            .map_err(FFError::from_io_err)?;
        Ok(())
    }
}
