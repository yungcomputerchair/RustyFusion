use std::{any::Any, fmt::Display, time::SystemTime};

use crate::{
    chunk::{ChunkCoords, InstanceID},
    entity::{Entity, EntityID},
    net::{
        packet::{sP_FE2CL_SHINY_ENTER, sP_FE2CL_SHINY_EXIT, sShinyAppearanceData, PacketID::*},
        FFClient,
    },
    state::ShardServerState,
    Position,
};

use super::Combatant;

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
impl Display for Egg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Egg({})", self.id)
    }
}
impl Entity for Egg {
    fn get_id(&self) -> EntityID {
        EntityID::Egg(self.id)
    }

    fn get_client(&self) -> Option<FFClient> {
        None
    }

    fn get_position(&self) -> Position {
        self.position
    }

    fn get_rotation(&self) -> i32 {
        0
    }

    fn get_speed(&self, _running: bool) -> i32 {
        0
    }

    fn get_chunk_coords(&self) -> ChunkCoords {
        ChunkCoords::from_pos_inst(self.position, self.instance_id)
    }

    fn set_position(&mut self, pos: Position) {
        self.position = pos;
    }

    fn set_rotation(&mut self, _rot: i32) {}

    fn send_enter(&self, client: &FFClient) {
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
        client.send_packet(P_FE2CL_SHINY_ENTER, &pkt);
    }

    fn send_exit(&self, client: &FFClient) {
        let pkt = sP_FE2CL_SHINY_EXIT { iShinyID: self.id };
        client.send_packet(P_FE2CL_SHINY_EXIT, &pkt);
    }

    fn cleanup(self: Box<Self>, _state: &mut ShardServerState) {}

    fn as_combatant(&self) -> Option<&dyn Combatant> {
        None
    }

    fn as_combatant_mut(&mut self) -> Option<&mut dyn Combatant> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
impl Egg {
    pub fn tick(state: &mut ShardServerState, time: &SystemTime, egg_id: i32) {
        let egg = state.get_egg_mut(egg_id).unwrap();
        if let Some(respawn_time) = egg.respawn_time {
            if time >= &respawn_time {
                egg.respawn_time = None;
                let chunk_coords = egg.get_chunk_coords();
                state
                    .entity_map
                    .update(EntityID::Egg(egg_id), Some(chunk_coords), true);
            }
        }
    }
}
