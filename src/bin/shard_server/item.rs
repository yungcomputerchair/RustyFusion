use rusty_fusion::{
    enums::{ItemLocation, ItemType},
    error::{catch_fail, FFError, Severity},
    tabledata::tdata_get,
    Item,
};

use super::*;

pub fn item_move(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_ITEM_MOVE = *client.get_packet(P_CL2FE_REQ_ITEM_MOVE);

    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;

    let location_from = pkt.eFrom.try_into()?;
    let mut item_from = player.set_item(location_from, pkt.iFromSlotNum as usize, None)?;

    let location_to = pkt.eTo.try_into()?;
    let mut item_to = player.set_item(location_to, pkt.iToSlotNum as usize, None)?;

    Item::transfer_items(&mut item_from, &mut item_to)?;
    player.set_item(location_from, pkt.iFromSlotNum as usize, item_from)?;
    player.set_item(location_to, pkt.iToSlotNum as usize, item_to)?;

    let resp = sP_FE2CL_PC_ITEM_MOVE_SUCC {
        eFrom: pkt.eFrom,
        iFromSlotNum: pkt.iFromSlotNum,
        FromSlotItem: item_from.into(),
        eTo: pkt.eTo,
        iToSlotNum: pkt.iToSlotNum,
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
    client.send_packet(P_FE2CL_REP_PC_ITEM_DELETE_SUCC, &resp)
}

pub fn vendor_start(client: &mut FFClient) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_PC_VENDOR_START = client.get_packet(P_CL2FE_REQ_PC_VENDOR_START);
    let resp = sP_FE2CL_REP_PC_VENDOR_START_SUCC {
        iNPC_ID: pkt.iNPC_ID,
        iVendorID: pkt.iVendorID,
    };
    client.send_packet(P_FE2CL_REP_PC_VENDOR_START_SUCC, &resp)
}

pub fn vendor_table_update(client: &mut FFClient) -> FFResult<()> {
    catch_fail(
        (|| {
            let pkt: &sP_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE =
                client.get_packet(P_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE);
            let vendor_data = tdata_get().get_vendor_data(pkt.iVendorID)?;
            let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC {
                item: vendor_data.as_arr()?,
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_TABLE_UPDATE_FAIL, &resp)
        },
    )
}

pub fn vendor_item_buy(
    client: &mut FFClient,
    state: &mut ShardServerState,
    time: SystemTime,
) -> FFResult<()> {
    catch_fail(
        (|| {
            let pkt: sP_CL2FE_REQ_PC_VENDOR_ITEM_BUY =
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_BUY);

            // sanitize the item
            let item: Option<Item> = pkt.Item.try_into()?;
            let mut item = item.ok_or(FFError::build(
                Severity::Warning,
                "Tried to buy nothing".to_string(),
            ))?;
            if item.get_type() == ItemType::Vehicle {
                // set expiration date
                let duration = Duration::from_secs(config_get().shard.vehicle_duration.get());
                let expires = time + duration;
                item.set_expiry_time(expires);
            }

            let vendor_data = tdata_get().get_vendor_data(pkt.iVendorID)?;
            if !vendor_data.has_item(item.get_id(), item.get_type()) {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Vendor {} doesn't sell item ({}, {:?})",
                        pkt.iVendorID,
                        item.get_id(),
                        item.get_type()
                    ),
                ));
            }

            let stats = item.get_stats()?;
            let price = stats.buy_price * item.get_quantity() as u32;
            let player = state.get_player_mut(client.get_player_id()?)?;
            if player.get_taros() < price {
                Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Not enough taros to buy item ({} < {})",
                        player.get_taros(),
                        price
                    ),
                ))
            } else {
                player.set_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize, Some(item))?;
                player.set_taros(player.get_taros() - price);

                let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_BUY_SUCC {
                    iCandy: player.get_taros() as i32,
                    iInvenSlotNum: pkt.iInvenSlotNum,
                    Item: Some(item).into(),
                };
                client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_BUY_SUCC, &resp)
            }
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_BUY_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_BUY_FAIL, &resp)
        },
    )
}

pub fn vendor_item_sell(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    catch_fail(
        (|| {
            let pkt: sP_CL2FE_REQ_PC_VENDOR_ITEM_SELL =
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_SELL);
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;

            let item = player
                .get_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize)?
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Tried to sell what's in empty slot {}", pkt.iInvenSlotNum),
                ))?;
            let stats = item.get_stats()?;

            if !stats.sellable {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Item not sellable: {:?}", item),
                ));
            }

            let mut remaining_item =
                player.set_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize, None)?;
            let quantity = pkt.iItemCnt as u16;
            let item = Item::split_items(&mut remaining_item, quantity);
            player
                .set_item(
                    ItemLocation::Inven,
                    pkt.iInvenSlotNum as usize,
                    remaining_item,
                )
                .unwrap();

            let sell_price = stats.sell_price * quantity as u32;
            let new_taros = player.set_taros(player.get_taros() + sell_price);
            let buyback_list = state.get_buyback_lists().entry(pc_id).or_default();
            buyback_list.push(item.unwrap());

            let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_SELL_SUCC {
                iCandy: new_taros as i32,
                iInvenSlotNum: pkt.iInvenSlotNum,
                Item: item.into(),
                ItemStay: remaining_item.into(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_SELL_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_SELL_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_SELL_FAIL, &resp)
        },
    )
}

pub fn vendor_item_restore_buy(
    client: &mut FFClient,
    state: &mut ShardServerState,
) -> FFResult<()> {
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let pkt: &sP_CL2FE_REQ_PC_VENDOR_ITEM_RESTORE_BUY =
                client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_RESTORE_BUY);
            let item: Option<Item> = pkt.Item.try_into()?;
            let item: Item = item.ok_or(FFError::build(
                Severity::Warning,
                format!("Bad item for buyback {:?}", pkt.Item),
            ))?;
            let buyback_list = state
                .get_buyback_lists()
                .get_mut(&pc_id)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Player {} has not sold any items", pc_id),
                ))?;

            let mut found_idx = None;
            for (i, list_item) in buyback_list.iter().enumerate() {
                if *list_item == item {
                    found_idx = Some(i);
                    break;
                }
            }
            let found_idx = found_idx.ok_or(FFError::build(
                Severity::Warning,
                format!(
                    "Player tried to buyback an item they didn't sell: {:?}",
                    item
                ),
            ))?;

            let item = buyback_list.remove(found_idx);
            let cost = item.get_stats()?.sell_price * item.get_quantity() as u32; // sell price is cost for buyback
            let player = state.get_player_mut(pc_id)?;

            if player.get_taros() < cost {
                Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Not enough taros to buyback item ({} < {})",
                        player.get_taros(),
                        cost
                    ),
                ))
            } else {
                player.set_item(ItemLocation::Inven, pkt.iInvenSlotNum as usize, Some(item))?;
                let new_taros = player.set_taros(player.get_taros() - cost);

                let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_RESTORE_BUY_SUCC {
                    iCandy: new_taros as i32,
                    iInvenSlotNum: pkt.iInvenSlotNum,
                    Item: Some(item).into(),
                };
                client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_RESTORE_BUY_SUCC, &resp)
            }
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_ITEM_RESTORE_BUY_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_ITEM_RESTORE_BUY_FAIL, &resp)
        },
    )
}
