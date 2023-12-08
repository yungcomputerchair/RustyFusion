use rusty_fusion::error::catch_fail;

use super::*;

pub fn trade_offer(clients: &mut ClientMap, state: &ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER = *client.get_packet(P_CL2FE_REQ_PC_TRADE_OFFER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let player = state.get_player(client.get_player_id()?)?;
            if player.get_trading_with().is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("From player {} already trading", player.get_player_id()),
                ));
            }

            let other_player = state.get_player(pkt.iID_To)?;
            if other_player.get_trading_with().is_some() {
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
            other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER, &resp)
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
