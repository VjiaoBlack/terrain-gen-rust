-- Pacifist strategy script
-- Focuses on food production and avoidance over military
--
-- Monitors food and population, warns when wolves approach.

function on_tick()
    if wolf_count > 2 then
        print("[Lua] FLEE: " .. wolf_count .. " wolves detected, stay near buildings")
    end
    if resources.food < 15 then
        print("[Lua] FARM: Food critical (" .. resources.food .. "), focus on farming")
    end
    if villager_count > 10 and resources.grain < 5 then
        print("[Lua] PRESERVE: Large population needs grain reserves for winter")
    end
end

function on_event()
    if event_name == "plague" then
        print("[Lua] HEAL: Plague! Build a bakery to prevent future outbreaks")
    elseif event_name == "blizzard" then
        print("[Lua] SHELTER: Blizzard! Ensure huts are built for all villagers")
    end
end
