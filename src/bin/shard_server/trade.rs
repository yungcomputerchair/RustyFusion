use rusty_fusion::{enums::ItemLocation, error::catch_fail, TradeContext};
use uuid::Uuid;

use super::*;

pub fn trade_offer(clients: &mut ClientMap, state: &ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TRADE_OFFER =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_TRADE_OFFER)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let player = state.get_player(client.get_player_id()?)?;
            if player.trade_id.is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("From player {} already trading", player.get_player_id()),
                ));
            }

            let other_player = state.get_player(pkt.iID_To)?;
            if other_player.trade_id.is_some() {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("To player {} already trading", other_player.get_player_id()),
                ));
            }

            let other_client = clients.get_from_player_id(pkt.iID_To)?;
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
            };
            let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER, &resp);
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
            let resp = sP_FE2CL_REP_PC_TRADE_OFFER_SUCC {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
            };
            let player_from = state.get_player_mut(pkt.iID_From)?;
            player_from.trade_id = Some(trade_id);
            let player_to = state.get_player_mut(pkt.iID_To)?;
            player_to.trade_id = Some(trade_id);
            let other_client = clients.get_from_player_id(pkt.iID_From)?;
            let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_SUCC, &resp);
            state
                .ongoing_trades
                .insert(trade_id, TradeContext::new([pkt.iID_From, pkt.iID_To]));
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

pub fn trade_offer_refusal(clients: &mut ClientMap) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL =
        client.get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_REFUSAL)?;
    let resp = sP_FE2CL_REP_PC_TRADE_OFFER_REFUSAL {
        iID_Request: pkt.iID_Request,
        iID_From: pkt.iID_From,
        iID_To: pkt.iID_To,
    };
    let other_client = clients.get_from_player_id(resp.iID_From)?;
    let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_REFUSAL, &resp);
    Ok(())
}

pub fn trade_offer_cancel(clients: &mut ClientMap) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_TRADE_OFFER_CANCEL =
        client.get_packet(P_CL2FE_REQ_PC_TRADE_OFFER_CANCEL)?;
    let resp = sP_FE2CL_REP_PC_TRADE_OFFER_CANCEL {
        iID_Request: pkt.iID_Request,
        iID_From: pkt.iID_From,
        iID_To: pkt.iID_To,
    };
    let other_client = clients.get_from_player_id(resp.iID_From)?;
    let _ = other_client.send_packet(P_FE2CL_REP_PC_TRADE_OFFER_CANCEL, &resp);
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

            let trade = state
                .ongoing_trades
                .get_mut(&trade_id)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Trade {} doesn't exist", trade_id),
                ))?;
            trade.set_taros(pc_id, req_taros)?;

            let resp = sP_FE2CL_REP_PC_TRADE_CASH_REGISTER_SUCC {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                iCandy: pkt.iCandy,
            };
            // just blast the packet to both parties
            let client_one = clients.get_from_player_id(pkt.iID_From)?;
            let _ = client_one.send_packet(P_FE2CL_REP_PC_TRADE_CASH_REGISTER_SUCC, &resp);
            let client_two = clients.get_from_player_id(pkt.iID_To)?;
            let _ = client_two.send_packet(P_FE2CL_REP_PC_TRADE_CASH_REGISTER_SUCC, &resp);
            Ok(())
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

            let trade = state
                .ongoing_trades
                .get_mut(&trade_id)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Trade {} doesn't exist", trade_id),
                ))?;
            let trade_slot_num = pkt.Item.iSlotNum as usize;
            let quantity_left = item.get_quantity()
                - trade.add_item(pc_id, trade_slot_num, inven_slot_num, quantity)?;

            let resp = sP_FE2CL_REP_PC_TRADE_ITEM_REGISTER_SUCC {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                TradeItem: pkt.Item,
                InvenItem: sItemTrade {
                    iOpt: quantity_left as i32,
                    ..pkt.Item
                },
            };
            // just blast the packet to both parties
            let client_one = clients.get_from_player_id(pkt.iID_From)?;
            let _ = client_one.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_REGISTER_SUCC, &resp);
            let client_two = clients.get_from_player_id(pkt.iID_To)?;
            let _ = client_two.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_REGISTER_SUCC, &resp);
            Ok(())
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

            let trade = state
                .ongoing_trades
                .get_mut(&trade_id)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Trade {} doesn't exist", trade_id),
                ))?;
            let trade_slot_num = pkt.Item.iSlotNum as usize;
            let (quantity_left, inven_slot_num) = trade.remove_item(pc_id, trade_slot_num)?;
            let item = state
                .get_player(pc_id)
                .unwrap()
                .get_item(ItemLocation::Inven, inven_slot_num)
                .unwrap()
                .unwrap();
            let quantity = item.get_quantity() - quantity_left;

            let resp = sP_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_SUCC {
                iID_Request: pkt.iID_Request,
                iID_From: pkt.iID_From,
                iID_To: pkt.iID_To,
                TradeItem: pkt.Item,
                InvenItem: sItemTrade {
                    iOpt: quantity as i32,
                    iInvenNum: inven_slot_num as i32,
                    /* IMPORTANT: the client sends us type 8 here so we MUST override it.
                     * This took me an entire day to diagnose. */
                    iType: item.get_type() as i16,
                    ..pkt.Item
                },
            };
            // just blast the packet to both parties
            let client_one = clients.get_from_player_id(pkt.iID_From)?;
            let _ = client_one.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_SUCC, &resp);
            let client_two = clients.get_from_player_id(pkt.iID_To)?;
            let _ = client_two.send_packet(P_FE2CL_REP_PC_TRADE_ITEM_UNREGISTER_SUCC, &resp);
            Ok(())
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
