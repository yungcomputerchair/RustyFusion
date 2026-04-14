use crate::{
    chunk::TickMode,
    entity::{Entity, NPC},
    enums::CombatantTeam,
    tabledata::tdata_get,
};

fn get_script_name(npc: &NPC) -> Option<&'static str> {
    let is_combatant = npc.as_combatant().is_some();
    if !is_combatant {
        return None;
    }

    let stats = tdata_get().get_npc_stats(npc.ty).unwrap();
    match stats.team {
        CombatantTeam::Friendly => Some("friendly_combatant"),
        CombatantTeam::Mob => {
            if npc.tight_follow.is_some() {
                Some("mob_pack_member")
            } else {
                Some("mob")
            }
        }
        _ => None,
    }
}

pub fn make_for_npc(npc: &NPC, force: bool) -> (Option<String>, TickMode) {
    let stats = tdata_get().get_npc_stats(npc.ty).unwrap();
    if !force && npc.path.is_none() && stats.ai_type == 0 {
        return (None, TickMode::Never);
    }

    let behavior_name = get_script_name(npc);
    let tick_mode = if npc.path.is_some() {
        TickMode::Always
    } else {
        TickMode::WhenLoaded
    };

    (behavior_name.map(|name| name.to_string()), tick_mode)
}
