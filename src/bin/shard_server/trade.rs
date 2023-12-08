use rusty_fusion::error::catch_fail;

use super::*;

pub fn trade_offer(clients: &mut ClientMap, state: &ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_TRADE_OFFER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let player = state.get_player(client.get_player_id()?)?;
            if player.trading_with.is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("From player {} already trading", player.get_player_id()),
                ));
            }

            let other_player = state.get_player(pkt.iID_To)?;
            if other_player.trading_with.is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("To player {} already trading", other_player.get_player_id()),
                ));
            }

            let other_client = clients.get_from_player_id(pkt.iID_To)?;
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
            };
            let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER, &resp);
            Ok(())
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_REFUSAL {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
            };
            client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_REFUSAL, &resp)
        },
    )
}

pub fn trade_offer_accept(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER_ACCEPT = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_ACCEPT)?;
    catch_fail(
        (|| {
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_SUCC {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
            };
            let player_from = state.get_player_mut(pkt.iID_From)?;
            player_from.trading_with = Some(pkt.iID_To);
            let player_to = state.get_player_mut(pkt.iID_To)?;
            player_to.trading_with = Some(pkt.iID_From);
            let other_client = clients.get_from_player_id(pkt.iID_From)?;
            let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_SUCC, &resp);
            Ok(())
        })(),
        || {
            if let Ok(player_from) = state.get_player_mut(pkt.iID_From) {
                player_from.trading_with = None;
            }
            if let Ok(player_to) = state.get_player_mut(pkt.iID_To) {
                player_to.trading_with = None;
            }

            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_ABORT {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TRADE_OFFER_ABORT, &resp)
        },
    )
}

pub fn trade_offer_refusal(clients: &mut ClientMap) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL =
        client.get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL)?;
    let resp = sP_FE2CL_REP_PC_TRADE_OFFER_REFUSAL {
        iID_Request: pkt.iID_Request,
        iID_From: pkt.iID_From,
        iID_To: pkt.iID_To,
    };
    let other_client = clients.get_from_player_id(resp.iID_From)?;
    let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_REFUSAL, &resp);
    Ok(())
}
