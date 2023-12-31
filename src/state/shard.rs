use std::{collections::HashMap, time::SystemTime};

use uuid::Uuid;

use crate::{
    chunk::EntityMap,
    enums::ItemType,
    error::{log, FFError, FFResult, Severity},
    helpers,
    net::{
        packet::{PacketID::*, *},
        ClientMap, LoginData, CONN_ID_DISCONNECTED,
    },
    npc::NPC,
    player::Player,
    tabledata::tdata_get,
    Entity, EntityID, Item, TradeContext,
};

pub struct ShardServerState {
    login_server_conn_id: i64,
    pub login_data: HashMap<i64, LoginData>,
    pub entity_map: EntityMap,
    pub buyback_lists: HashMap<i32, Vec<Item>>,
    pub ongoing_trades: HashMap<Uuid, TradeContext>,
}

impl Default for ShardServerState {
    fn default() -> Self {
        let mut state = Self {
            login_server_conn_id: CONN_ID_DISCONNECTED,
            login_data: HashMap::new(),
            entity_map: EntityMap::default(),
            buyback_lists: HashMap::new(),
            ongoing_trades: HashMap::new(),
        };
        for mut npc in tdata_get().get_npcs(&mut state.entity_map) {
            if let Some(path) = tdata_get().get_npc_path(npc.ty) {
                npc.set_path(path);
            }
            let chunk_pos = npc.get_chunk_coords();
            let entity_map = &mut state.entity_map;
            let id = entity_map.track(Box::new(npc));
            entity_map.update(id, Some(chunk_pos), None);
        }
        state
    }
}
impl ShardServerState {
    pub fn get_login_server_conn_id(&self) -> i64 {
        self.login_server_conn_id
    }

    pub fn set_login_server_conn_id(&mut self, conn_id: i64) {
        self.login_server_conn_id = conn_id;
    }

    pub fn find_npc_by_type(&self, npc_type: i32) -> Option<&NPC> {
        let id = self.entity_map.find_npc(|npc| npc.ty == npc_type);
        if let Some(npc_id) = id {
            Some(self.entity_map.get_npc(npc_id).unwrap())
        } else {
            None
        }
    }

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
                            let _ = client.send_packet(P_FE2CL_PC_VEHICLE_OFF_SUCC, &pkt);
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
                        let _ = client.flush();
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
                    let mut player = *self.entity_map.get_player_mut(pc_id).unwrap();
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
}
