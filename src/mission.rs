use std::time::Duration;

use crate::{enums::*, net::packet::*};

#[derive(Debug)]
pub struct TaskDefinition {
    pub task_id: i32,                 // m_iHTaskID
    pub mission_id: i32,              // m_iHMissionID
    pub repeatable: bool,             // m_iRepeatflag
    pub giver_npc_type: Option<i32>,  // m_iHNPCID
    pub task_type: TaskType,          // m_iHTaskType
    pub success_task_id: Option<i32>, // m_iSUOutgoingTask
    pub fail_task_id: Option<i32>,    // m_iFOutgoingTask

    // prerequisites
    pub prereq_completed_mission_ids: Vec<i32>, // m_iCSTReqMission
    pub prereq_nano_ids: Vec<i16>,              // m_iCSTRReqNano
    pub prereq_level: Option<i16>,              // m_iCTRReqLvMin
    pub prereq_guide: Option<PlayerGuide>,      // m_iCSTReqGuide
    pub prereq_items: Vec<(i16, usize)>,        // m_iCSTItemID, m_iCSTItemNumNeeded
    pub prereq_task_id_in_active_mission: Option<i32>, // m_iCSTTrigger

    // win conditions
    pub time_limit: Option<Duration>,          // m_iCSUCheckTimer
    pub destination_npc_type: Option<i32>,     // m_iHTerminatorNPCID
    pub destination_map_num: Option<u32>,      // m_iRequireInstanceID
    pub req_items: Vec<(i16, usize)>,          // m_iCSUItemID, m_iCSUItemNumNeeded
    pub req_defeat_enemies: Vec<(i32, usize)>, // m_iCSUEnemyID, m_iCSUNumToKill
    pub escort_npc_type: Option<i32>,          // m_iCSUDEFNPCID
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Task {
    task_id: i32,
    target_npc_ids: [i32; 3],
    target_npc_counts: [i32; 3],
    target_item_ids: [i32; 3],
    target_item_counts: [i32; 3],
}
impl From<Task> for sRunningQuest {
    fn from(value: Task) -> Self {
        Self {
            m_aCurrTaskID: value.task_id,
            m_aKillNPCID: value.target_npc_ids,
            m_aKillNPCCount: value.target_npc_counts,
            m_aNeededItemID: value.target_item_ids,
            m_aNeededItemCount: value.target_item_counts,
        }
    }
}
impl From<Option<Task>> for sRunningQuest {
    fn from(value: Option<Task>) -> Self {
        if let Some(task) = value {
            return task.into();
        }

        Self {
            m_aCurrTaskID: 0,
            m_aKillNPCID: [0, 0, 0],
            m_aKillNPCCount: [0, 0, 0],
            m_aNeededItemID: [0, 0, 0],
            m_aNeededItemCount: [0, 0, 0],
        }
    }
}
