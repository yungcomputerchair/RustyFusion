use std::{
    collections::HashSet,
    fmt::{Display, Formatter},
};

use uuid::Uuid;

use crate::{
    chunk::{ChunkCoords, InstanceID},
    defines::RANGE_INTERACT,
    entity::{Combatant, Entity, EntityID},
    enums::{BuffID, BuffType, CharType, CombatStyle, CombatantTeam},
    error::FFResult,
    helpers,
    net::{
        packet::{
            sNPCAppearanceData, sNPCGroupMemberInfo, sP_FE2CL_CHAR_TIME_BUFF_TIME_OUT,
            sP_FE2CL_NPC_ENTER, sP_FE2CL_NPC_EXIT, sP_FE2CL_NPC_MOVE, PacketID::*,
        },
        FFClient,
    },
    path::Path,
    scripting::scripting_get,
    skills::{BuffContainer, BuffInstance},
    state::ShardServerState,
    tabledata::tdata_get,
    util::{self, clamp_min},
    Position,
};

#[derive(Debug, Clone)]
pub struct NPC {
    pub id: i32,
    pub ty: i32,
    pub spawn_position: Position,
    position: Position,
    rotation: i32,
    hp: i32,
    pub team: CombatantTeam,
    pub target_id: Option<EntityID>,
    pub last_attacked_by: Option<EntityID>,
    pub invulnerable: bool,
    pub retreating: bool,
    pub instance_id: InstanceID,
    pub tight_follow: Option<(EntityID, Position)>,
    pub path: Option<Path>,
    pub group_id: Option<Uuid>,
    pub loose_follow: Option<EntityID>,
    pub interacting_pcs: HashSet<i32>,
    pub summoned: bool,
    pub ai: Option<String>,
    buffs: BuffContainer,
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
            spawn_position: position,
            position,
            rotation: angle % 360,
            hp: stats.max_hp as i32,
            team: stats.team,
            target_id: None,
            last_attacked_by: None,
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
            buffs: BuffContainer::default(),
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

    pub fn get_name(&self) -> &'static str {
        tdata_get().get_npc_name(self.ty).unwrap()
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

    pub fn tick_movement_along_path(npc_id: i32, path: &mut Path, state: &mut ShardServerState) {
        let speed = path.get_speed();
        let npc_eid = EntityID::NPC(npc_id);
        let old_pos = state.get_npc(npc_id).unwrap().position;
        let mut new_pos = old_pos;
        if path.tick(&mut new_pos) {
            // update angle, position, and chunks
            let npc = state.get_npc_mut(npc_id).unwrap();
            npc.set_position(new_pos);

            let new_angle = old_pos.angle_to(&new_pos) as i32;
            npc.set_rotation(util::angle_to_rotation(new_angle));

            let chunk_pos = npc.get_chunk_coords();
            state.entity_map.update(npc_eid, Some(chunk_pos), true);

            // broadcast movement
            let npc = state.get_npc(npc_id).unwrap(); // re-borrow
            let run_speed = tdata_get().get_npc_stats(npc.ty).unwrap().run_speed;
            let pkt = sP_FE2CL_NPC_MOVE {
                iNPC_ID: npc.id,
                iToX: npc.position.x,
                iToY: npc.position.y,
                iToZ: npc.position.z,
                iSpeed: speed,
                iMoveStyle: if speed >= run_speed { 1 } else { 0 },
            };

            state
                .entity_map
                .for_each_around(npc_eid, |c| c.send_packet(P_FE2CL_NPC_MOVE, &pkt));
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

        if self.ai.is_some() {
            return true;
        }

        let stats = tdata_get().get_npc_stats(self.ty).unwrap();
        stats.ai_type != 0 // no npcs without AI
        && stats.ai_type != 11 // no cars or animals
    }

    pub fn tick(state: &mut ShardServerState, npc_id: i32) {
        let npc_eid = EntityID::NPC(npc_id);

        // update interacting player list (no-alloc)
        let mut interacting_pc_ids =
            std::mem::take(&mut state.get_npc_mut(npc_id).unwrap().interacting_pcs);
        interacting_pc_ids.retain(|pc_id| {
            let pc_eid = EntityID::Player(*pc_id);
            state
                .entity_map
                .validate_proximity(&[npc_eid, pc_eid], RANGE_INTERACT)
                .is_ok()
        });
        state.get_npc_mut(npc_id).unwrap().interacting_pcs = interacting_pc_ids;

        // tick buffs
        let mut buff_effects = Vec::new();
        let npc = state.get_npc_mut(npc_id).unwrap();
        let buffs_updated = !npc.buffs.tick(npc_eid, &mut buff_effects).is_empty();

        if buffs_updated {
            let bcast = sP_FE2CL_CHAR_TIME_BUFF_TIME_OUT {
                eCT: CharType::NPC as i32,
                iID: npc_id,
                iConditionBitFlag: npc.buffs.get_bit_flags(),
            };

            state.entity_map.for_each_around(npc_eid, |c| {
                c.send_packet(P_FE2CL_CHAR_TIME_BUFF_TIME_OUT, &bcast)
            });
        }

        // tick path
        let npc = state.get_npc_mut(npc_id).unwrap(); // re-borrow
        if let Some(mut path) = npc.path.take() {
            if !npc.is_dead() {
                if !path.is_done() {
                    NPC::tick_movement_along_path(npc_id, &mut path, state);
                }

                if !path.is_done() {
                    state.get_npc_mut(npc_id).unwrap().path = Some(path);
                }
            }
        }

        // tick AI; we don't tick AI while PCs are interacting with the NPC
        let npc = state.get_npc(npc_id).unwrap(); // re-borrow
        if npc.ai.is_some() && npc.interacting_pcs.is_empty() {
            let scripting = scripting_get();
            scripting.lock().tick_npc(npc_id, state);
        }
    }
}
impl Display for NPC {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (id {}, type {})", self.get_name(), self.id, self.ty)
    }
}
impl Entity for NPC {
    fn get_id(&self) -> EntityID {
        EntityID::NPC(self.id)
    }

    fn get_client(&self) -> Option<FFClient> {
        None
    }

    fn get_position(&self) -> Position {
        self.position
    }

    fn get_rotation(&self) -> i32 {
        self.rotation
    }

    fn get_speed(&self, running: bool) -> i32 {
        if let Some(path) = &self.path {
            path.get_speed()
        } else {
            let stats = tdata_get().get_npc_stats(self.ty).unwrap();
            let base_speed = if running {
                stats.run_speed
            } else {
                stats.walk_speed
            };

            let buffed_speed = self.buffs.get_buff_value(BuffID::UpMoveSpeed).unwrap_or(0);
            base_speed + buffed_speed
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

    fn send_enter(&self, client: &FFClient) {
        let pkt = sP_FE2CL_NPC_ENTER {
            NPCAppearanceData: self.get_appearance_data(),
        };
        client.send_packet(P_FE2CL_NPC_ENTER, &pkt);
    }

    fn send_exit(&self, client: &FFClient) {
        let pkt = sP_FE2CL_NPC_EXIT { iNPC_ID: self.id };
        client.send_packet(P_FE2CL_NPC_EXIT, &pkt);
    }

    fn cleanup(self: Box<Self>, state: &mut ShardServerState) {
        // cleanup group
        if let Some(group_id) = self.group_id {
            helpers::remove_group_member(self.get_id(), group_id, state).unwrap();
        }

        // cleanup coroutine
        let scripting = scripting_get();
        scripting.lock().remove_npc(self.id);
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
        self.buffs.get_bit_flags()
    }

    fn get_group_id(&self) -> Option<Uuid> {
        self.group_id
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

    fn get_target(&self) -> Option<EntityID> {
        self.target_id
    }

    fn is_dead(&self) -> bool {
        self.get_hp() <= 0
    }

    fn has_buff(&self, buff_id: BuffID, buff_type: Option<BuffType>) -> bool {
        self.buffs.has_buff(buff_id, buff_type)
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

    fn take_damage(&mut self, damage: i32, source: Option<EntityID>) -> i32 {
        if self.invulnerable || self.retreating {
            return 0;
        }

        if let Some(source) = source {
            self.last_attacked_by = Some(source);
            if self.target_id.is_none() {
                self.target_id = Some(source);
            }
        }

        let init_hp = self.hp;
        self.hp = clamp_min(self.hp - damage, 0);
        init_hp - self.hp
    }

    fn heal(&mut self, amount: i32) -> i32 {
        let init_hp = self.hp;
        self.hp = clamp_min(self.hp + amount, self.get_max_hp());
        self.hp - init_hp
    }

    fn apply_buff(
        &mut self,
        buff_id: BuffID,
        buff: BuffInstance,
        source: Option<EntityID>,
    ) -> bool {
        self.buffs.add_buff(buff_id, buff, source)
    }

    fn remove_buff(&mut self, buff_id: BuffID, buff_type: Option<BuffType>) -> bool {
        self.buffs.remove_buff(buff_id, buff_type)
    }

    fn reset(&mut self) {
        self.last_attacked_by = None;
        self.target_id = None;
        self.retreating = false;
        self.hp = self.get_max_hp();
    }
}
