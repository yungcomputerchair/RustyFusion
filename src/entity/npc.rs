use std::{collections::HashSet, time::SystemTime};

use uuid::Uuid;

use crate::{
    ai::{Behavior, AI},
    chunk::{ChunkCoords, InstanceID},
    defines::RANGE_INTERACT,
    entity::{Combatant, Entity, EntityID},
    error::FFResult,
    net::{
        packet::{
            sNPCAppearanceData, sNPCGroupMemberInfo, sP_FE2CL_NPC_ENTER, sP_FE2CL_NPC_EXIT,
            sP_FE2CL_NPC_MOVE, PacketID,
        },
        ClientMap, FFClient,
    },
    path::Path,
    state::ShardServerState,
    tabledata::tdata_get,
    Position,
};

#[derive(Debug, Clone)]
pub struct NPC {
    pub id: i32,
    pub ty: i32,
    position: Position,
    rotation: i32,
    hp: i32,
    pub instance_id: InstanceID,
    pub follower_ids: HashSet<i32>,
    pub leader_id: Option<i32>,
    pub path: Option<Path>,
    pub group_id: Option<Uuid>,
    pub loose_follow: Option<EntityID>,
    pub interacting_pcs: HashSet<i32>,
    pub summoned: bool,
    pub ai: Option<AI>,
}
impl NPC {
    pub fn new(
        id: i32,
        ty: i32,
        position: Position,
        angle: i32,
        instance_id: InstanceID,
    ) -> FFResult<Self> {
        let stats = tdata_get().get_npc_stats(ty)?;
        Ok(Self {
            id,
            ty,
            position,
            rotation: angle % 360,
            hp: stats.max_hp as i32,
            instance_id,
            follower_ids: HashSet::new(),
            leader_id: None,
            path: None,
            group_id: None,
            loose_follow: None,
            interacting_pcs: HashSet::new(),
            summoned: false,
            ai: None,
        })
    }

    pub fn set_path(&mut self, path: Path) {
        self.path = Some(path);
    }

    pub fn set_follow(&mut self, entity_id: EntityID) {
        self.loose_follow = Some(entity_id);
    }

    pub fn get_group_member_info(&self) -> sNPCGroupMemberInfo {
        sNPCGroupMemberInfo {
            iNPC_ID: self.id,
            iNPC_Type: self.ty,
            iHP: self.hp,
            iMapType: unused!(),
            iMapNum: self.instance_id.map_num as i32,
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
        }
    }

    fn get_appearance_data(&self) -> sNPCAppearanceData {
        sNPCAppearanceData {
            iNPC_ID: self.id,
            iNPCType: self.ty,
            iHP: self.get_hp(),
            iConditionBitFlag: self.get_condition_bit_flag(),
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
            iAngle: self.rotation,
            iBarkerType: unused!(),
        }
    }

    pub fn tick_movement_along_path(
        &mut self,
        path: &mut Path,
        clients: &mut ClientMap,
        state: &mut ShardServerState,
    ) {
        let speed = path.get_speed();
        if path.tick(&mut self.position) {
            let chunk_pos = self.get_chunk_coords();
            state
                .entity_map
                .update(self.get_id(), Some(chunk_pos), Some(clients));

            let run_speed = tdata_get().get_npc_stats(self.ty).unwrap().run_speed;
            let pkt = sP_FE2CL_NPC_MOVE {
                iNPC_ID: self.id,
                iToX: self.position.x,
                iToY: self.position.y,
                iToZ: self.position.z,
                iSpeed: speed,
                iMoveStyle: if speed > run_speed { 1 } else { 0 },
            };
            state
                .entity_map
                .for_each_around(self.get_id(), clients, |c| {
                    c.send_packet(PacketID::P_FE2CL_NPC_MOVE, &pkt)
                });
        }
    }

    fn tick_movement(&mut self, clients: &mut ClientMap, state: &mut ShardServerState) {
        const FOLLOWING_DISTANCE: i32 = 200;

        let mut follow_path = if let Some(entity_id) = self.loose_follow {
            if let Some(entity) = state.entity_map.get_from_id(entity_id) {
                let target_pos = entity.get_position();
                let (target_pos, too_close) =
                    target_pos.interpolate(&self.position, FOLLOWING_DISTANCE as f32);
                // exceed target speed by 10% to not fall behind
                let target_speed = entity.get_speed() as f32 * 1.1;
                let mut path = Path::new_single(target_pos, target_speed as i32);
                if !too_close {
                    path.start();
                }
                Some(path)
            } else {
                // target entity is gone
                self.loose_follow = None;
                None
            }
        } else {
            None
        };

        // If we are following an entity, that takes priority over our own path
        let ticked_path = if let Some(path) = &mut follow_path {
            Some(path)
        } else {
            None // TODO
        };

        if let Some(path) = ticked_path {
            self.tick_movement_along_path(path, clients, state);
        }
    }

    pub fn add_base_behavior_group(&mut self, behaviors: Vec<Behavior>) {
        if let Some(ref mut ai) = self.ai {
            ai.add_base_node_with_behaviors(behaviors);
        } else {
            let mut new_ai = AI::default();
            new_ai.add_base_node_with_behaviors(behaviors);
            self.ai = Some(new_ai);
        }
    }

    pub fn add_base_behavior(&mut self, behavior: Behavior) {
        let behaviors = vec![behavior];
        self.add_base_behavior_group(behaviors);
    }
}
impl Entity for NPC {
    fn get_id(&self) -> EntityID {
        EntityID::NPC(self.id)
    }

    fn get_client<'a>(&self, _client_map: &'a mut ClientMap) -> Option<&'a mut FFClient> {
        None
    }

    fn get_position(&self) -> Position {
        self.position
    }

    fn get_rotation(&self) -> i32 {
        self.rotation
    }

    fn get_speed(&self) -> i32 {
        if let Some(path) = &self.path {
            path.get_speed()
        } else {
            let stats = tdata_get().get_npc_stats(self.ty).unwrap();
            stats.walk_speed
        }
    }

    fn get_chunk_coords(&self) -> ChunkCoords {
        ChunkCoords::from_pos_inst(self.position, self.instance_id)
    }

    fn set_position(&mut self, pos: Position) {
        self.position = pos;
    }

    fn set_rotation(&mut self, angle: i32) {
        self.rotation = angle % 360;
    }

    fn send_enter(&self, client: &mut FFClient) -> FFResult<()> {
        let pkt = sP_FE2CL_NPC_ENTER {
            NPCAppearanceData: self.get_appearance_data(),
        };
        client.send_packet(PacketID::P_FE2CL_NPC_ENTER, &pkt)
    }

    fn send_exit(&self, client: &mut FFClient) -> FFResult<()> {
        let pkt = sP_FE2CL_NPC_EXIT { iNPC_ID: self.id };
        client.send_packet(PacketID::P_FE2CL_NPC_EXIT, &pkt)
    }

    fn tick(&mut self, time: SystemTime, clients: &mut ClientMap, state: &mut ShardServerState) {
        let pc_ids: Vec<i32> = self.interacting_pcs.iter().copied().collect();
        for pc_id in pc_ids {
            let pc_eid = EntityID::Player(pc_id);
            if state
                .entity_map
                .validate_proximity(&[self.get_id(), pc_eid], RANGE_INTERACT)
                .is_err()
            {
                self.interacting_pcs.remove(&pc_id);
            }
        }
        if self.interacting_pcs.is_empty() {
            // we take the AI object out during tick to satisfy the borrow checker
            if let Some(ai) = self.ai.take() {
                self.ai = Some(ai.tick(self, state, clients, &time));
            }
            self.tick_movement(clients, state);
        }
    }

    fn cleanup(&mut self, clients: &mut ClientMap, state: &mut ShardServerState) {
        // cleanup group
        if let Some(group_id) = self.group_id {
            crate::helpers::remove_group_member(self.get_id(), group_id, state, clients).unwrap();
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
impl Combatant for NPC {
    fn get_condition_bit_flag(&self) -> i32 {
        placeholder!(0)
    }

    fn get_level(&self) -> i16 {
        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        stats.level
    }

    fn get_hp(&self) -> i32 {
        self.hp
    }

    fn get_max_hp(&self) -> i32 {
        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        stats.max_hp as i32
    }

    fn is_dead(&self) -> bool {
        self.get_hp() <= 0
    }
}
