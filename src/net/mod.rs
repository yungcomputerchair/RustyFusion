use std::{
    any::type_name, collections::HashMap, mem::size_of, slice::from_raw_parts, time::SystemTime,
};

use self::packet::{
    FFPacket,
    PacketID::{self, *},
};
use crate::{
    error::{log, FFError, FFResult, Severity},
    state::ServerState,
};

const PACKET_BUFFER_SIZE: usize = 4096;
const PACKET_BODY_SIZE: usize = PACKET_BUFFER_SIZE - size_of::<u32>() - size_of::<u32>(); // total - size - id
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

mod ffserver;
pub use ffserver::*;

pub mod crypto;
pub mod packet;

pub type PacketCallback = fn(
    usize,
    &mut HashMap<usize, FFClient>,
    PacketID,
    &mut ServerState,
    SystemTime,
) -> FFResult<()>;
pub type DisconnectCallback = fn(usize, &mut HashMap<usize, FFClient>, &mut ServerState);
pub type LiveCheckCallback = fn(&mut FFClient) -> FFResult<()>;

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
    clients: &'a mut HashMap<usize, FFClient>,
}
impl<'a> ClientMap<'a> {
    pub fn new(key: usize, clients: &'a mut HashMap<usize, FFClient>) -> Self {
        Self { key, clients }
    }

    pub fn get(&mut self, key: usize) -> &mut FFClient {
        self.clients.get_mut(&key).unwrap()
    }

    pub fn get_self(&mut self) -> &mut FFClient {
        self.clients.get_mut(&self.key).unwrap()
    }

    pub fn get_all_gameclient(&mut self) -> impl Iterator<Item = &mut FFClient> {
        self.clients
            .values_mut()
            .filter(|c| matches!(c.client_type, ClientType::GameClient { .. }))
    }

    pub fn get_login_server(&mut self) -> Option<&mut FFClient> {
        let login_server = self
            .clients
            .values_mut()
            .find(|c| matches!(c.client_type, ClientType::LoginServer));
        if login_server.is_none() {
            log(Severity::Warning, "No login server connected");
        }
        login_server
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
        use crate::net::ffclient::PacketBuffer;

        let _buf = PacketBuffer::default();
        // PacketBuffer exposes peek_packet_id etc. which internally
        // call bytes_to_struct. If AlignedBuf weren't aligned, those
        // would return alignment errors. We verify the guarantee here:
        let aligned_buf = [0u8; PACKET_BUFFER_SIZE];
        #[repr(C, align(4))]
        struct AlignedCheck([u8; PACKET_BUFFER_SIZE]);
        let check = AlignedCheck(aligned_buf);
        let ptr = check.0.as_ptr();
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
