--[[
    Friendly combatant AI.
    Follows assigned player, syncs target from player's opponent,
    attacks if in range, otherwise follows.
]]

local b = require("@lib/behaviors")

return function(npc)
    while true do
        -- 1. Handle death & respawn
        if not b.handle_death(npc) then
            -- 2. Attack current target if we have one
            if npc:has_target() then
                if not b.try_attack(npc) then
                    b.chase_target(npc)
                end
            end

            -- Friendly NPCs don't scan; they get targets from
            -- the player they follow (handled on the Rust side
            -- via SyncPlayerTarget / loose_follow).
        end

        yield()
    end
end
