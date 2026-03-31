# Playtest Notes

---

## 2026-03-31 — Automated Playtest Report

**Build:** release  
**Auto-build:** enabled (ToggleAutoBuild at tick 100)  
**Display size:** 70×25

### Per-Game Summary

| | Game 1 (seed 42) | Game 2 (seed 137) | Game 3 (seed 999) |
|---|---|---|---|
| **Terrain biome** | Grassland/forest | Desert/shrubland | Sandy flatlands |
| **Ticks run** | 36,000 | 36,000 | 40,000 |
| **Final season** | Winter Y1 D1 | Winter Y1 D1 | Winter Y1 D4 |
| **Final pop** | 189 | 92 | 116 |
| **Food** | 1,955 | 156 | 686 |
| **Wood** | 6,844 | 19,389 | 30,978 |
| **Stone** | 1,530 | 0 | 3 |
| **Planks/Masonry/Grain/Bread** | — | — | — |
| **Buildings visible** | ~6 huts + stockpile | ~2 huts + stockpile | ~1 hut + stockpile |
| **Wolves** | 0 | 0 | 0 |
| **Rabbits** | 0 | 0 | 0 |
| **Events** | None notable | Drought + Blizzard | Drought + 1 death |
| **Survived** | Yes | Yes (barely) | Yes |

**Mid-run population progression (Game 1):** 70 → 130 → 189 (healthy growth)  
**Mid-run population progression (Game 2):** 80 → 92 → 92 (stalled from autumn onward)  
**Mid-run population progression (Game 3):** 111 → 116 (near-stagnant over 20k ticks)

**Skill levels observed (Game 3, tick 20k):** Farm 65.2 | Mine 20.0 | Wood 95.4

---

### What Seems Fun

- **Emergent population explosion on a good map (seed 42):** Starting from 70 villagers, the game grew to 189 in roughly one in-game year with no player intervention. Watching the cluster of `██░░██` hut symbols spread across the map as population pressures triggered new builds felt organic and satisfying.
- **Seasonal tension is real:** The Summer→Autumn→Winter progression creates a natural narrative arc. Seeing food drop from 1466 to 1955 (growing through winter) in Game 1 vs. barely 156 food in Game 2 makes the terrain type feel like a meaningful strategic variable.
- **Event system adds flavor:** The Drought event in Games 2 and 3 (halving farm yields mid-run), the Blizzard slowing movement, and the single villager death notice all felt like genuine moments even in headless play. These events land better than expected.
- **Skill differentiation:** Game 3 showing Farm 65.2 vs. Mine 20.0 vs. Wood 95.4 suggests villagers are actually specializing over time, which is the right design direction.

---

### What Seems Broken

1. **Stone hard-caps at near-zero on most maps:** Game 2 ended with 0 stone, Game 3 ended with 3. Only Game 1 (grassland with presumably richer stone deposits) accumulated meaningful stone (1,530). This confirms the economy_design.md note: "Only 2 stone deposits = 10 stone" — the finite stone model is genuinely broken at scale. With pop 92+ you need stone constantly for buildings; stalling at 0 blocks all construction.

2. **Wood massively over-accumulates:** All three games show runaway wood stockpiling — 6,844 / 19,389 / 30,978 by end of year 1. With no processing buildings being built (no Workshop/Smithy visible), wood has no sink. Villagers are gathering wood at full speed with nowhere to spend it.

3. **Auto-build built almost nothing in Games 2 and 3:** Games 2 and 3 show only 1–2 huts + stockpile even at 36–40k ticks with 92–116 villagers. The stone shortage is almost certainly blocking auto-build from proceeding past initial huts. Huts cost 10w+4s — 4 stone per hut is expensive when stone total is 0.

4. **No rabbits or prey on any map:** All games show `Rabbits: 0` throughout. Either rabbit spawning is suppressed or they're dying off immediately. This removes a potential food and meat source.

5. **No wolves at all:** `Wolves: 0` in all three games across all frames, including winter. The game should have wolf surge events in winter, but none triggered. Either the event system's RNG thresholds weren't met or the wolf surge event is not firing.

6. **Population stagnates without stone:** Games 2 and 3 effectively hit a ceiling (92 and 116) once stone ran out. Without stone, auto-build can't place huts, and without huts villagers have no housing — a classic blocker cycle.

7. **Final frame printed twice:** Each game outputs the last frame with identical data twice in a row (same tick number, same resource values). This is a minor display artifact but could confuse players reading terminal output.

8. **No secondary resources appear:** Planks, masonry, grain, and bread all remain at zero across all games. Workshop and Smithy are never built (blocked by stone shortage), so no production chains ever activate.

---

### What Could Be Improved

1. **Infinite/renewable stone is the #1 fix** (already noted in economy_design.md): Mountain tiles should yield stone continuously at slow rate, or stone deposits should spawn more frequently (4–6 per map). Without this, Games 2 and 3 are unwinnable for non-grassland seeds.

2. **Wood needs a sink:** 30,000+ wood serves no purpose. Introduce a Lumber Mill or expand Workshop throughput so wood converts into planks faster. Alternatively, cap wood gathering by distance or add diminishing returns as piles grow.

3. **Auto-build should show its queue:** When watching headless play, it's impossible to know _why_ auto-build isn't placing buildings. A "waiting for X resource" indicator in the panel would make stalls diagnosable.

4. **Stone priority in gathering AI:** When stone drops to near-zero, all idle villagers should prioritize stone gathering over wood. Currently the 95:20 Wood:Mine skill ratio in Game 3 suggests gathering is wood-biased regardless of stockpile balance.

5. **Rabbit and prey spawning audit:** Three games, zero rabbits. Either spawn rates need to be confirmed working, or prey density needs tuning. Rabbits would provide both food variety and a predator/prey dynamic that's currently entirely absent.

6. **Wolf events need tuning:** Even a single winter with 0 wolves removes all threat. The grace period mechanic is good (Year 1 = no wolves per economy_design.md proposal), but it's unclear if Year 1 protection is triggering correctly given we're only seeing Y1 play.

7. **Seed-dependent terrain fairness:** Game 1 (grassland) vastly outperformed Games 2 and 3 (sandy/desert). Consider ensuring every seed has at least 2–3 accessible stone deposits near the starting area, so early game isn't luck-dependent.

---

### Priority Recommendation

**Immediate (breaks the game):**
1. Fix stone supply — infinite mountain mining or more/richer deposits per map
2. Diagnose rabbit spawning — prey population is completely absent

**High (blocks progression):**
3. Add a wood processing sink — wood accumulates to absurd levels with no use
4. Audit auto-build stone requirements — confirm it correctly signals "waiting for stone"

**Medium (balance/feel):**
5. Wolf event system — verify winter surge fires in Year 1 or clarify the grace period
6. Skill rebalancing — Wood skill dominance (95.4) vs. Mine (20.0) needs attention

**Low (polish):**
7. Fix last-frame-duplicate output artifact
8. "Waiting for resource X" auto-build status panel

The core loop (place buildings → villagers self-organize → population grows) works beautifully on good seeds. The stone bottleneck is the single largest barrier to fun on average seeds.

---

## 2026-03-31 (Run 2) — Automated Playtest Report

**Build:** release  
**Auto-build:** enabled (ToggleAutoBuild at tick 100)  
**Display size:** 70×25

### Per-Game Summary

| | Game 1 (seed 42) | Game 2 (seed 137) | Game 3 (seed 999) |
|---|---|---|---|
| **Terrain biome** | Grassland/forest | Sandy/desert | Sandy flatlands |
| **Ticks run** | 36,000 | 36,000 | 40,000 |
| **Final season** | Winter Y1 D1 | Winter Y1 D1 | Winter Y1 D4 |
| **Final pop** | 138 | 124 | 57 |
| **Food** | 1,781 | 458 | 111 |
| **Wood** | 11,629 | 19,950 | 10,151 |
| **Stone** | 408 | 2 | 0 |
| **Planks/Masonry/Grain/Bread** | — | — | — |
| **Buildings visible** | ~6+ huts + farms + stockpile | ~2 huts + stockpile | ~2 huts + stockpile |
| **Wolves** | 0 | 0 | 0 |
| **Rabbits** | 0 | 0 | 0 |
| **Events** | 1 death, Bountiful harvest | None | None |
| **Survived** | Yes | Yes (borderline) | Collapsing |

**Population progression:**
- Game 1 (seed 42): 83 → 105 → 138 (steady, healthy growth)
- Game 2 (seed 137): 69 → 124 → 124 (stalled completely — same pop between Autumn and Winter)
- Game 3 (seed 999): 95 → **57** (catastrophic crash, lost 38 villagers between Summer and Winter)

**Skill levels observed (Game 3, tick 20k):** Farm 61.3 | Mine 24.0 | Wood 95.4 | Build 47.1

---

### What Seems Fun

- **Good seeds deliver genuine settlement expansion:** Game 1 (seed 42) visually shows hut clusters spreading across the map with ██░░██ icons multiplying across the terrain, farm plots (##) appearing, and population growing from 83 to 138 in one in-game year. The spatial density of buildings on a favorable map feels like a thriving village.
- **Bountiful harvest event is a rewarding surprise:** Seeing "Bountiful harvest! Farm yields doubled." mid-game on Game 1 created a momentary boom (food 1181 → 1781 at Winter). Positive events like this give the player something to root for even in headless play.
- **Skill specialization is consistently visible:** Game 3 at tick 20k shows Farm 61.3 / Wood 95.4 / Mine 24.0 — the same asymmetric pattern as the previous playtest run. Villagers are genuinely specializing over time, which creates the intended feel of a living economy even if it's currently imbalanced.
- **Terrain variety produces very different game feels:** Grassland (seed 42) vs. desert/sandy (seeds 137, 999) are visually distinct and mechanically different. The `;;` desert character vs. `''` grassland creates an immediate read of "this map will be harder."

---

### What Seems Broken

1. **Game 3 population crash is severe and new:** Previous run had seed 999 ending at 116 pop; this run hit 57 — a 51% decline from Summer peak. Food fell from 611 to 111 with 57 villagers surviving. This is likely starvation in a stone-0 map where auto-build couldn't build enough huts for housing (huts need 4 stone), leaving many villagers unsheltered going into winter and dying from cold/hunger. The crash is so severe (−38 pop) it may indicate a death spiral rather than a soft decline.

2. **Stone still hits zero and stays there on most maps:** Games 2 and 3 both end at stone 0–2. Game 2 had 14 stone at Summer tick 12k and 2 stone by Autumn — so stone is being consumed but not replenished. This reproduces the confirmed issue from the previous run with no fix in place.

3. **Game 2 population frozen at 124 for 12,000 ticks:** Between Autumn (tick 24k) and Winter (tick 36k), Game 2 shows identical population (124) and food (458). This is not just stagnation — it looks like a complete freeze in births/deaths/resources. Food value doesn't change at all. Either the villager AI is deadlocked waiting for stone to build huts and no one is hungry enough to die, or there is a simulation freeze condition when stone=2.

4. **No rabbits or prey on any map (confirmed across 6 game runs):** Every single game run across both playtest sessions shows Rabbits: 0 in all frames. This is a consistent, reproducible absence — rabbit spawning appears non-functional.

5. **No wolves in any game (confirmed across 6 runs):** Wolves: 0 throughout all runs. The winter surge event did not fire in any game across either playtest session.

6. **`*` symbols appear on Game 3 map at tick 40k:** Two `*` characters appear at positions that were empty at tick 20k. These do not correspond to any documented terrain or entity symbol in CLAUDE.md. Could be dead villager markers, abandoned structures, or a display bug. Worth investigating.

7. **No secondary production chains ever activate:** Planks, masonry, grain, and bread remain at zero in all 6 game-runs. Workshop and Smithy require stone and planks respectively — both are blocked by stone shortage. The full production chain (Workshop → Smithy → Bakery → bread) has never been observed in any automated playtest.

8. **Frame duplication bug persists:** Each game's final frame is printed identically twice (same tick number, same resource values). Confirmed across both playtest runs.

---

### What Could Be Improved

1. **Winter survivability for stone-poor maps needs a floor:** The Game 3 collapse (95 → 57 pop) likely stems from a shortage of huts forcing villagers to sleep outdoors in freezing weather. A minimum hut count or emergency shelter auto-build using wood-only (no stone) would prevent death spirals on stone-scarce starts.

2. **Stone regeneration or guaranteed starting deposits is critical:** Three separate seeds across two playtest runs confirm stone depletion by mid-game on non-grassland maps. Even a slow passive stone income from mountain tiles (as proposed in economy_design.md) would be a significant improvement. The "infinite mountain mining" design is clearly necessary.

3. **Population stall at stone=2 needs root cause analysis:** Game 2's complete freeze (identical pop/food for 12,000 ticks) suggests either a deadlock or an edge case in the villager AI when stone is near-zero. All villagers may be in a "waiting to build a hut but can't mine stone" loop.

4. **Wood-to-stone gathering ratio needs rebalancing:** Across all 6 runs, wood always massively over-accumulates (11k–30k) while stone hits zero. The gathering priority clearly over-weights wood. Even when stone is critically low, villagers continue depositing 3–7 wood per event message visible in the log, with stone deposits absent. The "critical gather (stone < 5)" priority rule from economy_design.md is either not implemented or not overriding wood bias.

5. **Rabbit population needs an audit:** Zero rabbits across 6 runs is 100% reproducibility. Either spawn code is unreachable, spawn conditions are never met (no suitable terrain?), or the population cap starts at 0. A simple forced spawn at game start for testing would confirm the system works.

6. **`*` map symbols need documentation or investigation:** The unknown `*` character needs to be identified — if it's a new entity type or marker, it should be listed in CLAUDE.md.

---

### Priority Recommendation

**Blocking — fix before next playtest cycle:**
1. Stone regeneration (mountain mining or extra deposits) — confirmed broken across 6/6 runs
2. Wood vs. stone gathering priority — confirmed imbalanced across 6/6 runs
3. Rabbit spawning audit — 0 rabbits across 6/6 runs is a clear bug

**High — directly causes player loss:**
4. Winter shelter auto-build using wood-only (no stone) emergency huts to prevent death spirals
5. Investigate Game 2 population freeze condition (stone=2, pop frozen 12k ticks)

**Medium — balance and feel:**
6. Wolf event system verification — winter surge never fired across 6 runs
7. Identify `*` map character appearing in Game 3 at tick 40k

**Low — polish:**
8. Fix last-frame-duplicate output artifact
9. "Waiting for resource X" auto-build status indicator

Comparing both playtest runs: seed 42 is reliable (83→138 pop, stable food, significant building spread) and demonstrates the game loop works on good terrain. Seeds 137 and 999 consistently fail due to stone depletion. The game is currently bimodal: great on grassland seeds, unwinnable on desert/sandy seeds. Fixing stone supply would dramatically improve the median experience.
