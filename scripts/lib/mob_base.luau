local b = require("@lib/behaviors")

local mob_base = {}

function mob_base.run(npc: Npc)
    while true do
        if not b.handle_death(npc) then
            if not b.check_retreat(npc) then
                b.scan_for_target_aggro(npc)

                local target = npc:get_target()
                if target then
                    if not b.try_attack(npc) then
                        b.chase_target(npc)
                    end
                else
                    b.idle_roam(npc, function()
                        b.scan_for_target_aggro(npc)
                    end)
                end
            end
        end

        yield()
    end
end

return mob_base