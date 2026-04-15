--[[
    Friendly combatant AI.
    Follows assigned player, syncs target from player's opponent,
    attacks if in range, otherwise follows.
]]

local b = require("@lib/behaviors")

return function(npc: Npc)
    while true do
        if not b.handle_death(npc) then
            local leader = npc:get_follow_target()
            if leader and not leader:is_dead() then
                -- Sync target from the entity we're following
                if not npc:has_target() then
                    local leader_target = leader:get_target()
                    if leader_target then
                        npc:set_target(leader_target)
                    end
                end

                -- Attack or chase target; otherwise follow leader
                if npc:has_target() then
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                else
                    npc:move_toward_entity(leader, npc:walk_speed())
                end
            end
        end

        yield()
    end
end
