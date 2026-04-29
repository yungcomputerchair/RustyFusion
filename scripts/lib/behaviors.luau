--[[
    Shared NPC behavior functions.
    All functions take an NPC handle as the first argument.
    Functions that yield (wait/yield) will suspend the coroutine.
]]

local behaviors = {}

-- Death & respawn
-- Returns true if the NPC was dead and handled
function behaviors.handle_death(npc: Npc)
    if not npc:is_dead() then
        return false
    end

    npc:begin_death()
    wait(2.0) -- brief non-interruptible delay before despawn

    npc:despawn()
    wait(npc:regen_time()) -- non-interruptible wait for regen time

    npc:respawn() -- handles target reset, healing to full, and appearing at spawn
    return true
end

-- Retreat to spawn if target is dead, gone, or too far from spawn.
-- After retreat, fully heals the NPC (matching native reset behavior).
-- Returns true if retreating (caller should skip other behaviors).
function behaviors.check_retreat(npc: Npc)
    local target = npc:target()
    if not target then
        return false
    end

    local spawn = npc:spawn_position()
    local combat_range = npc:combat_range()
    local should_retreat = false

    if target:is_dead() then
        -- Target dead or gone
        should_retreat = true
    else
        -- NPC too far from spawn
        local npc_dist = npc:distance_to(spawn.x, spawn.y, spawn.z)
        if npc_dist > combat_range then
            should_retreat = true
        else
            -- Target too far from spawn
            local tpos = target:position()
            if tpos then
                local dx = tpos.x - spawn.x
                local dy = tpos.y - spawn.y
                local dz = tpos.z - spawn.z
                if math.sqrt(dx*dx + dy*dy + dz*dz) > combat_range then
                    should_retreat = true
                end
            end
        end
    end

    if not should_retreat then
        return false
    end

    -- Retreat to spawn
    npc:clear_target()
    npc:set_retreating(true)
    npc:move_to(spawn.x, spawn.y, spawn.z, npc:run_speed() * 2)
    while npc:is_moving() do
        yield()
    end
    npc:reset() -- full heal + clear state
    return true
end

-- Scan for nearest enemy within sight range (simple version).
-- Sets the target if found.
function behaviors.scan_for_target(npc: Npc)
    if npc:target() then
        return
    end

    local enemy = npc:find_nearest_enemy(npc:sight_range())
    if enemy then
        npc:set_target(enemy)
    end
end

-- Scan for enemies within aggro distance.
function behaviors.scan_for_target_aggro(npc: Npc)
    if npc:target() then
        return
    end

    local sight_range = npc:sight_range()
    local npc_pos = npc:position()
    local enemies = npc:find_enemies_in_range(sight_range)

    -- TODO: Make this more dynamic
    for _, enemy in ipairs(enemies) do
        local epos = enemy:position()
        if epos then
            local dx = epos.x - npc_pos.x
            local dy = epos.y - npc_pos.y
            local dz = (epos.z - npc_pos.z) * 2 -- z counts double to discourage vertical aggro
            local distance = math.sqrt(dx*dx + dy*dy + dz*dz)

            if distance <= sight_range then
                npc:set_target(enemy)
                return
            end
        end
    end
end

-- Attack the current target if in range.
-- Returns true if an attack was performed.
-- Clears target if it is dead or gone.
function behaviors.try_attack(npc: Npc)
    local target = npc:target()
    if not target then
        return false
    end

    if target:is_dead() then
        npc:clear_target()
        return false
    end

    if not npc:in_attack_range() then
        return false
    end

    npc:attack()
    wait(npc:delay_time(), function()
        -- Interrupt delay if npc dies
        return npc:is_dead()
    end)
    return true
end

-- Chase the current target at run speed.
function behaviors.chase_target(npc: Npc)
    local target = npc:target()
    if not target then
        return
    end

    npc:move_toward_entity(target, npc:run_speed())
end

-- Idle roam: pick a random point near spawn, walk to it, wait.
-- Returns early if the NPC dies or acquires a target.
-- Optional on_tick callback is called each tick (e.g. to scan for targets).
function behaviors.idle_roam(npc: Npc, on_tick: (() -> ())?)
    local idle_range = npc:idle_range()
    if idle_range <= 0 then
        yield()
        return
    end

    local roam_radius = idle_range / 2
    local point = random_point_in_range(npc:spawn_position(), roam_radius)
    npc:move_to(point.x, point.y, point.z)
    while npc:is_moving() do
        if on_tick then on_tick() end
        if npc:is_dead() or npc:target() then return end
        yield()
    end

    -- Random delay before next roam (interruptible)
    local delay_max = npc:delay_time() * 10
    local delay_ticks = math.ceil(math.random() * delay_max * 8)
    for i = 1, delay_ticks do
        if on_tick then on_tick() end
        if npc:is_dead() or npc:target() then return end
        yield()
    end
end

return behaviors
