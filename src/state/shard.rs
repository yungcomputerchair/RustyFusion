use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, TryRecvError},
    time::SystemTime,
};

use uuid::Uuid;

use crate::{
    chunk::EntityMap,
    config::config_get,
    defines::MAX_NUM_CHANNELS,
    enums::ItemType,
    error::{log, log_if_failed, panic_log, FFError, FFResult, Severity},
    helpers,
    net::{
        packet::{PacketID::*, *},
        ClientMap, LoginData,
    },
    npc::NPC,
    player::Player,
    tabledata::tdata_get,
    Entity, EntityID, Item, TradeContext,
};

pub struct ShardServerState {
    pub login_server_conn_id: Option<Uuid>,
    pub shard_id: Option<i32>,
    pub login_data: HashMap<i64, LoginData>,
    pub autosave_rx: Option<(SystemTime, Receiver<FFResult<()>>)>,
    pub entity_map: EntityMap,
    pub buyback_lists: HashMap<i32, Vec<Item>>,
    pub ongoing_trades: HashMap<Uuid, TradeContext>,
}

impl Default for ShardServerState {
    fn default() -> Self {
        let mut state = Self {
            login_server_conn_id: None,
            shard_id: None,
            login_data: HashMap::new(),
            autosave_rx: None,
            entity_map: EntityMap::default(),
            buyback_lists: HashMap::new(),
            ongoing_trades: HashMap::new(),
        };
        let num_channels = config_get().shard.num_channels.get();
        if num_channels == 0 || num_channels > MAX_NUM_CHANNELS {
            panic_log("Invalid number of channels");
        }
        for channel_num in 1..=num_channels {
            for mut npc in tdata_get().get_npcs(&mut state.entity_map, channel_num) {
                if let Some(path) = tdata_get().get_npc_path(npc.ty) {
                    npc.set_path(path);
                }
                let chunk_pos = npc.get_chunk_coords();
                let entity_map = &mut state.entity_map;
                let id = entity_map.track(Box::new(npc));
                entity_map.update(id, Some(chunk_pos), None);
            }
        }
        state
    }
}
impl ShardServerState {
    pub fn get_npc(&self, npc_id: i32) -> FFResult<&NPC> {
        self.entity_map.get_npc(npc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with ID {} doesn't exist", npc_id),
        ))
    }

    pub fn get_npc_mut(&mut self, npc_id: i32) -> FFResult<&mut NPC> {
        self.entity_map.get_npc_mut(npc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with ID {} doesn't exist", npc_id),
        ))
    }

    pub fn get_player(&self, pc_id: i32) -> FFResult<&Player> {
        self.entity_map.get_player(pc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }

    pub fn get_player_mut(&mut self, pc_id: i32) -> FFResult<&mut Player> {
        self.entity_map.get_player_mut(pc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }

    pub fn check_for_expired_vehicles(&mut self, time: SystemTime, clients: &mut ClientMap) {
        log(Severity::Debug, "Checking for expired vehicles");
        let pc_ids: Vec<i32> = self.entity_map.get_player_ids().collect();
        let mut pc_ids_dismounted = Vec::with_capacity(pc_ids.len());
        for pc_id in pc_ids {
            let player = self.entity_map.get_player_mut(pc_id).unwrap();
            for (location, slot_num) in player.find_items_any(|item| item.ty == ItemType::Vehicle) {
                let vehicle_slot = player.get_item_mut(location, slot_num).unwrap();
                if let Some(expiry_time) = vehicle_slot.unwrap().expiry_time {
                    if time > expiry_time {
                        vehicle_slot.take();

                        // dismount
                        let client = player.get_client(clients).unwrap();
                        if player.vehicle_speed.is_some() {
                            player.vehicle_speed = None;
                            let pkt = sP_FE2CL_PC_VEHICLE_OFF_SUCC { UNUSED: unused!() };
                            log_if_failed(client.send_packet(P_FE2CL_PC_VEHICLE_OFF_SUCC, &pkt));
                            pc_ids_dismounted.push(pc_id);
                        }

                        // delete
                        let pkt = sP_FE2CL_PC_DELETE_TIME_LIMIT_ITEM { iItemListCount: 1 };
                        let dat = sTimeLimitItemDeleteInfo2CL {
                            eIL: location as i32,
                            iSlotNum: slot_num as i32,
                        };
                        client.queue_packet(P_FE2CL_PC_DELETE_TIME_LIMIT_ITEM, &pkt);
                        client.queue_struct(&dat);
                        log_if_failed(client.flush());
                    }
                }
            }
        }

        for pc_id in pc_ids_dismounted {
            let player = self.entity_map.get_player(pc_id).unwrap();
            helpers::broadcast_state(pc_id, player.get_state_bit_flag(), clients, self);
        }
    }

    pub fn tick_entities(&mut self, time: SystemTime, clients: &mut ClientMap) {
        let eids: Vec<EntityID> = self.entity_map.get_all_ids().collect();
        for eid in eids {
            match eid {
                // we copy the entity here so we can mutably borrow the state.
                // we put it back when we're done.
                EntityID::Player(pc_id) => {
                    let mut player = self.entity_map.get_player_mut(pc_id).unwrap().clone();
                    player.tick(time, clients, self);
                    *self.entity_map.get_player_mut(pc_id).unwrap() = player;
                }
                EntityID::NPC(npc_id) => {
                    let mut npc = self.entity_map.get_npc_mut(npc_id).unwrap().clone();
                    npc.tick(time, clients, self);
                    *self.entity_map.get_npc_mut(npc_id).unwrap() = npc;
                }
            }
        }
    }

    pub fn check_receivers(&mut self) {
        if let Some((start_time, receiver)) = &self.autosave_rx {
            match receiver.try_recv() {
                Ok(Ok(())) => {
                    let elapsed = start_time.elapsed().unwrap();
                    log(
                        Severity::Info,
                        &format!("Autosave complete ({:.2}s)", elapsed.as_secs_f32()),
                    );
                }
                Ok(Err(e)) => log(
                    Severity::Warning,
                    &format!("Autosave failed: {}", e.get_msg()),
                ),
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Disconnected) => {
                    self.autosave_rx = None;
                }
            }
        }
    }
}
