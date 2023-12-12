use rusty_fusion::{enums::TransportationType, error::catch_fail};

use super::*;

pub fn regist_transportation_location(
    client: &mut FFClient,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_REGIST_TRANSPORTATION_LOCATION =
        *client.get_packet(P_CL2FE_REQ_REGIST_TRANSPORTATION_LOCATION)?;
    catch_fail(
        (|| {
            let transport_type: TransportationType = pkt.eTT.try_into()?;
            let player = state.get_player_mut(client.get_player_id()?)?;
            match transport_type {
                TransportationType::Warp => {
                    player.update_scamper_flags(pkt.iLocationID)?;
                }
                TransportationType::Wyvern => {
                    player.update_skyway_flags(pkt.iLocationID)?;
                }
                TransportationType::Bus => {}
            }

            let resp = sP_FE2CL_REP_PC_REGIST_TRANSPORTATION_LOCATION_SUCC {
                eTT: pkt.eTT,
                iLocationID: pkt.iLocationID,
                iWarpLocationFlag: player.get_scamper_flags(),
                aWyvernLocationFlag: player.get_skyway_flags(),
            };
            client.send_packet(P_FE2CL_REP_PC_REGIST_TRANSPORTATION_LOCATION_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_REGIST_TRANSPORTATION_LOCATION_FAIL {
                eTT: pkt.eTT,
                iLocationID: pkt.iLocationID,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_REGIST_TRANSPORTATION_LOCATION_FAIL, &resp)
        },
    )
}
