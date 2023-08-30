use std::{
    collections::HashMap,
    sync::atomic::{AtomicI64, Ordering},
    time::Duration,
};

use rusty_fusion::{
    net::{
        cnclient::CNClient,
        cnserver::CNServer,
        crypto::{gen_key, DEFAULT_KEY},
        packet::{
            sP_LS2FE_REP_CONNECT_SUCC,
            PacketID::{self, *},
        },
    },
    util::get_time,
    Result,
};

static NEXT_PC_UID: AtomicI64 = AtomicI64::new(1);
static NEXT_SHARD_UID: AtomicI64 = AtomicI64::new(1);

fn main() -> Result<()> {
    let addr: &str = "127.0.0.1:23000";
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: CNServer = CNServer::new(addr, Some(polling_interval))?;
    println!("Login server listening on {addr}");
    loop {
        server.poll(&handle_packet)?;
    }
}

fn handle_packet(
    key: &usize,
    clients: &mut HashMap<usize, CNClient>,
    pkt_id: PacketID,
) -> Result<()> {
    let client: &mut CNClient = clients.get_mut(key).unwrap();
    println!("{} sent {:?}", client.get_addr(), pkt_id);
    match pkt_id {
        P_FE2LS_REQ_CONNECT => shard_handshake(client),
        //
        P_CL2LS_REQ_LOGIN => handlers::login(client),
        P_CL2LS_REQ_CHECK_CHAR_NAME => handlers::check_char_name(client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => handlers::save_char_name(client),
        P_CL2LS_REQ_CHAR_CREATE => handlers::char_create(client),
        P_CL2LS_REQ_CHAR_SELECT => handlers::char_select(client),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

fn get_next_pc_uid() -> i64 {
    let next_id: i64 = NEXT_PC_UID.load(Ordering::Acquire);
    NEXT_PC_UID.store(next_id + 1, Ordering::Release);
    next_id
}

fn get_next_shard_uid() -> i64 {
    let next_id: i64 = NEXT_SHARD_UID.load(Ordering::Acquire);
    NEXT_SHARD_UID.store(next_id + 1, Ordering::Release);
    next_id
}

fn shard_handshake(server: &mut CNClient) -> Result<()> {
    server.set_client_type(ClientType::ShardServer);
    let conn_id: i64 = get_next_shard_uid();
    let resp = sP_LS2FE_REP_CONNECT_SUCC {
        uiSvrTime: get_time(),
        iConn_UID: conn_id,
    };
    server.send_packet(P_LS2FE_REP_CONNECT_SUCC, &resp)?;
    Ok(())
}

mod handlers {
    use super::*;
    use rusty_fusion::net::{cnclient::ClientType, packet::*};

    pub fn login(client: &mut CNClient) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet();
        let resp = sP_LS2CL_REP_LOGIN_SUCC {
            iCharCount: 0,
            iSlotNum: 0,
            iPaymentFlag: 1,
            iTempForPacking4: 69,
            uiSvrTime: get_time(),
            szID: pkt.szID,
            iOpenBetaFlag: 0,
        };
        let e_base: u64 = resp.uiSvrTime;
        let e_iv1: i32 = (resp.iCharCount + 1) as i32;
        let e_iv2: i32 = (resp.iSlotNum + 1) as i32;
        let fe_base: u64 = u64::from_le_bytes(DEFAULT_KEY.try_into().unwrap());
        let fe_iv1: i32 = pkt.iClientVerC;
        let fe_iv2: i32 = 1;

        client.send_packet(P_LS2CL_REP_LOGIN_SUCC, &resp)?;

        client.set_e_key(gen_key(e_base, e_iv1, e_iv2));
        client.set_fe_key(gen_key(fe_base, fe_iv1, fe_iv2));

        let serial_key: i64 = random();
        client.set_client_type(ClientType::GameClient(serial_key));

        Ok(())
    }

    pub fn check_char_name(client: &mut CNClient) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_CHECK_CHAR_NAME = client.get_packet();
        let resp = sP_LS2CL_REP_CHECK_CHAR_NAME_SUCC {
            szFirstName: pkt.szFirstName,
            szLastName: pkt.szLastName,
        };
        client.send_packet(P_LS2CL_REP_CHECK_CHAR_NAME_SUCC, &resp)?;

        Ok(())
    }

    pub fn save_char_name(client: &mut CNClient) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_SAVE_CHAR_NAME = client.get_packet();
        let resp = sP_LS2CL_REP_SAVE_CHAR_NAME_SUCC {
            iPC_UID: get_next_pc_uid(),
            iSlotNum: 0,
            iGender: (rand::random::<bool>() as i8) + 1,
            szFirstName: pkt.szFirstName,
            szLastName: pkt.szLastName,
        };
        client.send_packet(P_LS2CL_REP_SAVE_CHAR_NAME_SUCC, &resp)?;

        Ok(())
    }

    pub fn char_create(client: &mut CNClient) -> Result<()> {
        let pkt: &sP_CL2LS_REQ_CHAR_CREATE = client.get_packet();
        let resp = sP_LS2CL_REP_CHAR_CREATE_SUCC {
            iLevel: 1,
            sPC_Style: pkt.PCStyle,
            sPC_Style2: sPCStyle2 {
                iAppearanceFlag: 0,
                iTutorialFlag: 1,
                iPayzoneFlag: 0,
            },
            sOn_Item: pkt.sOn_Item,
        };
        client.send_packet(P_LS2CL_REP_CHAR_CREATE_SUCC, &resp)?;

        Ok(())
    }

    pub fn char_select(client: &mut CNClient) -> Result<()> {
        let mut shard_ip: [u8; 16] = [0; 16];
        let ip_str = b"127.0.0.1";
        shard_ip[..ip_str.len()].copy_from_slice(ip_str);
        let resp = sP_LS2CL_REP_SHARD_SELECT_SUCC {
            g_FE_ServerIP: shard_ip,
            g_FE_ServerPort: 23001,
            iEnterSerialKey: rand::random(),
        };
        client.send_packet(P_LS2CL_REP_SHARD_SELECT_SUCC, &resp)?;

        Ok(())
    }
}
