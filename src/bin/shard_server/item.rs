use rusty_fusion::{
    enums::ItemLocation,
    error::{catch_fail, FFError, Severity},
    placeholder,
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
    let item_from = player.set_item(location_from, pkt.iFromSlotNum as usize, None)?;

    let location_to = pkt.eTo.try_into()?;
    let item_to = player.set_item(location_to, pkt.iToSlotNum as usize, item_from)?;
    player.set_item(location_from, pkt.iFromSlotNum as usize, item_to)?;

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

pub fn item_delete(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_ITEM_DELETE = client.get_packet(P_CL2FE_REQ_PC_ITEM_DELETE);
    let player = state.get_player_mut(pc_id)?;
    player.set_item(pkt.eIL.try_into()?, pkt.iSlotNum as usize, None)?;
    let resp = sP_FE2CL_REP_PC_ITEM_DELETE_SUCC {
        eIL: pkt.eIL,
        iSlotNum: pkt.iSlotNum,
    };
    client.send_packet(P_FE2CL_REP_PC_ITEM_DELETE_SUCC, &resp)?;
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
    let vendor_data = tdata_get().get_vendor_data(pkt.iVendorID)?;
    let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC {
        item: vendor_data.as_arr(),
    };
    client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC, &resp)?;
    Ok(())
}

pub fn vendor_item_buy(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_VENDOR_ITEM_BUY = *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_BUY);
    let vendor_data = tdata_get().get_vendor_data(pkt.iVendorID)?;
    let vendor_item = vendor_data.get_item(pkt.Item.iID, pkt.Item.iType)?;

    // sanitize the item
    let item = pkt.Item.try_into()?;
    // TODO vehicles, stack pricing, etc

    let player = state.get_player_mut(client.get_player_id()?)?;
    if player.get_taros() < vendor_item.get_price() {
        Err(FFError::build(
            Severity::Warning,
            format!(
                "Not enough taros to buy item ({} < {})",
                player.get_taros(),
                vendor_item.get_price()
            ),
        ))
    } else {
        player.set_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize, item)?;
        player.set_taros(player.get_taros() - vendor_item.get_price());

        let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_BUY_SUCC {
            iCandy: player.get_taros(),
            iInvenSlotNum: pkt.iInvenSlotNum,
            Item: item.into(),
        };
        client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_BUY_SUCC, &resp)?;
        Ok(())
    }
}

pub fn vendor_item_sell(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let pkt: sP_CL2FE_REQ_PC_VENDOR_ITEM_SELL =
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_SELL);
            let player = state.get_player_mut(client.get_player_id()?)?;
            let item = player
                .get_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize)?
                .as_ref()
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Tried to sell what's in empty slot {}", pkt.iInvenSlotNum),
                ))?;
            let stats = item.get_stats()?;

            if !stats.is_sellable() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Item not sellable: {:?}", item),
                ));
            }

            let sell_price = stats.get_sell_price();
            let item = player.set_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize, None)?;
            player.set_taros(player.get_taros() + sell_price);

            let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_SELL_SUCC {
                iCandy: player.get_taros(),
                iInvenSlotNum: pkt.iInvenSlotNum,
                Item: item.into(),
                ItemStay: placeholder!(None).into(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_SELL_SUCC, &resp)?;
            Ok(())
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_SELL_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_SELL_FAIL, &resp)?;
            Ok(())
        },
    )
}
