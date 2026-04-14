--[[
    Mob pack member AI.
    Similar to mob AI but syncs target with pack leader
    and follows leader tightly instead of roaming.
]]

local b = require("@lib/behaviors")

return function(npc)
    while true do
        -- 1. Handle death & respawn
        if not b.handle_death(npc) then
            -- 2. Retreat if too far from spawn
            if not b.check_retreat(npc) then
                -- 3. Combat (target is synced from leader on the Rust side)
                if npc:has_target() then
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                end

                -- Pack members don't idle roam; they follow their
                -- leader via tight_follow (handled on the Rust side).
            end
        end

        yield()
    end
end
