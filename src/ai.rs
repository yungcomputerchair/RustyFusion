use std::time::{Duration, SystemTime};

use rand::{thread_rng, Rng};

use crate::{
    entity::{Entity, NPC},
    error::{log, Severity},
    net::ClientMap,
    path::Path,
    state::ShardServerState,
    tabledata::tdata_get,
    Position,
};

#[derive(Debug, Clone, Default)]
pub struct AI {
    root: AINode,
}
impl AI {
    pub fn tick(
        mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> Self {
        self.root = self
            .root
            .tick(npc, state, clients, time)
            .unwrap_or_else(|| {
                log(
                    Severity::Warning,
                    &format!("AI root node deleted ({:?})", npc.get_id()),
                );
                AINode::default()
            });
        self
    }

    pub fn add_base_node_with_behaviors(&mut self, behaviors: Vec<Behavior>) {
        // "base nodes" are nodes that are children of the root.
        // base behaviors or behavior groups are added to these nodes.
        // we do NOT add any behaviors to the root node.
        //
        // if multiple behaviors are passed in, they are grouped together.
        // group behaviors are ticked, popped, and replaced together
        // since they are bundled in the same node.
        let mut node = AINode::default();
        node.behaviors.extend(behaviors);
        self.root.children.push(node);
    }
}

#[derive(Debug, Clone)]
pub enum Behavior {
    RandomRoamAround(RandomRoamAroundCtx),
}

#[derive(Debug, Clone, Default)]
struct AINode {
    behaviors: Vec<Behavior>,
    children: Vec<AINode>,
}

#[allow(dead_code)]
enum NodeOperation {
    Nop,
    Push(AINode),
    Pop,
    Replace(AINode),
}

impl AINode {
    fn tick(
        mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> Option<Self> {
        if self.children.is_empty() {
            // no children means we are active
            return self.tick_behaviors(npc, state, clients, time);
        }

        let mut new_children = Vec::with_capacity(self.children.len());
        for child in self.children.drain(..) {
            if let Some(new_child) = child.tick(npc, state, clients, time) {
                new_children.push(new_child);
            }
        }
        self.children = new_children;
        Some(self)
    }

    fn tick_behaviors(
        mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> Option<Self> {
        for behavior in self.behaviors.iter_mut() {
            let op = match behavior {
                Behavior::RandomRoamAround(ctx) => ctx.tick(npc, state, clients, time),
            };
            match op {
                NodeOperation::Nop => (),
                NodeOperation::Push(node) => {
                    self.children.push(node);
                    break;
                }
                NodeOperation::Pop => {
                    return None;
                }
                NodeOperation::Replace(node) => {
                    return Some(node);
                }
            }
        }
        Some(self)
    }
}

#[derive(Debug, Clone)]
enum RoamState {
    Idle,
    Waiting(SystemTime),
    Moving(Path),
}

#[derive(Debug, Clone)]
pub struct RandomRoamAroundCtx {
    home: Position,
    roam_radius_range: (i32, i32),
    roam_delay_range_ms: (u64, u64),
    roam_state: RoamState,
}
impl RandomRoamAroundCtx {
    pub fn new(
        home: Position,
        roam_radius_range: (i32, i32),
        roam_delay_range_ms: (u64, u64),
    ) -> RandomRoamAroundCtx {
        RandomRoamAroundCtx {
            home,
            roam_radius_range,
            roam_delay_range_ms,
            roam_state: RoamState::Idle,
        }
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> NodeOperation {
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
        NodeOperation::Nop
    }
}
