use std::{any::Any, time::SystemTime};

use crate::{
    chunk::{ChunkCoords, InstanceID},
    entity::{Entity, EntityID},
    error::FFResult,
    net::{
        packet::{sP_FE2CL_SHINY_ENTER, sP_FE2CL_SHINY_EXIT, sShinyAppearanceData, PacketID::*},
        ClientMap, FFClient,
    },
    state::ShardServerState,
    Position,
};

#[derive(Debug, Clone)]
pub struct Egg {
    id: i32,
    ty: i32,
    position: Position,
    instance_id: InstanceID,
    respawn_time: Option<SystemTime>,
    summoned: bool,
}
impl Egg {
    pub fn new(
        id: i32,
        ty: i32,
        position: Position,
        instance_id: InstanceID,
        summoned: bool,
    ) -> Self {
        Self {
            id,
            ty,
            position,
            instance_id,
            respawn_time: None,
            summoned,
        }
    }

    pub fn is_live(&self) -> bool {
        self.respawn_time.is_none()
    }

    pub fn is_summoned(&self) -> bool {
        self.summoned
    }
}
impl Entity for Egg {
    fn get_id(&self) -> EntityID {
        EntityID::Egg(self.id)
    }

    fn get_client<'a>(&self, _: &'a mut ClientMap) -> Option<&'a mut FFClient> {
        None
    }

    fn get_position(&self) -> Position {
        self.position
    }

    fn get_rotation(&self) -> i32 {
        0
    }

    fn get_speed(&self) -> i32 {
        0
    }

    fn get_chunk_coords(&self) -> ChunkCoords {
        ChunkCoords::from_pos_inst(self.position, self.instance_id)
    }

    fn set_position(&mut self, pos: Position) {
        self.position = pos;
    }

    fn set_rotation(&mut self, _: i32) {}

    fn send_enter(&self, client: &mut FFClient) -> FFResult<()> {
        let pkt = sP_FE2CL_SHINY_ENTER {
            ShinyAppearanceData: sShinyAppearanceData {
                iShiny_ID: self.id,
                iShinyType: self.ty,
                iMapNum: self.instance_id.map_num as i32,
                iX: self.position.x,
                iY: self.position.y,
                iZ: self.position.z,
            },
        };
        client.send_packet(P_FE2CL_SHINY_ENTER, &pkt)
    }

    fn send_exit(&self, client: &mut FFClient) -> FFResult<()> {
        let pkt = sP_FE2CL_SHINY_EXIT { iShinyID: self.id };
        client.send_packet(P_FE2CL_SHINY_EXIT, &pkt)
    }

    fn tick(&mut self, time: SystemTime, clients: &mut ClientMap, state: &mut ShardServerState) {
        if let Some(respawn_time) = self.respawn_time {
            if time >= respawn_time {
                self.respawn_time = None;
                state.entity_map.update(
                    self.get_id(),
                    Some(self.get_chunk_coords()),
                    Some(clients),
                );
            }
        }
    }

    fn cleanup(&mut self, _: &mut ClientMap, _: &mut ShardServerState) {}

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
