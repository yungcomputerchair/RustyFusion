use std::{collections::HashSet, time::SystemTime};

use crate::{
    chunk::{ChunkCoords, InstanceID},
    error::FFResult,
    net::{
        ffclient::FFClient,
        packet::{
            sNPCAppearanceData, sP_FE2CL_NPC_ENTER, sP_FE2CL_NPC_EXIT, sP_FE2CL_NPC_MOVE, PacketID,
        },
        ClientMap,
    },
    state::shard::ShardServerState,
    Combatant, Entity, EntityID, Path, Position,
};

#[derive(Debug, Clone)]
pub struct NPC {
    pub id: i32,
    pub ty: i32,
    position: Position,
    rotation: i32,
    pub instance_id: InstanceID,
    pub follower_ids: HashSet<i32>,
    pub leader_id: Option<i32>,
    pub path: Option<Path>,
}
impl NPC {
    pub fn new(id: i32, ty: i32, position: Position, angle: i32, instance_id: InstanceID) -> Self {
        Self {
            id,
            ty,
            position,
            rotation: angle % 360,
            instance_id,
            follower_ids: HashSet::new(),
            leader_id: None,
            path: None,
        }
    }

    pub fn set_path(&mut self, path: Path) {
        self.path = Some(path);
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

    fn get_rotation(&self) -> i32 {
        self.rotation
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

    fn tick(&mut self, _time: SystemTime, clients: &mut ClientMap, state: &mut ShardServerState) {
        const RUN_SPEED: i32 = 400;
        if let Some(path) = self.path.as_mut() {
            let speed = path.get_speed();
            path.tick(&mut self.position);
            let chunk_pos = self.get_chunk_coords();
            state
                .entity_map
                .update(self.get_id(), Some(chunk_pos), Some(clients));

            let pkt = sP_FE2CL_NPC_MOVE {
                iNPC_ID: self.id,
                iToX: self.position.x,
                iToY: self.position.y,
                iToZ: self.position.z,
                iSpeed: speed,
                iMoveStyle: if speed > RUN_SPEED { 1 } else { 0 },
            };
            state
                .entity_map
                .for_each_around(self.get_id(), clients, |c| {
                    c.send_packet(PacketID::P_FE2CL_NPC_MOVE, &pkt)
                });
        }
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
        placeholder!(1)
    }

    fn get_hp(&self) -> i32 {
        placeholder!(400)
    }

    fn get_max_hp(&self) -> i32 {
        placeholder!(400)
    }
}
