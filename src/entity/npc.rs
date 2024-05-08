use std::{collections::HashSet, time::SystemTime};

use uuid::Uuid;

use crate::{
    ai::AI,
    chunk::{ChunkCoords, InstanceID},
    defines::RANGE_INTERACT,
    entity::{Combatant, Entity, EntityID},
    enums::{CharType, CombatStyle, CombatantTeam},
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
    util::{self, clamp_min},
    Position,
};

#[derive(Debug, Clone)]
pub struct NPC {
    pub id: i32,
    pub ty: i32,
    position: Position,
    rotation: i32,
    hp: i32,
    pub target_id: Option<EntityID>,
    pub invulnerable: bool,
    pub retreating: bool,
    pub instance_id: InstanceID,
    pub tight_follow: Option<(EntityID, Position)>,
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
            target_id: None,
            invulnerable: false,
            retreating: false,
            instance_id,
            tight_follow: None,
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
        let old_pos = self.position;
        if path.tick(&mut self.position) {
            let new_angle = old_pos.angle_to(&self.position) as i32;
            self.set_rotation(util::angle_to_rotation(new_angle));
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
                iMoveStyle: if speed >= run_speed { 1 } else { 0 },
            };
            state
                .entity_map
                .for_each_around(self.get_id(), clients, |c| {
                    c.send_packet(PacketID::P_FE2CL_NPC_MOVE, &pkt)
                });
        }
    }

    fn can_fight(&self) -> bool {
        // to reduce calculations in downstream code,
        // we don't consider NPCs with certain AI types combatants.
        // we check the stats instead of self.ai since the
        // AI object is taken out during tick.

        if self.path.is_some() {
            // exception: some NPCs have AI type 0, but are still combatants
            return true;
        }

        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        stats.ai_type != 0 // no npcs without AI
        && stats.ai_type != 11 // no cars or animals
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

    fn set_rotation(&mut self, rotation: i32) {
        self.rotation = rotation.rem_euclid(360);
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
            if let Some(mut ai) = self.ai.take() {
                ai.tick(self, state, clients, &time);
                self.ai = Some(ai);
            }
        }
    }

    fn cleanup(&mut self, clients: &mut ClientMap, state: &mut ShardServerState) {
        // cleanup group
        if let Some(group_id) = self.group_id {
            crate::helpers::remove_group_member(self.get_id(), group_id, state, clients).unwrap();
        }
    }

    fn as_combatant(&self) -> Option<&dyn Combatant> {
        if !self.can_fight() {
            return None;
        }
        Some(self)
    }

    fn as_combatant_mut(&mut self) -> Option<&mut dyn Combatant> {
        if !self.can_fight() {
            return None;
        }
        Some(self)
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

    fn get_style(&self) -> Option<CombatStyle> {
        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        Some(stats.style)
    }

    fn get_team(&self) -> CombatantTeam {
        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        stats.team
    }

    fn get_char_type(&self) -> CharType {
        match self.get_team() {
            CombatantTeam::Friendly => CharType::NPC,
            CombatantTeam::Mob => CharType::Mob,
            _ => CharType::Unknown,
        }
    }

    fn get_aggro_factor(&self) -> f32 {
        if self.invulnerable {
            0.0
        } else {
            1.0
        }
    }

    fn is_dead(&self) -> bool {
        self.get_hp() <= 0
    }

    fn get_single_power(&self) -> i32 {
        const NPC_BASE_POWER: i32 = 450;
        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        NPC_BASE_POWER + stats.power
    }

    fn get_multi_power(&self) -> i32 {
        self.get_single_power()
    }

    fn get_defense(&self) -> i32 {
        // OF's damage formula makes friendly NPCs wayyy too weak,
        // so we buff them here
        const FRIENDLY_DEFENSE_MULTIPLIER: f32 = 2.5;
        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        match stats.team {
            CombatantTeam::Friendly => (stats.defense as f32 * FRIENDLY_DEFENSE_MULTIPLIER) as i32,
            _ => stats.defense,
        }
    }

    fn take_damage(&mut self, damage: i32, source: EntityID) -> i32 {
        if self.invulnerable || self.retreating {
            return 0;
        }

        if self.target_id.is_none() {
            self.target_id = Some(source);
        }

        let init_hp = self.hp;
        self.hp = clamp_min(self.hp - damage, 0);
        init_hp - self.hp
    }

    fn reset(&mut self) {
        self.target_id = None;
        self.retreating = false;
        self.hp = self.get_max_hp();
    }
}
