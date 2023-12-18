use crate::{
    error::FFResult,
    net::{
        ffclient::FFClient,
        packet::{sNPCAppearanceData, sP_FE2CL_NPC_ENTER, sP_FE2CL_NPC_EXIT, PacketID},
        ClientMap,
    },
    state::shard::ShardServerState,
    CombatStats, Combatant, Entity, EntityID, Position,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct NPC {
    id: i32,
    pub ty: i32,
    position: Position,
    rotation: i32,
    _instance_id: u64,
    combat_stats: CombatStats,
}
impl NPC {
    pub fn new(id: i32, ty: i32, x: i32, y: i32, z: i32, angle: i32, instance_id: u64) -> Self {
        Self {
            id,
            ty,
            position: Position { x, y, z },
            rotation: angle % 360,
            _instance_id: instance_id,
            combat_stats: CombatStats {
                level: unused!(),
                _max_hp: unused!(),
                hp: 400,
            },
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

    fn set_position(&mut self, pos: Position) -> (i32, i32) {
        self.position = pos;
        self.position.chunk_coords()
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
        self.combat_stats.level
    }

    fn get_hp(&self) -> i32 {
        self.combat_stats.hp
    }
}
