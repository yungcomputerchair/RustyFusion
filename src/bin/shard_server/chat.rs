use super::*;

pub fn send_freechat_message(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_uid = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_SEND_FREECHAT_MESSAGE =
        client.get_packet(P_CL2FE_REQ_SEND_FREECHAT_MESSAGE);

    let resp = sP_FE2CL_REP_SEND_FREECHAT_MESSAGE_SUCC {
        iPC_ID: pc_uid as i32,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };
    state
        .get_entity_map()
        .for_each_around(EntityID::Player(pc_uid), clients, |client| {
            let _ = client.send_packet(P_FE2CL_REP_SEND_FREECHAT_MESSAGE_SUCC, &resp);
        });

    Ok(())
}

pub fn send_menuchat_message(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_uid = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_SEND_MENUCHAT_MESSAGE =
        client.get_packet(P_CL2FE_REQ_SEND_MENUCHAT_MESSAGE);

    let resp = sP_FE2CL_REP_SEND_MENUCHAT_MESSAGE_SUCC {
        iPC_ID: pc_uid as i32,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };
    state
        .get_entity_map()
        .for_each_around(EntityID::Player(pc_uid), clients, |client| {
            let _ = client.send_packet(P_FE2CL_REP_SEND_MENUCHAT_MESSAGE_SUCC, &resp);
        });

    Ok(())
}

pub fn pc_avatar_emotes_chat(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_uid = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT =
        client.get_packet(P_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT);

    let resp = sP_FE2CL_REP_PC_AVATAR_EMOTES_CHAT {
        iID_From: pkt.iID_From,
        iEmoteCode: pkt.iEmoteCode,
    };
    state
        .get_entity_map()
        .for_each_around(EntityID::Player(pc_uid), clients, |client| {
            let _ = client.send_packet(P_FE2CL_REP_PC_AVATAR_EMOTES_CHAT, &resp);
        });

    Ok(())
}
