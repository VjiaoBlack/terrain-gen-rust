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

## 2026-03-31 (Run 3) — Automated Playtest Report

**Build:** release  
**Auto-build:** enabled (ToggleAutoBuild at tick 100)  
**Display size:** 70×25

### Per-Game Summary

| | Game 1 (seed 42) | Game 2 (seed 137) | Game 3 (seed 999) |
|---|---|---|---|
| **Terrain biome** | Grassland/forest | Desert/sandy + grassland | Sandy/desert flatlands |
| **Ticks run** | 36,000 | 36,000 | 40,000 |
| **Final season** | Winter Y1 D1 | Winter Y1 D1 | Winter Y1 D4 |
| **Final pop** | 191 | 160 | 136 |
| **Food** | 2,551 | 579 | 492 |
| **Wood** | 698 | 21,010 | 26,235 |
| **Stone** | 548 | 0 | 3 |
| **Planks/Masonry/Grain/Bread** | — | — | — |
| **Buildings visible** | Many huts + farms + stockpile | 3+ hut clusters + stockpile | 2 hut clusters + stockpile |
| **Wolves** | 0 | 0 | 0 |
| **Rabbits** | 0 | 0 | 0 |
| **Events** | Blizzard, New villager born | Bountiful harvest | None notable |
| **Survived** | Yes — thriving | Yes | Yes |

**Population progression:**
- Game 1 (seed 42): 71 → 131 → 191 (strong sustained growth)
- Game 2 (seed 137): 70 → 130 → 160 (steady — best showing yet for this seed)
- Game 3 (seed 999): 109 → 136 (growth despite near-zero stone)

**Skill levels observed:**
- Game 1 (tick 36k): Farm **100.0** (capped!), Mine 68.6, Wood 77.1
- Game 3 (tick 20k): Farm 59.3, Mine 23.1, Wood 95.4, Build 41.7

---

### What Seems Fun

- **Farm skill reaching 100.0 is a satisfying milestone:** Game 1 showed `Farm 100.0` in the skills panel — the first confirmed skill cap across all playtests. Seeing villagers achieve maximum competence in a field creates a sense of progression. It also meaningfully changes the economy: at Farm 100, harvest yields are presumably maximized, which explains the exceptional food surplus (2551) in Game 1.

- **Game 1 shows the ideal state:** Pop 191 with 2551 food, 548 stone, and wood held at a balanced 698 (consumed as fast as it's gathered because stone enabled active building). The map at tick 36k shows farms (##), multiple hut clusters (██░░██), a `V`-symbol entity (possibly a skilled worker mid-task), and a settlement filling out its terrain. This is what the game *should* feel like, and it's genuinely compelling even in ASCII.

- **Bountiful harvest event in Game 2:** The event banner `Bountiful harvest! Farm yields doubled.` appeared at tick 24k in Game 2, right when stone ran out — a bittersweet moment where food is booming but construction has completely stalled. The juxtaposition of agricultural abundance against resource poverty feels narratively interesting.

- **Game 3 recovered vs. Run 2 crash:** In Run 2, seed 999 crashed from 95 to 57 pop. This run, the same seed reached 136 — a 51-villager swing in the same direction. This cross-run variance is striking and discussed below.

---

### What Seems Broken

1. **Same-seed non-determinism is a critical bug:** Seed 999 produced pop 116 (Run 1), pop 57 (Run 2), and pop 136 (Run 3) — a 79-villager spread across identical seeds. Seeds are supposed to make runs reproducible, but the AI's random decisions or event RNG is not seeded consistently. This makes bug reproduction difficult and undermines any attempt to balance around specific seeds.

2. **Stone stays at 0–3 on sandy/desert seeds (confirmed across 9 game-runs):** Games 2 and 3 again end with stone 0 and 3 respectively. However, **the wood-stone balance reveals the root cause more clearly this run**: Game 1 (stone 548) consumed wood steadily (698 total), while Games 2 and 3 with stone-0 let wood run to 21k–26k. When stone is available, auto-build consumes wood. When stone is gone, wood accumulates indefinitely. The stone shortage is the single root cause of both the wood problem and the building stagnation.

3. **Farm skill cap at 100.0 may lock out other skills:** Game 1's `Farm 100.0 / Mine 68.6 / Wood 77.1` at tick 36k is all from the same population of 191 villagers. If skill points are shared or if high farming skill biases AI toward farm tasks, villagers with Farm 100 may be permanently assigned to farm duty and never mine — which would explain why stone acquisition is slower than wood even on stone-rich maps.

4. **Zero rabbits and zero wolves — 9/9 runs confirmed:** Every game run in all three playtest sessions shows `Rabbits: 0` and `Wolves: 0` in all frames. This is 100% reproducible and cannot be coincidence. Both prey and predator spawning appear completely non-functional. Likely causes: spawn code is guarded by a condition never met (terrain type? time of day? resource threshold?), or animal entities are immediately dying on spawn.

5. **Secondary production chains never activate — 9/9 runs confirmed:** Planks, masonry, grain, and bread remain at zero across every single run. The Workshop/Smithy/Granary/Bakery chain has never been observed. With stone=0 on most seeds, Workshop (needs 8w+3s) and Smithy (5w+8s) cannot be built. Even Game 1 with stone 548 showed no planks or masonry — auto-build either doesn't prioritize processing buildings or they require conditions not met.

6. **`.` symbols appear in Game 1 Winter map:** At tick 36k, the map shows `.` characters at several positions that were empty in Autumn (e.g., `''''''''''.'''`, `''''''''''::.:`). These are new this run and weren't seen in Runs 1–2. Not documented in CLAUDE.md. May be snow particles, frost/ice overlay, or dead villager markers from the Blizzard event. Combined with the previously noted `*` symbols, there are now two undocumented map characters.

7. **`V` symbol appears briefly in Game 1 Autumn map:** At tick 24k, a `V` character appears at position `""V♣` (near forest/berry tiles). Gone by Winter. This may be a villager shown mid-action (movement vector?) or an undocumented entity/building state. Not listed in CLAUDE.md terrain/entity symbols.

8. **Frame duplication persists — Run 3 confirms it on all 3 games:** The final frame is printed twice with identical tick numbers, populations, and resource values in every game. Three separate playtest runs, nine games, all affected. This is a systematic bug in the `--play` output mode.

---

### What Could Be Improved

1. **Auto-build should attempt to build processing buildings:** Workshop costs 8w+3s — with 698 wood and 548 stone in Game 1, there was clearly enough to build Workshops. Instead, only huts and farms appear on the map. Auto-build's building priority queue apparently never reaches Workshop, or the trigger condition (enough planks/masonry?) creates a deadlock.

2. **Stone-poor seeds need a floor:** A minimum of 2–3 stone deposits guaranteed within foraging distance of the start point would transform Game 2 and 3 from "survives but stagnates" into "can eventually build processing buildings." The terrain already varies by biome — stone deposits should be biome-weighted, not purely random.

3. **Seeding audit for gameplay RNG:** The 79-villager swing across three runs of seed 999 suggests the game's AI event RNG (wolf events, random wandering decisions, birth/death rolls) is not seeded from the map seed. All randomness that affects gameplay replayability should be deterministic from the map seed when running `--play`.

4. **`--play` mode needs a resource-over-time log:** Currently the only data visible is the snapshot at each `frame` call. A summary line showing `tick:20000 pop:109 food:642 wood:3231 stone:3 events:[]` for every 1000 ticks would enable much richer analysis without lengthening terminal output significantly.

5. **Skill specialization needs a rebalancing cap:** Farm 100.0 while Mine is 68.6 and Wood 77.1 suggests villagers over-invest in farming once they start. A "diminishing returns above 80" system or a hard cap on time spent per task type would ensure all skills see use, preventing mono-skill populations.

6. **Confirm and document undocumented map symbols:** `.` and `V` need to be either documented in CLAUDE.md or removed. If `.` is a snow/blizzard effect it should be mentioned. If `V` is a villager state icon it should be listed alongside the terrain legend.

---

### Priority Recommendation

**Blocking — same issues as Runs 1 and 2, still unfixed:**
1. Stone regeneration (mountain mining / extra deposits) — confirmed broken across **9/9** runs
2. Rabbit and wolf spawning — **0 occurrences across 9 game-runs**; clearly non-functional
3. Gameplay non-determinism — same-seed runs diverge by 79 pop; breaks reproducibility

**High — directly blocks progression:**
4. Auto-build processing buildings (Workshop/Smithy) — Farm skill maxed, production chains never start
5. Wood accumulation sink — still hits 20k–26k on stone-poor seeds with no remedy

**Medium — balance/feel:**
6. Farm skill overcap and mono-specialization (Farm 100.0 while Mine lags)
7. Minimum stone deposit guarantee near settlement spawn
8. Document `.` snow overlay and `V` entity symbols in CLAUDE.md

**Low — polish:**
9. Fix last-frame-duplicate output in `--play` mode (9/9 games affected)
10. Add resource-over-time telemetry to `--play` mode output

**Cross-run comparison summary:** Seed 42 (grassland) has produced the best result in all three runs: pop growing 70→191 with food surplus and balanced resources when stone is available. Seed 137 (desert) consistently hits stone=0 by tick 24k and wood runaway by Winter. Seed 999 (sandy) is non-deterministic — either crashing hard (Run 2: pop 57) or surviving (Runs 1/3: pop 116–136) based on what appears to be uncontrolled variance. The game is strongest when stone is plentiful. Fixing stone supply would make the median experience match the best-case experience.

