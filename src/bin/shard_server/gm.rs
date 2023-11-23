use rusty_fusion::{defines, error::BadPayload, placeholder};

use super::*;

pub fn gm_pc_set_value(client: &mut FFClient, state: &mut ShardServerState) -> Result<()> {
    let pkt: sP_CL2FE_GM_REQ_PC_SET_VALUE = *client.get_packet(P_CL2FE_GM_REQ_PC_SET_VALUE);
    let pc_uid = pkt.iPC_ID as i64;
    let value = pkt.iSetValue;
    let value_type = pkt.iSetValueType;
    let player = state.get_player_mut(pc_uid);

    match value_type as u32 {
        defines::CN_GM_SET_VALUE_TYPE__HP => player.set_hp(value),
        defines::CN_GM_SET_VALUE_TYPE__WEAPON_BATTERY => player.set_weapon_boosts(value),
        defines::CN_GM_SET_VALUE_TYPE__NANO_BATTERY => player.set_nano_potions(value),
        defines::CN_GM_SET_VALUE_TYPE__FUSION_MATTER => player.set_fusion_matter(value),
        defines::CN_GM_SET_VALUE_TYPE__CANDY => player.set_taros(value),
        defines::CN_GM_SET_VALUE_TYPE__SPEED => placeholder!(()),
        defines::CN_GM_SET_VALUE_TYPE__JUMP => placeholder!(()),
        _ => {
            return Err(BadPayload::build(
                client,
                format!("Bad value type: {value_type}"),
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

pub fn gm_pc_give_item(client: &mut FFClient, state: &mut ShardServerState) -> Result<()> {
    let pc_uid = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_GIVE_ITEM = client.get_packet(P_CL2FE_REQ_PC_GIVE_ITEM);
    let player = state.get_player_mut(pc_uid);
    let slot_number = pkt.iSlotNum as usize;
    if let Some(item) = pkt.Item.into() {
        if let Err(e) = player.set_item(slot_number, item) {
            return Err(BadPayload::build(client, e.to_string()));
        }
    }

    let resp = sP_FE2CL_REP_PC_GIVE_ITEM_SUCC {
        eIL: pkt.eIL,
        iSlotNum: pkt.iSlotNum,
        Item: pkt.Item,
    };
    client.send_packet(P_FE2CL_REP_PC_GIVE_ITEM_SUCC, &resp)
}
