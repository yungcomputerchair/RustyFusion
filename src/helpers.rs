use crate::{
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    state::shard::ShardServerState,
    EntityID,
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
