use rusty_fusion::{
    enums::eItemLocation,
    error::{FFError, Severity},
    tabledata::tdata_get,
    unused,
};

use num_traits::FromPrimitive;

use super::*;

pub fn item_move(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_ITEM_MOVE = *client.get_packet(P_CL2FE_REQ_ITEM_MOVE);

    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    let location_from = eItemLocation::from_i32(pkt.eFrom).ok_or(FFError::build(
        Severity::Warning,
        format!("Bad eFrom {}", pkt.eFrom),
    ))?;
    let item_from =
        player.set_item_with_location(location_from, pkt.iFromSlotNum as usize, None)?;

    let location_to = eItemLocation::from_i32(pkt.eTo).ok_or(FFError::build(
        Severity::Warning,
        format!("Bad eTo {}", pkt.eTo),
    ))?;
    let item_to = player.set_item_with_location(location_to, pkt.iToSlotNum as usize, item_from)?;

    let resp = sP_FE2CL_PC_ITEM_MOVE_SUCC {
        eFrom: pkt.eTo,
        iFromSlotNum: pkt.iToSlotNum,
        FromSlotItem: item_from.into(),
        eTo: pkt.eFrom,
        iToSlotNum: pkt.iFromSlotNum,
        ToSlotItem: item_to.into(),
    };

    client.send_packet(P_FE2CL_PC_ITEM_MOVE_SUCC, &resp)?;

    let entity_id = player.get_id();
    if location_from == eItemLocation::eIL_Equip {
        state
            .get_entity_map()
            .for_each_around(entity_id, clients, |c| {
                let pkt = sP_FE2CL_PC_EQUIP_CHANGE {
                    iPC_ID: pc_id,
                    iEquipSlotNum: pkt.iFromSlotNum,
                    EquipSlotItem: item_to.into(),
                };
                let _ = c.send_packet(P_FE2CL_PC_EQUIP_CHANGE, &pkt);
            });
    }

    if location_to == eItemLocation::eIL_Equip {
        state
            .get_entity_map()
            .for_each_around(entity_id, clients, |c| {
                let pkt = sP_FE2CL_PC_EQUIP_CHANGE {
                    iPC_ID: pc_id,
                    iEquipSlotNum: pkt.iToSlotNum,
                    EquipSlotItem: item_from.into(),
                };
                let _ = c.send_packet(P_FE2CL_PC_EQUIP_CHANGE, &pkt);
            });
    }

    Ok(())
}

pub fn vendor_start(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_PC_VENDOR_START = client.get_packet(P_CL2FE_REQ_PC_VENDOR_START);
    let resp = sP_FE2CL_REP_PC_VENDOR_START_SUCC {
        iNPC_ID: pkt.iNPC_ID,
        iVendorID: pkt.iVendorID,
    };
    client.send_packet(P_FE2CL_REP_PC_VENDOR_START_SUCC, &resp)?;
    Ok(())
}

pub fn vendor_table_update(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE =
        client.get_packet(P_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE);
    let vendor_data = tdata_get().get_vendor_data(pkt.iVendorID);
    match vendor_data {
        Ok(vendor_data) => {
            let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC { item: vendor_data.as_arr() };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC, &resp)?;
            Ok(())
        }
        Err(e) => {
            let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_FAIL, &resp)?;
            Err(e)
        }
    }
}
