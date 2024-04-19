use rusty_fusion::{
    entity::{Entity, EntityID, Group},
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    state::ShardServerState,
    unused,
};
use uuid::Uuid;

pub fn pc_group_invite(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_GROUP_INVITE =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_GROUP_INVITE)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;

            let target_pc_id = pkt.iID_To;
            let target_player = state.get_player(target_pc_id)?;
            if target_player.group_id.is_some() {
                return Err(FFError::build(
                    Severity::Debug,
                    format!("{} is already in a group", target_player),
                ));
            }

            let target_client = target_player.get_client(clients).unwrap();
            let pkt = sP_FE2CL_PC_GROUP_INVITE { iHostID: pc_id };
            log_if_failed(target_client.send_packet(P_FE2CL_PC_GROUP_INVITE, &pkt));

            let player = state.get_player_mut(pc_id)?;
            player.group_offered_to = Some(target_pc_id);
            Ok(())
        })(),
        || {
            let pkt = sP_FE2CL_PC_GROUP_INVITE_FAIL {
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_PC_GROUP_INVITE_FAIL, &pkt)
        },
    )
}

pub fn pc_group_invite_refuse(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_PC_GROUP_INVITE_REFUSE =
        *client.get_packet(P_CL2FE_REQ_PC_GROUP_INVITE_REFUSE)?;
    let pc_id = client.get_player_id()?;

    let host_pc_id = pkt.iID_From;
    let host_player = state.get_player_mut(host_pc_id)?;
    if host_player.group_offered_to != Some(pc_id) {
        return Err(FFError::build(
            Severity::Debug,
            format!("Group offer from {} expired", host_player),
        ));
    }

    let host_client = host_player.get_client(clients).unwrap();
    let pkt = sP_FE2CL_PC_GROUP_INVITE_REFUSE { iID_To: pc_id };
    log_if_failed(host_client.send_packet(P_FE2CL_PC_GROUP_INVITE_REFUSE, &pkt));
    host_player.group_offered_to = None;
    Ok(())
}

pub fn pc_group_join(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_GROUP_JOIN =
        *clients.get_self().get_packet(P_CL2FE_REQ_PC_GROUP_JOIN)?;
    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;

            let host_pc_id = pkt.iID_From;
            let host_player = state.get_player_mut(host_pc_id)?;
            if host_player.group_offered_to != Some(pc_id) {
                return Err(FFError::build(
                    Severity::Debug,
                    format!("Group offer from {} expired", host_player),
                ));
            }
            host_player.group_offered_to = None;

            let group_id = host_player.group_id.unwrap_or(Uuid::new_v4());
            let mut group = if host_player.group_id.is_some() {
                state.groups.get(&group_id).unwrap().clone()
            } else {
                log(Severity::Debug, &format!("Creating group {}", group_id));
                Group::new(EntityID::Player(host_pc_id))
            };
            group.add_member(EntityID::Player(pc_id))?;

            let (pc_group_data, npc_group_data) = group.get_member_data(state);
            let pkt = sP_FE2CL_PC_GROUP_JOIN_SUCC {
                iID_NewMember: pc_id,
                iMemberPCCnt: pc_group_data.len() as i32,
                iMemberNPCCnt: npc_group_data.len() as i32,
            };
            for eid in group.get_member_ids() {
                let entity = state.entity_map.get_from_id(*eid).unwrap();
                if let Some(client) = entity.get_client(clients) {
                    client.queue_packet(P_FE2CL_PC_GROUP_JOIN_SUCC, &pkt);
                    for pc_data in &pc_group_data {
                        client.queue_struct(pc_data);
                    }
                    for npc_data in &npc_group_data {
                        client.queue_struct(npc_data);
                    }
                    log_if_failed(client.flush());
                }
            }

            state.groups.insert(group_id, group);
            state.get_player_mut(host_pc_id).unwrap().group_id = Some(group_id);
            state.get_player_mut(pc_id).unwrap().group_id = Some(group_id);
            Ok(())
        })(),
        || {
            let pkt = sP_FE2CL_PC_GROUP_JOIN_FAIL {
                iID: pkt.iID_From,
                iErrorCode: unused!(),
            };
            clients
                .get_self()
                .send_packet(P_FE2CL_PC_GROUP_JOIN_FAIL, &pkt)
        },
    )
}
