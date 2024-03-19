use rusty_fusion::{
    defines::*,
    entity::{Combatant, Entity, EntityID},
    enums::*,
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    state::ShardServerState,
    tabledata::tdata_get,
    unused,
};

pub fn regist_transportation_location(
    client: &mut FFClient,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_REGIST_TRANSPORTATION_LOCATION =
        *client.get_packet(P_CL2FE_REQ_REGIST_TRANSPORTATION_LOCATION)?;
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let npc = state.get_npc(pkt.iNPC_ID)?;
            let npc_type = npc.ty;
            state
                .entity_map
                .validate_proximity(&[EntityID::Player(pc_id), npc.get_id()], RANGE_INTERACT)?;

            let player = state.get_player_mut(pc_id)?;
            let transport_type: TransportationType = pkt.eTT.try_into()?;
            match transport_type {
                TransportationType::Warp => {
                    let location_data = tdata_get().get_scamper_data(pkt.iLocationID)?;
                    if location_data.npc_type != npc_type {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to register a S.C.A.M.P.E.R. location with the wrong NPC type",
                                player.get_player_id()
                            ),
                        ));
                    }
                    player.unlock_scamper_location(pkt.iLocationID)?;
                }
                TransportationType::Wyvern => {
                    let location_data = tdata_get().get_skyway_data(pkt.iLocationID)?;
                    if location_data.npc_type != npc_type {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to register a Skyway location with the wrong NPC type",
                                player.get_player_id()
                            ),
                        ));
                    }
                    player.unlock_skyway_location(pkt.iLocationID)?;
                }
                other => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Can't register transportation type {:?}", other),
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

            let npc = state.get_npc(pkt.iNPC_ID)?;
            let npc_type = npc.ty;
            state
                .entity_map
                .validate_proximity(&[EntityID::Player(pc_id), npc.get_id()], RANGE_INTERACT)?;

            let player = state.get_player_mut(pc_id)?;
            let trip_id = pkt.iTransporationID;
            let trip = tdata_get().get_trip_data(trip_id)?;
            if player.get_taros() < trip.cost {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} doesn't have enough taros to warp",
                        player.get_player_id()
                    ),
                ));
            }

            let new_taros = player.get_taros() - trip.cost;
            match trip.transportation_type {
                TransportationType::Warp => {
                    let src_data = tdata_get().get_scamper_data(trip.start_location)?;
                    if !player
                        .is_scamper_location_unlocked(trip.start_location)
                        .unwrap()
                    {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to warp from an unregistered S.C.A.M.P.E.R. location",
                                player.get_player_id()
                            ),
                        ));
                    }

                    let dest_data = tdata_get().get_scamper_data(trip.end_location)?;
                    if !player
                        .is_scamper_location_unlocked(trip.end_location)
                        .unwrap()
                    {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to warp to an unregistered S.C.A.M.P.E.R. location",
                                player.get_player_id()
                            ),
                        ));
                    }

                    if src_data.npc_type != npc_type {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to warp with the wrong NPC type",
                                player.get_player_id()
                            ),
                        ));
                    }

                    player.set_taros(new_taros);
                    player.set_position(dest_data.pos);
                }
                TransportationType::Wyvern => {
                    let src_data = tdata_get().get_skyway_data(trip.start_location)?;
                    if !player
                        .is_skyway_location_unlocked(trip.start_location)
                        .unwrap()
                    {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to warp from an unregistered Skyway location",
                                player.get_player_id()
                            ),
                        ));
                    }

                    if !player.is_skyway_location_unlocked(trip.end_location)? {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to warp to an unregistered Skyway location",
                                player.get_player_id()
                            ),
                        ));
                    }

                    if src_data.npc_type != npc_type {
                        return Err(FFError::build(
                            Severity::Warning,
                            format!(
                                "Player {} tried to warp with the wrong NPC type",
                                player.get_player_id()
                            ),
                        ));
                    }

                    let path = tdata_get().get_skyway_path(trip.route_number)?;
                    player.set_active_nano_slot(None).unwrap();
                    player.start_skyway_ride(trip, path);
                    // we don't charge the player until the ride is done
                }
                other => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Can't warp with transportation type {:?}", other),
                    ));
                }
            }

            let new_pos = player.get_position();
            let resp = sP_FE2CL_REP_PC_WARP_USE_TRANSPORTATION_SUCC {
                eTT: trip.transportation_type as i32,
                iX: new_pos.x,
                iY: new_pos.y,
                iZ: new_pos.z,
                iCandy: new_taros as i32,
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_USE_TRANSPORTATION_SUCC, &resp)?;

            if trip.transportation_type == TransportationType::Wyvern {
                rusty_fusion::helpers::broadcast_monkey(pc_id, RideType::Wyvern, clients, state);
            }
            Ok(())
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

pub fn warp_use_npc(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_WARP_USE_NPC =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_WARP_USE_NPC)?;
    catch_fail(
        (|| {
            let item_remaining = helpers::do_warp(
                clients,
                state,
                Some(pkt.iNPC_ID),
                pkt.iWarpID,
                pkt.eIL1,
                pkt.iItemSlot1 as usize,
                pkt.eIL2,
                pkt.iItemSlot2 as usize,
            )?;

            let client = clients.get_self();
            let player = state.get_player(client.get_player_id().unwrap()).unwrap();
            let pos = player.get_position();
            let taros_left = player.get_taros();
            let resp = sP_FE2CL_REP_PC_WARP_USE_NPC_SUCC {
                iX: pos.x,
                iY: pos.y,
                iZ: pos.z,
                eIL: pkt.eIL2,
                iItemSlotNum: pkt.iItemSlot2,
                Item: item_remaining.into(),
                iCandy: taros_left as i32,
            };
            client.send_packet(P_FE2CL_REP_PC_WARP_USE_NPC_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_WARP_USE_NPC_FAIL {
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_USE_NPC_FAIL, &resp)
        },
    )
}

pub fn time_to_go_warp(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TIME_TO_GO_WARP = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TIME_TO_GO_WARP)?;
    catch_fail(
        (|| {
            let player = state.get_player(clients.get_self().get_player_id()?)?;
            if player.is_future_done() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Player {} is in the past", player.get_player_id()),
                ));
            }

            let item_remaining = helpers::do_warp(
                clients,
                state,
                None,
                ID_TIME_MACHINE_WARP,
                pkt.eIL1,
                pkt.iItemSlot1 as usize,
                pkt.eIL2,
                pkt.iItemSlot2 as usize,
            )?;

            let client = clients.get_self();
            let player = state.get_player(client.get_player_id().unwrap()).unwrap();
            let pos = player.get_position();
            let taros_left = player.get_taros();
            let resp = sP_FE2CL_REP_PC_WARP_USE_NPC_SUCC {
                iX: pos.x,
                iY: pos.y,
                iZ: pos.z,
                eIL: pkt.eIL2,
                iItemSlotNum: pkt.iItemSlot2,
                Item: item_remaining.into(),
                iCandy: taros_left as i32,
            };
            client.send_packet(P_FE2CL_REP_PC_WARP_USE_NPC_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_WARP_USE_NPC_FAIL {
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_WARP_USE_NPC_FAIL, &resp)
        },
    )
}

mod helpers {
    #![allow(clippy::too_many_arguments)]

    use rusty_fusion::{chunk::InstanceID, defines::TYPE_TIME_MACHINE, item::Item, util};

    use super::*;

    pub fn do_warp(
        clients: &mut ClientMap,
        state: &mut ShardServerState,
        npc_id: Option<i32>,
        warp_id: i32,
        req_item_location_ord: i32,
        req_item_slot: usize,
        req_item_consume_location_ord: i32,
        req_item_consume_slot: usize,
    ) -> FFResult<Option<Item>> {
        let warp_data = tdata_get().get_warp_data(warp_id)?;
        let client = clients.get_self();
        let pc_id = client.get_player_id()?;

        let mut to_past = false;

        if let Some(npc_id) = npc_id {
            let npc = state.get_npc(npc_id)?;
            if npc.ty != warp_data.npc_type {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Player {} tried to warp with the wrong NPC type", pc_id),
                ));
            }

            // proximity check.
            // for some reason, the time machine NPC's range is HUGE
            // but we don't need to check anyway since it's a special case
            if npc.ty != TYPE_TIME_MACHINE {
                to_past = true;
                state
                    .entity_map
                    .validate_proximity(&[EntityID::Player(pc_id), npc.get_id()], RANGE_INTERACT)?;
            }
        }

        let player = state.get_player_mut(pc_id)?;

        // TODO group proximity check

        if player.get_taros() < warp_data.cost {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Player {} doesn't have enough taros to warp",
                    player.get_player_id()
                ),
            ));
        }

        if player.get_level() < warp_data.min_level {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "Player {} isn't a high enough level to warp ({} < {})",
                    player.get_player_id(),
                    player.get_level(),
                    warp_data.min_level
                ),
            ));
        }

        if let Some((item_type, item_id)) = warp_data.req_item {
            let item = player
                .get_item(req_item_location_ord.try_into()?, req_item_slot)?
                .as_ref();
            if !item.is_some_and(|item| item.ty == item_type && item.id == item_id) {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} doesn't have the required item ({:?}) to warp",
                        player.get_player_id(),
                        warp_data.req_item
                    ),
                ));
            }
        }

        let mut item_consumed = None;
        if let Some((item_type, item_id)) = warp_data.req_item_consumed {
            let item = player.get_item_mut(
                req_item_consume_location_ord.try_into()?,
                req_item_consume_slot,
            )?;
            if !item
                .as_mut()
                .is_some_and(|item| item.ty == item_type && item.id == item_id)
            {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} doesn't have the required consumed item ({:?}) to warp",
                        player.get_player_id(),
                        warp_data.req_item_consumed
                    ),
                ));
            }
            Item::split_items(item, 1); // consume item
            item_consumed = *item;
        }

        if to_past {
            player.set_future_done();
        }

        player.set_taros(player.get_taros() - warp_data.cost);
        player.set_pre_warp();
        player.set_position(warp_data.pos);
        let instance_id = InstanceID {
            channel_num: player.instance_id.channel_num,
            map_num: warp_data.map_num,
            instance_num: if warp_data.is_instance {
                // basically impossible to get 0, but we'll make sure anyway
                // since an instance ID of 0 represents no instance
                Some(util::rand_range_inclusive(1, u32::MAX))
            } else {
                None
            },
        };
        player.instance_id = instance_id;

        // force vehicle dismount
        player.vehicle_speed = None;
        rusty_fusion::helpers::broadcast_state(pc_id, player.get_state_bit_flag(), clients, state);

        // we remove the player from the chunk here and wait for PC_LOADING_COMPLETE to put them back.
        // it needs to be done this way or the client will miss the PC/NPC_ENTER packets.
        state
            .entity_map
            .update(EntityID::Player(pc_id), None, Some(clients));

        Ok(item_consumed)
    }
}
