use std::time::{Duration, SystemTime};

use rand::{thread_rng, Rng};

use crate::{
    chunk::TickMode,
    entity::{Entity, NPC},
    enums::NPCTeam,
    net::ClientMap,
    path::Path,
    state::ShardServerState,
    tabledata::tdata_get,
    Position,
};

trait AINode: std::fmt::Debug {
    fn box_clone(&self) -> Box<dyn AINode>;
    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> NodeStatus;
}
impl Clone for Box<dyn AINode> {
    fn clone(&self) -> Box<dyn AINode> {
        self.box_clone()
    }
}

#[derive(Debug, Clone)]
pub struct AI {
    root: Box<dyn AINode>,
}
impl AI {
    pub fn make_for_npc(npc: &NPC) -> (Option<Self>, TickMode) {
        let stats = tdata_get().get_npc_stats(npc.ty).unwrap();
        if stats.ai_type == 0 {
            return (None, TickMode::Never);
        }

        let mut movement_behaviors = vec![
            FollowAssignedPath::new_node(),
            FollowAssignedEntity::new_node(200),
        ];

        if stats.team == NPCTeam::Mob {
            // stats.ai_type == ???
            let roam_radius_max = stats.idle_range / 2;
            let roam_radius_range = (roam_radius_max / 2, roam_radius_max);
            let roam_delay_max_ms = stats.delay_time * 1000;
            let roam_delay_range_ms = (roam_delay_max_ms / 2, roam_delay_max_ms);
            movement_behaviors.push(RandomRoamAround::new_node(
                npc.get_position(),
                roam_radius_range,
                roam_delay_range_ms,
            ));
        }

        let movement_selector = SelectorNode::new_node(movement_behaviors);
        let root = SequenceNode::new_node(vec![movement_selector]);
        let tick_mode = if npc.path.is_some() {
            TickMode::Always
        } else {
            TickMode::WhenLoaded
        };

        (Some(AI { root }), tick_mode)
    }

    pub fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) {
        self.root.tick(npc, state, clients, time);
    }
}

#[allow(dead_code)]
#[derive(Debug)]
enum NodeStatus {
    Success,
    Failure,
    Running,
}

#[derive(Debug, Clone)]
struct SequenceNode {
    children: Vec<Box<dyn AINode>>,
    cursor: usize,
}
impl SequenceNode {
    fn new_node(children: Vec<Box<dyn AINode>>) -> Box<dyn AINode> {
        Box::new(Self {
            children,
            cursor: 0,
        })
    }
}
impl AINode for SequenceNode {
    fn box_clone(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> NodeStatus {
        while self.cursor < self.children.len() {
            let status = self.children[self.cursor].tick(npc, state, clients, time);
            match status {
                NodeStatus::Success => {
                    self.cursor += 1;
                }
                NodeStatus::Failure => {
                    self.cursor = 0;
                    return NodeStatus::Failure;
                }
                NodeStatus::Running => {
                    return NodeStatus::Running;
                }
            }
        }
        self.cursor = 0;
        NodeStatus::Success
    }
}

#[derive(Debug, Clone)]
struct SelectorNode {
    children: Vec<Box<dyn AINode>>,
    cursor: usize,
}
impl SelectorNode {
    fn new_node(children: Vec<Box<dyn AINode>>) -> Box<dyn AINode> {
        Box::new(Self {
            children,
            cursor: 0,
        })
    }
}
impl AINode for SelectorNode {
    fn box_clone(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> NodeStatus {
        while self.cursor < self.children.len() {
            let status = self.children[self.cursor].tick(npc, state, clients, time);
            match status {
                NodeStatus::Success => {
                    self.cursor = 0;
                    return NodeStatus::Success;
                }
                NodeStatus::Failure => {
                    self.cursor += 1;
                }
                NodeStatus::Running => {
                    return NodeStatus::Running;
                }
            }
        }
        self.cursor = 0;
        NodeStatus::Failure
    }
}

#[derive(Debug, Clone)]
struct FollowAssignedPath {}
impl FollowAssignedPath {
    fn new_node() -> Box<dyn AINode> {
        Box::new(Self {})
    }
}
impl AINode for FollowAssignedPath {
    fn box_clone(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        _time: &SystemTime,
    ) -> NodeStatus {
        let path = npc.path.take();
        match path {
            Some(mut path) => {
                npc.tick_movement_along_path(&mut path, clients, state);
                npc.path = Some(path);
                NodeStatus::Success
            }
            None => NodeStatus::Failure,
        }
    }
}

#[derive(Debug, Clone)]
struct FollowAssignedEntity {
    following_distance: i32,
}
impl FollowAssignedEntity {
    fn new_node(following_distance: i32) -> Box<dyn AINode> {
        Box::new(Self { following_distance })
    }
}
impl AINode for FollowAssignedEntity {
    fn box_clone(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        _time: &SystemTime,
    ) -> NodeStatus {
        match npc.loose_follow {
            Some(eid) => {
                let entity = match state.entity_map.get_from_id(eid) {
                    Some(entity) => entity,
                    None => return NodeStatus::Failure,
                };

                if entity.as_combatant().is_some_and(|cb| cb.is_dead()) {
                    return NodeStatus::Failure;
                }

                let target_pos = entity.get_position();
                let following_distance = self.following_distance; // TODO account for sizes
                let (target_pos, too_close) =
                    target_pos.interpolate(&npc.get_position(), following_distance as f32);
                if too_close {
                    return NodeStatus::Success;
                }

                // exceed target speed by 10% to not fall behind
                let target_speed = entity.get_speed() as f32 * 1.1;
                let mut path = Path::new_single(target_pos, target_speed as i32);
                path.start();
                npc.tick_movement_along_path(&mut path, clients, state);
                NodeStatus::Success
            }
            None => NodeStatus::Failure,
        }
    }
}

#[derive(Debug, Clone)]
enum RoamState {
    Idle,
    Waiting(SystemTime),
    Moving(Path),
}

#[derive(Debug, Clone)]
struct RandomRoamAround {
    home: Position,
    roam_radius_range: (i32, i32),
    roam_delay_range_ms: (u64, u64),
    roam_state: RoamState,
}
impl RandomRoamAround {
    fn new_node(
        home: Position,
        roam_radius_range: (i32, i32),
        roam_delay_range_ms: (u64, u64),
    ) -> Box<dyn AINode> {
        Box::new(Self {
            home,
            roam_radius_range,
            roam_delay_range_ms,
            roam_state: RoamState::Idle,
        })
    }
}
impl AINode for RandomRoamAround {
    fn box_clone(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> NodeStatus {
        match self.roam_state {
            RoamState::Idle => {
                let (delay_min, delay_max) = self.roam_delay_range_ms;
                let wait_time_ms = thread_rng().gen_range(delay_min..delay_max);
                let wait_time = Duration::from_millis(wait_time_ms);
                self.roam_state = RoamState::Waiting(*time + wait_time);
            }
            RoamState::Waiting(wait_time) => {
                if *time > wait_time {
                    let (min_radius, max_radius) = self.roam_radius_range;
                    let roam_radius = thread_rng().gen_range(min_radius..max_radius);
                    let target_pos = self.home.get_random_around(roam_radius, roam_radius, 0);
                    let speed = tdata_get().get_npc_stats(npc.ty).unwrap().walk_speed;
                    let mut path = Path::new_single(target_pos, speed);
                    path.start();
                    self.roam_state = RoamState::Moving(path);
                }
            }
            RoamState::Moving(ref mut path) => {
                npc.tick_movement_along_path(path, clients, state);
                if path.is_done() {
                    self.roam_state = RoamState::Idle;
                }
            }
        }
        NodeStatus::Success
    }
}
