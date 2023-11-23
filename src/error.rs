use std::{error::Error, fmt::Display};

use crate::net::{
    ffclient::{ClientType, FFClient},
    packet::PacketID,
};

#[derive(Debug)]
pub struct SimpleError {
    msg: String,
}
impl SimpleError {
    pub fn build(msg: String) -> Box<dyn Error> {
        Box::new(Self { msg })
    }
}
impl Error for SimpleError {}
impl Display for SimpleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[derive(Debug)]
pub struct BadPacketID {
    packet_id: u32,
}
impl BadPacketID {
    pub fn build(packet_id: u32) -> Box<dyn Error> {
        Box::new(Self { packet_id })
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
    pub fn build(client: &FFClient) -> Box<dyn Error> {
        Box::new(Self {
            addr: client.get_addr(),
            packet_id: client.get_packet_id(),
            client_type: client.get_client_type(),
        })
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
    pub fn build(client: &FFClient, reason: String) -> Box<dyn Error> {
        Box::new(Self {
            addr: client.get_addr(),
            packet_id: client.get_packet_id(),
            client_type: client.get_client_type(),
            reason,
        })
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
