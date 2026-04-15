--[[
    Simple follow AI.
]]

return function(npc)
    while true do
        local target = npc:get_follow_target()
        if target then
            local distance = npc:distance_to_entity(target)
            if distance > 1 then
                npc:move_toward_entity(target, distance)
            end
        end
        yield()
    end
end
