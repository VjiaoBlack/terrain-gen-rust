-- Default AI script - read-only access to game state
-- This demonstrates the Lua scripting API
--
-- Available globals (updated each tick by the engine):
--   villager_count  (number)
--   wolf_count      (number)
--   season          (string: "Spring", "Summer", "Autumn", "Winter")
--   resources       (table: .food, .wood, .stone, .planks, .masonry, .grain, .bread)
--
-- Available hooks (define these functions to be called by the engine):
--   on_tick()       -- called each game tick

function on_tick()
    if resources.food < 10 then
        print("[Lua] Warning: food is low! (" .. resources.food .. ")")
    end
    if wolf_count > 5 and season == "Winter" then
        print("[Lua] Danger: " .. wolf_count .. " wolves in winter!")
    end
end
