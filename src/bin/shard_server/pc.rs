use super::*;

use crate::ShardServerState;

pub fn pc_enter(client: &mut FFClient, key: usize, state: &mut ShardServerState) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_ENTER = client.get_packet();
    let serial_key: i64 = pkt.iEnterSerialKey;
    let login_data = state.login_data.remove(&serial_key).unwrap();
    let mut player = login_data.player;
    player.set_client_id(key);

    let resp = sP_FE2CL_REP_PC_ENTER_SUCC {
        iID: login_data.iPC_UID as i32,
        PCLoadData2CL: player.get_load_data(),
        uiSvrTime: get_time(),
    };

    client.set_client_type(ClientType::GameClient {
        serial_key: pkt.iEnterSerialKey,
        pc_uid: Some(login_data.iPC_UID),
    });

    let iv1: i32 = resp.iID + 1;
    let iv2: i32 = resp.PCLoadData2CL.iFusionMatter + 1;
    client.set_e_key(gen_key(resp.uiSvrTime, iv1, iv2));
    client.set_fe_key(login_data.uiFEKey.to_le_bytes());
    client.set_enc_mode(EncryptionMode::FEKey);

    state.entities.track(Box::new(player));

    client.send_packet(P_FE2CL_REP_PC_ENTER_SUCC, &resp)?;
    Ok(())
}

pub fn pc_loading_complete(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_LOADING_COMPLETE = client.get_packet();
    let resp = sP_FE2CL_REP_PC_LOADING_COMPLETE_SUCC { iPC_ID: pkt.iPC_ID };
    client.send_packet(P_FE2CL_REP_PC_LOADING_COMPLETE_SUCC, &resp)?;

    Ok(())
}

pub fn pc_goto(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_GOTO = client.get_packet();
    if let ClientType::GameClient {
        pc_uid: Some(_), ..
    } = client.get_client_type()
    {
        let resp = sP_FE2CL_REP_PC_GOTO_SUCC {
            iX: pkt.iToX,
            iY: pkt.iToY,
            iZ: pkt.iToZ,
        };
        client.send_packet(P_FE2CL_REP_PC_GOTO_SUCC, &resp)?;
        return Ok(());
    }

    Err(Box::new(BadRequest::new(client)))
}

pub fn pc_move(clients: &mut ClientMap, state: &mut ShardServerState) -> Result<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_MOVE = client.get_packet();
    let (x, y, z) = (pkt.iX, pkt.iY, pkt.iZ);
    if let ClientType::GameClient {
        pc_uid: Some(pc_uid),
        ..
    } = client.get_client_type()
    {
        let resp = sP_FE2CL_PC_MOVE {
            iCliTime: pkt.iCliTime,
            iX: pkt.iX,
            iY: pkt.iY,
            iZ: pkt.iZ,
            fVX: pkt.fVX,
            fVY: pkt.fVY,
            fVZ: pkt.fVZ,
            iAngle: pkt.iAngle,
            cKeyValue: pkt.cKeyValue,
            iSpeed: pkt.iSpeed,
            iID: pc_uid as i32,
            iSvrTime: get_time(),
        };
        clients
            .get_all_gameclient_but_self()
            .try_for_each(|c| c.send_packet(P_FE2CL_PC_MOVE, &resp))?;

        state.update_player(pc_uid, |player, state| {
            player.set_position(x, y, z, &mut state.entities, clients);
        })?;
        return Ok(());
    }

    Err(Box::new(BadRequest::new(client)))
}

pub fn pc_jump(clients: &mut ClientMap, state: &mut ShardServerState) -> Result<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_JUMP = client.get_packet();
    let (x, y, z) = (pkt.iX, pkt.iY, pkt.iZ);
    if let ClientType::GameClient {
        pc_uid: Some(pc_uid),
        ..
    } = client.get_client_type()
    {
        let resp = sP_FE2CL_PC_JUMP {
            iCliTime: pkt.iCliTime,
            iX: pkt.iX,
            iY: pkt.iY,
            iZ: pkt.iZ,
            iVX: pkt.iVX,
            iVY: pkt.iVY,
            iVZ: pkt.iVZ,
            iAngle: pkt.iAngle,
            cKeyValue: pkt.cKeyValue,
            iSpeed: pkt.iSpeed,
            iID: pc_uid as i32,
            iSvrTime: get_time(),
        };
        clients
            .get_all_gameclient_but_self()
            .try_for_each(|c| c.send_packet(P_FE2CL_PC_JUMP, &resp))?;

        state.update_player(pc_uid, |player, state| {
            player.set_position(x, y, z, &mut state.entities, clients);
        })?;
        return Ok(());
    }

    Err(Box::new(BadRequest::new(client)))
}

pub fn pc_stop(clients: &mut ClientMap, state: &mut ShardServerState) -> Result<()> {
    let client = clients.get_self();
    let pkt: &sP_CL2FE_REQ_PC_STOP = client.get_packet();
    let (x, y, z) = (pkt.iX, pkt.iY, pkt.iZ);
    if let ClientType::GameClient {
        pc_uid: Some(pc_uid),
        ..
    } = client.get_client_type()
    {
        let resp = sP_FE2CL_PC_STOP {
            iCliTime: pkt.iCliTime,
            iX: x,
            iY: y,
            iZ: z,
            iID: pc_uid as i32,
            iSvrTime: get_time(),
        };
        clients
            .get_all_gameclient_but_self()
            .try_for_each(|c| c.send_packet(P_FE2CL_PC_STOP, &resp))?;

        state.update_player(pc_uid, |player, state| {
            player.set_position(x, y, z, &mut state.entities, clients);
        })?;
        return Ok(());
    }

    Err(Box::new(BadRequest::new(client)))
}
