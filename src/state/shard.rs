use std::{collections::HashMap, time::SystemTime};

use rand::thread_rng;
use uuid::Uuid;

use crate::{
    ai::AI,
    chunk::{EntityMap, InstanceID, TickMode},
    config::config_get,
    database::DbResult,
    defines::*,
    entity::{Combatant, Egg, Entity, EntityID, Group, Player, Slider, NPC},
    enums::ItemType,
    error::{log, log_if_failed, panic_log, FFError, FFResult, Severity},
    helpers,
    item::Item,
    net::{
        packet::{PacketID::*, *},
        ClientMap, LoginData,
    },
    tabledata::tdata_get,
    trade::TradeContext,
};

use super::FFReceiver;

pub struct ShardServerState {
    pub shard_id: i32,
    pub login_server_conn_id: Option<Uuid>,
    pub login_data: HashMap<i64, LoginData>,
    pub save_rx: Option<FFReceiver<DbResult>>,
    pub entity_map: EntityMap,
    pub buyback_lists: HashMap<i32, Vec<Item>>,
    pub ongoing_trades: HashMap<Uuid, TradeContext>,
    pub groups: HashMap<Uuid, Group>,
}

impl ShardServerState {
    pub fn new(shard_id: i32) -> Self {
        let mut state = Self {
            login_server_conn_id: None,
            shard_id,
            login_data: HashMap::new(),
            save_rx: None,
            entity_map: EntityMap::default(),
            buyback_lists: HashMap::new(),
            ongoing_trades: HashMap::new(),
            groups: HashMap::new(),
        };
        let num_channels = config_get().shard.num_channels.get();
        if num_channels == 0 || num_channels > MAX_NUM_CHANNELS as u8 {
            panic_log("Invalid number of channels");
        }
        for channel_num in 1..=num_channels {
            for mut npc in tdata_get().make_all_npcs(&mut state.entity_map, channel_num) {
                if let Some(path) = tdata_get().get_npc_path(npc.ty) {
                    npc.path = Some(path);
                }

                let (ai, tick_mode) = AI::make_for_npc(&npc, false);
                npc.ai = ai;

                let chunk_pos = npc.get_chunk_coords();
                let entity_map = &mut state.entity_map;
                let id = entity_map.track(Box::new(npc), tick_mode);
                entity_map.update(id, Some(chunk_pos), None);
            }

            for egg in tdata_get().make_eggs(&mut state.entity_map, channel_num) {
                let chunk_pos = egg.get_chunk_coords();
                let entity_map = &mut state.entity_map;
                let id = entity_map.track(Box::new(egg), TickMode::Always);
                entity_map.update(id, Some(chunk_pos), None);
            }

            // spawn sliders uniformly across the circuit
            let mut slider_circuit = tdata_get().get_slider_path();
            let num_sliders = config_get().shard.num_sliders.get();
            let slider_gap_size = slider_circuit.get_total_length() / num_sliders as u32;
            let mut pos = slider_circuit.get_points()[0].pos;
            let mut dist_to_next = 0;
            let mut sliders_spawned = 0;
            loop {
                if dist_to_next > 0 {
                    let target_pos = slider_circuit.get_target_pos();
                    let dist_to_target = target_pos.distance_to(&pos);
                    if dist_to_target <= dist_to_next {
                        // next point is closer than the distance to the next slider,
                        // so we advance to the next point and continue
                        pos = target_pos;
                        dist_to_next -= dist_to_target;
                        slider_circuit.advance();
                    } else {
                        // next point is farther than the distance to the next slider,
                        // so we interpolate the position and prime a slider spawn
                        let (new_pos, _) = pos.interpolate(&target_pos, dist_to_next as f32);
                        pos = new_pos;
                        dist_to_next = 0;
                    }
                    continue;
                }

                // spawn slider here
                let instance_id = InstanceID {
                    channel_num,
                    map_num: ID_OVERWORLD,
                    instance_num: None,
                };
                let entity_map = &mut state.entity_map;
                let slider = Slider::new(
                    entity_map.gen_next_slider_id(),
                    pos,
                    0,
                    Some(slider_circuit.clone()),
                    instance_id,
                );
                sliders_spawned += 1;
                let chunk_pos = slider.get_chunk_coords();
                let id = entity_map.track(Box::new(slider), TickMode::Always);
                entity_map.update(id, Some(chunk_pos), None);
                dist_to_next = slider_gap_size;
                if sliders_spawned as usize == num_sliders {
                    break;
                }
            }
            log(
                Severity::Debug,
                &format!("Spawned {} sliders", sliders_spawned),
            );
        }
        state
    }

    pub fn get_npc(&self, npc_id: i32) -> FFResult<&NPC> {
        let id = EntityID::NPC(npc_id);
        self.entity_map.get_entity(id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with ID {} doesn't exist", npc_id),
        ))
    }

    pub fn get_npc_mut(&mut self, npc_id: i32) -> FFResult<&mut NPC> {
        let id = EntityID::NPC(npc_id);
        self.entity_map.get_entity_mut(id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with ID {} doesn't exist", npc_id),
        ))
    }

    pub fn get_player(&self, pc_id: i32) -> FFResult<&Player> {
        let id = EntityID::Player(pc_id);
        self.entity_map.get_entity(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }

    pub fn get_player_mut(&mut self, pc_id: i32) -> FFResult<&mut Player> {
        let id = EntityID::Player(pc_id);
        self.entity_map.get_entity_mut(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }

    pub fn get_slider(&self, slider_id: i32) -> FFResult<&Slider> {
        let id = EntityID::Slider(slider_id);
        self.entity_map.get_entity(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Slider with ID {} doesn't exist", slider_id),
        ))
    }

    pub fn get_slider_mut(&mut self, slider_id: i32) -> FFResult<&mut Slider> {
        let id = EntityID::Slider(slider_id);
        self.entity_map.get_entity_mut(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Slider with ID {} doesn't exist", slider_id),
        ))
    }

    pub fn get_egg(&self, egg_id: i32) -> FFResult<&Egg> {
        let id = EntityID::Egg(egg_id);
        self.entity_map.get_entity(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Egg with ID {} doesn't exist", egg_id),
        ))
    }

    pub fn get_egg_mut(&mut self, egg_id: i32) -> FFResult<&mut Egg> {
        let id = EntityID::Egg(egg_id);
        self.entity_map.get_entity_mut(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Egg with ID {} doesn't exist", egg_id),
        ))
    }

    pub fn get_combatant(&self, id: EntityID) -> FFResult<&dyn Combatant> {
        let entity = self.entity_map.get_entity_raw(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} doesn't exist", id),
        ))?;

        entity.as_combatant().ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} isn't a combatant", id),
        ))
    }

    pub fn get_combatant_mut(&mut self, id: EntityID) -> FFResult<&mut dyn Combatant> {
        let entity = self
            .entity_map
            .get_entity_raw_mut(id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Entity with ID {:?} doesn't exist", id),
            ))?;

        entity.as_combatant_mut().ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} isn't a combatant", id),
        ))
    }

    pub fn check_for_expired_vehicles(&mut self, time: SystemTime, clients: &mut ClientMap) {
        log(Severity::Debug, "Checking for expired vehicles");
        let pc_ids: Vec<i32> = self.entity_map.get_player_ids().collect();
        let mut pc_ids_dismounted = Vec::with_capacity(pc_ids.len());
        for pc_id in pc_ids {
            let player = self.get_player_mut(pc_id).unwrap();
            for (location, slot_num) in player.find_items_any(|item| item.ty == ItemType::Vehicle) {
                let vehicle_slot = player.get_item_mut(location, slot_num).unwrap();
                if let Some(expiry_time) = vehicle_slot.unwrap().get_expiry_time() {
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
            let player = self.get_player(pc_id).unwrap();
            helpers::broadcast_state(pc_id, player.get_state_bit_flag(), clients, self);
        }
    }

    pub fn tick_garbage_collection(&mut self, clients: &mut ClientMap) {
        let mut removed_entities = self.entity_map.garbage_collect_instances();
        removed_entities.extend(self.entity_map.garbage_collect_entities());
        for entity in removed_entities.iter_mut() {
            entity.cleanup(clients, self);
        }

        if !removed_entities.is_empty() {
            log(
                Severity::Debug,
                &format!("Garbage collected {} entities", removed_entities.len()),
            );
        }
    }

    pub fn tick_groups(&mut self, clients: &mut ClientMap) {
        for group in self.groups.values() {
            let (pc_group_data, npc_group_data) = group.get_member_data(self);
            let pkt = sP_FE2CL_PC_GROUP_MEMBER_INFO {
                iID: unused!(),
                iMemberPCCnt: pc_group_data.len() as i32,
                iMemberNPCCnt: npc_group_data.len() as i32,
            };
            for eid in group.get_member_ids() {
                let entity = self.entity_map.get_entity_raw(*eid).unwrap();
                if let Some(client) = entity.get_client(clients) {
                    client.queue_packet(P_FE2CL_PC_GROUP_JOIN_SUCC, &pkt);
                    for pc_data in &pc_group_data {
                        client.queue_struct(pc_data);
                    }
                    for npc_data in &npc_group_data {
                        client.queue_struct(npc_data);
                    }
                    log_if_failed(client.flush());
                }
            }
        }
    }

    pub fn tick_entities(&mut self, time: SystemTime, clients: &mut ClientMap) {
        let mut rng = thread_rng();
        let eids: Vec<EntityID> = self.entity_map.get_tickable_ids().collect();
        for eid in eids {
            match eid {
                // we copy the entity here so we can mutably borrow the state.
                // we put it back when we're done.
                EntityID::Player(pc_id) => {
                    let mut player = self.get_player_mut(pc_id).unwrap().clone();
                    player.tick(&time, clients, self, &mut rng);
                    *self.get_player_mut(pc_id).unwrap() = player;
                }
                EntityID::NPC(npc_id) => {
                    let mut npc = self.get_npc_mut(npc_id).unwrap().clone();
                    npc.tick(&time, clients, self, &mut rng);
                    *self.get_npc_mut(npc_id).unwrap() = npc;
                }
                EntityID::Slider(slider_id) => {
                    let mut slider = self.get_slider_mut(slider_id).unwrap().clone();
                    slider.tick(&time, clients, self, &mut rng);
                    *self.get_slider_mut(slider_id).unwrap() = slider;
                }
                EntityID::Egg(egg_id) => {
                    let mut egg = self.get_egg_mut(egg_id).unwrap().clone();
                    egg.tick(&time, clients, self, &mut rng);
                    *self.get_egg_mut(egg_id).unwrap() = egg;
                }
            }
        }
    }

    pub fn check_receivers(&mut self) -> bool {
        if let Some(receiver) = &self.save_rx {
            match receiver.try_recv() {
                None => (), // in progress
                Some(Ok(res)) => {
                    let elapsed = res.completed.elapsed().unwrap_or_default();
                    log(
                        Severity::Info,
                        &format!("Save complete ({:.2}s)", elapsed.as_secs_f32()),
                    );
                    self.save_rx = None;
                }
                Some(Err(e)) => {
                    log(Severity::Warning, &format!("Save failed: {}", e.get_msg()));
                    self.save_rx = None;
                }
            }
        }

        self.save_rx.is_some()
    }
}
