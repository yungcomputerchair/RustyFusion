--[[
    Standard mob AI.
    Handles death/respawn, retreat, target scanning, combat, and idle roaming.
]]

local b = require("@lib/behaviors")

return function(npc)
    while true do
        -- 1. Handle death & respawn
        if not b.handle_death(npc) then
            -- 2. Retreat if too far from spawn
            if not b.check_retreat(npc) then
                -- 3. Scan for targets
                b.scan_for_target(npc)

                -- 4. Combat or idle
                if npc:has_target() then
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                else
                    b.idle_roam(npc)
                end
            end
        end

        yield()
    end
end
