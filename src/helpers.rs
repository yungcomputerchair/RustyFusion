use uuid::Uuid;

use crate::{
    entity::{Combatant, Entity, EntityID},
    enums::RideType,
    error::{log, log_if_failed, FFResult, Severity},
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    state::ShardServerState,
};

pub fn broadcast_state(
    pc_id: i32,
    player_sbf: i8,
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) {
    let bcast = sP_FE2CL_PC_STATE_CHANGE {
        iPC_ID: pc_id,
        iState: player_sbf,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            client.send_packet(P_FE2CL_PC_STATE_CHANGE, &bcast)
        });
}

pub fn broadcast_monkey(
    pc_id: i32,
    ride_type: RideType,
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) {
    let player = state.get_player(pc_id).unwrap();

    // monkey activate packet
    let pkt_monkey = sP_FE2CL_PC_RIDING {
        iPC_ID: pc_id,
        eRT: ride_type as i32,
    };

    // nano stash packets
    let pkt_nano = sP_FE2CL_REP_NANO_ACTIVE_SUCC {
        iActiveNanoSlotNum: -1,
        eCSTB___Add: 0,
    };
    let pkt_nano_bcast = sP_FE2CL_NANO_ACTIVE {
        iPC_ID: pc_id,
        Nano: None.into(),
        iConditionBitFlag: player.get_condition_bit_flag(),
        eCSTB___Add: 0,
    };

    let client = player.get_client(clients).unwrap();
    log_if_failed(client.send_packet(P_FE2CL_REP_PC_RIDING_SUCC, &pkt_monkey));
    log_if_failed(client.send_packet(P_FE2CL_REP_NANO_ACTIVE_SUCC, &pkt_nano));
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_RIDING, &pkt_monkey)?;
            c.send_packet(P_FE2CL_NANO_ACTIVE, &pkt_nano_bcast)
        });
}

pub fn remove_group_member(
    leaver_id: EntityID,
    group_id: Uuid,
    state: &mut ShardServerState,
    clients: &mut ClientMap,
) -> FFResult<()> {
    let mut group = state.groups.get(&group_id).unwrap().clone();
    group.remove_member(leaver_id)?;

    if group.should_disband() {
        // we can just tell all players that they've left the group
        // (except the leaver; that is the caller's job)
        let leaver_pkt = sP_FE2CL_PC_GROUP_LEAVE_SUCC { UNUSED: unused!() };
        for eid in group.get_member_ids() {
            let entity = state.entity_map.get_from_id(*eid).unwrap();
            if let Some(client) = entity.get_client(clients) {
                log_if_failed(client.send_packet(P_FE2CL_PC_GROUP_LEAVE_SUCC, &leaver_pkt));
            }
            match eid {
                EntityID::Player(pc_id) => {
                    state.get_player_mut(*pc_id).unwrap().group_id = None;
                }
                EntityID::NPC(npc_id) => {
                    state.get_npc_mut(*npc_id).unwrap().group_id = None;
                }
                _ => unreachable!(),
            }
        }

        log(Severity::Debug, &format!("Disbanded group {}", group_id));
        state.groups.remove(&group_id);
        return Ok(());
    }

    // notify clients of the group member removal
    let (pc_group_data, npc_group_data) = group.get_member_data(state);
    match leaver_id {
        EntityID::Player(leaver_pc_id) => {
            let update_pkt = sP_FE2CL_PC_GROUP_LEAVE {
                iID_LeaveMember: leaver_pc_id,
                iMemberPCCnt: pc_group_data.len() as i32,
                iMemberNPCCnt: npc_group_data.len() as i32,
            };
            for eid in group.get_member_ids() {
                let entity = state.entity_map.get_from_id(*eid).unwrap();
                if let Some(client) = entity.get_client(clients) {
                    client.queue_packet(P_FE2CL_PC_GROUP_LEAVE, &update_pkt);
                    for pc_data in &pc_group_data {
                        client.queue_struct(pc_data);
                    }
                    for npc_data in &npc_group_data {
                        client.queue_struct(npc_data);
                    }
                    log_if_failed(client.flush());
                }
            }
        }
        EntityID::NPC(leaver_npc_id) => {
            let update_pkt = sP_FE2CL_REP_NPC_GROUP_KICK_SUCC {
                iPC_ID: unused!(),
                iNPC_ID: leaver_npc_id,
                iMemberPCCnt: pc_group_data.len() as i32,
                iMemberNPCCnt: npc_group_data.len() as i32,
            };
            for eid in group.get_member_ids() {
                let entity = state.entity_map.get_from_id(*eid).unwrap();
                if let Some(client) = entity.get_client(clients) {
                    client.queue_packet(P_FE2CL_REP_NPC_GROUP_KICK_SUCC, &update_pkt);
                    for pc_data in &pc_group_data {
                        client.queue_struct(pc_data);
                    }
                    for npc_data in &npc_group_data {
                        client.queue_struct(npc_data);
                    }
                    log_if_failed(client.flush());
                }
            }
        }
        _ => unreachable!(),
    }

    // save group state
    state.groups.insert(group_id, group);
    Ok(())
}
