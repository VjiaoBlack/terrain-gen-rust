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
