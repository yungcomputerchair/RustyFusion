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
    wait(2.0) -- brief delay before despawn

    npc:despawn()
    wait_noint(npc:regen_time()) -- non-interruptible wait for regen time

    -- Reset position to spawn and respawn
    local spawn = npc:spawn_position()
    npc:set_position(spawn.x, spawn.y, spawn.z)
    npc:respawn()
    return true
end

-- Retreat to spawn if target is dead, gone, or too far away.
-- Returns true if retreating (caller should skip other behaviors).
function behaviors.check_retreat(npc: Npc)
    if not npc:has_target() then
        return false
    end

    local spawn = npc:spawn_position()
    local dist_to_spawn = npc:distance_to(spawn.x, spawn.y, spawn.z)
    if dist_to_spawn <= npc:combat_range() then
        return false
    end

    -- Too far from spawn; retreat
    npc:clear_target()
    npc:set_retreating(true)
    npc:move_to(spawn.x, spawn.y, spawn.z, npc:run_speed() * 2)
    while npc:is_moving() do
        yield()
    end
    npc:set_retreating(false)
    return true
end

-- Scan for nearest enemy within sight range.
-- Sets the target if found.
function behaviors.scan_for_target(npc: Npc)
    if npc:has_target() then
        return
    end

    local enemy = npc:find_nearest_enemy(npc:sight_range())
    if enemy then
        npc:set_target(enemy)
    end
end

-- Attack the current target if in range.
-- Returns true if an attack was performed.
-- Clears target if it is dead or gone.
function behaviors.try_attack(npc: Npc)
    if not npc:has_target() then
        return false
    end

    if not npc:is_target_alive() then
        npc:clear_target()
        return false
    end

    if not npc:in_attack_range() then
        return false
    end

    npc:attack()
    wait(npc:delay_time())
    return true
end

-- Chase the current target at run speed.
function behaviors.chase_target(npc: Npc)
    if not npc:has_target() then
        return
    end

    npc:move_toward_target(npc:run_speed())
end

-- Follow the assigned path (for NPCs with preset routes).
-- Ticks the path forward one step.
function behaviors.follow_path(npc: Npc)
    -- The path is ticked on the Rust side via move_to;
    -- for path-following NPCs the path is managed natively.
    -- This is a no-op yield to let the native path tick happen.
    yield()
end

-- Idle roam: pick a random point near spawn, walk to it, wait.
-- Returns early if the NPC dies or acquires a target.
function behaviors.idle_roam(npc: Npc)
    local idle_range = npc:idle_range()
    if idle_range <= 0 then
        yield()
        return
    end

    local roam_radius = idle_range / 2
    local point = npc:random_point_in_range(roam_radius)
    npc:move_to(point.x, point.y, point.z, npc:walk_speed())
    while npc:is_moving() do
        if npc:is_dead() or npc:has_target() then return end
        yield()
    end

    -- Random delay before next roam (interruptible)
    local delay_max = npc:delay_time() * 10
    local delay_ticks = math.ceil(math.random() * delay_max * 8)
    for i = 1, delay_ticks do
        if npc:is_dead() or npc:has_target() then return end
        yield()
    end
end

return behaviors
