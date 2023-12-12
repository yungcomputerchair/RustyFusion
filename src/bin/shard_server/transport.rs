use rusty_fusion::{enums::TransportationType, error::catch_fail, tabledata::tdata_get};

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
                TransportationType::Bus => {
                    return Err(FFError::build(
                        Severity::Warning,
                        "Bus reg invalid".to_string(),
                    ));
                }
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

pub fn warp_use_transportation(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_WARP_USE_TRANSPORTATION = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_WARP_USE_TRANSPORTATION)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;
            let trip = tdata_get().get_trip_data(pkt.iTransporationID)?;
            if player.get_taros() < trip.cost {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} doesn't have enough taros to warp",
                        player.get_player_id()
                    ),
                ));
            }

            match trip.transportation_type {
                TransportationType::Warp => {
                    let dest_data = tdata_get().get_scamper_data(trip.end_location)?;
                    player.set_position(dest_data.pos);
                }
                TransportationType::Wyvern => {}
                TransportationType::Bus => {
                    return Err(FFError::build(
                        Severity::Warning,
                        "Bus warp invalid".to_string(),
                    ));
                }
            }

            let player = state.get_player_mut(pc_id)?;
            let new_pos = player.get_position();
            let taros_left = player.set_taros(player.get_taros() - trip.cost);
            let resp = sP_FE2CL_REP_PC_WARP_USE_TRANSPORTATION_SUCC {
                eTT: trip.transportation_type as i32,
                iX: new_pos.x,
                iY: new_pos.y,
                iZ: new_pos.z,
                iCandy: taros_left as i32,
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_USE_TRANSPORTATION_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_WARP_USE_TRANSPORTATION_FAIL {
                iTransportationID: pkt.iTransporationID,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_USE_TRANSPORTATION_FAIL, &resp)
        },
    )
}
