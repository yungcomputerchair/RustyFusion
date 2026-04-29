--[[
    Simple follow AI.
]]

return function(npc: Npc)
    while true do
        local target = npc:get_follow_target()
        if target then
            npc:move_toward_entity(target)
        end
        yield()
    end
end
