use rusty_fusion::{
    chunk::InstanceID,
    database::db_run_sync,
    defines::*,
    entity::{BuddyListEntry, Entity, EntityID, PlayerSearchQuery},
    error::{codes::BuddyWarpErr, *},
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    state::ShardServerState,
    util,
};

const ERROR_CODE_BUDDY_DENY: i32 = 6;

pub fn get_buddy_state(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;

    let mut req = sP_FE2LS_REQ_GET_BUDDY_STATE {
        iPC_UID: player.get_uid(),
        aBuddyUID: [0; 50],
    };

    let buddy_info = player.get_all_buddy_info();
    for (i, buddy_uid) in buddy_info.iter().map(|info| info.pc_uid).enumerate() {
        req.aBuddyUID[i] = buddy_uid;
    }

    if let Some(login_server) = clients.get_login_server() {
        log_if_failed(login_server.send_packet(P_FE2LS_REQ_GET_BUDDY_STATE, &req));
        Ok(())
    } else {
        Err(FFError::build(
            Severity::Warning,
            "No login server connected".to_string(),
        ))
    }
}

pub fn request_make_buddy(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_REQUEST_MAKE_BUDDY =
        *client.get_packet(P_CL2FE_REQ_REQUEST_MAKE_BUDDY)?;

    let pc_id = client.get_player_id()?;
    let buddy_id = pkt.iBuddyID;
    let buddy_uid = pkt.iBuddyPCUID;

    state.entity_map.validate_proximity(
        &[EntityID::Player(pc_id), EntityID::Player(buddy_id)],
        RANGE_INTERACT,
    )?;

    let player = state.get_player(pc_id)?;
    if player.is_buddies_with(buddy_uid) {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} is already buddies with player {}", player, buddy_uid),
        ));
    }

    if player.get_num_buddies() >= SIZEOF_BUDDYLIST_SLOT as usize {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} has too many buddies", player),
        ));
    }

    let req_pkt = sP_FE2CL_REP_REQUEST_MAKE_BUDDY_SUCC_TO_ACCEPTER {
        iRequestID: pc_id,
        iBuddyID: buddy_id,
        szFirstName: util::encode_utf16(&player.first_name),
        szLastName: util::encode_utf16(&player.last_name),
    };

    let buddy = state.get_player(buddy_id)?;
    if buddy.get_uid() != buddy_uid {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Buddy UID mismatch (client: {}, server: {})",
                buddy_uid,
                buddy.get_uid()
            ),
        ));
    }
    if buddy.get_num_buddies() >= SIZEOF_BUDDYLIST_SLOT as usize {
        // instant deny
        let deny_pkt = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_FAIL {
            iBuddyID: buddy_id,
            iBuddyPCUID: buddy_uid,
            iErrorCode: ERROR_CODE_BUDDY_DENY,
        };
        return client.send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_FAIL, &deny_pkt);
    }

    let buddy_client = buddy.get_client(clients).unwrap();
    if buddy_client
        .send_packet(P_FE2CL_REP_REQUEST_MAKE_BUDDY_SUCC_TO_ACCEPTER, &req_pkt)
        .is_ok()
    {
        let player = state.get_player_mut(pc_id).unwrap();
        player.buddy_offered_to = Some(buddy_uid);
    }

    Ok(())
}

pub fn find_name_make_buddy(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_FIND_NAME_MAKE_BUDDY =
        *client.get_packet(P_CL2FE_REQ_PC_FIND_NAME_MAKE_BUDDY)?;

    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;
    let pc_uid = player.get_uid();
    if player.get_num_buddies() >= SIZEOF_BUDDYLIST_SLOT as usize {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} has too many buddies", player),
        ));
    }

    let first_name = util::parse_utf16(&pkt.szFirstName)?;
    let last_name = util::parse_utf16(&pkt.szLastName)?;

    let search = PlayerSearchQuery::ByName(first_name, last_name);
    let res = search.execute(state);
    if res.is_none() {
        // TODO cross-shard
        return Ok(());
    }
    let buddy_id = res.unwrap();

    let buddy = state.get_player(buddy_id).unwrap();
    let buddy_uid = buddy.get_uid();
    if buddy.is_buddies_with(pc_uid) {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} is already buddies with player {}", player, buddy_id),
        ));
    }

    if buddy.get_num_buddies() >= SIZEOF_BUDDYLIST_SLOT as usize {
        // instant deny
        let deny_pkt = sP_FE2CL_REP_PC_FIND_NAME_MAKE_BUDDY_FAIL {
            iErrorCode: ERROR_CODE_BUDDY_DENY,
            szFirstName: pkt.szFirstName,
            szLastName: pkt.szLastName,
        };
        let client = clients.get_self();
        return client.send_packet(P_FE2CL_REP_PC_FIND_NAME_MAKE_BUDDY_FAIL, &deny_pkt);
    }

    let buddy_client = buddy.get_client(clients).unwrap();
    let player = state.get_player_mut(pc_id).unwrap();
    player.buddy_offered_to = Some(buddy_uid);
    let req_pkt = sP_FE2CL_REP_PC_FIND_NAME_MAKE_BUDDY_SUCC {
        szFirstName: util::encode_utf16(&player.first_name),
        szLastName: util::encode_utf16(&player.last_name),
        iPCUID: pc_uid,
        iNameCheckFlag: player.flags.name_check as i8,
    };
    log_if_failed(buddy_client.send_packet(P_FE2CL_REP_PC_FIND_NAME_MAKE_BUDDY_SUCC, &req_pkt));
    Ok(())
}

pub fn accept_make_buddy(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_ACCEPT_MAKE_BUDDY = *client.get_packet(P_CL2FE_REQ_ACCEPT_MAKE_BUDDY)?;

    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;
    let player_buddy_info = BuddyListEntry::new(player);
    let pc_uid = state.get_player(pc_id)?.get_uid();
    let buddy_id = pkt.iBuddyID;
    let accepted = pkt.iAcceptFlag == 1;

    let buddy = state.get_player_mut(buddy_id)?;
    let buddy_uid = buddy.get_uid();
    if buddy.buddy_offered_to != Some(pc_uid) {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} did not send buddy request to player {}", buddy, pc_id),
        ));
    }
    buddy.buddy_offered_to = None;

    catch_fail(
        (|| {
            let buddy = state.get_player_mut(buddy_id).unwrap(); // re-borrow
            if !accepted {
                // this failure will be caught and the deny packet will be sent
                return Err(FFError::build(
                    Severity::Debug,
                    format!("{} denied buddy request from player {}", buddy, pc_id),
                ));
            }

            // player -> buddy
            let pkt_buddy = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC {
                iBuddySlot: buddy.add_buddy(player_buddy_info.clone())? as i8,
                BuddyInfo: player_buddy_info.into(),
            };
            log_if_failed(
                buddy
                    .get_client(clients)
                    .unwrap()
                    .send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC, &pkt_buddy),
            );

            // buddy -> player
            let buddy_buddy_info = BuddyListEntry::new(buddy);
            let player = state.get_player_mut(pc_id).unwrap();
            let pkt_player = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC {
                iBuddySlot: player.add_buddy(buddy_buddy_info.clone())? as i8,
                BuddyInfo: buddy_buddy_info.into(),
            };
            log_if_failed(
                clients
                    .get_self()
                    .send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC, &pkt_player),
            );

            Ok(())
        })(),
        || {
            let player = state.get_player_mut(pc_id).unwrap();
            let _ = player.remove_buddy(buddy_uid);

            let buddy = state.get_player_mut(buddy_id).unwrap();
            let _ = buddy.remove_buddy(pc_uid);

            // we send the deny packet to the buddy in case of failure
            let deny_pkt = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_FAIL {
                iBuddyID: pc_id,
                iBuddyPCUID: pc_uid,
                iErrorCode: ERROR_CODE_BUDDY_DENY,
            };
            let buddy_client = buddy.get_client(clients).unwrap();
            log_if_failed(buddy_client.send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_FAIL, &deny_pkt));
            Ok(())
        },
    )
}

pub fn find_name_accept_buddy(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_FIND_NAME_ACCEPT_BUDDY =
        *client.get_packet(P_CL2FE_REQ_PC_FIND_NAME_ACCEPT_BUDDY)?;
    let accepted = pkt.iAcceptFlag == 1;

    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;
    let player_buddy_info = BuddyListEntry::new(player);
    let pc_uid = player.get_uid();
    if player.get_num_buddies() >= SIZEOF_BUDDYLIST_SLOT as usize {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} has too many buddies", player),
        ));
    }

    let buddy_uid = pkt.iBuddyPCUID;
    let search = PlayerSearchQuery::ByUID(buddy_uid);
    let res = search.execute(state);
    if res.is_none() {
        // TODO cross-shard
        return Ok(());
    }
    let buddy_id = res.unwrap();

    let buddy = state.get_player_mut(buddy_id)?;
    if buddy.buddy_offered_to != Some(pc_uid) {
        return Err(FFError::build(
            Severity::Warning,
            format!("{} did not send buddy request to player {}", buddy, pc_id),
        ));
    }
    buddy.buddy_offered_to = None;

    catch_fail(
        (|| {
            let buddy = state.get_player_mut(buddy_id).unwrap(); // re-borrow
            if !accepted {
                // this failure will be caught and the deny packet will be sent
                return Err(FFError::build(
                    Severity::Debug,
                    format!("{} denied buddy request from player {}", buddy, pc_id),
                ));
            }

            // player -> buddy
            let pkt_buddy = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC {
                iBuddySlot: buddy.add_buddy(player_buddy_info.clone())? as i8,
                BuddyInfo: player_buddy_info.into(),
            };
            log_if_failed(
                buddy
                    .get_client(clients)
                    .unwrap()
                    .send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC, &pkt_buddy),
            );

            // buddy -> player
            let buddy_buddy_info = BuddyListEntry::new(buddy);
            let player = state.get_player_mut(pc_id).unwrap();
            let pkt_player = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC {
                iBuddySlot: player.add_buddy(buddy_buddy_info.clone())? as i8,
                BuddyInfo: buddy_buddy_info.into(),
            };
            log_if_failed(
                clients
                    .get_self()
                    .send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_SUCC, &pkt_player),
            );

            Ok(())
        })(),
        || {
            let player = state.get_player_mut(pc_id).unwrap();
            let _ = player.remove_buddy(buddy_uid);

            let buddy = state.get_player_mut(buddy_id).unwrap();
            let _ = buddy.remove_buddy(pc_uid);

            // we send the deny packet to the buddy in case of failure
            let deny_pkt = sP_FE2CL_REP_ACCEPT_MAKE_BUDDY_FAIL {
                iBuddyID: pc_id,
                iBuddyPCUID: pc_uid,
                iErrorCode: ERROR_CODE_BUDDY_DENY,
            };
            let buddy_client = buddy.get_client(clients).unwrap();
            log_if_failed(buddy_client.send_packet(P_FE2CL_REP_ACCEPT_MAKE_BUDDY_FAIL, &deny_pkt));
            Ok(())
        },
    )
}

pub fn pc_buddy_warp(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_BUDDY_WARP = *client.get_packet(P_CL2FE_REQ_PC_BUDDY_WARP)?;

    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;
    let player_uid = player.get_player_uid();
    let player_is_on_skyway = player.get_skyway_ride().is_some();
    let player_payzone_flag = player.get_payzone_flag();
    let player_is_warp_on_cooldown = player.is_warp_on_cooldown();
    let buddy_uid = pkt.iBuddyPCUID;

    let mut invalid_warp = |msg: String| -> FFResult<()> {
        let response = sP_FE2CL_REP_PC_BUDDY_WARP_FAIL {
            iBuddyPCUID: buddy_uid,
            iErrorCode: BuddyWarpErr::CantWarpToLocation as i32,
        };
        log_if_failed(
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_BUDDY_WARP_FAIL, &response),
        );
        Err(FFError::build(Severity::Info, msg))
    };

    if !player.is_buddies_with(buddy_uid) {
        return invalid_warp(format!(
            "Buddy {} is not buddies with player {}",
            player_uid, buddy_uid
        ));
    }

    if player_is_on_skyway {
        return invalid_warp(format!(
            "Player {} is currently on a skyway ride",
            player_uid
        ));
    }

    if player_is_warp_on_cooldown {
        return invalid_warp(format!("Player {}'s buddy warp is on cooldown", player_uid));
    }

    let search = PlayerSearchQuery::ByUID(buddy_uid);
    let res = search.execute(state);
    if res.is_none() {
        let login_server = match clients.get_login_server() {
            Some(ls) => ls,
            None => {
                return Err(FFError::build(
                    Severity::Warning,
                    "No login server connected for cross-shard buddy warp".to_string(),
                ));
            }
        };

        let req_pkt = sP_FE2LS_REQ_BUDDY_WARP {
            iPCPayzoneFlag: player_payzone_flag as i8,
            iFromPCUID: player_uid,
            iBuddyPCUID: buddy_uid,
        };

        return login_server.send_packet(P_FE2LS_REQ_BUDDY_WARP, &req_pkt);
    }

    let buddy_id = res.unwrap();

    let buddy = state.get_player_mut(buddy_id)?;
    let buddy_is_on_skyway = buddy.get_skyway_ride().is_some();
    let buddy_payzone_flag = buddy.get_payzone_flag();
    let buddy_instance_id = buddy.get_instance_id();
    let buddy_position = buddy.get_position();

    if buddy_is_on_skyway {
        return invalid_warp(format!(
            "Player {} is currently on a skyway ride",
            buddy_uid
        ));
    }

    if player_payzone_flag != buddy_payzone_flag {
        return invalid_warp(format!(
            "Buddy {} is in a different payzone state",
            buddy_uid,
        ));
    }

    if buddy_instance_id.map_num != ID_OVERWORLD {
        return invalid_warp(format!(
            "Buddy {} is not in the overworld instance",
            buddy_uid,
        ));
    }

    catch_fail(
        {
            let player = state.get_player_mut(pc_id).unwrap();
            player.set_position(buddy_position);
            player.set_instance_id(InstanceID {
                map_num: buddy_instance_id.map_num,
                channel_num: buddy_instance_id.channel_num,
                instance_num: None,
            });
            let player_saved = player.clone();

            log_if_failed(db_run_sync(move |db| db.save_player(&player_saved)));

            state
                .entity_map
                .update(EntityID::Player(pc_id), None, Some(clients));

            // this packet in client code seems to just leave group
            let same_shard_succ_pkt = sP_FE2CL_REP_PC_BUDDY_WARP_SAME_SHARD_SUCC { UNUSED: 0 };

            // this packet in client code loads the new position
            let goto_succ_pkt = sP_FE2CL_REP_PC_GOTO_SUCC {
                iX: buddy_position.x,
                iY: buddy_position.y,
                iZ: buddy_position.z,
            };

            let result = clients
                .get_self()
                .send_packet(
                    P_FE2CL_REP_PC_BUDDY_WARP_SAME_SHARD_SUCC,
                    &same_shard_succ_pkt,
                )
                .and_then(|_| {
                    clients
                        .get_self()
                        .send_packet(P_FE2CL_REP_PC_GOTO_SUCC, &goto_succ_pkt)
                });

            result
        },
        || {
            let response = sP_FE2CL_REP_PC_BUDDY_WARP_FAIL {
                iBuddyPCUID: buddy_uid,
                iErrorCode: BuddyWarpErr::CantWarpToLocation as i32,
            };
            log_if_failed(
                clients
                    .get_self()
                    .send_packet(P_FE2CL_REP_PC_BUDDY_WARP_FAIL, &response),
            );
            Ok(())
        },
    )
}
