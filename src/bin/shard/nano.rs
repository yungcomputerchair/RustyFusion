use rusty_fusion::{
    defines::*,
    entity::{Combatant, Entity, EntityID},
    enums::*,
    error::*,
    item::Item,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    placeholder,
    state::ShardServerState,
    tabledata::tdata_get,
    unused,
};

pub fn nano_equip(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_NANO_EQUIP = client.get_packet(P_CL2FE_REQ_NANO_EQUIP)?;

    let player = state.get_player(pc_id)?;
    if player.perms as u32 > CN_ACCOUNT_LEVEL__DEVELOPER {
        // check for nano station
        let nano_station_ids = state.entity_map.find_npcs(|npc| {
            npc.ty == TYPE_NANO_MACHINE
                && npc.get_position().distance_to(&player.get_position()) <= RANGE_INTERACT
                && npc.instance_id == player.instance_id
        });
        if nano_station_ids.is_empty() {
            return Err(FFError::build(
                Severity::Warning,
                format!("{} tried to equip a nano without a nano station", player),
            ));
        }
    }

    let player = state.get_player_mut(pc_id)?;
    player.change_nano(pkt.iNanoSlotNum as usize, Some(pkt.iNanoID))?;

    let deactivate = player.get_active_nano_slot() == Some(pkt.iNanoSlotNum as usize);
    let resp = sP_FE2CL_REP_NANO_EQUIP_SUCC {
        iNanoID: pkt.iNanoID,
        iNanoSlotNum: pkt.iNanoSlotNum,
        bNanoDeactive: if deactivate { 1 } else { 0 },
    };

    if deactivate {
        player.set_active_nano_slot(None).unwrap();
        let bcast = sP_FE2CL_NANO_ACTIVE {
            iPC_ID: pc_id,
            Nano: None.into(),
            iConditionBitFlag: player.get_condition_bit_flag(),
            eCSTB___Add: 0,
        };
        state
            .entity_map
            .for_each_around(EntityID::Player(pc_id), clients, |c| {
                c.send_packet(P_FE2CL_NANO_ACTIVE, &bcast)
            });
    }

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_NANO_EQUIP_SUCC, &resp)
}

pub fn nano_unequip(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_NANO_UNEQUIP = client.get_packet(P_CL2FE_REQ_NANO_UNEQUIP)?;

    let player = state.get_player_mut(pc_id)?;
    player.change_nano(pkt.iNanoSlotNum as usize, None)?;

    let deactivate = player.get_active_nano_slot() == Some(pkt.iNanoSlotNum as usize);
    let resp = sP_FE2CL_REP_NANO_UNEQUIP_SUCC {
        iNanoSlotNum: pkt.iNanoSlotNum,
        bNanoDeactive: if deactivate { 1 } else { 0 },
    };

    if deactivate {
        player.set_active_nano_slot(None).unwrap();
        let bcast = sP_FE2CL_NANO_ACTIVE {
            iPC_ID: pc_id,
            Nano: None.into(),
            iConditionBitFlag: player.get_condition_bit_flag(),
            eCSTB___Add: 0,
        };
        state
            .entity_map
            .for_each_around(EntityID::Player(pc_id), clients, |c| {
                c.send_packet(P_FE2CL_NANO_ACTIVE, &bcast)
            });
    }

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_NANO_UNEQUIP_SUCC, &resp)
}

pub fn nano_active(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_NANO_ACTIVE = client.get_packet(P_CL2FE_REQ_NANO_ACTIVE)?;

    let player = state.get_player_mut(pc_id)?;
    if pkt.iNanoSlotNum == -1 {
        player.set_active_nano_slot(None).unwrap();
    } else {
        player.set_active_nano_slot(Some(pkt.iNanoSlotNum as usize))?;
    }

    let resp = sP_FE2CL_REP_NANO_ACTIVE_SUCC {
        iActiveNanoSlotNum: pkt.iNanoSlotNum,
        eCSTB___Add: placeholder!(0),
    };

    let bcast = sP_FE2CL_NANO_ACTIVE {
        iPC_ID: pc_id,
        Nano: player.get_active_nano().cloned().into(),
        iConditionBitFlag: player.get_condition_bit_flag(),
        eCSTB___Add: placeholder!(0),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_NANO_ACTIVE, &bcast)
        });

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_NANO_ACTIVE_SUCC, &resp)
}

pub fn nano_tune(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_NANO_TUNE = *client.get_packet(P_CL2FE_REQ_NANO_TUNE)?;
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;

            let tuning = tdata_get().get_nano_tuning(pkt.iTuneID)?;
            let skill_id = tuning.skill_id;

            // check for + consume tuning items
            let mut item_slots = [-1; 10];
            let mut items = [None.into(); 10];
            let mut quantity_left = tuning.req_item_quantity;

            let mut player_working = player.clone();
            if player_working
                .get_nano(pkt.iNanoID)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Player does not have nano {}", pkt.iNanoID),
                ))?
                .selected_skill
                .is_some()
            {
                // existing skill = not free. consume items
                for (i, slot_num) in pkt.aiNeedItemSlotNum.iter().enumerate() {
                    if quantity_left == 0 {
                        break;
                    }

                    let slot =
                        player_working.get_item_mut(ItemLocation::Inven, *slot_num as usize)?;
                    if slot.is_some_and(|stack| stack.id == tuning.req_item_id) {
                        let removed = Item::split_items(slot, quantity_left);
                        quantity_left -= removed.unwrap().quantity;
                        item_slots[i] = *slot_num;
                        items[i] = (*slot).into();
                    }
                }

                if quantity_left != 0 {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Not enough items to tune nano ({} < {})",
                            tuning.req_item_quantity - quantity_left,
                            tuning.req_item_quantity
                        ),
                    ));
                }

                // consume FM
                if player_working.get_fusion_matter() < tuning.fusion_matter_cost {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Not enough fusion matter to tune nano {} ({} < {})",
                            pkt.iNanoID,
                            player_working.get_fusion_matter(),
                            tuning.fusion_matter_cost
                        ),
                    ));
                }
                player_working.set_fusion_matter(
                    player_working.get_fusion_matter() - tuning.fusion_matter_cost,
                    None, // no broadcast since there's no chance of leveling up
                );
            }

            player_working.tune_nano(pkt.iNanoID, Some(skill_id))?;
            *player = player_working; // commit changes

            let resp = sP_FE2CL_REP_NANO_TUNE_SUCC {
                iNanoID: pkt.iNanoID,
                iSkillID: skill_id,
                iPC_FusionMatter: player.get_fusion_matter() as i32,
                aiItemSlotNum: item_slots,
                aItem: items,
            };
            client.send_packet(P_FE2CL_REP_NANO_TUNE_SUCC, &resp)
        })(),
        || {
            let pc_id = client.get_player_id()?;
            let resp = sP_FE2CL_REP_NANO_TUNE_FAIL {
                iPC_ID: pc_id,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_NANO_TUNE_FAIL, &resp)
        },
    )
}
