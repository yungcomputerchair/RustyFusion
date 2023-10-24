use crate::{
    chunk::{EntityMap, MAP_BOUNDS, NCHUNKS},
    net::{
        ffclient::FFClient,
        packet::{sNPCAppearanceData, sP_FE2CL_NPC_ENTER, sP_FE2CL_NPC_EXIT, PacketID},
        ClientMap,
    },
    CombatStats, Combatant, Entity, EntityID, Position, Result,
};

pub struct NPC {
    id: i32,
    npc_type: i32,
    position: Position,
    rotation: i32,
    _instance_id: u64,
    combat_stats: CombatStats,
}
impl NPC {
    fn get_appearance_data(&self) -> sNPCAppearanceData {
        sNPCAppearanceData {
            iNPC_ID: self.id,
            iNPCType: self.npc_type,
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
    fn get_id(&self) -> crate::EntityID {
        EntityID::NPC(self.id)
    }

    fn get_client<'a>(
        &self,
        _client_map: &'a mut crate::net::ClientMap,
    ) -> Option<&'a mut crate::net::ffclient::FFClient> {
        None
    }

    fn set_position(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        entity_map: &mut EntityMap,
        client_map: &mut ClientMap,
    ) {
        self.position = Position { x, y, z };
        let chunk_x = (x * NCHUNKS as i32) / MAP_BOUNDS;
        let chunk_y = (y * NCHUNKS as i32) / MAP_BOUNDS;
        entity_map.update(self.get_id(), Some((chunk_x, chunk_y)), client_map);
    }

    fn set_rotation(&mut self, angle: i32) {
        self.rotation = angle % 360;
    }

    fn send_enter(&self, client: &mut FFClient) -> Result<()> {
        let pkt = sP_FE2CL_NPC_ENTER {
            NPCAppearanceData: self.get_appearance_data(),
        };
        client.send_packet(PacketID::P_FE2CL_NPC_ENTER, &pkt)
    }

    fn send_exit(&self, client: &mut FFClient) -> Result<()> {
        let pkt = sP_FE2CL_NPC_EXIT { iNPC_ID: self.id };
        client.send_packet(PacketID::P_FE2CL_NPC_EXIT, &pkt)
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
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
