use std::{any::Any, time::SystemTime};

use crate::{
    chunk::ChunkCoords,
    error::FFResult,
    net::{ClientMap, FFClient},
    state::ShardServerState,
    Position,
};

mod egg;
pub use egg::*;

mod npc;
pub use npc::*;

mod player;
pub use player::*;

mod slider;
pub use slider::*;

#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
pub enum EntityID {
    Player(i32),
    NPC(i32),
    Slider(i32),
    Egg(i32),
}

pub trait Entity {
    fn get_id(&self) -> EntityID;
    fn get_client<'a>(&self, client_map: &'a mut ClientMap) -> Option<&'a mut FFClient>;
    fn get_position(&self) -> Position;
    fn get_rotation(&self) -> i32;
    fn get_speed(&self) -> i32;
    fn get_chunk_coords(&self) -> ChunkCoords;
    fn set_position(&mut self, pos: Position);
    fn set_rotation(&mut self, angle: i32);
    fn send_enter(&self, client: &mut FFClient) -> FFResult<()>;
    fn send_exit(&self, client: &mut FFClient) -> FFResult<()>;

    fn tick(&mut self, time: SystemTime, clients: &mut ClientMap, state: &mut ShardServerState);
    fn cleanup(&mut self, clients: &mut ClientMap, state: &mut ShardServerState);

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub trait Combatant {
    fn get_condition_bit_flag(&self) -> i32;
    fn get_level(&self) -> i16;
    fn get_hp(&self) -> i32;
    fn get_max_hp(&self) -> i32;
    fn is_dead(&self) -> bool;
}
