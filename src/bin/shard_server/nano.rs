use rusty_fusion::{error::catch_fail, placeholder, tabledata::tdata_get, Combatant};

use super::*;

pub fn nano_equip(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_NANO_EQUIP = client.get_packet(P_CL2FE_REQ_NANO_EQUIP)?;

    let player = state.get_player_mut(pc_id)?;
    player.change_nano(pkt.iNanoSlotNum as usize, Some(pkt.iNanoID as u16))?;

    let deactivate = player.get_active_nano_slot() == Some(pkt.iNanoSlotNum as usize);
    let resp = sP_FE2CL_REP_NANO_EQUIP_SUCC {
        iNanoID: pkt.iNanoID,
        iNanoSlotNum: pkt.iNanoSlotNum,
        bNanoDeactive: if deactivate { 1 } else { 0 },
    };

    if deactivate {
        player.set_active_nano_slot(None).unwrap();
        let bcast = sP_FE2CL_NANO_ACTIVE {
            iPC_ID: pc_id,
            Nano: None.into(),
            iConditionBitFlag: player.get_condition_bit_flag(),
            eCSTB___Add: 0,
        };
        state
            .entity_map
            .for_each_around(EntityID::Player(pc_id), clients, |c| {
                let _ = c.send_packet(P_FE2CL_NANO_ACTIVE, &bcast);
            });
    }

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_NANO_EQUIP_SUCC, &resp)
}

pub fn nano_unequip(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_NANO_UNEQUIP = client.get_packet(P_CL2FE_REQ_NANO_UNEQUIP)?;

    let player = state.get_player_mut(pc_id)?;
    player.change_nano(pkt.iNanoSlotNum as usize, None)?;

    let deactivate = player.get_active_nano_slot() == Some(pkt.iNanoSlotNum as usize);
    let resp = sP_FE2CL_REP_NANO_UNEQUIP_SUCC {
        iNanoSlotNum: pkt.iNanoSlotNum,
        bNanoDeactive: if deactivate { 1 } else { 0 },
    };

    if deactivate {
        player.set_active_nano_slot(None).unwrap();
        let bcast = sP_FE2CL_NANO_ACTIVE {
            iPC_ID: pc_id,
            Nano: None.into(),
            iConditionBitFlag: player.get_condition_bit_flag(),
            eCSTB___Add: 0,
        };
        state
            .entity_map
            .for_each_around(EntityID::Player(pc_id), clients, |c| {
                let _ = c.send_packet(P_FE2CL_NANO_ACTIVE, &bcast);
            });
    }

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_NANO_UNEQUIP_SUCC, &resp)
}

pub fn nano_active(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_NANO_ACTIVE = client.get_packet(P_CL2FE_REQ_NANO_ACTIVE)?;

    let player = state.get_player_mut(pc_id)?;
    if pkt.iNanoSlotNum == -1 {
        player.set_active_nano_slot(None).unwrap();
    } else {
        player.set_active_nano_slot(Some(pkt.iNanoSlotNum as usize))?;
    }

    let resp = sP_FE2CL_REP_NANO_ACTIVE_SUCC {
        iActiveNanoSlotNum: pkt.iNanoSlotNum,
        eCSTB___Add: placeholder!(0),
    };

    let bcast = sP_FE2CL_NANO_ACTIVE {
        iPC_ID: pc_id,
        Nano: player.get_active_nano().copied().into(),
        iConditionBitFlag: player.get_condition_bit_flag(),
        eCSTB___Add: placeholder!(0),
    };

    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            let _ = c.send_packet(P_FE2CL_NANO_ACTIVE, &bcast);
        });

    clients
        .get_self()
        .send_packet(P_FE2CL_REP_NANO_ACTIVE_SUCC, &resp)
}

pub fn nano_tune(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_NANO_TUNE = *client.get_packet(P_CL2FE_REQ_NANO_TUNE)?;
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            let player = state.get_player_mut(pc_id)?;

            // TODO consume tuning items
            let item_slots = placeholder!([-1; 10]);
            let items = placeholder!([None.into(); 10]);

            let stats = tdata_get().get_nano_stats(pkt.iNanoID)?;
            let skill_idx = stats
                .skills
                .iter()
                .position(|sid| *sid == pkt.iTuneID)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("Bad skill ID {} for nano {}", pkt.iTuneID, pkt.iNanoID),
                ))?;

            player.tune_nano(pkt.iNanoID as usize, Some(skill_idx))?;

            let resp = sP_FE2CL_REP_NANO_TUNE_SUCC {
                iNanoID: pkt.iNanoID,
                iSkillID: pkt.iTuneID,
                iPC_FusionMatter: player.get_fusion_matter() as i32,
                aiItemSlotNum: item_slots,
                aItem: items,
            };
            client.send_packet(P_FE2CL_REP_NANO_TUNE_SUCC, &resp)
        })(),
        || {
            let pc_id = client.get_player_id()?;
            let resp = sP_FE2CL_REP_NANO_TUNE_FAIL {
                iPC_ID: pc_id,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_NANO_TUNE_FAIL, &resp)
        },
    )
}
