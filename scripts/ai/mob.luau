--[[
    Standard mob AI.
    Handles death/respawn, retreat, target scanning, combat, and idle roaming.
    Uses gradual aggro accumulation for target selection.
]]

local mob_base = require("@lib/mob_base")

return function(npc: Npc)
    mob_base.run(npc)
end
