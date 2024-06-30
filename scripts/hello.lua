local done = false

local function tick()
    if not done then
        print("Hello from entity ".. entity_id .. "!")
        done = true
    end
end

table.insert(onTick, tick)