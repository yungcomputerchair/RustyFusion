use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use rand::{rngs::ThreadRng, thread_rng, Rng};

use crate::{
    chunk::TickMode,
    defines::{RANGE_GROUP_PARTICIPATE, SHARD_TICKS_PER_SECOND},
    entity::{Combatant, Entity, EntityID, NPC},
    enums::CombatantTeam,
    error::*,
    helpers,
    net::ClientMap,
    path::Path,
    skills,
    state::ShardServerState,
    tabledata::tdata_get,
    util::*,
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
        rng: &mut ThreadRng,
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
    pub fn make_for_npc(npc: &NPC, force: bool) -> (Option<Self>, TickMode) {
        const DECHUNK_DELAY_MS: u64 = 2000;

        let stats = tdata_get().get_npc_stats(npc.ty).unwrap();
        if !force && npc.path.is_none() && stats.ai_type == 0 {
            return (None, TickMode::Never);
        }

        let include_combatant_nodes = npc.as_combatant().is_some();
        let include_friendly_nodes =
            include_combatant_nodes && stats.team == CombatantTeam::Friendly;
        let include_mob_nodes = include_combatant_nodes && stats.team == CombatantTeam::Mob;
        let include_pack_follower_nodes = include_mob_nodes && npc.tight_follow.is_some();

        let root_sequence = SequenceNode::new_node({
            let mut root_behaviors = Vec::new();

            // Combatants: check for defeat
            if include_combatant_nodes {
                let respawn_time_ms = stats.regen_time * 100;
                root_behaviors.push(CheckDead::new_node(
                    npc.get_position(),
                    Duration::from_millis(DECHUNK_DELAY_MS),
                    Duration::from_millis(respawn_time_ms),
                ));
            }

            // Mobs: check for retreat
            if include_mob_nodes {
                let retreat_selector = SelectorNode::new_node({
                    let mut retreat_behaviors = Vec::new();

                    // Pack followers: check for leader retreat
                    if include_pack_follower_nodes {
                        retreat_behaviors.push(CheckLeaderRetreat::new_node());
                    }

                    // Retreat if needed
                    let retreat_threshold = stats.combat_range;
                    let retreat_to = npc.get_position();
                    retreat_behaviors.push(CheckRetreat::new_node(retreat_to, retreat_threshold));

                    retreat_behaviors
                });
                root_behaviors.push(retreat_selector);
            }

            // Pack followers: sync aggro with leader.
            // This has to happen before movement to avoid yo-yoing
            if include_pack_follower_nodes {
                root_behaviors.push(SyncPackLeaderTarget::new_node());
            }

            // Friendly combatants: sync target with player
            if include_friendly_nodes {
                root_behaviors.push(SyncPlayerTarget::new_node());
            }

            // Do movement
            let movement_selector = SelectorNode::new_node({
                let mut movement_behaviors = Vec::new();

                // Combatants: follow target
                if include_combatant_nodes {
                    let stay_within_range = stats.attack_range + stats.radius;
                    let following_distance = stats.radius;
                    movement_behaviors.push(FollowEntityLoose::new_node(
                        FollowTarget::CombatTarget,
                        stay_within_range,
                        following_distance,
                        stats.run_speed,
                    ));
                }

                // Pack followers: follow leader
                if include_pack_follower_nodes {
                    let tolerance = stats.radius / 2;
                    let max_speed = stats.run_speed * 2;
                    movement_behaviors.push(FollowEntityTight::new_node(tolerance, max_speed));
                }

                // Follow assigned entity
                let stay_within_range = 300;
                let following_distance = 200;
                movement_behaviors.push(FollowEntityLoose::new_node(
                    FollowTarget::AssignedEntity,
                    stay_within_range,
                    following_distance,
                    stats.run_speed,
                ));

                // Follow assigned path
                movement_behaviors.push(FollowAssignedPath::new_node());

                // Mobs with non-zero idle range: roam around spawn
                if include_mob_nodes && stats.idle_range > 0 {
                    let roam_radius_max = stats.idle_range / 2;
                    let roam_radius_range = (roam_radius_max / 2, roam_radius_max);
                    let roam_delay_max_ms = stats.delay_time * 1000;
                    let roam_delay_range_ms = (roam_delay_max_ms / 2, roam_delay_max_ms);
                    movement_behaviors.push(PatrolPoint::new_node(
                        npc.get_position(),
                        roam_radius_range,
                        roam_delay_range_ms,
                    ));
                }

                movement_behaviors
            });
            root_behaviors.push(movement_selector);

            // Combatants: find and attack targets
            if include_combatant_nodes {
                let combat_sequence = SequenceNode::new_node({
                    let mut combat_behaviors = Vec::new();

                    // Mobs: scan for non-mob targets
                    if include_mob_nodes {
                        // tweak as needed
                        let scan_radius = stats.sight_range;
                        let distance_factor = 0.5;
                        let level_factor = 0.1;
                        let aggro_rates = (1.5, -1.0);
                        let aggro_threshold = 1.0;
                        combat_behaviors.push(ScanForTargets::new_node(
                            Some(CombatantTeam::Friendly),
                            scan_radius,
                            distance_factor,
                            level_factor,
                            aggro_rates,
                            aggro_threshold,
                        ));
                    }

                    // Attack target
                    let attack_range = stats.attack_range + stats.radius;
                    let attack_cooldown = Duration::from_millis(stats.delay_time * 100);
                    combat_behaviors.push(CheckAttack::new_node(attack_range, attack_cooldown));

                    combat_behaviors
                });
                root_behaviors.push(combat_sequence);
            }
            root_behaviors
        });

        let tick_mode = if npc.path.is_some() {
            TickMode::Always
        } else {
            TickMode::WhenLoaded
        };

        (
            Some(AI {
                root: root_sequence,
            }),
            tick_mode,
        )
    }

    pub fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
        rng: &mut ThreadRng,
    ) {
        self.root.tick(npc, state, clients, time, rng);
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
        rng: &mut ThreadRng,
    ) -> NodeStatus {
        while self.cursor < self.children.len() {
            let status = self.children[self.cursor].tick(npc, state, clients, time, rng);
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
        rng: &mut ThreadRng,
    ) -> NodeStatus {
        while self.cursor < self.children.len() {
            let status = self.children[self.cursor].tick(npc, state, clients, time, rng);
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
        _rng: &mut ThreadRng,
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
struct FollowEntityLoose {
    target: FollowTarget,
    stay_within: u32,
    following_distance: u32,
    speed: i32,
}
impl FollowEntityLoose {
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
impl AINode for FollowEntityLoose {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        _time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        let target_id = match self.target {
            FollowTarget::AssignedEntity => npc.loose_follow,
            FollowTarget::CombatTarget => npc.target_id,
        };

        match target_id {
            Some(eid) => {
                let entity = match state.entity_map.get_entity_raw(eid) {
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
struct FollowEntityTight {
    tolerance: u32,
    max_speed: i32,
}
impl FollowEntityTight {
    fn new_node(tolerance: u32, max_speed: i32) -> Box<dyn AINode> {
        Box::new(Self {
            tolerance,
            max_speed,
        })
    }
}
impl AINode for FollowEntityTight {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        _time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        let (leader_id, offset) = match npc.tight_follow {
            Some(tight_follow) => tight_follow,
            None => return NodeStatus::Failure,
        };

        let leader = match state.entity_map.get_entity_raw(leader_id) {
            Some(leader) => leader,
            None => return NodeStatus::Failure,
        };

        if leader.as_combatant().is_some_and(|cb| cb.is_dead()) {
            return NodeStatus::Failure;
        }

        let pack_pos = leader.get_position() + offset;
        let distance = npc.get_position().distance_to(&pack_pos);
        if distance <= self.tolerance {
            return NodeStatus::Success;
        }

        let speed = clamp_max(distance as i32, self.max_speed);
        let mut path = Path::new_single(pack_pos, speed);
        path.start();
        npc.tick_movement_along_path(&mut path, clients, state);
        NodeStatus::Success
    }
}

#[derive(Debug, Clone)]
enum RoamState {
    Idle,
    Waiting(SystemTime),
    Moving(Path),
}

#[derive(Debug, Clone)]
struct PatrolPoint {
    home: Position,
    roam_radius_range: (u32, u32),
    roam_delay_range_ms: (u64, u64),
    roam_state: RoamState,
}
impl PatrolPoint {
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
impl AINode for PatrolPoint {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
        _rng: &mut ThreadRng,
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
enum DeadState {
    Alive,
    Dying(SystemTime),
    Dead(SystemTime),
    PermaDead,
    Done,
}

#[derive(Debug, Clone)]
struct CheckDead {
    spawn_pos: Position,
    dechunk_after: Duration,
    respawn_after: Duration,
    dead_state: DeadState,
}
impl CheckDead {
    fn new_node(
        spawn_pos: Position,
        dechunk_after: Duration,
        respawn_after: Duration,
    ) -> Box<dyn AINode> {
        Box::new(Self {
            spawn_pos,
            dechunk_after,
            respawn_after,
            dead_state: DeadState::Alive,
        })
    }
}
impl AINode for CheckDead {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
        rng: &mut ThreadRng,
    ) -> NodeStatus {
        match self.dead_state {
            DeadState::Alive => {
                if !npc.is_dead() {
                    return NodeStatus::Success;
                }
                if let Some(defeater_id) = npc.last_attacked_by {
                    log_if_failed(on_mob_defeated(npc.id, defeater_id, state, clients, rng));
                }
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
                // N.B. can't do this in the Dead state
                // because the NPC state doesn't get written
                // to the entity map until after the tick
                let chunk_pos = npc.get_chunk_coords();
                state
                    .entity_map
                    .update(npc.get_id(), Some(chunk_pos), Some(clients));
                self.dead_state = DeadState::Alive;
                return NodeStatus::Success;
            }
        }
        NodeStatus::Running
    }
}

#[derive(Debug, Clone)]
enum LeaderRetreatState {
    Idle,
    LeaderRetreating,
}

#[derive(Debug, Clone)]
struct CheckLeaderRetreat {
    leader_retreat_state: LeaderRetreatState,
}
impl CheckLeaderRetreat {
    fn new_node() -> Box<dyn AINode> {
        Box::new(Self {
            leader_retreat_state: LeaderRetreatState::Idle,
        })
    }
}
impl AINode for CheckLeaderRetreat {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        _clients: &mut ClientMap,
        _time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        let leader_npc_id = match npc.tight_follow {
            Some((EntityID::NPC(leader_npc_id), _)) => leader_npc_id,
            _ => return NodeStatus::Failure,
        };

        let leader_npc = match state.get_npc_mut(leader_npc_id) {
            Ok(leader_npc) => leader_npc,
            Err(_) => return NodeStatus::Failure,
        };

        if leader_npc.is_dead() {
            return NodeStatus::Failure;
        }

        match self.leader_retreat_state {
            LeaderRetreatState::Idle => {
                if leader_npc.retreating {
                    npc.retreating = true;
                    self.leader_retreat_state = LeaderRetreatState::LeaderRetreating;
                }
            }
            LeaderRetreatState::LeaderRetreating => {
                if !leader_npc.retreating {
                    npc.reset();
                    // TODO full heal effect
                    self.leader_retreat_state = LeaderRetreatState::Idle;
                }
            }
        }
        NodeStatus::Success
    }
}

#[derive(Debug, Clone)]
struct SyncPlayerTarget {}
impl SyncPlayerTarget {
    fn new_node() -> Box<dyn AINode> {
        Box::new(Self {})
    }
}
impl AINode for SyncPlayerTarget {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        _clients: &mut ClientMap,
        _time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        if npc.target_id.is_some() {
            return NodeStatus::Success;
        }

        let player_id = match npc.loose_follow {
            Some(EntityID::Player(player_id)) => player_id,
            _ => return NodeStatus::Success,
        };

        let player = match state.get_player(player_id) {
            Ok(player) => player,
            Err(_) => return NodeStatus::Success,
        };

        if player.is_dead() {
            return NodeStatus::Success;
        }

        if let Some(opponent) = player.last_attacked_by {
            npc.target_id = Some(opponent);
        }

        NodeStatus::Success
    }
}

#[derive(Debug, Clone)]
struct SyncPackLeaderTarget {}
impl SyncPackLeaderTarget {
    fn new_node() -> Box<dyn AINode> {
        Box::new(Self {})
    }
}
impl AINode for SyncPackLeaderTarget {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        _clients: &mut ClientMap,
        _time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        let leader_npc_id = match npc.tight_follow {
            Some((EntityID::NPC(leader_npc_id), _)) => leader_npc_id,
            _ => return NodeStatus::Success,
        };

        let leader_npc = match state.get_npc_mut(leader_npc_id) {
            Ok(leader_npc) => leader_npc,
            Err(_) => return NodeStatus::Success,
        };

        if leader_npc.is_dead() {
            return NodeStatus::Success;
        }

        if leader_npc.retreating {
            npc.target_id = None;
            return NodeStatus::Success;
        }

        match npc.target_id {
            None => {
                npc.target_id = leader_npc.target_id;
                // if npc.target_id.is_some() {
                //     println!(
                //         "{:?} sync aggro to {:?}",
                //         npc.get_id(),
                //         leader_npc.target_id
                //     );
                // }
            }
            Some(target_id) => {
                if leader_npc.target_id.is_none() {
                    leader_npc.target_id = Some(target_id);
                    //println!("{:?} sync aggro to {:?}", leader_npc.get_id(), target_id);
                }
            }
        }

        NodeStatus::Success
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
        // need to scale the threshold up to account for the tickrate.
        // higher tickrate = more ticks per second = more aggro per second.
        // this scaling cancels that out
        let aggro_threshold_scaled = aggro_threshold * SHARD_TICKS_PER_SECOND as f32;
        Box::new(Self {
            target_team,
            scan_radius,
            distance_factor,
            level_factor,
            aggro_rates,
            aggro_threshold: aggro_threshold_scaled,
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
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        if npc.target_id.is_some() {
            return NodeStatus::Success;
        }

        let (aggro_up_rate, aggro_down_rate) = self.aggro_rates;

        // gain aggro
        for eid in state.entity_map.get_around_entity(npc.get_id()) {
            let entity = state.entity_map.get_entity_raw(eid).unwrap();
            let cb = match entity.as_combatant() {
                Some(cb) => cb,
                None => continue,
            };

            // z distance counts double to avoid mobs on different floors from aggroing too easily
            let weights = (1.0, 1.0, 2.0);
            let distance = cb
                .get_position()
                .distance_to_weighted(&npc.get_position(), weights);
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

            // distance difference (normalized; edge of radius = 0, center = 1)
            let distance_diff_rel = distance as f32 / self.scan_radius as f32;
            let distance_diff_up = (1.0 - distance_diff_rel) * self.distance_factor;

            // total
            let scale = cb.get_aggro_factor();
            let aggro_up = (aggro_up_rate + level_diff_up + distance_diff_up) * scale;
            if aggro_up <= 0.0 {
                continue;
            }

            let aggro = self.aggros.entry(eid).or_insert(0.0);
            *aggro += aggro_up;
            if *aggro >= self.aggro_threshold {
                npc.target_id = Some(eid);
                self.aggros.clear();
                return NodeStatus::Success;
            }

            // gained aggro this tick, so cancel out the upcoming decay
            *aggro -= aggro_down_rate;
        }

        // lose aggro
        self.aggros.retain(|_, aggro| {
            *aggro += aggro_down_rate;
            *aggro >= 0.0
        });

        NodeStatus::Failure
    }
}

#[derive(Debug, Clone)]
enum RetreatState {
    Idle,
    Retreating(Path),
}

#[derive(Debug, Clone)]
struct CheckRetreat {
    retreat_to: Position,
    retreat_threshold: u32,
    retreat_state: RetreatState,
}
impl CheckRetreat {
    fn new_node(retreat_to_initial: Position, retreat_threshold: u32) -> Box<dyn AINode> {
        Box::new(Self {
            retreat_to: retreat_to_initial,
            retreat_threshold,
            retreat_state: RetreatState::Idle,
        })
    }
}
impl AINode for CheckRetreat {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        _time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        match &mut self.retreat_state {
            RetreatState::Idle => {
                let target_id = match npc.target_id {
                    Some(target_id) => target_id,
                    None => {
                        // no target; update retreat position
                        self.retreat_to = npc.get_position();
                        return NodeStatus::Success;
                    }
                };

                let should_retreat = match state.entity_map.get_entity_raw(target_id) {
                    Some(target) => {
                        let cb = target.as_combatant().unwrap();
                        cb.is_dead() // target dead
                        // or no longer aggroable
                        || cb.get_aggro_factor() <= 0.0
                        // or they've gone too far
                        || cb.get_position()
                            .distance_to(&self.retreat_to) > self.retreat_threshold
                        // or we've gone too far
                        || npc.get_position()
                            .distance_to(&self.retreat_to) > self.retreat_threshold
                    }
                    None => true, // target gone
                };

                if should_retreat {
                    let speed = tdata_get().get_npc_stats(npc.ty).unwrap().run_speed * 2;
                    let mut path = Path::new_single(self.retreat_to, speed);
                    path.start();
                    self.retreat_state = RetreatState::Retreating(path);
                    npc.retreating = true;
                    NodeStatus::Running
                } else {
                    NodeStatus::Success
                }
            }
            RetreatState::Retreating(path) => {
                npc.tick_movement_along_path(path, clients, state);
                if path.is_done() {
                    self.retreat_state = RetreatState::Idle;
                    npc.reset();
                    // TODO full heal effect
                    NodeStatus::Success
                } else {
                    NodeStatus::Running
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
enum AttackState {
    Attacking,
    OnCooldown(SystemTime),
    Casting(SystemTime),
}

#[derive(Debug, Clone)]
struct CheckAttack {
    attack_range: u32,
    attack_cooldown: Duration,
    attack_state: AttackState,
}
impl CheckAttack {
    fn new_node(attack_range: u32, attack_cooldown: Duration) -> Box<dyn AINode> {
        Box::new(Self {
            attack_range,
            attack_cooldown,
            attack_state: AttackState::Attacking,
        })
    }
}
impl AINode for CheckAttack {
    fn clone_node(&self) -> Box<dyn AINode> {
        Box::new(self.clone())
    }

    fn tick(
        &mut self,
        npc: &mut NPC,
        state: &mut ShardServerState,
        clients: &mut ClientMap,
        time: &SystemTime,
        _rng: &mut ThreadRng,
    ) -> NodeStatus {
        let target_id = match npc.target_id {
            Some(target_id) => target_id,
            None => return NodeStatus::Failure,
        };

        let target = match state.get_combatant(target_id) {
            Ok(target) => target,
            Err(_) => {
                npc.target_id = None;
                return NodeStatus::Failure;
            }
        };

        if target.is_dead() {
            npc.target_id = None;
            return NodeStatus::Failure;
        }

        if npc.get_position().distance_to(&target.get_position()) > self.attack_range {
            return NodeStatus::Failure;
        }

        match self.attack_state {
            AttackState::Attacking => {
                let do_skill = placeholder!(false);
                if do_skill {
                    // TODO
                    let cast_time = placeholder!(Duration::from_secs(3));
                    self.attack_state = AttackState::Casting(*time + cast_time);
                    NodeStatus::Running
                } else {
                    let target_ids = &[target_id];
                    log_if_failed(skills::do_basic_attack(
                        npc.get_id(),
                        target_ids,
                        false,
                        (None, None),
                        (None, None),
                        state,
                        clients,
                    ));

                    let wait_time = *time + self.attack_cooldown;
                    self.attack_state = AttackState::OnCooldown(wait_time);
                    NodeStatus::Success
                }
            }
            AttackState::OnCooldown(wait_time) => {
                if *time > wait_time {
                    self.attack_state = AttackState::Attacking;
                }
                NodeStatus::Failure
            }
            AttackState::Casting(wait_time) => {
                if npc.is_dead() {
                    self.attack_state = AttackState::Attacking;
                    return NodeStatus::Failure;
                }

                if *time > wait_time {
                    self.attack_state = AttackState::OnCooldown(*time + self.attack_cooldown);
                    // TODO effect
                    NodeStatus::Success
                } else {
                    NodeStatus::Running
                }
            }
        }
    }
}

fn on_mob_defeated(
    npc_id: i32,
    defeater_id: EntityID,
    state: &mut ShardServerState,
    clients: &mut ClientMap,
    rng: &mut ThreadRng,
) -> FFResult<()> {
    let defeated_type = state.get_npc(npc_id).unwrap().ty;
    if let EntityID::Player(pc_id) = defeater_id {
        let player = state.get_player_mut(pc_id)?;
        helpers::give_defeat_rewards(player, defeated_type, clients, rng);
    }

    let defeater = state.get_combatant(defeater_id)?;
    if let Some(group_id) = defeater.get_group_id() {
        let position = defeater.get_position();
        let group = state.groups.get(&group_id).unwrap().clone();
        for eid in group.get_member_ids() {
            if let EntityID::Player(member_pc_id) = *eid {
                if defeater_id == *eid {
                    // already rewarded
                    continue;
                }
                let player = state.get_player_mut(member_pc_id).unwrap();
                if player.get_position().distance_to(&position) < RANGE_GROUP_PARTICIPATE {
                    helpers::give_defeat_rewards(player, defeated_type, clients, rng);
                }
            }
        }
    }
    Ok(())
}
