--[[
    Friendly combatant AI.
    Follows assigned player, syncs target from player's opponent,
    attacks if in range, otherwise follows.
]]

local b = require("@lib/behaviors")

return function(npc: Npc)
    while true do
        if not b.handle_death(npc) then
            local target = npc:get_target()
            if target then
                if target:is_dead() then
                    npc:clear_target()
                else
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                end
            else
                -- no target
                local leader = npc:get_follow_target()
                if leader and not leader:is_dead() then
                    -- follow leader
                    local distance = npc:distance_to_entity(leader)
                    npc:move_toward_entity(leader, distance)

                    -- sync target from leader
                    local leader_target = leader:target()
                    if leader_target then
                        npc:set_target(leader_target)
                    end
                end
            end
        end -- handle_death

        yield()
    end
end
