use rusty_fusion::{
    net::{
        cnclient::CNClient,
        cnserver::CNServer,
        packet::{
            sP_CL2LS_REQ_LOGIN, sP_LS2CL_REP_LOGIN_SUCC,
            PacketID::{self, *},
        },
    },
    util::get_time,
    Result,
};

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
        P_CL2LS_REQ_LOGIN => req_login(client),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

fn req_login(client: &mut CNClient) -> Result<()> {
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
    client.send_packet(P_LS2CL_REP_LOGIN_SUCC, &resp)?;

    Ok(())
}
