use std::time::Duration;

use rusty_fusion::{
    net::{
        cnclient::CNClient,
        cnserver::CNServer,
        packet::{
            sP_CL2LS_REQ_LOGIN, sP_LS2CL_REP_LOGIN_FAIL,
            PacketID::{self, *},
        },
    },
    Result,
};

fn main() -> Result<()> {
    let addr = "127.0.0.1:23001";
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: CNServer = CNServer::new(addr, Some(polling_interval))?;
    println!("Shard server listening on {addr}");
    loop {
        server.poll(&handle_packet)?;
    }
}

fn handle_packet(client: &mut CNClient, pkt_id: PacketID) -> Result<()> {
    println!("{} sent {:?}", client.get_addr(), pkt_id);
    match pkt_id {
        P_CL2LS_REQ_LOGIN => wrong_server(client),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

fn wrong_server(client: &mut CNClient) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet();
    let resp = sP_LS2CL_REP_LOGIN_FAIL {
        iErrorCode: 4, // "Login error"
        szID: pkt.szID,
    };
    client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)?;

    Ok(())
}
