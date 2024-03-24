use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};

use crate::{
    defines::{SIZEOF_QUESTFLAG_NUMBER, SIZEOF_RQUEST_SLOT},
    enums::*,
    error::{panic_log, FFError, FFResult, Severity},
    net::packet::sRunningQuest,
    tabledata::tdata_get,
};

#[derive(Debug)]
pub struct MissionDefinition {
    pub mission_id: i32,
    pub mission_name: String,
    pub first_task_id: i32,
    pub mission_type: MissionType,
}

#[derive(Debug)]
pub struct TaskDefinition {
    pub task_id: i32,        // m_iHTaskID
    pub mission_id: i32,     // m_iHMissionID
    pub task_type: TaskType, // m_iHTaskType

    // prerequisites
    pub prereq_npc_type: Option<i32>,               // m_iHNPCID
    pub prereq_completed_mission_ids: HashSet<i32>, // m_iCSTReqMission
    pub prereq_nano_ids: HashSet<i16>,              // m_iCSTRReqNano
    pub prereq_level: Option<i16>,                  // m_iCTRReqLvMin
    pub prereq_guide: Option<PlayerGuide>,          // m_iCSTReqGuide
    pub prereq_map_num: Option<u32>,                // m_iRequireInstanceID

    // objectives
    pub obj_npc_type: Option<i32>,        // m_iHTerminatorNPCID
    pub obj_qitems: HashMap<i16, usize>,  // m_iCSUItemID -> m_iCSUItemNumNeeded
    pub obj_enemies: HashMap<i32, usize>, // m_iCSUEnemyID -> m_iCSUNumToKill
    pub obj_enemy_id_ordering: Vec<i32>, // m_iCSUEnemyID (needed for loading counts correctly from DB)
    pub obj_escort_npc_type: Option<i32>, // m_iCSUDEFNPCID
    pub obj_time_limit: Option<Duration>, // m_iCSUCheckTimer

    // failure
    pub fail_task_id: Option<i32>,        // m_iFOutgoingTask
    pub fail_qitems: HashMap<i16, isize>, // m_iFItemID -> m_iFItemNumNeeded

    // success
    pub succ_task_id: Option<i32>,        // m_iSUOutgoingTask
    pub succ_qitems: HashMap<i16, isize>, // m_iSUItem -> m_iSUInstancename

    // delete
    pub del_qitems: HashSet<i16>, // m_iDelItemID
}

#[derive(Debug, Clone)]
pub struct Task {
    task_id: i32,
    pub remaining_enemy_defeats: HashMap<i32, usize>,
    pub fail_time: Option<SystemTime>,
    pub completed: bool,
}
impl Task {
    pub fn get_task_id(&self) -> i32 {
        self.task_id
    }

    pub fn get_task_def(&self) -> &'static TaskDefinition {
        tdata_get().get_task_definition(self.task_id).unwrap()
    }

    pub fn get_mission_def(&self) -> &'static MissionDefinition {
        let task_def = self.get_task_def();
        tdata_get()
            .get_mission_definition(task_def.mission_id)
            .unwrap()
    }

    pub fn get_remaining_enemy_defeats(&self) -> [usize; 3] {
        let task_def = self.get_task_def();
        let enemy_types = task_def.obj_enemy_id_ordering.as_slice();
        let mut counts = [0; 3];
        for (idx, enemy_type) in enemy_types.iter().enumerate() {
            counts[idx] = self.remaining_enemy_defeats[enemy_type];
        }
        counts
    }

    pub fn set_remaining_enemy_defeats(&mut self, counts: [usize; 3]) {
        let task_def = self.get_task_def();
        let enemy_types = task_def.obj_enemy_id_ordering.as_slice();
        self.remaining_enemy_defeats = task_def.obj_enemies.clone();
        for (idx, enemy_type) in enemy_types.iter().enumerate() {
            self.remaining_enemy_defeats
                .insert(*enemy_type, counts[idx]);
        }
    }
}
impl From<&TaskDefinition> for Task {
    fn from(task_def: &TaskDefinition) -> Self {
        Task {
            task_id: task_def.task_id,
            remaining_enemy_defeats: task_def.obj_enemies.clone(),
            fail_time: task_def.obj_time_limit.map(|d| SystemTime::now() + d),
            completed: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MissionJournal {
    current_nano_mission: Option<Task>,
    current_guide_mission: Option<Task>,
    current_world_missions: Vec<Task>,
    active_mission_slot: Option<usize>,
    pub completed_mission_flags: [i64; SIZEOF_QUESTFLAG_NUMBER as usize],
}
impl MissionJournal {
    fn get_task_iter(&self) -> impl Iterator<Item = &Task> {
        let mut tasks = Vec::new();
        if let Some(task) = &self.current_nano_mission {
            tasks.push(task);
        }
        if let Some(task) = &self.current_guide_mission {
            tasks.push(task);
        }
        tasks.extend(self.current_world_missions.iter());
        tasks.into_iter()
    }

    fn get_task_iter_mut(&mut self) -> impl Iterator<Item = &mut Task> {
        let mut tasks = Vec::new();
        if let Some(task) = &mut self.current_nano_mission {
            tasks.push(task);
        }
        if let Some(task) = &mut self.current_guide_mission {
            tasks.push(task);
        }
        tasks.extend(self.current_world_missions.iter_mut());
        tasks.into_iter()
    }

    fn get_current_task_by_idx(&self, idx: usize) -> Option<&Task> {
        match idx {
            0 => self.current_nano_mission.as_ref(),
            1 => self.current_guide_mission.as_ref(),
            2..=5 => self.current_world_missions.get(idx - 2),
            _ => panic_log("Invalid mission slot index"),
        }
    }

    pub fn get_current_tasks(&self) -> Vec<Task> {
        self.get_task_iter().cloned().collect()
    }

    pub fn get_running_quests(&self) -> [sRunningQuest; SIZEOF_RQUEST_SLOT as usize] {
        let mut running_quests = [sRunningQuest::default(); SIZEOF_RQUEST_SLOT as usize];
        for (i, quest) in running_quests.iter_mut().enumerate().take(6) {
            let task = self.get_current_task_by_idx(i);
            if let Some(task) = task {
                let task_def = task.get_task_def();
                quest.m_aCurrTaskID = task_def.task_id;
                for (j, (npc_id, count)) in task.remaining_enemy_defeats.iter().enumerate() {
                    quest.m_aKillNPCID[j] = *npc_id;
                    quest.m_aKillNPCCount[j] = *count as i32;
                }
                for (j, (item_id, count)) in task_def.obj_qitems.iter().enumerate() {
                    quest.m_aNeededItemID[j] = *item_id as i32;
                    quest.m_aNeededItemCount[j] = *count as i32;
                }
            }
        }
        running_quests
    }

    pub fn get_active_mission_id(&self) -> Option<i32> {
        let idx = self.active_mission_slot?;
        let active_task = self.get_current_task_by_idx(idx)?;
        let task_def = active_task.get_task_def();
        Some(task_def.mission_id)
    }

    pub fn get_current_task_ids(&self) -> Vec<i32> {
        let mut task_ids = Vec::new();
        for task in self.get_task_iter() {
            let task_def = task.get_task_def();
            task_ids.push(task_def.task_id);
        }
        task_ids
    }

    pub fn is_mission_completed(&self, mission_id: i32) -> FFResult<bool> {
        const MAX_MISSION_ID: i32 = SIZEOF_QUESTFLAG_NUMBER as i32 * 64;
        match mission_id {
            1..=MAX_MISSION_ID => {
                let offset = mission_id - 1;
                let flags_idx = offset / 32;
                let bit_idx = offset % 32;
                Ok((self.completed_mission_flags[flags_idx as usize] & (1 << bit_idx)) != 0)
            }
            _ => Err(FFError::build(
                Severity::Warning,
                format!("Invalid mission ID {}", mission_id),
            )),
        }
    }

    pub fn set_active_mission_id(&mut self, mission_id: i32) -> FFResult<usize> {
        let mut current_mission_slot = None;
        for idx in 0..6 {
            if let Some(task) = self.get_current_task_by_idx(idx) {
                let task_def = task.get_task_def();
                if task_def.mission_id == mission_id {
                    current_mission_slot = Some(idx);
                    break;
                }
            }
        }

        match current_mission_slot {
            Some(idx) => {
                self.active_mission_slot = Some(idx);
                Ok(idx)
            }
            None => Err(FFError::build(
                Severity::Warning,
                format!("No current task for mission ID {}", mission_id),
            )),
        }
    }

    pub fn start_task(&mut self, task: Task) -> FFResult<bool> {
        let mission_def = task.get_mission_def();
        let mission_existing_task = self
            .get_task_iter_mut()
            .find(|t| t.get_task_def().mission_id == mission_def.mission_id);
        let new_mission = if let Some(existing_task) = mission_existing_task {
            if !existing_task.completed {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Tried to start task {} while task {} for mission {} is in progress",
                        task.task_id, existing_task.task_id, mission_def.mission_id
                    ),
                ));
            }
            *existing_task = task; // replace existing task
            false
        } else {
            match mission_def.mission_type {
                MissionType::Unknown => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} for unknown mission type",
                            task.task_id
                        ),
                    ))
                }
                MissionType::Guide => {
                    if self.current_guide_mission.is_some() {
                        return Err(FFError::build(
                            Severity::Warning,
                            "Guide mission already in progress".to_string(),
                        ));
                    }
                    self.current_guide_mission = Some(task);
                }
                MissionType::Nano => {
                    if self.current_nano_mission.is_some() {
                        return Err(FFError::build(
                            Severity::Warning,
                            "Nano mission already in progress".to_string(),
                        ));
                    }
                    self.current_nano_mission = Some(task);
                }
                MissionType::Normal => {
                    if self.current_world_missions.len() >= 4 {
                        return Err(FFError::build(
                            Severity::Warning,
                            "No empty world mission slots".to_string(),
                        ));
                    }
                    self.current_world_missions.push(task);
                }
            };
            true
        };
        Ok(new_mission)
    }
}

impl Default for sRunningQuest {
    fn default() -> Self {
        sRunningQuest {
            m_aCurrTaskID: 0,
            m_aKillNPCID: [0; 3],
            m_aKillNPCCount: [0; 3],
            m_aNeededItemID: [0; 3],
            m_aNeededItemCount: [0; 3],
        }
    }
}
