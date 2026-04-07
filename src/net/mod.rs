use std::{
    any::type_name,
    cell::Cell,
    collections::HashMap,
    future::Future,
    mem::size_of,
    ops::{Deref, DerefMut},
    pin::Pin,
    slice::from_raw_parts,
};

use self::packet::{
    FFPacket,
    PacketID::{self, *},
};
use crate::{
    error::{log, FFError, FFResult, Severity},
    net::packet::Packet,
    state::ServerState,
};

const PACKET_BUFFER_SIZE: usize = 4096; // payload buffer size; includes ID, but not length
const PACKET_LENGTH_SIZE: usize = size_of::<u32>(); // not encrypted
const PACKET_ID_SIZE: usize = size_of::<u32>(); // encrypted
const PACKET_BODY_SIZE: usize = PACKET_BUFFER_SIZE - PACKET_ID_SIZE;

const UNKNOWN_CT_ALLOWED_PACKETS: [PacketID; 3] = [
    P_FE2LS_REQ_AUTH_CHALLENGE,
    P_CL2LS_REQ_LOGIN,
    P_CL2FE_REQ_PC_ENTER,
];
const SILENCED_PACKETS: [PacketID; 15] = [
    P_LS2FE_REP_AUTH_CHALLENGE,
    P_LS2FE_REP_CONNECT_SUCC,
    //
    P_FE2LS_REQ_CONNECT,
    P_FE2LS_UPDATE_PC_STATUSES,
    P_FE2LS_UPDATE_MONITOR,
    //
    P_CL2FE_REQ_PC_MOVE,
    P_CL2FE_REQ_PC_JUMP,
    P_CL2FE_REQ_PC_STOP,
    P_CL2FE_REQ_PC_MOVETRANSPORTATION,
    P_CL2FE_REQ_SEND_FREECHAT_MESSAGE,
    P_CL2FE_REQ_SEND_MENUCHAT_MESSAGE,
    P_CL2FE_REQ_SEND_ALL_GROUP_FREECHAT_MESSAGE,
    P_CL2FE_REQ_SEND_ALL_GROUP_MENUCHAT_MESSAGE,
    P_CL2FE_REQ_SEND_BUDDY_FREECHAT_MESSAGE,
    P_CL2FE_REQ_SEND_BUDDY_MENUCHAT_MESSAGE,
];

mod ffclient;
pub use ffclient::*;

mod ffconnection;
pub use ffconnection::*;

mod ffserver;
pub use ffserver::*;

pub mod crypto;
pub mod packet;

pub type PacketCallback = for<'a> fn(
    Packet,
    usize,
    &'a HashMap<usize, FFClient>,
    &'a mut ServerState,
) -> Pin<Box<dyn Future<Output = FFResult<()>> + Send + 'a>>;
pub type DisconnectCallback = fn(usize, &HashMap<usize, FFClient>, &mut ServerState);
pub type LiveCheckCallback = fn(&FFClient);

#[derive(Clone, Copy)]
#[repr(C, align(4))]
pub struct AlignedBuf([u8; PACKET_BUFFER_SIZE]);
impl Deref for AlignedBuf {
    type Target = [u8; PACKET_BUFFER_SIZE];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Default for AlignedBuf {
    fn default() -> Self {
        Self([0; PACKET_BUFFER_SIZE])
    }
}

#[cfg(test)]
impl AlignedBuf {
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}

#[derive(Clone)]
struct PacketBuffer {
    buf: AlignedBuf,
    cursor: usize,
}
impl Default for PacketBuffer {
    fn default() -> Self {
        Self {
            buf: AlignedBuf::default(),
            cursor: 0,
        }
    }
}
impl PacketBuffer {
    fn reset(&mut self) {
        self.cursor = 0;
    }

    fn len(&self) -> usize {
        self.cursor
    }

    fn peek_packet_id(&self) -> FFResult<PacketID> {
        if self.cursor < PACKET_ID_SIZE {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Couldn't peek packet ID; not enough bytes came in: {}",
                    self.cursor
                ),
            ));
        }

        let id_ord = u32::from_le_bytes(self.buf[..4].try_into().unwrap());
        let id: FFResult<PacketID> = id_ord.try_into();
        id.map_err(|_| FFError::build(Severity::Warning, format!("Bad packet ID: {}", id_ord)))
    }

    fn push_bytes(&mut self, bytes: &[u8]) -> FFResult<()> {
        let new_len = self.cursor + bytes.len();
        if new_len > self.buf.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Packet buffer overflow: current length {}, incoming bytes {}, would be {}",
                    self.cursor,
                    bytes.len(),
                    new_len
                ),
            ));
        }

        self.buf[self.cursor..new_len].copy_from_slice(bytes);
        self.cursor = new_len;
        Ok(())
    }

    fn get_bytes(&self) -> &[u8] {
        &self.buf[..self.cursor]
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct LoginData {
    pub iAccountID: i64,
    pub iPC_UID: i64,
    pub uiFEKey: u64,
    pub uiSvrTime: u64,
    pub iChannelRequestNum: u8,
    pub iBuddyWarpTime: u32,
}

fn bytes_to_struct<T: FFPacket>(bytes: &[u8]) -> FFResult<&T> {
    let ptr = bytes.as_ptr();
    let misalignment = ptr.align_offset(align_of::<T>());
    if misalignment != 0 {
        return Err(FFError::build_dc(
            Severity::Warning,
            format!(
                "Misaligned packet data for {}: align should be {}, misaligned by {}",
                type_name::<T>(),
                align_of::<T>(),
                misalignment
            ),
        ));
    }

    // we're aligned, so this transmutation is sound
    Ok(unsafe { &*ptr.cast::<T>() })
}

fn struct_to_bytes<T: FFPacket>(pkt: &T) -> &[u8] {
    let sz: usize = size_of::<T>();
    let struct_ptr: *const T = pkt;
    let buf_ptr: *const u8 = struct_ptr.cast();

    // always safe because T is always valid, aligned, and the size is correct
    unsafe { from_raw_parts(buf_ptr, sz) }
}

pub struct ClientMap<'a> {
    key: usize,
    login_server_key: Cell<Option<usize>>,
    clients: &'a HashMap<usize, FFClient>,
}
impl<'a> ClientMap<'a> {
    pub fn new(key: usize, clients: &'a HashMap<usize, FFClient>) -> Self {
        Self {
            key,
            login_server_key: Cell::new(None),
            clients,
        }
    }

    pub fn get(&self, key: usize) -> Option<&FFClient> {
        self.clients.get(&key)
    }

    pub fn get_self(&self) -> &FFClient {
        self.clients.get(&self.key).unwrap()
    }

    pub fn get_all_gameclient(&self) -> Vec<&FFClient> {
        self.clients
            .values()
            .filter(|c| {
                let meta = c.meta.read();
                matches!(meta.client_type, ClientType::GameClient { .. })
            })
            .collect()
    }

    pub fn get_shard_server(&self, shard_id: i32) -> Option<&FFClient> {
        self.clients.values().find(|c| {
            let meta = c.meta.read();
            meta.client_type == ClientType::ShardServer(shard_id)
        })
    }

    pub fn get_login_server(&self) -> Option<&FFClient> {
        let cached_key = self.login_server_key.get();
        let cache_valid = cached_key
            .and_then(|key| self.clients.get(&key))
            .is_some_and(|c| {
                let meta = c.meta.read();
                meta.client_type == ClientType::LoginServer
            });

        if !cache_valid {
            let found_key = self
                .clients
                .iter()
                .find(|(_, c)| {
                    let meta = c.meta.read();
                    meta.client_type == ClientType::LoginServer
                })
                .map(|(k, _)| *k);

            if found_key.is_none() {
                log(Severity::Warning, "No login server connected");
            }

            self.login_server_key.set(found_key);
        }

        self.login_server_key
            .get()
            .and_then(|key| self.clients.get(&key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::packet::*;

    #[test]
    fn test_misaligned_bytes_rejected() {
        // Manually create a misaligned slice and verify bytes_to_struct rejects it
        let raw: [u8; 32] = [0; 32];
        let misaligned = &raw[1..17]; // offset by 1 byte — not 4-aligned

        let result = bytes_to_struct::<sP_CL2FE_REQ_ITEM_MOVE>(misaligned);
        assert!(result.is_err(), "Misaligned data should be rejected");
        // The old code would have UB/abort here — now it's a clean error
    }

    #[test]
    fn test_aligned_bytes_accepted() {
        // Create a properly aligned buffer (same as AlignedBuf guarantees)
        #[repr(C, align(4))]
        struct Aligned([u8; 16]);
        let buf = Aligned([0u8; 16]);

        let result = bytes_to_struct::<sP_CL2FE_REQ_ITEM_MOVE>(&buf.0);
        assert!(result.is_ok(), "4-aligned data should be accepted");
        let pkt = result.unwrap();
        assert_eq!(pkt.eFrom, 0);
        assert_eq!(pkt.iFromSlotNum, 0);
    }

    #[test]
    fn test_aligned_buf_is_always_aligned() {
        // Verify that AlignedBuf's pointer is always 4-aligned,
        // which is the invariant that makes our bytes_to_struct sound.
        let buf = AlignedBuf::default();
        let ptr = buf.as_ptr();
        assert_eq!(
            ptr.align_offset(4),
            0,
            "AlignedBuf must be 4-byte aligned, got address {:p}",
            ptr
        );
    }

    #[test]
    fn test_crafted_packet_roundtrip() {
        // Simulate the exact flow: struct_to_bytes → aligned copy → bytes_to_struct
        let original = sP_CL2FE_REQ_ITEM_MOVE {
            eFrom: 1,
            iFromSlotNum: 42,
            eTo: 2,
            iToSlotNum: 7,
        };

        let bytes: &[u8] = struct_to_bytes(&original);

        // Copy into an aligned buffer (as AlignedBuf would provide)
        #[repr(C, align(4))]
        struct Aligned([u8; 16]);
        let mut aligned = Aligned([0u8; 16]);
        aligned.0[..bytes.len()].copy_from_slice(bytes);

        let result = bytes_to_struct::<sP_CL2FE_REQ_ITEM_MOVE>(&aligned.0[..bytes.len()]);
        assert!(result.is_ok());
        let pkt = result.unwrap();
        assert_eq!(pkt.eFrom, 1);
        assert_eq!(pkt.iFromSlotNum, 42);
        assert_eq!(pkt.eTo, 2);
        assert_eq!(pkt.iToSlotNum, 7);
    }
}
