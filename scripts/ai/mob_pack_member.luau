--[[
    Mob pack member AI.
    Similar to mob AI but:
    - Syncs retreat state with pack leader
    - Bi-directional target sync with leader
    - Follows leader tightly with speed scaled by distance
    - Falls back to standard mob behavior if leader is dead/gone
]]

local b = require("@lib/behaviors")
local mob_base = require("@lib/mob_base")

return function(npc: Npc)
    local leader = npc:get_pack_leader()
    if not leader or leader:is_dead() then
        return mob_base.run(npc)
    end

    local was_leader_retreating = false

    while true do
        -- Leader died; switch to regular mob AI for the rest of this life
        if leader:is_dead() then
            return mob_base.run(npc)
        end

        if not b.handle_death(npc) then
            -- Leader retreat sync
            local leader_retreating = leader:retreating()
            if leader_retreating then
                npc:set_retreating(true)
                npc:clear_target()
            elseif was_leader_retreating then
                npc:reset()
            end
            was_leader_retreating = leader_retreating

            if not b.check_retreat(npc) then
                -- Bi-directional target sync
                if not leader_retreating then
                    local leader_target = leader:get_target()
                    if not npc:has_target() then
                        if leader_target then
                            npc:set_target(leader_target)
                        end
                    else
                        if not leader_target then
                            local my_target = npc:get_target()
                            if my_target then
                                leader:set_target(my_target)
                            end
                        end
                    end
                end

                b.scan_for_target_aggro(npc)

                if npc:has_target() then
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                else
                    -- Tight follow: leader position + offset, speed scales with distance
                    local leader_pos = leader:position()
                    if leader_pos then
                        local offset = npc:get_pack_offset()
                        local tx = leader_pos.x + offset.x
                        local ty = leader_pos.y + offset.y
                        local tz = leader_pos.z + offset.z
                        local dist = npc:distance_to(tx, ty, tz)
                        local tolerance = npc:attack_range() / 2
                        if dist > tolerance then
                            local max_speed = npc:run_speed() * 2
                            local speed = math.min(dist, max_speed)
                            npc:move_to(tx, ty, tz, speed)
                        end
                    end
                end
            end
        end

        yield()
    end
end
