use rusty_fusion::{
    defines,
    error::{FFError, Severity},
    placeholder, Item,
};

use super::*;

pub fn gm_pc_set_value(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_GM_REQ_PC_SET_VALUE = *client.get_packet(P_CL2FE_GM_REQ_PC_SET_VALUE);
    let pc_id = pkt.iPC_ID;
    let value = pkt.iSetValue;
    let value_type = pkt.iSetValueType;
    let player = state.get_player_mut(pc_id)?;

    match value_type as u32 {
        defines::CN_GM_SET_VALUE_TYPE__HP => player.set_hp(value),
        defines::CN_GM_SET_VALUE_TYPE__WEAPON_BATTERY => player.set_weapon_boosts(value),
        defines::CN_GM_SET_VALUE_TYPE__NANO_BATTERY => player.set_nano_potions(value),
        defines::CN_GM_SET_VALUE_TYPE__FUSION_MATTER => player.set_fusion_matter(value),
        defines::CN_GM_SET_VALUE_TYPE__CANDY => player.set_taros(value),
        defines::CN_GM_SET_VALUE_TYPE__SPEED => placeholder!(()),
        defines::CN_GM_SET_VALUE_TYPE__JUMP => placeholder!(()),
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
    client.send_packet(P_FE2CL_GM_REP_PC_SET_VALUE, &resp)?;

    Ok(())
}

pub fn gm_pc_give_item(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_GIVE_ITEM = client.get_packet(P_CL2FE_REQ_PC_GIVE_ITEM);
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
}
