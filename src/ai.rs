use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use rand::{thread_rng, Rng};

use crate::{
    chunk::TickMode,
    defines::SHARD_TICKS_PER_SECOND,
    entity::{Combatant, Entity, EntityID, NPC},
    enums::CombatantTeam,
    net::ClientMap,
    path::Path,
    state::ShardServerState,
    tabledata::tdata_get,
    Position,
};

trait AINode: std::fmt::Debug {
    fn clone_node(&self) -> Box<dyn AINode>;
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
        self.clone_node()
    }
}

#[derive(Debug, Clone)]
pub struct AI {
    root: Box<dyn AINode>,
}
impl AI {
    pub fn make_for_npc(npc: &NPC) -> (Option<Self>, TickMode) {
        const DECHUNK_DELAY_MS: u64 = 2000;

        let stats = tdata_get().get_npc_stats(npc.ty).unwrap();
        if stats.ai_type == 0 {
            return (None, TickMode::Never);
        }

        // Defeat beahviors
        let respawn_time_ms = stats.regen_time * 100;
        let defeat_behaviors = vec![
            CheckAlive::new_node(),
            Dead::new_node(
                npc.get_position(),
                Duration::from_millis(DECHUNK_DELAY_MS),
                Duration::from_millis(respawn_time_ms),
            ),
        ];
        let defeat_selector = SelectorNode::new_node(defeat_behaviors);

        // Movement behaviors
        let mut movement_behaviors = vec![
            FollowAssignedPath::new_node(),
            FollowEntity::new_node(FollowTarget::AssignedEntity, 300, 200, stats.walk_speed),
        ];
        if stats.team == CombatantTeam::Mob {
            // stats.ai_type == ???

            // Follow combat target
            let stay_within_range = stats.attack_range + stats.radius;
            let following_distance = stats.radius;
            movement_behaviors.push(FollowEntity::new_node(
                FollowTarget::CombatTarget,
                stay_within_range,
                following_distance,
                stats.run_speed,
            ));

            // Roam around spawn
            if stats.idle_range > 0 {
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
        }
        let movement_selector = SelectorNode::new_node(movement_behaviors);

        // Combat behaviors
        let mut combat_behaviors = vec![];
        if stats.team == CombatantTeam::Mob {
            let scan_radius = stats.sight_range;
            let distance_factor = 0.1;
            let level_factor = 0.1;
            let aggro_rates = (
                1.0 / SHARD_TICKS_PER_SECOND as f32,
                -0.5 / SHARD_TICKS_PER_SECOND as f32,
            );
            let aggro_threshold = 100.0;
            combat_behaviors.push(ScanForTargets::new_node(
                Some(CombatantTeam::Friendly),
                scan_radius,
                distance_factor,
                level_factor,
                aggro_rates,
                aggro_threshold,
            ));
        }
        let combat_sequence = SequenceNode::new_node(combat_behaviors);

        let root =
            SequenceNode::new_node(vec![defeat_selector, movement_selector, combat_sequence]);
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
    fn clone_node(&self) -> Box<dyn AINode> {
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
    fn clone_node(&self) -> Box<dyn AINode> {
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
    fn clone_node(&self) -> Box<dyn AINode> {
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
enum FollowTarget {
    AssignedEntity,
    CombatTarget,
}

#[derive(Debug, Clone)]
struct FollowEntity {
    target: FollowTarget,
    stay_within: u32,
    following_distance: u32,
    speed: i32,
}
impl FollowEntity {
    fn new_node(
        target: FollowTarget,
        stay_within: u32,
        following_distance: u32,
        speed: i32,
    ) -> Box<dyn AINode> {
        Box::new(Self {
            target,
            stay_within,
            following_distance,
            speed,
        })
    }
}
impl AINode for FollowEntity {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        _time: &SystemTime,
    ) -> NodeStatus {
        let target_id = match self.target {
            FollowTarget::AssignedEntity => npc.loose_follow,
            FollowTarget::CombatTarget => npc.target_id,
        };

        match target_id {
            Some(eid) => {
                let entity = match state.entity_map.get_from_id(eid) {
                    Some(entity) => entity,
                    None => return NodeStatus::Failure,
                };

                if entity.as_combatant().is_some_and(|cb| cb.is_dead()) {
                    return NodeStatus::Failure;
                }

                let self_pos = npc.get_position();
                let target_pos = entity.get_position();
                if target_pos.distance_to(&self_pos) < self.stay_within {
                    return NodeStatus::Success;
                }

                let (target_pos, too_close) =
                    target_pos.interpolate(&npc.get_position(), self.following_distance as f32);
                if too_close {
                    return NodeStatus::Success;
                }

                let mut path = Path::new_single(target_pos, self.speed);
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
    roam_radius_range: (u32, u32),
    roam_delay_range_ms: (u64, u64),
    roam_state: RoamState,
}
impl RandomRoamAround {
    fn new_node(
        home: Position,
        roam_radius_range: (u32, u32),
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
    fn clone_node(&self) -> Box<dyn AINode> {
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

#[derive(Debug, Clone)]
struct CheckAlive {}
impl CheckAlive {
    fn new_node() -> Box<dyn AINode> {
        Box::new(Self {})
    }
}
impl AINode for CheckAlive {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        _state: &mut ShardServerState,
        _clients: &mut ClientMap,
        _time: &SystemTime,
    ) -> NodeStatus {
        if npc.as_combatant().is_some_and(|cb| cb.is_dead()) {
            NodeStatus::Failure
        } else {
            NodeStatus::Success
        }
    }
}

#[derive(Debug, Clone)]
enum DeadState {
    Init,
    Dying(SystemTime),
    Dead(SystemTime),
    PermaDead,
    Done,
}

#[derive(Debug, Clone)]
struct Dead {
    spawn_pos: Position,
    dechunk_after: Duration,
    respawn_after: Duration,
    dead_state: DeadState,
}
impl Dead {
    fn new_node(
        spawn_pos: Position,
        dechunk_after: Duration,
        respawn_after: Duration,
    ) -> Box<dyn AINode> {
        Box::new(Self {
            spawn_pos,
            dechunk_after,
            respawn_after,
            dead_state: DeadState::Init,
        })
    }
}
impl AINode for Dead {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
    ) -> NodeStatus {
        match self.dead_state {
            DeadState::Init => {
                let dechunk_time = *time + self.dechunk_after;
                self.dead_state = DeadState::Dying(dechunk_time);
            }
            DeadState::Dying(dechunk_time) => {
                if *time > dechunk_time {
                    state.entity_map.update(npc.get_id(), None, Some(clients));
                    if npc.summoned {
                        state.entity_map.mark_for_cleanup(npc.get_id());
                        self.dead_state = DeadState::PermaDead;
                    } else {
                        let respawn_time = *time + self.respawn_after - self.dechunk_after;
                        self.dead_state = DeadState::Dead(respawn_time);
                    }
                }
            }
            DeadState::Dead(respawn_time) => {
                if *time > respawn_time {
                    npc.reset();
                    npc.set_position(self.spawn_pos);
                    self.dead_state = DeadState::Done;
                }
            }
            DeadState::PermaDead => {}
            DeadState::Done => {
                // N.B. can't do this in the previous state
                // because the NPC state doesn't get saved until
                // after the tick
                let chunk_pos = npc.get_chunk_coords();
                state
                    .entity_map
                    .update(npc.get_id(), Some(chunk_pos), Some(clients));
                self.dead_state = DeadState::Init;
                return NodeStatus::Success;
            }
        }
        NodeStatus::Running
    }
}

#[derive(Debug, Clone)]
struct ScanForTargets {
    target_team: Option<CombatantTeam>,
    scan_radius: u32,
    distance_factor: f32,
    level_factor: f32,
    aggro_rates: (f32, f32),
    aggro_threshold: f32,
    aggros: HashMap<EntityID, f32>,
}
impl ScanForTargets {
    fn new_node(
        target_team: Option<CombatantTeam>,
        scan_radius: u32,
        distance_factor: f32,
        level_factor: f32,
        aggro_rates: (f32, f32),
        aggro_threshold: f32,
    ) -> Box<dyn AINode> {
        Box::new(Self {
            target_team,
            scan_radius,
            distance_factor,
            level_factor,
            aggro_rates,
            aggro_threshold,
            aggros: HashMap::new(),
        })
    }
}
impl AINode for ScanForTargets {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        _clients: &mut ClientMap,
        _time: &SystemTime,
    ) -> NodeStatus {
        if npc.target_id.is_some() {
            return NodeStatus::Success;
        }

        // gain aggro
        let aggro_up_rate = self.aggro_rates.0;
        for eid in state.entity_map.get_around_entity(npc.get_id()) {
            let entity = state.entity_map.get_from_id(eid).unwrap();
            let cb = match entity.as_combatant() {
                Some(cb) => cb,
                None => continue,
            };

            let distance = cb.get_position().distance_to(&npc.get_position());
            if distance > self.scan_radius {
                continue;
            }

            let cb_team = cb.get_team();
            let is_opponent = match self.target_team {
                Some(team) => cb_team == team,
                None => cb_team != npc.get_team(),
            };
            if !is_opponent {
                continue;
            }

            // level difference
            let level_diff = (npc.get_level() - cb.get_level()) as f32;
            let level_diff_up = level_diff * self.level_factor;
            // distance difference
            let distance_diff = distance as f32;
            let distance_diff_up = distance_diff * self.distance_factor;
            // total
            let scale = cb.get_aggro_factor();
            let aggro_up = (aggro_up_rate + level_diff_up + distance_diff_up) * scale;

            let aggro = self.aggros.entry(eid).or_insert(0.0);
            *aggro += aggro_up;
            if *aggro >= self.aggro_threshold {
                npc.target_id = Some(eid);
                self.aggros.clear();
                return NodeStatus::Success;
            }
        }

        // lose aggro
        let aggro_down_rate = self.aggro_rates.1;
        self.aggros.retain(|_, aggro| {
            *aggro += aggro_down_rate;
            *aggro >= 0.0
        });

        NodeStatus::Failure
    }
}
