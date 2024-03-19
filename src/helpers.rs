use crate::{
    entity::{Combatant, Entity, EntityID},
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
