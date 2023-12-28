use std::collections::HashSet;

use crate::{
    chunk::{ChunkCoords, InstanceID},
    error::FFResult,
    net::{
        ffclient::FFClient,
        packet::{sNPCAppearanceData, sP_FE2CL_NPC_ENTER, sP_FE2CL_NPC_EXIT, PacketID},
        ClientMap,
    },
    state::shard::ShardServerState,
    CombatStats, Combatant, Entity, EntityID, Position,
};

#[derive(Debug, Clone)]
pub struct NPC {
    pub id: i32,
    pub ty: i32,
    position: Position,
    rotation: i32,
    pub instance_id: InstanceID,
    combat_stats: Option<CombatStats>,
    pub follower_ids: HashSet<i32>,
    pub leader_id: Option<i32>,
}
impl NPC {
    pub fn new(id: i32, ty: i32, position: Position, angle: i32, instance_id: InstanceID) -> Self {
        Self {
            id,
            ty,
            position,
            rotation: angle % 360,
            instance_id,
            combat_stats: None,
            follower_ids: HashSet::new(),
            leader_id: None,
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

    fn cleanup(&mut self, _clients: &mut ClientMap, _state: &mut ShardServerState) {}

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
        self.combat_stats.unwrap_or_default().level
    }

    fn get_hp(&self) -> i32 {
        self.combat_stats.unwrap_or_default().hp
    }

    fn get_max_hp(&self) -> i32 {
        self.combat_stats.unwrap_or_default().max_hp
    }
}
