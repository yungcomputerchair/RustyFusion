use rand::seq::IteratorRandom;
use rusty_fusion::{
    entity::EntityID,
    error::*,
    net::{
        packet::{PacketID::*, *},
        FFClient,
    },
    state::ShardServerState,
    tabledata::tdata_get,
};

pub fn npc_interaction(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_NPC_INTERACTION = *client.get_packet(P_CL2FE_REQ_NPC_INTERACTION)?;
    let pc_id = client.get_player_id()?;
    let npc_id = pkt.iNPC_ID;

    let npc = state.get_npc_mut(npc_id)?;
    if pkt.bFlag == 0 {
        if !npc.interacting_pcs.remove(&pc_id) {
            log(
                Severity::Warning,
                &format!(
                    "Player {} tried to stop interacting with NPC {} when not interacting",
                    pc_id, npc_id
                ),
            );
        }
    } else if !npc.interacting_pcs.insert(pc_id) {
        log(
            Severity::Warning,
            &format!(
                "Player {} tried to start interacting with NPC {} when already interacting",
                pc_id, npc_id
            ),
        );
    }

    Ok(())
}

pub fn npc_bark(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: &sP_CL2FE_REQ_BARKER = client.get_packet(P_CL2FE_REQ_BARKER)?;
    let task_id = pkt.iMissionTaskID;
    let pc_id = client.get_player_id()?;

    let task_def = tdata_get().get_task_definition(task_id)?;
    let barks = &task_def.barks;
    if barks.is_empty() {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "Player requested barker for task with no barks ({})",
                task_id
            ),
        ));
    }

    // we ignore pkt.iNPC_ID because the client always picks the first NPC in its container.
    // instead, we pick at random from the nearby NPCS that have a compatible barker type
    // for this mission. thanks @FinnHornhoover for finding this workaround (OpenFusion #266)
    let nearby_npc_ids = state.entity_map.get_around_entity(EntityID::Player(pc_id));
    let mut compatible_barks = Vec::with_capacity(nearby_npc_ids.len());
    for npc_id in nearby_npc_ids {
        if let EntityID::NPC(npc_id) = npc_id {
            let npc = state.get_npc(npc_id).unwrap();
            let npc_stats = tdata_get().get_npc_stats(npc.ty).unwrap();
            if npc_stats.bark_type.is_some_and(|bt| bt <= barks.len()) {
                compatible_barks.push((npc_id, npc_stats.bark_type.unwrap() - 1));
            }
        }
    }

    let chosen_bark = compatible_barks.iter().choose(&mut rand::thread_rng());
    match chosen_bark {
        Some(&(npc_id, bark_idx)) => {
            let bark_id = barks[bark_idx];
            let pkt = sP_FE2CL_REP_BARKER {
                iNPC_ID: npc_id,
                iMissionStringID: bark_id,
            };
            client.send_packet(P_FE2CL_REP_BARKER, &pkt)
        }
        None => Ok(()),
    }
}
