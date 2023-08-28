use std::{
    io::{Read, Write},
    mem::size_of,
    net::{SocketAddr, TcpStream},
};

use num_traits::{FromPrimitive, ToPrimitive};

use crate::{
    error::BadPacketID, net::struct_to_bytes, util::get_time, Result, CN_PACKET_BUFFER_SIZE,
};

use super::{
    crypto::{decrypt_packet, encrypt_packet, EncryptionMode, CRYPTO_KEY_SIZE, DEFAULT_KEY},
    packet::PacketID,
};

pub struct CNClient {
    sock: TcpStream,
    addr: SocketAddr,
    buf: [u8; CN_PACKET_BUFFER_SIZE],
    e_key: [u8; CRYPTO_KEY_SIZE],
    fe_key: [u8; CRYPTO_KEY_SIZE],
    enc_mode: EncryptionMode,
    last_heartbeat: u64,
}

impl CNClient {
    pub fn new(conn_data: (TcpStream, SocketAddr)) -> Self {
        let default_key: [u8; CRYPTO_KEY_SIZE] = DEFAULT_KEY.try_into().unwrap();
        Self {
            sock: conn_data.0,
            addr: conn_data.1,
            buf: [0; CN_PACKET_BUFFER_SIZE],
            e_key: default_key,
            fe_key: default_key,
            enc_mode: EncryptionMode::EKey,
            last_heartbeat: get_time(),
        }
    }

    pub fn get_sock(&self) -> &TcpStream {
        &self.sock
    }

    pub fn get_addr(&self) -> String {
        self.addr.to_string()
    }

    pub fn read_packet(&mut self) -> Result<(PacketID, &[u8])> {
        self.last_heartbeat = get_time();

        // read the size
        let mut sz_buf: [u8; 4] = [0; 4];
        self.sock.read_exact(&mut sz_buf)?;
        let sz: usize = u32::from_le_bytes(sz_buf) as usize;

        // read the packet
        let buf: &mut [u8] = &mut self.buf[..sz];
        self.sock.read_exact(buf)?;

        // decrypt the packet
        match self.enc_mode {
            EncryptionMode::EKey => decrypt_packet(buf, &self.e_key),
            EncryptionMode::FEKey => decrypt_packet(buf, &self.fe_key),
        }

        let id: u32 = u32::from_le_bytes(buf[..4].try_into().unwrap());
        let id: PacketID = match PacketID::from_u32(id) {
            Some(id) => id,
            None => {
                return Err(Box::new(BadPacketID::new(id)));
            }
        };
        println!("{:?}", id);

        Ok((id, &buf[4..]))
    }

    pub fn send_packet<T>(&mut self, pkt_id: PacketID, pkt: &T) -> Result<()> {
        // send the size
        let sz: usize = size_of::<T>();
        let mut sz_buf: [u8; 4] = u32::to_le_bytes(sz as u32);
        self.sock.write_all(&sz_buf)?;

        // prepare the packet (reuse sz_buf for id)
        sz_buf = PacketID::to_u32(&pkt_id).unwrap().to_le_bytes();
        let pkt_buf: &[u8] = unsafe { struct_to_bytes(pkt) };
        let mut out_buf: Vec<u8> = [&sz_buf, pkt_buf].concat();

        // encrypt the packet
        match self.enc_mode {
            EncryptionMode::EKey => encrypt_packet(&mut out_buf, &self.e_key),
            EncryptionMode::FEKey => encrypt_packet(&mut out_buf, &self.fe_key),
        }

        // send the packet
        self.sock.write_all(&out_buf)?;
        Ok(())
    }
}
