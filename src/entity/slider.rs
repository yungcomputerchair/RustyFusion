use std::{any::Any, fmt::Display};

use crate::{
    chunk::{ChunkCoords, InstanceID},
    defines::TYPE_SLIDER,
    entity::{Entity, EntityID},
    enums::TransportationType,
    net::{
        packet::{
            sP_FE2CL_TRANSPORTATION_ENTER, sP_FE2CL_TRANSPORTATION_EXIT,
            sP_FE2CL_TRANSPORTATION_MOVE, sTransportationAppearanceData, PacketID::*,
        },
        FFClient,
    },
    path::Path,
    state::ShardServerState,
    Position,
};

use super::Combatant;

#[derive(Debug, Clone)]
pub struct Slider {
    id: i32,
    position: Position,
    rotation: i32,
    instance_id: InstanceID,
    path: Option<Path>, // optional since we may have scripted sliders in the future
}
impl Slider {
    pub fn new(
        id: i32,
        position: Position,
        angle: i32,
        mut path: Option<Path>,
        instance_id: InstanceID,
    ) -> Self {
        if let Some(ref mut p) = path {
            p.start();
        }
        Self {
            id,
            position,
            rotation: angle % 360,
            instance_id,
            path,
        }
    }

    pub fn get_appearance_data(&self) -> sTransportationAppearanceData {
        sTransportationAppearanceData {
            eTT: TransportationType::Bus as i32,
            iT_ID: self.id,
            iT_Type: TYPE_SLIDER,
            iX: self.position.x,
            iY: self.position.y,
            iZ: self.position.z,
        }
    }
}
impl Display for Slider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Slider({})", self.id)
    }
}
impl Entity for Slider {
    fn get_id(&self) -> EntityID {
        EntityID::Slider(self.id)
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

    fn get_speed(&self, _running: bool) -> i32 {
        if let Some(path) = &self.path {
            path.get_speed()
        } else {
            0
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
        let pkt = sP_FE2CL_TRANSPORTATION_ENTER {
            AppearanceData: self.get_appearance_data(),
        };
        client.send_packet(P_FE2CL_TRANSPORTATION_ENTER, &pkt);
    }

    fn send_exit(&self, client: &FFClient) {
        let pkt = sP_FE2CL_TRANSPORTATION_EXIT {
            eTT: TransportationType::Bus as i32,
            iT_ID: self.id,
        };
        client.send_packet(P_FE2CL_TRANSPORTATION_EXIT, &pkt);
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
impl Slider {
    pub fn tick(state: &mut ShardServerState, slider_id: i32) {
        let slider = state.get_slider_mut(slider_id).unwrap();
        if let Some(path) = slider.path.as_mut() {
            let speed = path.get_speed();
            path.tick(&mut slider.position);
            let chunk_pos = slider.get_chunk_coords();
            state
                .entity_map
                .update(EntityID::Slider(slider_id), Some(chunk_pos), true);

            let slider = state.get_slider(slider_id).unwrap(); // re-borrow
            let pkt = sP_FE2CL_TRANSPORTATION_MOVE {
                eTT: TransportationType::Bus as i32,
                iT_ID: slider.id,
                iToX: slider.position.x,
                iToY: slider.position.y,
                iToZ: slider.position.z,
                iSpeed: speed,
                iMoveStyle: unused!(),
            };

            state
                .entity_map
                .for_each_around(EntityID::Slider(slider_id), |c| {
                    c.send_packet(P_FE2CL_TRANSPORTATION_MOVE, &pkt)
                });
        }
    }
}
