use std::cmp::min;

use rand::random;
use rusty_fusion::{
    defines::{PC_BATTERY_MAX, RANGE_INTERACT},
    enums::{ItemLocation, ItemType},
    error::{catch_fail, FFError, Severity},
    placeholder,
    tabledata::tdata_get,
    Item,
};

use super::*;

pub fn item_move(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_ITEM_MOVE = *client.get_packet(P_CL2FE_REQ_ITEM_MOVE)?;

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
        state.entity_map.for_each_around(entity_id, clients, |c| {
            let pkt = sP_FE2CL_PC_EQUIP_CHANGE {
                iPC_ID: pc_id,
                iEquipSlotNum: pkt.iFromSlotNum,
                EquipSlotItem: item_to.into(),
            };
            let _ = c.send_packet(P_FE2CL_PC_EQUIP_CHANGE, &pkt);
        });
    }

    if location_to == ItemLocation::Equip {
        state.entity_map.for_each_around(entity_id, clients, |c| {
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
    let pkt: &sP_CL2FE_REQ_PC_ITEM_DELETE = client.get_packet(P_CL2FE_REQ_PC_ITEM_DELETE)?;
    let player = state.get_player_mut(pc_id)?;
    player.set_item(pkt.eIL.try_into()?, pkt.iSlotNum as usize, None)?;
    let resp = sP_FE2CL_REP_PC_ITEM_DELETE_SUCC {
        eIL: pkt.eIL,
        iSlotNum: pkt.iSlotNum,
    };
    client.send_packet(P_FE2CL_REP_PC_ITEM_DELETE_SUCC, &resp)
}

pub fn item_combination(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_ITEM_COMBINATION =
        *client.get_packet(P_CL2FE_REQ_PC_ITEM_COMBINATION)?;
    catch_fail(
        (|| {
            let player = state.get_player_mut(client.get_player_id()?)?;

            let looks_item = player
                .get_item(ItemLocation::Inven, pkt.iCostumeItemSlot as usize)?
                .as_ref()
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Costume item (slot {}) empty", pkt.iCostumeItemSlot),
                ))?;
            let looks_item_stats = looks_item.get_stats()?;
            let looks_item_rarity = looks_item_stats.rarity.ok_or(FFError::build(
                Severity::Warning,
                format!("Costume item has no rarity: {:?}", looks_item),
            ))?;

            let stats_item = player
                .get_item(ItemLocation::Inven, pkt.iStatItemSlot as usize)?
                .as_ref()
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Stats item (slot {}) empty", pkt.iStatItemSlot),
                ))?;
            let stats_item_stats = stats_item.get_stats()?;
            let stats_item_rarity = stats_item_stats.rarity.ok_or(FFError::build(
                Severity::Warning,
                format!("Stats item has no rarity: {:?}", stats_item),
            ))?;

            let level_gap =
                (looks_item_stats.required_level - stats_item_stats.required_level).abs();
            let rarity_gap = (looks_item_rarity - stats_item_rarity).unsigned_abs();
            if rarity_gap > 3 {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Rarity gap {} larger than 3", rarity_gap),
                ));
            }

            let crocpot_data = tdata_get().get_crocpot_data(level_gap)?;
            let cost = (looks_item_stats.buy_price * crocpot_data.price_multiplier_looks)
                + (stats_item_stats.buy_price * crocpot_data.price_multiplier_stats);
            if player.get_taros() < cost {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Not enough taros to perform combination ({} < {})",
                        player.get_taros(),
                        cost
                    ),
                ));
            }
            let taros_left = player.set_taros(player.get_taros() - cost);

            let looks_item = player
                .set_item(ItemLocation::Inven, pkt.iCostumeItemSlot as usize, None)
                .unwrap()
                .unwrap();
            let mut stats_item = player
                .set_item(ItemLocation::Inven, pkt.iStatItemSlot as usize, None)
                .unwrap()
                .unwrap();

            let success_chance = crocpot_data.base_chance
                * crocpot_data.rarity_diff_multipliers[rarity_gap as usize];
            let roll: f32 = random();
            let succeeded = roll < success_chance;
            if succeeded {
                // set the appearance of the stats item
                stats_item.set_appearance(&looks_item);

                // put it back (where the looks item came from, since that's what the client expects)
                player
                    .set_item(
                        ItemLocation::Inven,
                        pkt.iCostumeItemSlot as usize,
                        Some(stats_item),
                    )
                    .unwrap();
            } else {
                // put the items back
                player
                    .set_item(
                        ItemLocation::Inven,
                        pkt.iCostumeItemSlot as usize,
                        Some(looks_item),
                    )
                    .unwrap();
                player
                    .set_item(
                        ItemLocation::Inven,
                        pkt.iStatItemSlot as usize,
                        Some(stats_item),
                    )
                    .unwrap();
            }

            let resp = sP_FE2CL_REP_PC_ITEM_COMBINATION_SUCC {
                iNewItemSlot: pkt.iCostumeItemSlot,
                sNewItem: Some(stats_item).into(),
                iStatItemSlot: pkt.iStatItemSlot,
                iCashItemSlot1: pkt.iCashItemSlot1,
                iCashItemSlot2: pkt.iCashItemSlot2,
                iCandy: taros_left as i32,
                iSuccessFlag: if succeeded { 1 } else { 0 },
            };
            client.send_packet(P_FE2CL_REP_PC_ITEM_COMBINATION_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_ITEM_COMBINATION_FAIL {
                iErrorCode: unused!(),
                iCostumeItemSlot: pkt.iCostumeItemSlot,
                iStatItemSlot: pkt.iStatItemSlot,
                iCashItemSlot1: pkt.iCashItemSlot1,
                iCashItemSlot2: pkt.iCashItemSlot2,
            };
            client.send_packet(P_FE2CL_REP_PC_ITEM_COMBINATION_FAIL, &resp)
        },
    )
}

pub fn item_chest_open(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_ITEM_CHEST_OPEN = *client.get_packet(P_CL2FE_REQ_ITEM_CHEST_OPEN)?;
    catch_fail(
        (|| {
            let player = state.get_player_mut(client.get_player_id()?)?;
            let location: ItemLocation = pkt.eIL.try_into()?;
            if location != ItemLocation::Inven {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("C.R.A.T.E. not in main inventory: {:?}", location),
                ));
            }

            let chest = player
                .set_item(location, pkt.iSlotNum as usize, None)?
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("C.R.A.T.E. in empty slot: {}", pkt.iSlotNum),
                ))?;

            if chest.get_type() != ItemType::Chest {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Item is not a C.R.A.T.E.: {:?}", chest),
                ));
            }

            let reward_item = tdata_get()
                .get_item_from_crate(chest.get_id(), player.get_style().iGender as i32)
                .unwrap_or(placeholder!(Item::new(ItemType::General, 119)));

            player.set_item(location, pkt.iSlotNum as usize, Some(reward_item))?;

            let reward_pkt = sP_FE2CL_REP_REWARD_ITEM {
                m_iCandy: player.get_taros() as i32,
                m_iFusionMatter: player.get_fusion_matter() as i32,
                m_iBatteryN: player.get_nano_potions() as i32,
                m_iBatteryW: player.get_weapon_boosts() as i32,
                iItemCnt: 1,
                iFatigue: unused!(),
                iFatigue_Level: unused!(),
                iNPC_TypeID: unused!(),
                iTaskID: unused!(),
            };
            let reward_item_s = sItemReward {
                sItem: Some(reward_item).into(),
                eIL: location as i32,
                iSlotNum: pkt.iSlotNum,
            };
            client.queue_packet(P_FE2CL_REP_REWARD_ITEM, &reward_pkt)?;
            client.queue_struct(&reward_item_s)?;
            client.flush()?;

            let resp = sP_FE2CL_REP_ITEM_CHEST_OPEN_SUCC {
                iSlotNum: pkt.iSlotNum,
            };
            client.send_packet(P_FE2CL_REP_ITEM_CHEST_OPEN_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_ITEM_CHEST_OPEN_FAIL {
                iSlotNum: pkt.iSlotNum,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_ITEM_CHEST_OPEN_FAIL, &resp)
        },
    )
}

pub fn vendor_start(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_VENDOR_START = *client.get_packet(P_CL2FE_REQ_PC_VENDOR_START)?;
    catch_fail(
        (|| {
            helpers::validate_vendor(client, state, pkt.iVendorID)?;
            let resp = sP_FE2CL_REP_PC_VENDOR_START_SUCC {
                iNPC_ID: pkt.iNPC_ID,
                iVendorID: pkt.iVendorID,
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_START_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_START_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_START_FAIL, &resp)
        },
    )
}

pub fn vendor_table_update(client: &mut FFClient) -> FFResult<()> {
    catch_fail(
        (|| {
            let pkt: &sP_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE =
                client.get_packet(P_CL2FE_REQ_PC_VENDOR_TABLE_UPDATE)?;
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
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_BUY)?;
            helpers::validate_vendor(client, state, pkt.iVendorID)?;

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
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_SELL)?;
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
            let buyback_list = state.buyback_lists.entry(pc_id).or_default();
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
            let pkt: sP_CL2FE_REQ_PC_VENDOR_ITEM_RESTORE_BUY =
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_ITEM_RESTORE_BUY)?;
            helpers::validate_vendor(client, state, pkt.iVendorID)?;

            let item: Option<Item> = pkt.Item.try_into()?;
            let item: Item = item.ok_or(FFError::build(
                Severity::Warning,
                format!("Bad item for buyback {:?}", pkt.Item),
            ))?;
            let buyback_list = state.buyback_lists.get_mut(&pc_id).ok_or(FFError::build(
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

pub fn vendor_battery_buy(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    const BATTERY_TYPE_BOOST: i16 = 3;
    const BATTERY_TYPE_POTION: i16 = 4;

    catch_fail(
        (|| {
            let pkt: sP_CL2FE_REQ_PC_VENDOR_BATTERY_BUY =
                *client.get_packet(P_CL2FE_REQ_PC_VENDOR_BATTERY_BUY)?;
            helpers::validate_vendor(client, state, pkt.iVendorID)?;

            let battery_type = pkt.Item.iID;
            let mut quantity = pkt.Item.iOpt as u32 * 100;

            let player = state.get_player_mut(client.get_player_id()?)?;
            match battery_type {
                BATTERY_TYPE_BOOST => {
                    quantity = min(player.get_weapon_boosts() + quantity, PC_BATTERY_MAX)
                        - player.get_weapon_boosts();
                }
                BATTERY_TYPE_POTION => {
                    quantity = min(player.get_nano_potions() + quantity, PC_BATTERY_MAX)
                        - player.get_nano_potions();
                }
                other => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Bad battery type: {}", other),
                    ));
                }
            }

            let cost = quantity;
            if player.get_taros() < cost {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Not enough taros to buyback item ({} < {})",
                        player.get_taros(),
                        cost
                    ),
                ));
            }

            match battery_type {
                BATTERY_TYPE_BOOST => {
                    player.set_weapon_boosts(player.get_weapon_boosts() + quantity);
                }
                BATTERY_TYPE_POTION => {
                    player.set_nano_potions(player.get_nano_potions() + quantity);
                }
                other => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!("Bad battery type: {}", other),
                    ));
                }
            }
            let taros_new = player.set_taros(player.get_taros() - cost);

            let resp = sP_FE2CL_REP_PC_VENDOR_BATTERY_BUY_SUCC {
                iCandy: taros_new as i32,
                iBatteryW: player.get_weapon_boosts() as i32,
                iBatteryN: player.get_nano_potions() as i32,
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_BATTERY_BUY_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_VENDOR_BATTERY_BUY_FAIL {
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_VENDOR_BATTERY_BUY_FAIL, &resp)
        },
    )
}

mod helpers {
    use super::*;

    pub fn validate_vendor(
        client: &mut FFClient,
        state: &mut ShardServerState,
        vendor_id: i32,
    ) -> FFResult<()> {
        /*
         * due to a client bug where the iNPC_ID field in vendor packets is incorrectly
         * set to the same value as iVendorID, we need to lookup the NPC by its type
         * instead (which is equal to iVendorID for whatever reason)
         */
        let npc = state.find_npc_by_type(vendor_id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with type {} doesn't exist", vendor_id),
        ))?;

        let pc_id = client.get_player_id()?;
        state
            .entity_map
            .validate_proximity(&[EntityID::Player(pc_id), npc.get_id()], RANGE_INTERACT)
    }
}
