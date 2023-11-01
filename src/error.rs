use std::{error::Error, fmt::Display};

use crate::net::{
    ffclient::{ClientType, FFClient},
    packet::PacketID,
};

#[derive(Debug)]
pub struct BadPacketID {
    packet_id: u32,
}
impl BadPacketID {
    pub fn new(packet_id: u32) -> Self {
        Self { packet_id }
    }
}
impl Error for BadPacketID {}
impl Display for BadPacketID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bad packet ID {}", self.packet_id)
    }
}

#[derive(Debug)]
pub struct BadRequest {
    addr: String,
    packet_id: PacketID,
    client_type: ClientType,
}
impl BadRequest {
    pub fn new(client: &FFClient) -> Self {
        Self {
            addr: client.get_addr(),
            packet_id: client.get_packet_id(),
            client_type: client.get_client_type().clone(),
        }
    }
}
impl Error for BadRequest {}
impl Display for BadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Bad request from {} (client type {:?}): {:?}",
            self.addr, self.client_type, self.packet_id
        )
    }
}

#[derive(Debug)]
pub struct BadPayload {
    addr: String,
    packet_id: PacketID,
    client_type: ClientType,
    reason: String,
}
impl BadPayload {
    pub fn new(client: &FFClient, reason: String) -> Self {
        Self {
            addr: client.get_addr(),
            packet_id: client.get_packet_id(),
            client_type: client.get_client_type(),
            reason,
        }
    }
}
impl Error for BadPayload {}
impl Display for BadPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Bad {:?} payload from {} (client type {:?}): {}",
            self.packet_id, self.addr, self.client_type, self.reason
        )
    }
}
