local ticks = 0

local function tick()
    ticks = ticks + 1
    if ticks == 100 then
        print("ONE HUNDRED TICKS!")
    end
end

table.insert(onTick, tick)