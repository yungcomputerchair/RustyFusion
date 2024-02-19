use crate::{
    entity::{Entity, EntityID},
    enums::RideType,
    error::log_if_failed,
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
    let pkt = sP_FE2CL_PC_RIDING {
        iPC_ID: pc_id,
        eRT: ride_type as i32,
    };
    log_if_failed(
        state
            .get_player(pc_id)
            .unwrap()
            .get_client(clients)
            .unwrap()
            .send_packet(P_FE2CL_REP_PC_RIDING_SUCC, &pkt),
    );
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |c| {
            c.send_packet(P_FE2CL_PC_RIDING, &pkt)
        });
}
