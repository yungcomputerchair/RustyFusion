use std::sync::atomic::{AtomicI64, Ordering};

use rusty_fusion::{
    net::{
        cnclient::CNClient,
        cnserver::CNServer,
        crypto::{gen_key, DEFAULT_KEY},
        packet::{
            sP_CL2LS_REQ_CHECK_CHAR_NAME, sP_CL2LS_REQ_LOGIN, sP_CL2LS_REQ_SAVE_CHAR_NAME,
            sP_LS2CL_REP_CHECK_CHAR_NAME_SUCC, sP_LS2CL_REP_LOGIN_SUCC,
            sP_LS2CL_REP_SAVE_CHAR_NAME_SUCC,
            PacketID::{self, *},
        },
    },
    util::get_time,
    Result,
};

static NEXT_PC_UID: AtomicI64 = AtomicI64::new(1);

fn main() -> Result<()> {
    println!("Hello from login server!");
    let mut server: CNServer = CNServer::new(None).unwrap();
    loop {
        server.poll(&handle_packet)?;
    }
}

fn handle_packet(client: &mut CNClient, pkt_id: PacketID) -> Result<()> {
    println!("{} sent {:?}", client.get_addr(), pkt_id);
    match pkt_id {
        P_CL2LS_REQ_LOGIN => handlers::login(client),
        P_CL2LS_REQ_CHECK_CHAR_NAME => handlers::check_char_name(client),
        P_CL2LS_REQ_SAVE_CHAR_NAME => handlers::save_char_name(client),
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

mod handlers {
    use super::*;

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
}
