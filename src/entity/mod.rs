use std::{any::Any, cmp::Ordering, time::SystemTime};

use crate::{
    chunk::ChunkCoords,
    defines::*,
    error::{FFError, FFResult, Severity},
    net::{packet::{sNPCGroupMemberInfo, sPCGroupMemberInfo}, ClientMap, FFClient},
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

#[derive(Debug, Clone)]
pub struct Group {
    members: Vec<EntityID>,
}
impl Group {
    pub fn new(creator_id: EntityID) -> Self {
        let mut members = Vec::with_capacity(GROUP_MAX_PLAYER_COUNT + GROUP_MAX_NPC_COUNT);
        members.push(creator_id);
        Self { members }
    }

    pub fn add_member(&mut self, id: EntityID) -> FFResult<()> {
        if self.members.contains(&id) {
            return Err(FFError::build(
                Severity::Warning,
                format!("{:?} is already in the group", id),
            ));
        }

        match id {
            EntityID::Player(_) => {
                if self.get_num_players() >= GROUP_MAX_PLAYER_COUNT {
                    return Err(FFError::build(
                        Severity::Warning,
                        "Group is full of players".to_string(),
                    ));
                }
            }
            EntityID::NPC(_) => {
                if self.get_num_npcs() >= GROUP_MAX_NPC_COUNT {
                    return Err(FFError::build(
                        Severity::Warning,
                        "Group is full of NPCs".to_string(),
                    ));
                }
            }
            other => {
                return Err(FFError::build(
                    Severity::Warning,
                    format!("{:?} cannot join group; must be a player or NPC", other),
                ));
            }
        }

        self.members.push(id);
        self.members.sort_by(|a, b| {
            if matches!(a, EntityID::Player(_)) && !matches!(b, EntityID::Player(_)) {
                Ordering::Less
            } else if !matches!(a, EntityID::Player(_)) && matches!(b, EntityID::Player(_)) {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        });
        Ok(())
    }

    pub fn remove_member(&mut self, id: EntityID) -> FFResult<()> {
        if let Some(index) = self.members.iter().position(|&x| x == id) {
            self.members.remove(index);
            Ok(())
        } else {
            Err(FFError::build(
                Severity::Warning,
                format!("{:?} is not in the group", id),
            ))
        }
    }

    pub fn get_member_ids(&self) -> &Vec<EntityID> {
        &self.members
    }

    fn get_num_players(&self) -> usize {
        self.members
            .iter()
            .filter(|&id| matches!(id, EntityID::Player(_)))
            .count()
    }

    fn get_num_npcs(&self) -> usize {
        self.members
            .iter()
            .filter(|&id| matches!(id, EntityID::NPC(_)))
            .count()
    }

    pub fn should_disband(&self) -> bool {
        self.members.len() <= 1 || matches!(self.members[0], EntityID::NPC(_))
    }

    pub fn get_member_data(&self, state: &ShardServerState) -> (Vec<sPCGroupMemberInfo>, Vec<sNPCGroupMemberInfo>) {
        let mut pc_group_data = Vec::with_capacity(GROUP_MAX_PLAYER_COUNT);
        let mut npc_group_data = Vec::with_capacity(GROUP_MAX_NPC_COUNT);
        for eid in &self.members {
            match eid {
                EntityID::Player(pc_id) => {
                    let player = state.get_player(*pc_id).unwrap();
                    pc_group_data.push(player.get_group_member_info());
                }
                EntityID::NPC(npc_id) => {
                    let npc = state.get_npc(*npc_id).unwrap();
                    npc_group_data.push(npc.get_group_member_info());
                }
                _ => unreachable!(),
            }
        }
        (pc_group_data, npc_group_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group() {
        // test setup
        let player1 = EntityID::Player(1);
        let mut group = Group::new(player1);
        assert_eq!(group.get_member_ids(), &vec![player1]);
        assert!(group.should_disband());

        // adding same type
        let player2 = EntityID::Player(2);
        group.add_member(player2).unwrap();
        assert_eq!(group.get_member_ids(), &vec![player1, player2]);
        assert!(!group.should_disband());

        // removing leader
        group.remove_member(player1).unwrap();
        assert_eq!(group.get_member_ids(), &vec![player2]);
        assert!(group.should_disband());

        // adding new type
        let npc1 = EntityID::NPC(1);
        group.add_member(npc1).unwrap();
        assert_eq!(group.get_member_ids(), &vec![player2, npc1]);
        assert!(!group.should_disband());

        // removing last player
        group.remove_member(player2).unwrap();
        assert_eq!(group.get_member_ids(), &vec![npc1]);
        assert!(group.should_disband());

        // adding second NPC (past limit)
        let npc2 = EntityID::NPC(2);
        assert!(group.add_member(npc2).is_err());

        // adding only player
        let player3 = EntityID::Player(3);
        group.add_member(player3).unwrap();
        assert_eq!(group.get_member_ids(), &vec![player3, npc1]);
        assert!(!group.should_disband());

        // adding existing
        assert!(group.add_member(player3).is_err());
        assert!(group.add_member(npc1).is_err());

        // adding player in mixed group
        group.add_member(player1).unwrap();
        assert_eq!(group.get_member_ids(), &vec![player3, player1, npc1]);

        // removing non-member
        let player4 = EntityID::Player(4);
        assert!(group.remove_member(player4).is_err());

        // adding up to full group
        group.add_member(player4).unwrap();
        let player5 = EntityID::Player(5);
        group.add_member(player5).unwrap();
        assert_eq!(
            group.get_member_ids(),
            &vec![player3, player1, player4, player5, npc1]
        );

        // adding over full group
        let player6 = EntityID::Player(6);
        assert!(group.add_member(player6).is_err());

        // adding fifth player (past limit) to non-full group
        group.remove_member(npc1).unwrap();
        assert!(group.add_member(player6).is_err());
    }
}
