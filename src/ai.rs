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

#[derive(Debug, Clone)]
pub struct AI {
    head: AINode,
}
impl AI {
    pub fn new(behavior: Behavior) -> AI {
        AI {
            head: AINode {
                behaviors: vec![behavior],
                parent: None,
            },
        }
    }

    pub fn tick(
        mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> Self {
        self.head = self.head.tick(npc, state, clients, time);
        self
    }

    pub fn add_base_behavior(&mut self, behavior: Behavior) {
        let mut node = &mut self.head;
        while let Some(parent) = &mut node.parent {
            node = parent;
        }
        node.behaviors.push(behavior);
    }
}

#[derive(Debug, Clone)]
pub enum Behavior {
    RandomRoamAround(RandomRoamAroundCtx),
}

#[derive(Debug, Clone)]
struct AINode {
    behaviors: Vec<Behavior>,
    parent: Option<Box<AINode>>,
}

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
    ) -> Self {
        for behavior in self.behaviors.iter_mut() {
            let op = match behavior {
                Behavior::RandomRoamAround(ctx) => ctx.tick(npc, state, clients, time),
            };
            match op {
                NodeOperation::Nop => (),
                NodeOperation::Push(mut node) => {
                    node.parent = Some(Box::new(self));
                    return node;
                }
                NodeOperation::Pop => {
                    if let Some(parent) = self.parent {
                        return *parent;
                    } else {
                        log(
                            Severity::Warning,
                            &format!("AI attempted to pop root node ({:?})", npc.get_id()),
                        );
                    }
                }
                NodeOperation::Replace(node) => {
                    return node;
                }
            }
        }
        self
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
    roam_radius: i32,
    roam_delay_range_ms: (u64, u64),
    roam_state: RoamState,
}
impl RandomRoamAroundCtx {
    pub fn new(
        home: Position,
        roam_radius: i32,
        roam_delay_range_ms: (u64, u64),
    ) -> RandomRoamAroundCtx {
        RandomRoamAroundCtx {
            home,
            roam_radius,
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
                    let target_pos =
                        self.home
                            .get_random_around(self.roam_radius, self.roam_radius, 0);
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
