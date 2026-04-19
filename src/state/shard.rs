use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

use uuid::Uuid;

use crate::{
    ai,
    chunk::{ChunkCoords, EntityMap, InstanceID, TickMode},
    config::config_get,
    defines::*,
    entity::{Combatant, Egg, Entity, EntityID, Group, Player, Slider, NPC},
    enums::ItemType,
    error::{log, log_if_failed, FFError, FFResult, Severity},
    helpers,
    item::Item,
    net::{
        packet::{PacketID::*, *},
        FFClient, LoginData,
    },
    skills::BuffEffect,
    tabledata::tdata_get,
    trade::TradeContext,
    Position,
};

pub struct ShardServerState {
    pub shard_id: Option<i32>,
    pub login_server_conn_id: Option<Uuid>,
    pub login_data: HashMap<i64, LoginData>,
    pub entity_map: EntityMap,
    pub players: HashMap<i32, Player>,
    pub npcs: HashMap<i32, NPC>,
    pub sliders: HashMap<i32, Slider>,
    pub eggs: HashMap<i32, Egg>,
    pub buyback_lists: HashMap<i32, Vec<Item>>,
    pub ongoing_trades: HashMap<Uuid, TradeContext>,
    pub groups: HashMap<Uuid, Group>,
    pub player_uid_to_id: HashMap<i64, i32>,
    pub pending_entering_uids: HashSet<i64>,
    pub pending_buff_effects: Vec<BuffEffect>,
}
impl Default for ShardServerState {
    fn default() -> Self {
        let mut state = Self {
            login_server_conn_id: None,
            shard_id: None,
            login_data: HashMap::new(),
            entity_map: EntityMap::default(),
            players: HashMap::new(),
            npcs: HashMap::new(),
            sliders: HashMap::new(),
            eggs: HashMap::new(),
            buyback_lists: HashMap::new(),
            ongoing_trades: HashMap::new(),
            groups: HashMap::new(),
            player_uid_to_id: HashMap::new(),
            pending_entering_uids: HashSet::new(),
            pending_buff_effects: Vec::new(),
        };

        let num_channels = config_get().shard.num_channels.get();
        if num_channels == 0 || num_channels > MAX_NUM_CHANNELS as u8 {
            panic!("Invalid number of channels {}", num_channels);
        }

        for channel_num in 1..=num_channels {
            for mut npc in tdata_get().make_all_npcs(&mut state.entity_map, channel_num) {
                if let Some(path) = tdata_get().get_npc_path(npc.ty) {
                    npc.path = Some(path);
                }

                let (ai, tick_mode) = ai::make_for_npc(&npc, false);
                npc.ai = ai;

                let chunk_pos = npc.get_chunk_coords();
                let npc_id = npc.id;
                let eid = EntityID::NPC(npc_id);
                state.npcs.insert(npc_id, npc);
                state.entity_map.track(eid, tick_mode);
                state.entity_map.update(eid, Some(chunk_pos));
            }

            for egg in tdata_get().make_eggs(&mut state.entity_map, channel_num) {
                let chunk_pos = egg.get_chunk_coords();
                let egg_id = egg.id;
                let eid = EntityID::Egg(egg_id);
                state.eggs.insert(egg_id, egg);
                state.entity_map.track(eid, TickMode::Always);
                state.entity_map.update(eid, Some(chunk_pos));
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
                        pos = target_pos;
                        dist_to_next -= dist_to_target;
                        slider_circuit.advance();
                    } else {
                        let (new_pos, _) = pos.interpolate(&target_pos, dist_to_next as f32);
                        pos = new_pos;
                        dist_to_next = 0;
                    }
                    continue;
                }

                let instance_id = InstanceID {
                    channel_num,
                    map_num: ID_OVERWORLD,
                    instance_num: None,
                };
                let slider_id = state.entity_map.gen_next_slider_id();
                let slider =
                    Slider::new(slider_id, pos, 0, Some(slider_circuit.clone()), instance_id);
                sliders_spawned += 1;
                let chunk_pos = slider.get_chunk_coords();
                let eid = EntityID::Slider(slider_id);
                state.sliders.insert(slider_id, slider);
                state.entity_map.track(eid, TickMode::Always);
                state.entity_map.update(eid, Some(chunk_pos));
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
}

// =============================================================================
// Typed entity accessors
// =============================================================================

impl ShardServerState {
    pub fn get_npc(&self, npc_id: i32) -> FFResult<&NPC> {
        self.npcs.get(&npc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with ID {} doesn't exist", npc_id),
        ))
    }

    pub fn get_npc_mut(&mut self, npc_id: i32) -> FFResult<&mut NPC> {
        self.npcs.get_mut(&npc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("NPC with ID {} doesn't exist", npc_id),
        ))
    }

    pub fn get_player(&self, pc_id: i32) -> FFResult<&Player> {
        self.players.get(&pc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }

    pub fn get_player_mut(&mut self, pc_id: i32) -> FFResult<&mut Player> {
        self.players.get_mut(&pc_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Player with ID {} doesn't exist", pc_id),
        ))
    }

    pub fn get_player_by_uid(&self, pc_uid: i64) -> Option<&Player> {
        self.player_uid_to_id
            .get(&pc_uid)
            .and_then(|pc_id| self.get_player(*pc_id).ok())
    }

    pub fn get_slider(&self, slider_id: i32) -> FFResult<&Slider> {
        self.sliders.get(&slider_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Slider with ID {} doesn't exist", slider_id),
        ))
    }

    pub fn get_slider_mut(&mut self, slider_id: i32) -> FFResult<&mut Slider> {
        self.sliders.get_mut(&slider_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Slider with ID {} doesn't exist", slider_id),
        ))
    }

    pub fn get_egg(&self, egg_id: i32) -> FFResult<&Egg> {
        self.eggs.get(&egg_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Egg with ID {} doesn't exist", egg_id),
        ))
    }

    pub fn get_egg_mut(&mut self, egg_id: i32) -> FFResult<&mut Egg> {
        self.eggs.get_mut(&egg_id).ok_or(FFError::build(
            Severity::Warning,
            format!("Egg with ID {} doesn't exist", egg_id),
        ))
    }
}

// =============================================================================
// Polymorphic dispatch helpers
// =============================================================================

impl ShardServerState {
    pub fn get_entity(&self, id: EntityID) -> Option<&dyn Entity> {
        match id {
            EntityID::Player(pc_id) => self.players.get(&pc_id).map(|p| p as &dyn Entity),
            EntityID::NPC(npc_id) => self.npcs.get(&npc_id).map(|n| n as &dyn Entity),
            EntityID::Slider(sid) => self.sliders.get(&sid).map(|s| s as &dyn Entity),
            EntityID::Egg(eid) => self.eggs.get(&eid).map(|e| e as &dyn Entity),
        }
    }

    pub fn get_combatant(&self, id: EntityID) -> FFResult<&dyn Combatant> {
        let entity = self.get_entity(id).ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} doesn't exist", id),
        ))?;
        entity.as_combatant().ok_or(FFError::build(
            Severity::Warning,
            format!("Entity with ID {:?} isn't a combatant", id),
        ))
    }

    pub fn get_combatant_mut(&mut self, id: EntityID) -> FFResult<&mut dyn Combatant> {
        let err_not_found = || {
            FFError::build(
                Severity::Warning,
                format!("Entity with ID {:?} doesn't exist", id),
            )
        };
        let err_not_combatant = || {
            FFError::build(
                Severity::Warning,
                format!("Entity with ID {:?} isn't a combatant", id),
            )
        };
        match id {
            EntityID::Player(pc_id) => {
                let p = self.players.get_mut(&pc_id).ok_or_else(err_not_found)?;
                p.as_combatant_mut().ok_or_else(err_not_combatant)
            }
            EntityID::NPC(npc_id) => {
                let n = self.npcs.get_mut(&npc_id).ok_or_else(err_not_found)?;
                n.as_combatant_mut().ok_or_else(err_not_combatant)
            }
            _ => Err(err_not_combatant()),
        }
    }

    pub fn get_client_for(&self, id: EntityID) -> Option<FFClient> {
        self.get_entity(id).and_then(|e| e.get_client())
    }

    pub fn get_position_for(&self, id: EntityID) -> Option<Position> {
        self.get_entity(id).map(|e| e.get_position())
    }

    pub fn for_each_around(&self, id: EntityID, mut f: impl FnMut(&FFClient)) {
        for eid in self.entity_map.get_around_entity(id) {
            if let Some(client) = self.get_client_for(eid) {
                f(&client);
            }
        }
    }

    pub fn for_each_around_chunk(&self, coords: ChunkCoords, mut f: impl FnMut(&FFClient)) {
        for eid in self.entity_map.get_around_chunk(coords) {
            if let Some(client) = self.get_client_for(eid) {
                f(&client);
            }
        }
    }

    pub fn validate_proximity(&self, ids: &[EntityID], range: u32) -> FFResult<()> {
        let mut locations = Vec::with_capacity(ids.len());
        for id in ids {
            let entity = self.get_entity(*id).ok_or(FFError::build(
                Severity::Warning,
                format!("Entity with ID {:?} doesn't exist", id),
            ))?;
            locations.push((entity.get_position(), entity.get_chunk_coords().i));
        }

        for i in 0..locations.len() {
            for j in (i + 1)..locations.len() {
                let inst1 = locations[i].1;
                let inst2 = locations[j].1;
                if inst1 != inst2 {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Entity with ID {:?} is in a different instance than entity with ID {:?} ({} != {})",
                            ids[i], ids[j], inst1, inst2
                        ),
                    ));
                }

                let pos1 = locations[i].0;
                let pos2 = locations[j].0;
                let distance = pos1.distance_to(&pos2);
                if distance > range {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Entity with ID {:?} is too far from entity with ID {:?} ({} > {})",
                            ids[i], ids[j], distance, range
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn find_players(&self, f: impl Fn(&Player) -> bool) -> Vec<i32> {
        self.players
            .iter()
            .filter_map(|(pc_id, player)| if f(player) { Some(*pc_id) } else { None })
            .collect()
    }

    pub fn find_npcs(&self, f: impl Fn(&NPC) -> bool) -> Vec<i32> {
        self.npcs
            .iter()
            .filter_map(|(npc_id, npc)| if f(npc) { Some(*npc_id) } else { None })
            .collect()
    }
}

// =============================================================================
// Entity chunk updates with broadcast
// =============================================================================

impl ShardServerState {
    /// Update an entity's chunk position and broadcast enter/exit packets to
    /// entities that gained or lost visibility.
    pub fn update_entity_chunk(&mut self, id: EntityID, to_chunk: Option<ChunkCoords>) {
        let Some((removed, added)) = self.entity_map.update(id, to_chunk) else {
            return;
        };

        // Send exit packets for entities we can no longer see
        for e in &removed {
            // us to them
            if let Some(our_client) = self.get_client_for(id) {
                if let Some(them) = self.get_entity(*e) {
                    them.send_exit(&our_client);
                }
            }
            // them to us
            if let Some(their_client) = self.get_client_for(*e) {
                if let Some(us) = self.get_entity(id) {
                    us.send_exit(&their_client);
                }
            }
        }

        // Send enter packets for entities we can now see
        for e in &added {
            // us to them
            if let Some(our_client) = self.get_client_for(id) {
                if let Some(them) = self.get_entity(*e) {
                    them.send_enter(&our_client);
                }
            }
            // them to us
            if let Some(their_client) = self.get_client_for(*e) {
                if let Some(us) = self.get_entity(id) {
                    us.send_enter(&their_client);
                }
            }
        }
    }

    /// Update chunk without broadcast (e.g. during initialization).
    pub fn update_entity_chunk_silent(&mut self, id: EntityID, to_chunk: Option<ChunkCoords>) {
        self.entity_map.update(id, to_chunk);
    }
}

// =============================================================================
// NPC instance cloning
// =============================================================================

impl ShardServerState {
    /// Clone NPCs from the main instance into a new instance. Called lazily
    /// when a new instance is first entered.
    pub fn clone_npcs_to_instance(&mut self, instance_id: InstanceID) {
        let main_instance = InstanceID {
            channel_num: instance_id.channel_num,
            map_num: instance_id.map_num,
            instance_num: None,
        };

        // Collect template NPC IDs and their tick modes from the main instance
        let template_ids: Vec<(EntityID, TickMode)> = self
            .entity_map
            .get_instance_ids(main_instance)
            .into_iter()
            .filter(|id| matches!(id, EntityID::NPC(_)))
            .map(|id| {
                (
                    id,
                    self.entity_map.get_tick_mode(id).unwrap_or(TickMode::Never),
                )
            })
            .collect();

        let mut id_mappings = HashMap::new();
        let mut tight_follow_mappings = HashMap::new();
        let mut npc_count = 0;

        for (template_id, tick_mode) in template_ids {
            if let EntityID::NPC(old_npc_id) = template_id {
                let mut npc = self.npcs.get(&old_npc_id).unwrap().clone();
                npc.instance_id = instance_id;
                let new_id = self.entity_map.gen_next_npc_id();
                id_mappings.insert(template_id, EntityID::NPC(new_id));
                npc.id = new_id;

                if let Some(tight_follow) = npc.tight_follow {
                    tight_follow_mappings.insert(new_id, tight_follow);
                }

                let chunk_pos = npc.get_chunk_coords();
                let eid = EntityID::NPC(new_id);
                self.npcs.insert(new_id, npc);
                self.entity_map.track(eid, tick_mode);
                self.entity_map.update(eid, Some(chunk_pos));
                npc_count += 1;
            }
        }

        // Update follow leaders to point at the cloned IDs
        for (new_npc_id, (old_leader_id, offset)) in tight_follow_mappings {
            let new_leader_id = id_mappings[&old_leader_id];
            let npc = self.npcs.get_mut(&new_npc_id).unwrap();
            npc.tight_follow = Some((new_leader_id, offset));
        }

        log(
            Severity::Debug,
            &format!("Copied {} NPCs to instance {}", npc_count, instance_id),
        );
    }
}

// =============================================================================
// Tick logic
// =============================================================================

impl ShardServerState {
    pub fn check_for_expired_vehicles(&mut self, time: SystemTime) {
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
                        let client = player.get_client().unwrap();
                        if player.vehicle_speed.is_some() {
                            player.vehicle_speed = None;
                            let pkt = sP_FE2CL_PC_VEHICLE_OFF_SUCC { UNUSED: unused!() };
                            client.send_packet(P_FE2CL_PC_VEHICLE_OFF_SUCC, &pkt);
                            pc_ids_dismounted.push(pc_id);
                        }

                        // delete
                        if let Some(pkt) = log_if_failed(
                            PacketBuilder::new(P_FE2CL_PC_DELETE_TIME_LIMIT_ITEM)
                                .with(&sP_FE2CL_PC_DELETE_TIME_LIMIT_ITEM { iItemListCount: 1 })
                                .with(&sTimeLimitItemDeleteInfo2CL {
                                    eIL: location as i32,
                                    iSlotNum: slot_num as i32,
                                })
                                .build(),
                        ) {
                            client.send_payload(pkt);
                        }
                    }
                }
            }
        }

        for pc_id in pc_ids_dismounted {
            let player = self.get_player(pc_id).unwrap();
            helpers::broadcast_state(pc_id, player.get_state_bit_flag(), self);
        }
    }

    pub fn tick_garbage_collection(&mut self) {
        let mut removed_ids = self.entity_map.garbage_collect_instances();
        removed_ids.extend(self.entity_map.garbage_collect_entities());

        for id in &removed_ids {
            match id {
                EntityID::Player(pc_id) => {
                    if let Some(mut player) = self.players.remove(pc_id) {
                        player.cleanup(self);
                    }
                }
                EntityID::NPC(npc_id) => {
                    if let Some(mut npc) = self.npcs.remove(npc_id) {
                        npc.cleanup(self);
                    }
                }
                EntityID::Slider(sid) => {
                    if let Some(mut slider) = self.sliders.remove(sid) {
                        slider.cleanup(self);
                    }
                }
                EntityID::Egg(eid) => {
                    if let Some(mut egg) = self.eggs.remove(eid) {
                        egg.cleanup(self);
                    }
                }
            }
        }

        if !removed_ids.is_empty() {
            log(
                Severity::Debug,
                &format!("Garbage collected {} entities", removed_ids.len()),
            );
        }
    }

    pub fn tick_groups(&mut self) {
        for group in self.groups.values() {
            let (pc_group_data, npc_group_data) = group.get_member_data(self);
            let mut pkt = PacketBuilder::new(P_FE2CL_PC_GROUP_JOIN_SUCC).with(
                &sP_FE2CL_PC_GROUP_MEMBER_INFO {
                    iID: unused!(),
                    iMemberPCCnt: pc_group_data.len() as i32,
                    iMemberNPCCnt: npc_group_data.len() as i32,
                },
            );

            for pc_data in &pc_group_data {
                pkt.push(pc_data);
            }
            for npc_data in &npc_group_data {
                pkt.push(npc_data);
            }

            if let Some(pkt) = log_if_failed(pkt.build()) {
                for eid in group.get_member_ids() {
                    if let Some(client) = self.get_client_for(*eid) {
                        client.send_payload(pkt.clone());
                    }
                }
            }
        }
    }

    pub fn tick_entities(&mut self, time: SystemTime) {
        let eids: Vec<EntityID> = self.entity_map.get_tickable_ids().collect();
        for eid in eids {
            match eid {
                EntityID::Player(pc_id) => {
                    if let Some(mut player) = self.players.remove(&pc_id) {
                        player.tick(&time, self);
                        self.players.insert(pc_id, player);
                    }
                }
                EntityID::NPC(npc_id) => {
                    if let Some(mut npc) = self.npcs.remove(&npc_id) {
                        npc.tick(&time, self);
                        self.npcs.insert(npc_id, npc);
                    }
                }
                EntityID::Slider(sid) => {
                    if let Some(mut slider) = self.sliders.remove(&sid) {
                        slider.tick(&time, self);
                        self.sliders.insert(sid, slider);
                    }
                }
                EntityID::Egg(eid) => {
                    if let Some(mut egg) = self.eggs.remove(&eid) {
                        egg.tick(&time, self);
                        self.eggs.insert(eid, egg);
                    }
                }
            }
        }

        // Process all buff effects that were generated during this tick
        let buff_effects = std::mem::take(&mut self.pending_buff_effects);
        for buff_effect in buff_effects {
            match buff_effect {
                BuffEffect::HealEntity { target, amount } => {
                    if let Some(combatant) = log_if_failed(self.get_combatant_mut(target)) {
                        combatant.heal(amount);
                    }
                }
                BuffEffect::DamageEntity {
                    target,
                    source,
                    damage,
                } => {
                    if let Some(combatant) = log_if_failed(self.get_combatant_mut(target)) {
                        combatant.take_damage(damage, source);
                    }
                }
            }
        }
    }
}
