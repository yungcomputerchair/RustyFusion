use std::time::{Duration, SystemTime};

use crate::{
    defines::{SIZEOF_QUESTFLAG_NUMBER, SIZEOF_REPEAT_QUESTFLAG_NUMBER},
    enums::*,
    error::{FFError, FFResult, Severity},
    net::packet::sRunningQuest,
    tabledata::tdata_get,
};

#[derive(Debug)]
pub struct MissionDefinition {
    pub mission_id: i32,
    pub mission_name: String,
    pub task_ids: Vec<i32>,
    pub mission_type: MissionType,
}

#[derive(Debug)]
pub struct TaskDefinition {
    pub task_id: i32,                 // m_iHTaskID
    pub mission_id: i32,              // m_iHMissionID
    pub repeatable: bool,             // m_iRepeatflag
    pub task_type: TaskType,          // m_iHTaskType
    pub success_task_id: Option<i32>, // m_iSUOutgoingTask
    pub fail_task_id: Option<i32>,    // m_iFOutgoingTask

    // prerequisites
    pub giver_npc_type: Option<i32>,            // m_iHNPCID
    pub prereq_completed_mission_ids: Vec<i32>, // m_iCSTReqMission
    pub prereq_nano_ids: Vec<i16>,              // m_iCSTRReqNano
    pub prereq_level: Option<i16>,              // m_iCTRReqLvMin
    pub prereq_guide: Option<PlayerGuide>,      // m_iCSTReqGuide
    pub prereq_items: Vec<(i16, usize)>,        // m_iCSTItemID, m_iCSTItemNumNeeded
    pub prereq_running_task_id: Option<i32>,    // m_iCSTTrigger

    // win conditions
    pub time_limit: Option<Duration>,          // m_iCSUCheckTimer
    pub destination_npc_type: Option<i32>,     // m_iHTerminatorNPCID
    pub destination_map_num: Option<u32>,      // m_iRequireInstanceID
    pub req_items: Vec<(i16, usize)>,          // m_iCSUItemID, m_iCSUItemNumNeeded
    pub req_defeat_enemies: Vec<(i32, usize)>, // m_iCSUEnemyID, m_iCSUNumToKill
    pub escort_npc_type: Option<i32>,          // m_iCSUDEFNPCID
}

#[derive(Debug, Clone)]
pub struct Task {
    task_id: i32,
    pub remaining_enemies: Vec<(i32, usize)>,
    pub fail_time: Option<SystemTime>,
    pub completed: bool,
}
impl Task {
    pub fn get_task_def(&self) -> FFResult<&TaskDefinition> {
        tdata_get().get_task_definition(self.task_id)
    }
}
impl From<&TaskDefinition> for Task {
    fn from(task_def: &TaskDefinition) -> Self {
        Task {
            task_id: task_def.task_id,
            remaining_enemies: task_def.req_defeat_enemies.clone(),
            fail_time: task_def.time_limit.map(|d| SystemTime::now() + d),
            completed: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MissionJournal {
    pub current_nano_mission: Option<Task>,
    pub current_guide_mission: Option<Task>,
    pub current_world_missions: [Option<Task>; 4],
    pub active_mission_slot: Option<usize>,
    completed_mission_flags: [i64; SIZEOF_QUESTFLAG_NUMBER as usize],
    repeat_mission_flags: [i64; SIZEOF_REPEAT_QUESTFLAG_NUMBER as usize],
}
impl MissionJournal {
    pub fn get_task_iter(&self) -> impl Iterator<Item = &Task> {
        let mut tasks = Vec::new();
        if let Some(task) = &self.current_nano_mission {
            tasks.push(task);
        }
        if let Some(task) = &self.current_guide_mission {
            tasks.push(task);
        }
        tasks.extend(
            self.current_world_missions
                .iter()
                .filter_map(Option::as_ref),
        );
        tasks.into_iter()
    }

    pub fn get_task_iter_mut(&mut self) -> impl Iterator<Item = &mut Task> {
        let mut tasks = Vec::new();
        if let Some(task) = &mut self.current_nano_mission {
            tasks.push(task);
        }
        if let Some(task) = &mut self.current_guide_mission {
            tasks.push(task);
        }
        tasks.extend(
            self.current_world_missions
                .iter_mut()
                .filter_map(Option::as_mut),
        );
        tasks.into_iter()
    }

    pub fn get_mission_flags(&self) -> [i64; SIZEOF_QUESTFLAG_NUMBER as usize] {
        self.completed_mission_flags
    }

    pub fn get_repeat_mission_flags(&self) -> [i64; SIZEOF_REPEAT_QUESTFLAG_NUMBER as usize] {
        self.repeat_mission_flags
    }

    pub fn is_mission_completed(&self, mission_id: i32) -> bool {
        let chunk = mission_id / 64;
        let offset = mission_id % 64;
        (self.completed_mission_flags[chunk as usize] & (1 << offset)) != 0
    }

    pub fn start_task(&mut self, task: Task) -> FFResult<()> {
        let task_def = task.get_task_def()?;
        let mission_existing_task = self.get_task_iter_mut().find(|t| {
            t.get_task_def()
                .is_ok_and(|def| def.mission_id == task_def.mission_id)
        });
        if let Some(existing_task) = mission_existing_task {
            if !existing_task.completed {
                return Err(FFError::build(
                    Severity::Warning,
                    format!(
                        "Tried to start task {} while another task for mission {} is in progress",
                        task_def.task_id, task_def.mission_id
                    ),
                ));
            }
            *existing_task = task; // replace existing task
        } else {
            let mission_def = tdata_get().get_mission_definition(task_def.mission_id)?;
            let slot = match mission_def.mission_type {
                MissionType::Unknown => {
                    return Err(FFError::build(
                        Severity::Warning,
                        format!(
                            "Tried to start task {} for unknown mission type",
                            task_def.task_id
                        ),
                    ))
                }
                MissionType::Guide => &mut self.current_guide_mission,
                MissionType::Nano => &mut self.current_nano_mission,
                MissionType::Normal => self
                    .current_world_missions
                    .iter_mut()
                    .find(|slot| slot.is_none())
                    .ok_or(FFError::build(
                        Severity::Warning,
                        "No empty world mission slots".to_string(),
                    ))?,
            };
            *slot = Some(task);
        }
        Ok(())
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
