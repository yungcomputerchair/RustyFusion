use std::time::{Duration, SystemTime};

use rusty_fusion::{
    chunk::{TickMode, MAP_SQUARE_SIZE},
    config::config_get,
    database::db_run_sync,
    defines::*,
    entity::{Combatant, Entity, EntityID, Player},
    enums::*,
    error::*,
    net::{
        crypto::{self, EncryptionMode},
        packet::{PacketID::*, *},
        ClientMap, ClientType, FFClient,
    },
    state::ShardServerState,
    tabledata::tdata_get,
    unused, util, Position,
};

pub fn pc_enter(
    clients: &mut ClientMap,
    key: usize,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_ENTER = *client.get_packet(P_CL2FE_REQ_PC_ENTER)?;
    let serial_key: i64 = pkt.iEnterSerialKey;
    let login_data = state.login_data.remove(&serial_key).unwrap();

    // check if this player is already in the shard and kick if so.
    // important that we save the current player to DB first to avoid state desync
    if let Some(existing_pc_id) = state
        .entity_map
        .find_players(|p| p.get_uid() == login_data.iPC_UID)
        .first()
        .copied()
    {
        log(
            Severity::Warning,
            &format!(
                "Player with UID {} already in the shard as player {}; kicking...",
                login_data.iPC_UID, existing_pc_id
            ),
        );
        let existing_player = state.get_player(existing_pc_id).unwrap();
        let existing_client = existing_player.get_client(clients).unwrap();
        let pkt = sP_FE2CL_REP_PC_EXIT_DUPLICATE {
            iErrorCode: unused!(),
        };
        log_if_failed(existing_client.send_packet(P_FE2CL_REP_PC_EXIT_DUPLICATE, &pkt));
        Player::disconnect(existing_pc_id, state, clients); // saves to DB synchronously
    }

    let pc_id = state.entity_map.gen_next_pc_id();
    let mut player =
        db_run_sync(move |db| db.load_player(login_data.iAccountID, login_data.iPC_UID))?;
    player.set_player_id(pc_id);
    player.set_client_id(key);

    let channel_num = state.entity_map.get_min_pop_channel_num();
    player.instance_id.channel_num = channel_num;

    let resp = sP_FE2CL_REP_PC_ENTER_SUCC {
        iID: pc_id,
        PCLoadData2CL: player.get_load_data(),
        uiSvrTime: util::get_timestamp_ms(time),
    };

    let client = clients.get_self();
    client.client_type = ClientType::GameClient {
        account_id: login_data.iAccountID,
        serial_key: pkt.iEnterSerialKey,
        pc_id: Some(pc_id),
    };

    let iv1: i32 = resp.iID + 1;
    let iv2: i32 = resp.PCLoadData2CL.iFusionMatter + 1;
    client.e_key = crypto::gen_key(resp.uiSvrTime, iv1, iv2);
    client.fe_key = login_data.uiFEKey.to_le_bytes();
    client.enc_mode = EncryptionMode::FEKey;

    let pkt_pc = sP_FE2LS_UPDATE_PC_SHARD {
        iPC_UID: player.get_uid(),
        ePSS: PlayerShardStatus::Entered as i8,
    };
    let pkt_chan = sP_FE2LS_UPDATE_CHANNEL_STATUSES {
        aChannelStatus: state.entity_map.get_channel_statuses().map(|s| s as u8),
    };
    let pkt_motd = sP_FE2LS_REQ_MOTD { iPC_ID: pc_id };
    match clients.get_login_server() {
        Some(login_server) => {
            log_if_failed(login_server.send_packet(P_FE2LS_UPDATE_PC_SHARD, &pkt_pc));
            log_if_failed(login_server.send_packet(P_FE2LS_UPDATE_CHANNEL_STATUSES, &pkt_chan));
            log_if_failed(login_server.send_packet(P_FE2LS_REQ_MOTD, &pkt_motd));
        }
        None => {
            log(
                Severity::Warning,
                "P_CL2FE_REQ_PC_ENTER: No login server connected! Things may break.",
            );
        }
    }

    log(
        Severity::Info,
        &format!(
            "{} joined (channel {})",
            player, player.instance_id.channel_num
        ),
    );
    state.entity_map.track(Box::new(player), TickMode::Always);

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_ENTER_SUCC, &resp)
}

pub fn pc_exit(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.clear_player_id()?;
    Player::disconnect(pc_id, state, clients);
    let resp = sP_FE2CL_REP_PC_EXIT_SUCC {
        iID: pc_id,
        iExitCode: EXIT_CODE_REQ_BY_PC as i32,
    };
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_EXIT_SUCC, &resp)
}

pub fn pc_loading_complete(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let _pkt: &sP_CL2FE_REQ_PC_LOADING_COMPLETE = clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_LOADING_COMPLETE)?;
    let resp = sP_FE2CL_REP_PC_LOADING_COMPLETE_SUCC { iPC_ID: unused!() };
    catch_fail(
        (|| {
            let pc_id = clients.get_self().get_player_id()?;
            let player = state.get_player(pc_id)?;
            let map_num = player.instance_id.map_num;
            let chunk = player.get_chunk_coords();
            state
                .entity_map
                .update(player.get_id(), Some(chunk), Some(clients));
            let client = clients.get_self();
            client.send_packet(P_FE2CL_REP_PC_LOADING_COMPLETE_SUCC, &resp)?;

            // map info sync
            if map_num != ID_OVERWORLD {
                let map_data = tdata_get().get_map_data(map_num)?;
                let x_min = map_data.map_square.0 * MAP_SQUARE_SIZE;
                let y_min = map_data.map_square.1 * MAP_SQUARE_SIZE;
                let mut map_info_pkt = sP_FE2CL_INSTANCE_MAP_INFO {
                    iInstanceMapNum: map_num as i32,
                    iCreateTick: unused!(),
                    iMapCoordX_Min: x_min,
                    iMapCoordX_Max: x_min + MAP_SQUARE_SIZE,
                    iMapCoordY_Min: y_min,
                    iMapCoordY_Max: y_min + MAP_SQUARE_SIZE,
                    iMapCoordZ_Min: i32::MIN,
                    iMapCoordZ_Max: i32::MAX,
                    iEP_ID: unused!(),
                    iEPTopRecord_Score: unused!(),
                    iEPTopRecord_Rank: unused!(),
                    iEPTopRecord_Time: unused!(),
                    iEPTopRecord_RingCount: unused!(),
                    iEPSwitch_StatusON_Cnt: unused!(),
                };

                if let Some(ep_id) = map_data.ep_id {
                    map_info_pkt.iEP_ID = ep_id as i32;
                    // TODO remaining EP data
                }
                client.send_packet(P_FE2CL_INSTANCE_MAP_INFO, &map_info_pkt)?;
            }

            // buddy list sync.
            // we only want to do this once and we can't do it on initial load
            let player = state.get_player_mut(pc_id).unwrap();
            if !player.buddy_list_synced {
                let buddy_info = player.get_all_buddy_info();
                let buddy_list_pkt = sP_FE2CL_REP_PC_BUDDYLIST_INFO_SUCC {
                    iID: unused!(),
                    iPCUID: unused!(),
                    iListNum: 0, // we don't need to chunk the buddy list
                    iBuddyCnt: buddy_info.len() as i8,
                };
                client.queue_packet(P_FE2CL_REP_PC_BUDDYLIST_INFO_SUCC, &buddy_list_pkt);
                for entry in buddy_info {
                    let buddy_pkt: sBuddyBaseInfo = entry.into();
                    client.queue_struct(&buddy_pkt);
                }
                client.flush()?;
                player.buddy_list_synced = true;
            }

            Ok(())
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

    // TODO anticheat

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
            client.send_packet(P_FE2CL_PC_MOVE, &resp)
        });

    let player = state.get_player_mut(pc_id)?;
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

    // TODO anticheat

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
            client.send_packet(P_FE2CL_PC_JUMP, &resp)
        });

    let player = state.get_player_mut(pc_id)?;
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

    // TODO anticheat

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
            client.send_packet(P_FE2CL_PC_STOP, &resp)
        });

    let player = state.get_player_mut(pc_id)?;
    let entity_id = player.get_id();
    player.set_position(pos);
    let chunk = player.get_chunk_coords();
    state
        .entity_map
        .update(entity_id, Some(chunk), Some(clients));
    Ok(())
}

pub fn pc_movetransportation(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_MOVETRANSPORTATION =
        client.get_packet(P_CL2FE_REQ_PC_MOVETRANSPORTATION)?;
    let pos = Position {
        x: pkt.iX,
        y: pkt.iY,
        z: pkt.iZ,
    };
    let angle = pkt.iAngle;

    let _slider = state.get_slider(pkt.iT_ID)?;
    // TODO anticheat

    let resp = sP_FE2CL_PC_MOVETRANSPORTATION {
        iCliTime: pkt.iCliTime,
        iLcX: pkt.iLcX,
        iLcY: pkt.iLcY,
        iLcZ: pkt.iLcZ,
        iX: pkt.iX,
        iY: pkt.iY,
        iZ: pkt.iZ,
        fVX: pkt.fVX,
        fVY: pkt.fVY,
        fVZ: pkt.fVZ,
        iT_ID: pkt.iT_ID,
        iAngle: pkt.iAngle,
        cKeyValue: pkt.cKeyValue,
        iSpeed: pkt.iSpeed,
        iPC_ID: pc_id,
        iSvrTime: util::get_timestamp_ms(time),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            client.send_packet(P_FE2CL_PC_MOVETRANSPORTATION, &resp)
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

pub fn pc_transport_warp(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_PC_TRANSPORT_WARP = client.get_packet(P_CL2FE_REQ_PC_TRANSPORT_WARP)?;

    let slider = state.get_slider(pkt.iTransport_ID)?;
    let resp = sP_FE2CL_REP_PC_TRANSPORT_WARP_SUCC {
        TransportationAppearanceData: slider.get_appearance_data(),
        iLcX: pkt.iLcX,
        iLcY: pkt.iLcY,
        iLcZ: pkt.iLcZ,
    };

    client.send_packet(P_FE2CL_REP_PC_TRANSPORT_WARP_SUCC, &resp)
}

pub fn pc_vehicle_on(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;

            if player.instance_id.map_num != ID_OVERWORLD {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} tried to mount a vehicle outside the overworld: {}",
                        pc_id, player.instance_id
                    ),
                ));
            }

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
                panic_log(&format!("Vehicle has no speed: {:?}", vehicle));
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

    match pkt.iSpecialStateFlag as u32 {
        CN_SPECIAL_STATE_FLAG__FULL_UI => {
            player.in_menu = !player.in_menu;
        }
        _ => {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "P_CL2FE_REQ_PC_SPECIAL_STATE_SWITCH: invalid special state flag: {}",
                    pkt.iSpecialStateFlag
                ),
            ));
        }
    }

    let special_state_flags = player.get_special_state_bit_flag();

    let resp = sP_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC {
        iPC_ID: pc_id,
        iReqSpecialStateFlag: pkt.iSpecialStateFlag,
        iSpecialState: special_state_flags,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_SPECIAL_STATE_CHANGE, &resp)
        });
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC, &resp)
}

pub fn pc_combat_begin_end(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
    in_combat: bool,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;

    let player = state.get_player_mut(pc_id)?;
    player.in_combat = in_combat; // TODO anticheat
    if !in_combat {
        player.reset();
    }

    let special_state_flags = player.get_special_state_bit_flag();

    let resp = sP_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC {
        iPC_ID: pc_id,
        iReqSpecialStateFlag: CN_SPECIAL_STATE_FLAG__COMBAT as i8,
        iSpecialState: special_state_flags,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_SPECIAL_STATE_CHANGE, &resp)
        });
    Ok(())
}

fn get_respawn_point(pos: Position, map_num: u32) -> Position {
    let respawn_pos = tdata_get().get_nearest_respawn_point(pos, map_num);
    respawn_pos.unwrap_or_else(|| {
        log(
            Severity::Warning,
            &format!("Couldn't find a respawn point in map {}", map_num),
        );
        pos
    })
}

pub fn pc_regen(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    const WARP_AWAY_COOLDOWN: Duration = Duration::from_secs(60);

    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_REGEN = client.get_packet(P_CL2FE_REQ_PC_REGEN)?;
    let revive_type: PCRegenType = pkt.iRegenType.try_into()?;

    let player = state.get_player_mut(pc_id)?;
    let new_chunk_coords = match revive_type {
        PCRegenType::Xcom => {
            if !player.is_dead() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("{} tried to revive while not dead", player),
                ));
            }
            player.set_position(get_respawn_point(
                player.get_position(),
                player.instance_id.map_num,
            ));
            Some(player.get_chunk_coords())
        }
        PCRegenType::HereByPhoenix => todo!(),
        PCRegenType::HereByPhoenixGroup => todo!(),
        PCRegenType::Unstick => {
            // check warp away timer
            if let Some(last_warp_away) = player.last_warp_away_time {
                let time_now = SystemTime::now();
                if time_now.duration_since(last_warp_away).unwrap_or_default() < WARP_AWAY_COOLDOWN
                {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("{} tried to warp away too soon", player),
                    ));
                } else {
                    player.last_warp_away_time = Some(time_now);
                }
            }

            player.set_position(get_respawn_point(
                player.get_position(),
                player.instance_id.map_num,
            ));
            Some(player.get_chunk_coords())
        }
        other => {
            return Err(FFError::build(
                Severity::Warning,
                format!("Unsupported regen type: {:?}", other),
            ));
        }
    };
    player.do_revive();

    let (regen_data, regen_data_bcast) = player.get_regen_data();

    let resp = sP_FE2CL_REP_PC_REGEN_SUCC {
        PCRegenData: regen_data,
        iFusionMatter: player.get_fusion_matter() as i32,
        // we do NOT want the client to do GC. this is because we re-chunk the player serverside.
        // we can't de-chunk and wait for the client to ask for re-chunking because the player
        // might be in an instance that would get cleaned up prematurely.
        bMoveLocation: 0,
    };
    log_if_failed(client.send_packet(P_FE2CL_REP_PC_REGEN_SUCC, &resp));
    if let Some(new_chunk) = new_chunk_coords {
        state
            .entity_map
            .update(EntityID::Player(pc_id), Some(new_chunk), Some(clients));
    }

    let bcast = sP_FE2CL_PC_REGEN {
        PCRegenDataForOtherPC: regen_data_bcast,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_REGEN, &bcast)
        });
    Ok(())
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
            if player.get_level() < 4 {
                return Err(FFError::build(
                    Severity::Warning,
                    "Player tried to change mentor before level 4".to_string(),
                ));
            }

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

pub fn pc_channel_num(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let player = state.get_player(client.get_player_id()?)?;
    let resp = sP_FE2CL_REP_PC_CHANNEL_NUM {
        iChannelNum: player.instance_id.channel_num as i32,
    };
    client.send_packet(P_FE2CL_REP_PC_CHANNEL_NUM, &resp)
}

pub fn pc_channel_info(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let player = state.get_player(client.get_player_id()?)?;
    let channel_num = player.instance_id.channel_num;
    let num_channels = config_get().shard.num_channels.get();
    let resp = sP_FE2CL_REP_CHANNEL_INFO {
        iCurrChannelNum: channel_num as i32,
        iChannelCnt: num_channels as i32,
    };
    client.queue_packet(P_FE2CL_REP_CHANNEL_INFO, &resp);
    for channel_num in 1..=num_channels {
        let channel_info = sChannelInfo {
            iChannelNum: channel_num as i32,
            iCurrentUserCnt: state.entity_map.get_channel_population(channel_num) as i32,
        };
        client.queue_struct(&channel_info); // will panic if you have more than 127 channels :)
    }
    client.flush()
}

pub fn pc_warp_channel(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_WARP_CHANNEL =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_WARP_CHANNEL)?;
    let mut error_code = 0;
    catch_fail(
        (|| {
            let pc_id = clients.get_self().get_player_id()?;
            let channel_num = pkt.iChannelNum as usize;
            let num_channels = config_get().shard.num_channels.get();

            if channel_num == 0 || channel_num > num_channels {
                error_code = 3; // "the channel number is invalid."
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Invalid channel number: {}", channel_num),
                ));
            }

            let max_channel_pop = config_get().shard.max_channel_pop.get();
            if state.entity_map.get_channel_population(channel_num) >= max_channel_pop {
                error_code = 4; // "the channel is full."
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Channel {} is full", channel_num),
                ));
            }

            let player = state.get_player_mut(pc_id)?;
            if player.instance_id.channel_num == channel_num {
                error_code = 2; // "you're already in the channel."
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Player {} is already in channel {}", pc_id, channel_num),
                ));
            }

            player.instance_id.channel_num = channel_num;
            let chunk_coords = player.get_chunk_coords();

            let resp = sP_FE2CL_REP_PC_WARP_CHANNEL_SUCC { UNUSED: unused!() };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_CHANNEL_SUCC, &resp)?;

            state
                .entity_map
                .update(EntityID::Player(pc_id), Some(chunk_coords), Some(clients));

            if let Some(login_server) = clients.get_login_server() {
                let pkt_chan = sP_FE2LS_UPDATE_CHANNEL_STATUSES {
                    aChannelStatus: state.entity_map.get_channel_statuses().map(|s| s as u8),
                };
                log_if_failed(login_server.send_packet(P_FE2LS_UPDATE_CHANNEL_STATUSES, &pkt_chan));
            }
            Ok(())
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_WARP_CHANNEL_FAIL {
                iErrorCode: error_code,
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_CHANNEL_FAIL, &resp)
        },
    )
}
