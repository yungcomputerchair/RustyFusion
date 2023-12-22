use rusty_fusion::{
    defines::{EQUIP_SLOT_VEHICLE, EXIT_CODE_REQ_BY_PC},
    enums::ItemLocation,
    error::catch_fail,
    util, Position,
};

use super::*;

use crate::ShardServerState;

pub fn pc_enter(
    client: &mut FFClient,
    key: usize,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_PC_ENTER = client.get_packet(P_CL2FE_REQ_PC_ENTER)?;
    let serial_key: i64 = pkt.iEnterSerialKey;
    let login_data = state.login_data.remove(&serial_key).unwrap();
    let pc_id = state.entity_map.gen_next_pc_id();
    let mut player = login_data.player;
    player.set_player_id(pc_id);
    player.set_client_id(key);

    let resp = sP_FE2CL_REP_PC_ENTER_SUCC {
        iID: pc_id,
        PCLoadData2CL: player.get_load_data(),
        uiSvrTime: util::get_timestamp_ms(time),
    };

    client.client_type = ClientType::GameClient {
        serial_key: pkt.iEnterSerialKey,
        pc_id: Some(pc_id),
    };

    let iv1: i32 = resp.iID + 1;
    let iv2: i32 = resp.PCLoadData2CL.iFusionMatter + 1;
    client.e_key = gen_key(resp.uiSvrTime, iv1, iv2);
    client.fe_key = login_data.uiFEKey.to_le_bytes();
    client.enc_mode = EncryptionMode::FEKey;

    state.entity_map.track(Box::new(player));

    client.send_packet(P_FE2CL_REP_PC_ENTER_SUCC, &resp)
}

pub fn pc_exit(client: &mut FFClient) -> FFResult<()> {
    let resp = sP_FE2CL_REP_PC_EXIT_SUCC {
        iID: client.get_player_id()?,
        iExitCode: EXIT_CODE_REQ_BY_PC as i32,
    };
    client.send_packet(P_FE2CL_REP_PC_EXIT_SUCC, &resp)
}

pub fn pc_loading_complete(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_PC_LOADING_COMPLETE = clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_LOADING_COMPLETE)?;
    let resp = sP_FE2CL_REP_PC_LOADING_COMPLETE_SUCC { iPC_ID: pkt.iPC_ID };
    catch_fail(
        (|| {
            let player = state.get_player(clients.get_self().get_player_id()?)?;
            let chunk = player.get_chunk_coords();
            state
                .entity_map
                .update(player.get_id(), Some(chunk), Some(clients));
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_LOADING_COMPLETE_SUCC, &resp)
        })(),
        || {
            Err(FFError::build_dc(
                Severity::Warning,
                "Loading complete failed".to_string(),
            ))
        },
    )
}

pub fn pc_move(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_MOVE = client.get_packet(P_CL2FE_REQ_PC_MOVE)?;
    let pos = Position {
        x: pkt.iX,
        y: pkt.iY,
        z: pkt.iZ,
    };
    let angle = pkt.iAngle;

    let resp = sP_FE2CL_PC_MOVE {
        iCliTime: pkt.iCliTime,
        iX: pkt.iX,
        iY: pkt.iY,
        iZ: pkt.iZ,
        fVX: pkt.fVX,
        fVY: pkt.fVY,
        fVZ: pkt.fVZ,
        iAngle: pkt.iAngle,
        cKeyValue: pkt.cKeyValue,
        iSpeed: pkt.iSpeed,
        iID: pc_id,
        iSvrTime: util::get_timestamp_ms(time),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            let _ = client.send_packet(P_FE2CL_PC_MOVE, &resp);
        });

    let player = state.get_player_mut(pc_id)?;

    // TODO anticheat

    let entity_id = player.get_id();
    player.set_position(pos);
    player.set_rotation(angle);
    let chunk = player.get_chunk_coords();
    state
        .entity_map
        .update(entity_id, Some(chunk), Some(clients));
    Ok(())
}

pub fn pc_jump(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_JUMP = client.get_packet(P_CL2FE_REQ_PC_JUMP)?;
    let pos = Position {
        x: pkt.iX,
        y: pkt.iY,
        z: pkt.iZ,
    };
    let angle = pkt.iAngle;

    let resp = sP_FE2CL_PC_JUMP {
        iCliTime: pkt.iCliTime,
        iX: pkt.iX,
        iY: pkt.iY,
        iZ: pkt.iZ,
        iVX: pkt.iVX,
        iVY: pkt.iVY,
        iVZ: pkt.iVZ,
        iAngle: pkt.iAngle,
        cKeyValue: pkt.cKeyValue,
        iSpeed: pkt.iSpeed,
        iID: pc_id,
        iSvrTime: util::get_timestamp_ms(time),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            let _ = client.send_packet(P_FE2CL_PC_JUMP, &resp);
        });

    let player = state.get_player_mut(pc_id)?;

    // TODO anticheat

    let entity_id = player.get_id();
    player.set_position(pos);
    player.set_rotation(angle);
    let chunk = player.get_chunk_coords();
    state
        .entity_map
        .update(entity_id, Some(chunk), Some(clients));
    Ok(())
}

pub fn pc_stop(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_STOP = client.get_packet(P_CL2FE_REQ_PC_STOP)?;
    let pos = Position {
        x: pkt.iX,
        y: pkt.iY,
        z: pkt.iZ,
    };

    let resp = sP_FE2CL_PC_STOP {
        iCliTime: pkt.iCliTime,
        iX: pkt.iX,
        iY: pkt.iY,
        iZ: pkt.iZ,
        iID: pc_id,
        iSvrTime: util::get_timestamp_ms(time),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            let _ = client.send_packet(P_FE2CL_PC_STOP, &resp);
        });

    let player = state.get_player_mut(pc_id)?;

    // TODO anticheat

    let entity_id = player.get_id();
    player.set_position(pos);
    let chunk = player.get_chunk_coords();
    state
        .entity_map
        .update(entity_id, Some(chunk), Some(clients));
    Ok(())
}

pub fn pc_vehicle_on(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;

            let vehicle = player
                .get_item(ItemLocation::Equip, EQUIP_SLOT_VEHICLE as usize)
                .unwrap();
            if vehicle.is_none() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} tried to mount a vehicle without one equipped",
                        pc_id
                    ),
                ));
            }
            let vehicle = vehicle.as_ref().unwrap();

            if let Some(vehicle_speed) = vehicle.get_stats()?.speed {
                player.vehicle_speed = Some(vehicle_speed);
            } else {
                log(
                    Severity::Fatal,
                    &format!("Vehicle has no speed: {:?}", vehicle),
                );
                panic!();
            }
            rusty_fusion::helpers::broadcast_state(
                pc_id,
                player.get_state_bit_flag(),
                clients,
                state,
            );

            let resp = sP_FE2CL_PC_VEHICLE_ON_SUCC { UNUSED: unused!() };
            clients
                .get_self()
                .send_packet(P_FE2CL_PC_VEHICLE_ON_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_PC_VEHICLE_ON_FAIL {
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_PC_VEHICLE_ON_FAIL, &resp)
        },
    )
}

pub fn pc_vehicle_off(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;

            player.vehicle_speed = None;
            rusty_fusion::helpers::broadcast_state(
                pc_id,
                player.get_state_bit_flag(),
                clients,
                state,
            );

            let resp = sP_FE2CL_PC_VEHICLE_OFF_SUCC { UNUSED: unused!() };
            clients
                .get_self()
                .send_packet(P_FE2CL_PC_VEHICLE_OFF_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_PC_VEHICLE_OFF_FAIL {
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_PC_VEHICLE_OFF_FAIL, &resp)
        },
    )
}

pub fn pc_special_state_switch(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_SPECIAL_STATE_SWITCH =
        client.get_packet(P_CL2FE_REQ_PC_SPECIAL_STATE_SWITCH)?;

    let player = state.get_player_mut(pc_id)?;
    let special_state = player.update_special_state(pkt.iSpecialStateFlag);

    let resp = sP_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC {
        iPC_ID: pkt.iPC_ID,
        iReqSpecialStateFlag: pkt.iSpecialStateFlag,
        iSpecialState: special_state,
    };
    client.send_packet(P_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC, &resp)
}

pub fn pc_first_use_flag_set(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_FIRST_USE_FLAG_SET =
        client.get_packet(P_CL2FE_REQ_PC_FIRST_USE_FLAG_SET)?;

    let player = state.get_player_mut(pc_id)?;
    player.update_first_use_flag(pkt.iFlagCode)?;
    Ok(())
}

pub fn pc_change_mentor(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_CHANGE_MENTOR = *client.get_packet(P_CL2FE_REQ_PC_CHANGE_MENTOR)?;
    catch_fail(
        (|| {
            let player = state.get_player_mut(client.get_player_id()?)?;
            let guide_count = player.update_guide(pkt.iMentor.try_into()?);

            let resp = sP_FE2CL_REP_PC_CHANGE_MENTOR_SUCC {
                iMentor: pkt.iMentor,
                iMentorCnt: guide_count as i16,
                iFusionMatter: player.get_fusion_matter() as i32,
            };
            client.send_packet(P_FE2CL_REP_PC_CHANGE_MENTOR_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_CHANGE_MENTOR_FAIL {
                iMentor: pkt.iMentor,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_CHANGE_MENTOR_FAIL, &resp)
        },
    )
}
