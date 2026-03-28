-- Defend First strategy script
-- Prioritizes garrison and wall construction
--
-- This script demonstrates monitoring game state and providing
-- strategic recommendations via print output.

function on_tick()
    if wolf_count > 3 and resources.wood > 15 and resources.stone > 15 then
        print("[Lua] DEFEND: Build garrison! " .. wolf_count .. " wolves nearby")
    end
    if season == "Winter" and resources.food < 20 then
        print("[Lua] DEFEND: Winter food crisis! Stockpile before building")
    end
end

function on_event()
    if event_name == "wolf_surge" then
        print("[Lua] ALERT: Wolf surge detected! Prioritize walls and garrisons")
    elseif event_name == "bandit_raid" then
        print("[Lua] ALERT: Bandits incoming! Check stockpile after raid")
    end
end
