use uuid::Uuid;

use rusty_fusion::{
    database::db_run_async,
    defines::*,
    entity::{Entity, EntityID},
    enums::*,
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    state::ShardServerState,
    trade::TradeContext,
    unused,
};

pub fn trade_offer(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_TRADE_OFFER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            state.entity_map.validate_proximity(
                &[EntityID::Player(pc_id), EntityID::Player(pkt.iID_To)],
                RANGE_INTERACT,
            )?;

            let player = state.get_player_mut(pc_id)?;
            if player.trade_id.is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("From player {} already trading", player.get_player_id()),
                ));
            }

            let other_pc_id = pkt.iID_To;
            let other_player = state.get_player(other_pc_id)?;
            if other_player.trade_id.is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("To player {} already trading", other_player.get_player_id()),
                ));
            }
            let other_client = other_player.get_client(clients).unwrap();
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER {
                iID_Request: pc_id,
                iID_From: pc_id,
                iID_To: other_pc_id,
            };
            log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER, &resp));

            // to avoid other clients making offers on our behalf
            let player = state.get_player_mut(pc_id)?;
            player.trade_offered_to = Some(other_pc_id);
            Ok(())
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_REFUSAL {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
            };
            client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_REFUSAL, &resp)
        },
    )
}

pub fn trade_offer_accept(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER_ACCEPT = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_ACCEPT)?;
    catch_fail(
        (|| {
            let trade_id = Uuid::new_v4();

            let pc_id = clients.get_self().get_player_id()?;
            let pc_id_other = pkt.iID_From;

            let player_from = state.get_player_mut(pc_id_other)?;
            if player_from.trade_offered_to != Some(pc_id) {
                return Err(FFError::build(
                    Severity::Info,
                    format!("Trade offer from {} to {} expired", pkt.iID_From, pc_id),
                ));
            } else {
                player_from.trade_offered_to = None;
            }
            player_from.trade_id = Some(trade_id);

            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_SUCC {
                iID_Request: pc_id,
                iID_From: pc_id_other,
                iID_To: pc_id,
            };
            let other_client = player_from.get_client(clients).unwrap();
            log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_SUCC, &resp));

            let player_to = state.get_player_mut(pc_id)?;
            player_to.trade_id = Some(trade_id);

            state
                .ongoing_trades
                .insert(trade_id, TradeContext::new(pc_id_other, pc_id));
            Ok(())
        })(),
        || {
            if let Ok(player_from) = state.get_player_mut(pkt.iID_From) {
                player_from.trade_id = None;
            }
            if let Ok(player_to) = state.get_player_mut(pkt.iID_To) {
                player_to.trade_id = None;
            }

            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_ABORT {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TRADE_OFFER_ABORT, &resp)
        },
    )
}

pub fn trade_offer_refusal(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL =
        *client.get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL)?;

    let pc_id = client.get_player_id()?;
    let pc_id_other = pkt.iID_From;

    let player_from = state.get_player_mut(pc_id_other)?;
    if player_from.trade_offered_to != Some(pc_id) {
        return Err(FFError::build(
            Severity::Info,
            format!("Trade offer from {} to {} expired", pkt.iID_From, pc_id),
        ));
    } else {
        player_from.trade_offered_to = None;
    }

    let resp = sP_FE2CL_REP_PC_TRADE_OFFER_REFUSAL {
        iID_Request: pc_id,
        iID_From: pc_id_other,
        iID_To: pc_id,
    };
    let other_client = state
        .get_player(resp.iID_From)?
        .get_client(clients)
        .unwrap();
    log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_REFUSAL, &resp));
    Ok(())
}

pub fn trade_offer_cancel(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let _pkt: &sP_CL2FE_REQ_PC_TRADE_OFFER_CANCEL =
        client.get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_CANCEL)?;

    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;
    let trade_id = player.trade_id.ok_or(FFError::build(
        Severity::Warning,
        format!("Player {} is not trading", player.get_player_id()),
    ))?;
    let trade = state.ongoing_trades.get(&trade_id).unwrap();
    let other_pc_id = trade.get_other_id(pc_id);

    let resp = sP_FE2CL_REP_PC_TRADE_OFFER_CANCEL {
        iID_Request: pc_id,
        iID_From: trade.get_id_from(),
        iID_To: trade.get_id_to(),
    };
    let other_client = state.get_player(other_pc_id)?.get_client(clients).unwrap();
    log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_CANCEL, &resp));
    Ok(())
}

pub fn trade_cash_register(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_CASH_REGISTER = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TRADE_CASH_REGISTER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player(pc_id)?;
            let trade_id = player.trade_id.ok_or(FFError::build(
                Severity::Warning,
                format!("Player {} is not trading", player.get_player_id()),
            ))?;

            let req_taros = pkt.iCandy as u32;
            if player.get_taros() < req_taros {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player doesn't have enough taros ({} < {})",
                        player.get_taros(),
                        req_taros
                    ),
                ));
            }

            let trade = state.ongoing_trades.get_mut(&trade_id).unwrap();
            trade.set_taros(pc_id, req_taros)?;

            let resp = sP_FE2CL_REP_PC_TRADE_CASH_REGISTER_SUCC {
                iID_Request: pc_id,
                iID_From: trade.get_id_from(),
                iID_To: trade.get_id_to(),
                iCandy: req_taros as i32,
            };
            let other_id = trade.get_other_id(pc_id);
            let other_client = state.get_player(other_id)?.get_client(clients).unwrap();
            log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_CASH_REGISTER_SUCC, &resp));
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TRADE_CASH_REGISTER_SUCC, &resp)
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_TRADE_CASH_REGISTER_FAIL {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_TRADE_CASH_REGISTER_FAIL, &resp)
        },
    )
}

pub fn trade_item_register(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_ITEM_REGISTER = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TRADE_ITEM_REGISTER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player(pc_id)?;
            let trade_id = player.trade_id.ok_or(FFError::build(
                Severity::Warning,
                format!("Player {} is not trading", player.get_player_id()),
            ))?;

            // client sends an iOpt of 0 for unstackables
            let quantity = if pkt.Item.iOpt > 0 {
                pkt.Item.iOpt as u16
            } else {
                1
            };

            let inven_slot_num = pkt.Item.iInvenNum as usize;
            let item = player
                .get_item(ItemLocation::Inven, inven_slot_num)?
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!(
                        "Player {} tried to trade nothing (slot {})",
                        pc_id, inven_slot_num
                    ),
                ))?;
            if !item.get_stats()?.tradeable {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Item not tradeable: {:?}", item),
                ));
            }

            let trade = state.ongoing_trades.get_mut(&trade_id).unwrap();
            let trade_slot_num = pkt.Item.iSlotNum as usize;
            let quantity_left =
                item.quantity - trade.add_item(pc_id, trade_slot_num, inven_slot_num, quantity)?;

            let resp = sP_FE2CL_REP_PC_TRADE_ITEM_REGISTER_SUCC {
                iID_Request: pc_id,
                iID_From: trade.get_id_from(),
                iID_To: trade.get_id_to(),
                TradeItem: pkt.Item,
                InvenItem: sItemTrade {
                    iOpt: quantity_left as i32,
                    ..pkt.Item
                },
            };
            let other_id = trade.get_other_id(pc_id);
            let other_client = state.get_player(other_id)?.get_client(clients).unwrap();
            log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_REGISTER_SUCC, &resp));
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TRADE_ITEM_REGISTER_SUCC, &resp)
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_TRADE_ITEM_REGISTER_FAIL {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_REGISTER_FAIL, &resp)
        },
    )
}

pub fn trade_item_unregister(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_ITEM_UNREGISTER = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TRADE_ITEM_UNREGISTER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player(pc_id)?;
            let trade_id = player.trade_id.ok_or(FFError::build(
                Severity::Warning,
                format!("Player {} is not trading", player.get_player_id()),
            ))?;

            let trade = state.ongoing_trades.get_mut(&trade_id).unwrap();
            let from_id = trade.get_id_from();
            let to_id = trade.get_id_to();
            let other_pc_id = trade.get_other_id(pc_id);

            let trade_slot_num = pkt.Item.iSlotNum as usize;
            let (quantity_left, inven_slot_num) = trade.remove_item(pc_id, trade_slot_num)?;
            let item = state
                .get_player(pc_id)
                .unwrap()
                .get_item(ItemLocation::Inven, inven_slot_num)
                .unwrap()
                .unwrap();
            let quantity = item.quantity - quantity_left;

            let resp = sP_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_SUCC {
                iID_Request: pc_id,
                iID_From: from_id,
                iID_To: to_id,
                TradeItem: pkt.Item,
                InvenItem: sItemTrade {
                    iOpt: quantity as i32,
                    iInvenNum: inven_slot_num as i32,
                    /* IMPORTANT: the client sends us type 8 here so we MUST override it.
                     * This took me an entire day to diagnose. */
                    iType: item.ty as i16,
                    ..pkt.Item
                },
            };
            let other_client = state.get_player(other_pc_id)?.get_client(clients).unwrap();
            log_if_failed(
                other_client.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_SUCC, &resp),
            );
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_SUCC, &resp)
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_FAIL {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_FAIL, &resp)
        },
    )
}

pub fn trade_confirm_cancel(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let _pkt: &sP_CL2FE_REQ_PC_TRADE_CONFIRM_CANCEL =
        client.get_packet(P_CL2FE_REQ_PC_TRADE_CONFIRM_CANCEL)?;

    let pc_id = client.get_player_id()?;
    let player = state.get_player_mut(pc_id)?;
    let trade_id = player.trade_id.ok_or(FFError::build(
        Severity::Warning,
        format!("Player {} is not trading", player.get_player_id()),
    ))?;

    player.trade_id = None;

    let trade = state.ongoing_trades.remove(&trade_id).unwrap();

    let other_pc_id = trade.get_other_id(pc_id);
    let other_player = state.get_player_mut(other_pc_id).unwrap();
    other_player.trade_id = None;

    let resp = sP_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL {
        iID_Request: pc_id,
        iID_From: trade.get_id_from(),
        iID_To: trade.get_id_to(),
    };
    let other_client = other_player.get_client(clients).unwrap();
    log_if_failed(other_client.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_CANCEL, &resp));
    Ok(())
}

pub fn trade_confirm(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let _pkt: &sP_CL2FE_REQ_PC_TRADE_CONFIRM = client.get_packet(P_CL2FE_REQ_PC_TRADE_CONFIRM)?;

    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;
    let trade_id = player.trade_id.ok_or(FFError::build(
        Severity::Warning,
        format!("Player {} is not trading", player.get_player_id()),
    ))?;

    let trade = state.ongoing_trades.get_mut(&trade_id).unwrap();
    let pc_id_other = trade.get_other_id(pc_id);
    let both_ready = trade.lock_in(pc_id)?;

    let resp = sP_FE2CL_REP_PC_TRADE_CONFIRM {
        iID_Request: pc_id,
        iID_From: trade.get_id_from(),
        iID_To: trade.get_id_to(),
    };
    client.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM, &resp)?;
    let client_other = state.get_player(pc_id_other)?.get_client(clients).unwrap();
    log_if_failed(client_other.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM, &resp));

    if !both_ready {
        return Ok(());
    }

    // carry out trade

    let player = state.get_player_mut(pc_id).unwrap();
    player.trade_id = None;
    let mut player = player.clone();

    let player_other = state.get_player_mut(pc_id_other).unwrap();
    player_other.trade_id = None;
    let mut player_other = player_other.clone();

    let trade = state.ongoing_trades.remove(&trade_id).unwrap();
    let id_from = trade.get_id_from();
    let id_to = trade.get_id_to();
    if let Ok((items, items_other)) = trade.resolve((&mut player, &mut player_other)) {
        let player_taros = player.get_taros();
        let player_other_taros = player_other.get_taros();

        // save traded state
        *state.get_player_mut(pc_id).unwrap() = player.clone();
        *state.get_player_mut(pc_id_other).unwrap() = player_other.clone();

        // update the players in the DB
        db_run_async(move |db| db.save_players(&[&player, &player_other]));

        let resp = sP_FE2CL_REP_PC_TRADE_CONFIRM_SUCC {
            iID_Request: pc_id,
            iID_From: id_from,
            iID_To: id_to,
            iCandy: player_other_taros as i32,
            Item: items_other,
            ItemStay: items,
        };
        log_if_failed(client_other.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_SUCC, &resp));

        let resp = sP_FE2CL_REP_PC_TRADE_CONFIRM_SUCC {
            iCandy: player_taros as i32,
            Item: items,
            ItemStay: items_other,
            ..resp
        };
        clients
            .get_self()
            .send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_SUCC, &resp)
    } else {
        let resp = sP_FE2CL_REP_PC_TRADE_CONFIRM_ABORT {
            iID_Request: resp.iID_Request,
            iID_From: resp.iID_From,
            iID_To: resp.iID_To,
        };
        log_if_failed(client_other.send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_ABORT, &resp));
        clients
            .get_self()
            .send_packet(P_FE2CL_REP_PC_TRADE_CONFIRM_ABORT, &resp)
    }
}

pub fn trade_emotes_chat(clients: &mut ClientMap, state: &ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_EMOTES_CHAT = *clients
        .get_self()
        .get_packet(P_CL2FE_REQ_PC_TRADE_EMOTES_CHAT)?;
    catch_fail(
        (|| {
            let pc_id = clients.get_self().get_player_id()?;
            let player = state.get_player(pc_id)?;
            let trade_id = player.trade_id.ok_or(FFError::build(
                Severity::Warning,
                format!("Player {} is not trading", player.get_player_id()),
            ))?;
            let trade = state.ongoing_trades.get(&trade_id).unwrap();
            let id_from = trade.get_id_from();
            let id_to = trade.get_id_to();

            // TODO process chat

            let resp = sP_FE2CL_REP_PC_TRADE_EMOTES_CHAT {
                iID_Request: pc_id,
                iID_From: id_from,
                iID_To: id_to,
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
            };
            let client_one = state.get_player(id_from)?.get_client(clients).unwrap();
            log_if_failed(client_one.send_packet(P_FE2CL_REP_PC_TRADE_EMOTES_CHAT, &resp));
            let client_two = state.get_player(id_to)?.get_client(clients).unwrap();
            log_if_failed(client_two.send_packet(P_FE2CL_REP_PC_TRADE_EMOTES_CHAT, &resp));
            Ok(())
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_TRADE_EMOTES_CHAT_FAIL {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_REP_PC_TRADE_EMOTES_CHAT_FAIL, &resp)
        },
    )
}
