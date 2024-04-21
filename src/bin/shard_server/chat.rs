use rusty_fusion::{
    defines::*,
    entity::{Entity, EntityID},
    error::*,
    net::{
        packet::{PacketID::*, *},
        ClientMap, FFClient,
    },
    state::ShardServerState,
    unused, util,
};

pub fn send_freechat_message(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_SEND_FREECHAT_MESSAGE =
        *client.get_packet(P_CL2FE_REQ_SEND_FREECHAT_MESSAGE)?;
    catch_fail(
        (|| {
            let msg = util::parse_utf16(&pkt.szFreeChat)?;
            if let Some(cmdstr) = msg.strip_prefix('/') {
                let tokens = cmdstr.split_whitespace().collect::<Vec<_>>();
                if !tokens.is_empty() {
                    return commands::handle_custom_command(tokens, clients, state);
                }
            }

            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player(pc_id)?;
            if player.freechat_muted {
                return Err(FFError::build_dc(
                    Severity::Warning,
                    "Muted player sent freechat packet".to_string(),
                ));
            }

            let msg = helpers::process_freechat_message(msg);
            if msg.trim().is_empty() {
                return Ok(());
            }

            log(Severity::Info, &format!("{}: \"{}\"", player, msg));

            let resp = sP_FE2CL_REP_SEND_FREECHAT_MESSAGE_SUCC {
                iPC_ID: pc_id,
                szFreeChat: util::encode_utf16(&msg),
                iEmoteCode: pkt.iEmoteCode,
            };
            state
                .entity_map
                .for_each_around(EntityID::Player(pc_id), clients, |client| {
                    client.send_packet(P_FE2CL_REP_SEND_FREECHAT_MESSAGE_SUCC, &resp)
                });
            Ok(())
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_SEND_FREECHAT_MESSAGE_FAIL {
                iErrorCode: unused!(),
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
            };
            client.send_packet(P_FE2CL_REP_SEND_FREECHAT_MESSAGE_FAIL, &resp)
        },
    )
}

pub fn send_menuchat_message(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_SEND_MENUCHAT_MESSAGE =
        *client.get_packet(P_CL2FE_REQ_SEND_MENUCHAT_MESSAGE)?;

    catch_fail(
        (|| {
            let client = clients.get_self();
            let pc_id = client.get_player_id()?;
            let player = state.get_player(pc_id)?;

            let msg = util::parse_utf16(&pkt.szFreeChat)?;
            if !helpers::validate_menuchat_message(&msg) {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("Invalid menuchat message\n\t{}: '{}'", player, msg),
                ));
            }

            log(Severity::Info, &format!("{}: '{}'", player, msg));

            let resp = sP_FE2CL_REP_SEND_MENUCHAT_MESSAGE_SUCC {
                iPC_ID: pc_id,
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
            };
            state
                .entity_map
                .for_each_around(EntityID::Player(pc_id), clients, |client| {
                    client.send_packet(P_FE2CL_REP_SEND_MENUCHAT_MESSAGE_SUCC, &resp)
                });
            Ok(())
        })(),
        || {
            let client = clients.get_self();
            let resp = sP_FE2CL_REP_SEND_MENUCHAT_MESSAGE_FAIL {
                iErrorCode: unused!(),
                szFreeChat: pkt.szFreeChat,
                iEmoteCode: pkt.iEmoteCode,
            };
            client.send_packet(P_FE2CL_REP_SEND_MENUCHAT_MESSAGE_FAIL, &resp)
        },
    )
}

pub fn send_group_freechat_message(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_SEND_ALL_GROUP_FREECHAT_MESSAGE =
        *client.get_packet(P_CL2FE_REQ_SEND_ALL_GROUP_FREECHAT_MESSAGE)?;
    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;

    let msg = util::parse_utf16(&pkt.szFreeChat)?;
    if player.freechat_muted {
        return Err(FFError::build_dc(
            Severity::Warning,
            "Muted player sent freechat packet".to_string(),
        ));
    }

    let msg = helpers::process_freechat_message(msg);
    if msg.trim().is_empty() {
        return Ok(());
    }

    log(
        Severity::Info,
        &format!("{} (to group): \"{}\"", player, msg),
    );

    let pkt = sP_FE2CL_REP_SEND_ALL_GROUP_FREECHAT_MESSAGE_SUCC {
        iSendPCID: pc_id,
        szFreeChat: util::encode_utf16(&msg),
        iEmoteCode: pkt.iEmoteCode,
    };
    if let Some(group_id) = player.group_id {
        let group = state.groups.get(&group_id).unwrap();
        for eid in group.get_member_ids() {
            let entity = state.entity_map.get_from_id(*eid).unwrap();
            if let Some(client) = entity.get_client(clients) {
                log_if_failed(
                    client.send_packet(P_FE2CL_REP_SEND_ALL_GROUP_FREECHAT_MESSAGE_SUCC, &pkt),
                );
            }
        }
    }

    Ok(())
}

pub fn send_group_menuchat_message(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pkt: sP_CL2FE_REQ_SEND_ALL_GROUP_MENUCHAT_MESSAGE =
        *client.get_packet(P_CL2FE_REQ_SEND_ALL_GROUP_MENUCHAT_MESSAGE)?;
    let pc_id = client.get_player_id()?;
    let player = state.get_player(pc_id)?;

    let msg = util::parse_utf16(&pkt.szFreeChat)?;
    if !helpers::validate_menuchat_message(&msg) {
        return Err(FFError::build(
            Severity::Warning,
            format!("Invalid menuchat message\n\t{}: '{}'", player, msg),
        ));
    }

    log(Severity::Info, &format!("{} (to group): '{}'", player, msg));

    let pkt = sP_FE2CL_REP_SEND_ALL_GROUP_MENUCHAT_MESSAGE_SUCC {
        iSendPCID: pc_id,
        szFreeChat: pkt.szFreeChat,
        iEmoteCode: pkt.iEmoteCode,
    };
    if let Some(group_id) = player.group_id {
        let group = state.groups.get(&group_id).unwrap();
        for eid in group.get_member_ids() {
            let entity = state.entity_map.get_from_id(*eid).unwrap();
            if let Some(client) = entity.get_client(clients) {
                log_if_failed(
                    client.send_packet(P_FE2CL_REP_SEND_ALL_GROUP_MENUCHAT_MESSAGE_SUCC, &pkt),
                );
            }
        }
    }

    Ok(())
}

pub fn pc_avatar_emotes_chat(
    clients: &mut ClientMap,
    state: &mut ShardServerState,
) -> FFResult<()> {
    let client = clients.get_self();
    let pc_id = client.get_player_id()?;
    let pkt: &sP_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT =
        client.get_packet(P_CL2FE_REQ_PC_AVATAR_EMOTES_CHAT)?;

    let resp = sP_FE2CL_REP_PC_AVATAR_EMOTES_CHAT {
        iID_From: pkt.iID_From,
        iEmoteCode: pkt.iEmoteCode,
    };
    state
        .entity_map
        .for_each_around(EntityID::Player(pc_id), clients, |client| {
            client.send_packet(P_FE2CL_REP_PC_AVATAR_EMOTES_CHAT, &resp)
        });
    Ok(())
}

mod helpers {
    pub fn validate_menuchat_message(_msg: &str) -> bool {
        // TODO validate
        true
    }

    pub fn process_freechat_message(msg: String) -> String {
        // TODO process
        msg
    }
}

mod commands {
    use std::{collections::HashMap, sync::OnceLock};

    use rusty_fusion::database::db_get;

    use super::*;

    struct Command {
        description: &'static str,
        handler: CommandHandler,
    }

    static AVAILABLE_COMMANDS: OnceLock<HashMap<&'static str, Command>> = OnceLock::new();
    type CommandHandler = fn(Vec<&str>, &mut ClientMap, &mut ShardServerState) -> FFResult<()>;

    fn init_commands() -> HashMap<&'static str, Command> {
        #[rustfmt::skip]
        let commands: [(&'static str, &'static str, CommandHandler); 4] = [
            ("about", "Show information about the server", cmd_about),
            ("perms", "View or change a player's permissions level", cmd_perms),
            ("refresh", "Reinsert the player into the current chunk", cmd_refresh),
            ("help", "Show this help message", cmd_help),
        ];

        commands
            .into_iter()
            .map(|(name, description, handler)| {
                (
                    name,
                    Command {
                        description,
                        handler,
                    },
                )
            })
            .collect()
    }

    fn send_system_message(client: &mut FFClient, msg: &str) -> FFResult<()> {
        let resp = sP_FE2CL_PC_MOTD_LOGIN {
            iType: unused!(),
            szSystemMsg: util::encode_utf16(msg),
        };
        client.send_packet(P_FE2CL_PC_MOTD_LOGIN, &resp)
    }

    fn parse_pc_id(token: &str) -> Result<Option<i32>, ()> {
        if token == "." {
            return Ok(None);
        }
        token.parse::<i32>().map_err(|_| ()).map(Some)
    }

    pub fn handle_custom_command(
        tokens: Vec<&str>,
        clients: &mut ClientMap,
        state: &mut ShardServerState,
    ) -> FFResult<()> {
        let cmds = AVAILABLE_COMMANDS.get_or_init(init_commands);

        let cmd_name = tokens[0];
        if let Some(cmd) = cmds.get(cmd_name) {
            (cmd.handler)(tokens, clients, state)
        } else {
            send_system_message(
                clients.get_self(),
                &format!(
                    "Unknown command /{}\nUse /help for a list of available commands",
                    cmd_name
                ),
            )
        }
    }

    fn cmd_about(
        _tokens: Vec<&str>,
        clients: &mut ClientMap,
        _state: &mut ShardServerState,
    ) -> FFResult<()> {
        send_system_message(
            clients.get_self(),
            &format!(
                "RustyFusion by ycc\n\
            Library version: {}\n\
            Protocol version: {}\n\
            Database version: {}",
                LIB_VERSION.unwrap_or("unknown"),
                PROTOCOL_VERSION,
                DB_VERSION,
            ),
        )
    }

    fn cmd_perms(
        tokens: Vec<&str>,
        clients: &mut ClientMap,
        state: &mut ShardServerState,
    ) -> FFResult<()> {
        let client = clients.get_self();
        if tokens.len() < 2 {
            return send_system_message(
                client,
                "Usage: /perms <pc_id> [new_level] [\"save\"]\n\
                Use . for pc_id to select yourself\n\
                Leave new_level empty to view the current level\n\
                Add \"save\" to save the new level to the account",
            );
        }

        let pc_id = client.get_player_id()?;
        let player = state.get_player(pc_id)?;
        let own_perms = player.perms;

        let target_pc_id = match parse_pc_id(tokens[1]) {
            Ok(Some(pc_id)) => pc_id,
            Ok(None) => pc_id,
            Err(_) => return send_system_message(client, "Invalid player ID"),
        };

        let Ok(target_player) = state.get_player_mut(target_pc_id) else {
            return send_system_message(client, &format!("Player {} not found", target_pc_id));
        };
        let target_perms = target_player.perms;

        if tokens.len() < 3 {
            return send_system_message(
                client,
                &format!("{} has permissions level {}", target_player, target_perms),
            );
        }

        let Ok(new_perms) = tokens[2].parse::<i16>() else {
            return send_system_message(client, "Invalid permissions level");
        };

        if !(1..=99).contains(&new_perms) {
            return send_system_message(client, "Permissions level out of range [1, 99]");
        }

        if new_perms <= own_perms {
            return send_system_message(
                client,
                &format!(
                    "Can only grant weaker permissions than your own (> {})",
                    own_perms
                ),
            );
        }

        if pc_id != target_pc_id && target_perms <= own_perms {
            return send_system_message(
                client, &format!(
                    "Can only change the permissions of a player with weaker ones than your own (> {})",
                    own_perms
                ),
            );
        }

        target_player.perms = new_perms;
        log_if_failed(send_system_message(
            client,
            &format!(
                "Permissions level changed to {} for {}",
                new_perms, target_player
            ),
        ));

        if tokens.get(3).is_some_and(|arg| *arg == "save") {
            let mut db = db_get();
            let acc = db
                .find_account_from_player(target_player.get_uid())
                .unwrap();
            match db.change_account_level(acc.id, new_perms as i32) {
                Ok(()) => log_if_failed(send_system_message(
                    client,
                    "Permissions level saved to account!",
                )),
                Err(e) => log_if_failed(send_system_message(
                    client,
                    &format!("Failed to save permissions level: {}", e.get_msg()),
                )),
            }
        }
        Ok(())
    }

    fn cmd_refresh(
        _tokens: Vec<&str>,
        clients: &mut ClientMap,
        state: &mut ShardServerState,
    ) -> FFResult<()> {
        let pc_id = clients.get_self().get_player_id()?;
        let player = state.get_player(pc_id)?;
        let chunk_coords = player.get_chunk_coords();
        state
            .entity_map
            .update(EntityID::Player(pc_id), None, Some(clients));
        state
            .entity_map
            .update(EntityID::Player(pc_id), Some(chunk_coords), Some(clients));
        Ok(())
    }

    fn cmd_help(
        _tokens: Vec<&str>,
        clients: &mut ClientMap,
        _state: &mut ShardServerState,
    ) -> FFResult<()> {
        let mut help_msg = "Available commands\n".to_string();
        for (cmd_name, cmd) in AVAILABLE_COMMANDS.get().unwrap() {
            help_msg.push_str(&format!("/{}: {}\n", cmd_name, cmd.description));
        }
        help_msg.pop(); // remove trailing newline
        send_system_message(clients.get_self(), &help_msg)
    }
}
