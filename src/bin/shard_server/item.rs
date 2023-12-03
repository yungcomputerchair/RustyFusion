use rusty_fusion::{
    enums::ItemLocation,
    error::{catch_fail, FFError, Severity},
    tabledata::tdata_get,
    unused,
};

use super::*;

pub fn item_move(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_ITEM_MOVE = *client.get_packet(P_CL2FE_REQ_ITEM_MOVE);

    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    let location_from = pkt.eFrom.try_into()?;
    let item_from =
        player.set_item_with_location(location_from, pkt.iFromSlotNum as usize, None)?;

    let location_to = pkt.eTo.try_into()?;
    let item_to = player.set_item_with_location(location_to, pkt.iToSlotNum as usize, item_from)?;
    player.set_item_with_location(location_from, pkt.iFromSlotNum as usize, item_to)?;

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
    if location_from == ItemLocation::Equip {
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

    if location_to == ItemLocation::Equip {
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
    let vendor_data = catch_fail(tdata_get().get_vendor_data(pkt.iVendorID), || {
        let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_FAIL {
            iErrorCode: unused!(),
        };
        client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_FAIL, &resp)
    })?;
    let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC {
        item: vendor_data.as_arr(),
    };
    client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC, &resp)?;
    Ok(())
}

pub fn vendor_item_buy(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    fn send_fail(client: &mut FFClient) -> FFResult<()> {
        let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_BUY_FAIL {
            iErrorCode: unused!(),
        };
        client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_BUY_FAIL, &resp)
    }

    let pkt: sP_CL2FE_REQ_PC_VENDOR_ITEM_BUY = *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_BUY);
    let vendor_data = catch_fail(tdata_get().get_vendor_data(pkt.iVendorID), || {
        send_fail(client)
    })?;
    let (item_id, item_type) = (pkt.Item.iID, pkt.Item.iType);
    let vendor_item = catch_fail(vendor_data.get_item(item_id, item_type), || {
        send_fail(client)
    })?;

    // sanitize the item
    let item = pkt.Item.try_into()?;

    let player = state.get_player_mut(client.get_player_id()?)?;
    if player.get_taros() < vendor_item.get_price() {
        send_fail(client)?;
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Not enough taros to buy item ({} < {})",
                player.get_taros(),
                vendor_item.get_price()
            ),
        ));
    }
    player.set_item_with_location(ItemLocation::Inven, pkt.iInvenSlotNum as usize, item)?;
    player.set_taros(player.get_taros() - vendor_item.get_price());

    let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_BUY_SUCC {
        iCandy: player.get_taros(),
        iInvenSlotNum: pkt.iInvenSlotNum,
        Item: item.into(),
    };
    client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_BUY_SUCC, &resp)?;
    Ok(())
}
