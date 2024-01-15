use std::cmp::max;

use rusty_fusion::{
    chunk::InstanceID,
    defines,
    error::{catch_fail, FFError, Severity},
    placeholder, Combatant, Item, Position,
};

use super::*;

// TODO anticheat

pub fn gm_pc_set_value(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_GM_REQ_PC_SET_VALUE = *client.get_packet(P_CL2FE_GM_REQ_PC_SET_VALUE)?;
    let pc_id = pkt.iPC_ID;
    let value = pkt.iSetValue;
    let value_type = pkt.iSetValueType;
    let player = state.get_player_mut(pc_id)?;

    let value = match value_type as u32 {
        defines::CN_GM_SET_VALUE_TYPE__HP => player.set_hp(value),
        defines::CN_GM_SET_VALUE_TYPE__WEAPON_BATTERY => {
            player.set_weapon_boosts(value as u32) as i32
        }
        defines::CN_GM_SET_VALUE_TYPE__NANO_BATTERY => player.set_nano_potions(value as u32) as i32,
        defines::CN_GM_SET_VALUE_TYPE__FUSION_MATTER => {
            player.set_fusion_matter(value as u32) as i32
        }
        defines::CN_GM_SET_VALUE_TYPE__CANDY => player.set_taros(value as u32) as i32,
        defines::CN_GM_SET_VALUE_TYPE__SPEED => placeholder!(value),
        defines::CN_GM_SET_VALUE_TYPE__JUMP => placeholder!(value),
        _ => {
            return Err(FFError::build(
                Severity::Warning,
                format!("Bad value type: {}", value_type),
            ));
        }
    };

    let resp = sP_FE2CL_GM_REP_PC_SET_VALUE {
        iPC_ID: pkt.iPC_ID,
        iSetValue: value,
        iSetValueType: value_type,
    };
    client.send_packet(P_FE2CL_GM_REP_PC_SET_VALUE, &resp)
}

pub fn gm_pc_give_item(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let pkt: &sP_CL2FE_REQ_PC_GIVE_ITEM = client.get_packet(P_CL2FE_REQ_PC_GIVE_ITEM)?;
            let player = state.get_player_mut(pc_id)?;
            let slot_number = pkt.iSlotNum as usize;

            let location = pkt.eIL.try_into()?;
            let item: Option<Item> = pkt.Item.try_into()?;

            player.set_item(location, slot_number, item)?;

            let resp = sP_FE2CL_REP_PC_GIVE_ITEM_SUCC {
                eIL: pkt.eIL,
                iSlotNum: pkt.iSlotNum,
                Item: item.into(),
            };
            client.send_packet(P_FE2CL_REP_PC_GIVE_ITEM_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_GIVE_ITEM_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_GIVE_ITEM_FAIL, &resp)
        },
    )
}

pub fn gm_pc_give_nano(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_GIVE_NANO =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_GIVE_NANO)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let nano_id = pkt.iNanoID;

            let bcast = sP_FE2CL_REP_PC_NANO_CREATE {
                iPC_ID: pc_id,
                iNanoID: pkt.iNanoID,
            };
            state
                .entity_map
                .for_each_around(EntityID::Player(pc_id), clients, |c| {
                    let _ = c.send_packet(P_FE2CL_REP_PC_NANO_CREATE, &bcast);
                });

            let player = state.get_player_mut(pc_id)?;
            let new_level = max(player.get_level(), nano_id);
            player.set_level(new_level);
            let nano = player.unlock_nano(nano_id)?.clone();

            let resp = sP_FE2CL_REP_PC_NANO_CREATE_SUCC {
                iPC_FusionMatter: player.get_fusion_matter() as i32,
                iQuestItemSlotNum: 0,
                QuestItem: None.into(),
                Nano: Some(nano).into(),
                iPC_Level: player.get_level(),
            };

            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_NANO_CREATE_SUCC, &resp)
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_NANO_CREATE_FAIL {
                iPC_ID: client.get_player_id()?,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_NANO_CREATE_FAIL, &resp)
        },
    )
}

pub fn gm_pc_goto(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_GOTO = client.get_packet(P_CL2FE_REQ_PC_GOTO)?;
    let new_pos = Position {
        x: pkt.iToX,
        y: pkt.iToY,
        z: pkt.iToZ,
    };
    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    player.set_position(new_pos);
    player.instance_id = InstanceID::default();

    state
        .entity_map
        .update(EntityID::Player(pc_id), None, Some(clients));

    let resp = sP_FE2CL_REP_PC_GOTO_SUCC {
        iX: new_pos.x,
        iY: new_pos.y,
        iZ: new_pos.z,
    };
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_GOTO_SUCC, &resp)
}

pub fn gm_pc_special_state_switch(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH =
        client.get_packet(P_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH)?;

    let player = state.get_player_mut(pc_id)?;

    match pkt.iSpecialStateFlag as u32 {
        defines::CN_SPECIAL_STATE_FLAG__PRINT_GM => {
            player.show_gm_marker = !player.show_gm_marker;
        }
        defines::CN_SPECIAL_STATE_FLAG__INVISIBLE => {
            player.invisible = !player.invisible;
        }
        defines::CN_SPECIAL_STATE_FLAG__INVULNERABLE => {
            player.invulnerable = !player.invulnerable;
        }
        _ => {
            return Err(FFError::build(
                Severity::Warning,
                format!(
                    "P_CL2FE_GM_REQ_PC_SPECIAL_STATE_SWITCH: invalid special state flag: {}",
                    pkt.iSpecialStateFlag
                ),
            ));
        }
    }

    let special_state_flags = player.get_special_state_bit_flag();

    let resp = sP_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC {
        iPC_ID: pkt.iPC_ID,
        iReqSpecialStateFlag: pkt.iSpecialStateFlag,
        iSpecialState: special_state_flags,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pkt.iPC_ID), clients, |c| {
            let _ = c.send_packet(P_FE2CL_PC_SPECIAL_STATE_CHANGE, &resp);
        });
    clients
        .get_self()
        .send_packet(P_FE2CL_REP_PC_SPECIAL_STATE_SWITCH_SUCC, &resp)
}
