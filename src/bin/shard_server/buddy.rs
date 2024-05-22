use rusty_fusion::{
    defines::*,
    entity::{Entity, EntityID},
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap,
    },
    state::ShardServerState,
    util,
};

pub fn request_make_buddy(clients: &mut ClientMap, state: &mut ShardServerState) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_REQUEST_MAKE_BUDDY =
        *client.get_packet(P_CL2FE_REQ_REQUEST_MAKE_BUDDY)?;

    let pc_id = client.get_player_id()?;
    let buddy_id = pkt.iBuddyID;

    state.entity_map.validate_proximity(
        &[EntityID::Player(pc_id), EntityID::Player(buddy_id)],
        RANGE_INTERACT,
    )?;

    let player = state.get_player_mut(pc_id)?;
    //player.buddy_offered_to = Some(buddy_id);

    let req_pkt = sP_FE2CL_REP_REQUEST_MAKE_BUDDY_SUCC_TO_ACCEPTER {
        iRequestID: pc_id,
        iBuddyID: pkt.iBuddyID,
        szFirstName: util::encode_utf16(&player.first_name),
        szLastName: util::encode_utf16(&player.last_name),
    };
    let buddy = state.get_player(buddy_id)?;
    // TODO check if already buddies

    let buddy_client = buddy.get_client(clients).unwrap();
    log_if_failed(
        buddy_client.send_packet(P_FE2CL_REP_REQUEST_MAKE_BUDDY_SUCC_TO_ACCEPTER, &req_pkt),
    );

    Ok(())
}
