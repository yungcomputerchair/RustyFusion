use rusty_fusion::{
    error::*,
    net::{
        packet::{PacketID::*, *},
        FFClient,
    },
    state::ShardServerState,
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
