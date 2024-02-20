use rusty_fusion::{
    defines::RANGE_INTERACT,
    entity::EntityID,
    error::*,
    net::{
        packet::{PacketID::*, *},
        FFClient,
    },
    placeholder,
    state::ShardServerState,
    unused,
};

pub fn task_start(client: &mut FFClient, state: &mut ShardServerState) -> FFResult<()> {
    let pkt: sP_CL2FE_REQ_PC_TASK_START = *client.get_packet(P_CL2FE_REQ_PC_TASK_START)?;
    catch_fail(
        (|| {
            let pc_id = client.get_player_id()?;
            state.entity_map.validate_proximity(
                &[EntityID::Player(pc_id), EntityID::NPC(pkt.iNPC_ID)],
                RANGE_INTERACT,
            )?;

            // TODO implement

            let resp = sP_FE2CL_REP_PC_TASK_START_SUCC {
                iTaskNum: pkt.iTaskNum,
                iRemainTime: placeholder!(-1),
            };
            client.send_packet(P_FE2CL_REP_PC_TASK_START_SUCC, &resp)
        })(),
        || {
            let resp = sP_FE2CL_REP_PC_TASK_START_FAIL {
                iTaskNum: pkt.iTaskNum,
                iErrorCode: unused!(),
            };
            client.send_packet(P_FE2CL_REP_PC_TASK_START_FAIL, &resp)
        },
    )
}
