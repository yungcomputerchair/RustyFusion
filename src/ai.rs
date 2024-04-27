use std::time::SystemTime;

use crate::{
    entity::{Entity, NPC},
    error::{log, Severity},
    net::ClientMap,
    state::ShardServerState,
    Position,
};

#[derive(Debug, Clone)]
pub struct AI {
    head: AINode,
}
impl AI {
    pub fn init() -> AI {
        AI {
            head: AINode {
                behaviors: vec![],
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
}

#[derive(Debug, Clone)]
pub enum Behavior {
    ExampleBehavior,
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
                Behavior::ExampleBehavior => NodeOperation::Nop,
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
