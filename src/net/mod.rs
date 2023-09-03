use std::{collections::HashMap, mem::size_of, slice::from_raw_parts};

use self::{
    ffclient::{ClientType, FFClient},
    packet::{sPCStyle, FFPacket, PacketID},
};
use crate::Result;

pub mod crypto;
pub mod ffclient;
pub mod ffserver;
pub mod packet;

pub type PacketCallback =
    &'static dyn Fn(&usize, &mut HashMap<usize, FFClient>, PacketID) -> Result<()>;
pub type DisconnectCallback = &'static dyn Fn(FFClient);

#[allow(non_snake_case)]
pub struct LoginData {
    pub iPC_UID: i64,
    pub uiFEKey: u64,
    pub uiSvrTime: u64,
    pub PCStyle: sPCStyle,
}

unsafe fn bytes_to_struct<T: FFPacket>(bytes: &[u8]) -> &T {
    // haters will call this "undefined behavior"
    let struct_ptr: *const T = bytes.as_ptr().cast();
    &*struct_ptr
}

unsafe fn struct_to_bytes<T: FFPacket>(pkt: &T) -> &[u8] {
    let sz: usize = size_of::<T>();
    let struct_ptr: *const T = pkt;
    let buf_ptr: *const u8 = struct_ptr.cast();
    from_raw_parts(buf_ptr, sz)
}

pub fn send_to_others<T: FFPacket>(
    pkt_id: PacketID,
    pkt: &T,
    our_pc_uid: i64,
    clients: &mut HashMap<usize, FFClient>,
) -> Result<()> {
    clients
        .values_mut()
        .filter(|c| {
            matches!(c.get_client_type(), ClientType::GameClient {
        pc_uid: Some(other_id), ..
    } if *other_id != our_pc_uid)
        })
        .try_for_each(|co| co.send_packet(pkt_id, pkt))
}
