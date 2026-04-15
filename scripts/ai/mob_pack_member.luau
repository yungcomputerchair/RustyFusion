--[[
    Mob pack member AI.
    Similar to mob AI but syncs target with pack leader
    and follows leader tightly instead of roaming.
]]

local b = require("@lib/behaviors")

return function(npc)
    while true do
        if not b.handle_death(npc) then
            if not b.check_retreat(npc) then
                -- Sync target from leader
                local leader = npc:get_pack_leader()
                if leader and not npc:has_target() then
                    local leader_target = npc:get_entity_target(leader)
                    if leader_target then
                        npc:set_target(leader_target)
                    end
                end

                -- Combat or follow leader
                if npc:has_target() then
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                elseif leader then
                    -- Tight follow: stay at leader position + offset
                    local leader_pos = npc:entity_position(leader)
                    if leader_pos then
                        local offset = npc:get_pack_offset()
                        npc:move_to(
                            leader_pos.x + offset.x,
                            leader_pos.y + offset.y,
                            leader_pos.z + offset.z,
                            npc:run_speed()
                        )
                    end
                end
            end
        end

        yield()
    end
end
