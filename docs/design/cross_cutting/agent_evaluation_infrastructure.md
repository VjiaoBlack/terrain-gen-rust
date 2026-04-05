# Agent Evaluation Infrastructure — Making AI "See" the Game

**Status:** Design — not yet implemented
**Depends on:** `automated_visual_qa.md` (Layer 4-6), `terrain_test_harness.md`
**References:** TITAN (arxiv 2509.22170), VideoGameQA-Bench, NetHack Learning Environment, Anthropic harness design blog, AutoHarness (arxiv 2603.03329)

---

## Problem

AI agents can write code and run tests, but they can't evaluate whether the game is *good*. They don't know if terrain looks natural, if settlements make geographic sense, if the simulation feels alive. Without this, the autonomous dev loop is: health check finds issues → gamedev agent fixes issues → but nobody checks if the game got *better* or *worse* overall.

We need the agent to perceive the game the way a player does — visually, holistically, and qualitatively.

---

## Architecture: Three Perception Channels

### Channel 1: Visual (screenshots → Claude vision)

Claude Code's Read tool already handles images. The game has `--screenshot` mode. This is the most direct path to "the agent sees the game."

**Implementation:**

```bash
# scripts/capture_eval_frames.sh
# Captures PNG screenshots at key moments for visual evaluation

SEEDS="42 137 777"
TICKS="100 1000 5000 10000"

for seed in $SEEDS; do
  for tick in $TICKS; do
    cargo run --release -- --screenshot --seed $seed --width 80 --height 30 \
      --inputs "input:ToggleAutoBuild,tick:$tick" \
      > "eval_frames/seed${seed}_tick${tick}.png" 2>/dev/null
  done
done
```

**What the agent evaluates visually:**
- Does terrain have visible variety (not all one color)?
- Do rivers flow convincingly toward low ground?
- Is the settlement placed sensibly (near water, resources)?
- Are seasons visually distinct across the timelapse?
- Do different seeds produce visibly different worlds?

**Key insight from VideoGameQA-Bench:** VLMs hit 82% on glitch detection from screenshots but struggle with fine UI details. Terminal games with clear color patterns should perform *better* than complex 3D games — less visual noise, more structured layout.

### Channel 2: Structured State Dumps (JSON metrics)

Quantitative data the agent can reason about without vision.

**New flag: `--dump-state`**

```rust
// In game/mod.rs or a new game/diagnostics.rs
pub fn dump_state_json(&self) -> serde_json::Value {
    json!({
        "tick": self.tick,
        "seed": self.seed,
        "season": format!("{:?}", self.day_night.season()),
        "year": self.day_night.year(),
        "population": {
            "total": self.population_count(),
            "villagers": self.villager_count(),
            "predators": self.predator_count(),
            "prey": self.prey_count(),
        },
        "resources": {
            "food": self.resources.food,
            "wood": self.resources.wood,
            "stone": self.resources.stone,
            "planks": self.resources.planks,
            "masonry": self.resources.masonry,
        },
        "buildings": self.building_summary(), // {type: count}
        "terrain": {
            "biome_distribution": self.biome_distribution(), // {type: percentage}
            "water_coverage_pct": self.water_coverage_pct(),
            "avg_elevation": self.avg_elevation(),
            "elevation_std": self.elevation_std(),
        },
        "simulation": {
            "avg_moisture": self.avg_moisture(),
            "avg_vegetation": self.avg_vegetation(),
            "wind_energy": self.wind_energy(),
            "water_volume": self.total_water_volume(),
        },
        "settlement": {
            "footprint_tiles": self.settlement_footprint(),
            "path_connectivity": self.path_connectivity_pct(),
            "defense_coverage": self.defense_coverage_pct(),
            "exploration_pct": self.exploration_pct(),
        }
    })
}
```

**Usage:**
```bash
cargo run --release -- --play --seed 42 --inputs "tick:100,input:ToggleAutoBuild,tick:12000,dump-state"
```

### Channel 3: TITAN-style Perception Abstraction (narrative report)

Convert raw metrics into human-readable assessments that the agent can reason about qualitatively. This bridges the gap between numbers and judgment.

```rust
pub fn generate_report_card(&self) -> String {
    let pop = self.population_count();
    let food = self.resources.food;
    let pop_status = match pop {
        0 => "DEAD",
        1..=3 => "Critical (near extinction)",
        4..=8 => "Fragile (one bad winter away)",
        9..=15 => "Healthy (growing)",
        _ => "Thriving (expanding)",
    };
    let food_status = match (food, pop) {
        (f, p) if p > 0 && f as usize / p < 2 => "Critical (starvation imminent)",
        (f, p) if p > 0 && f as usize / p < 5 => "Tight (no buffer)",
        (f, p) if p > 0 && f as usize / p < 15 => "Adequate",
        _ => "Surplus",
    };
    // ... terrain, defense, simulation health ...
    format!("Population: {} | Food: {} | ...", pop_status, food_status)
}
```

**Why this matters:** The agent doesn't need to figure out "is 3 food per villager good?" every time. The abstraction layer encodes domain knowledge into the perception pipeline, like TITAN's discretization of health into High/Medium/Low.

---

## Evaluation Rubric

The agent scores each playtest against this rubric. Scores are 1-5 per category.

### Terrain Quality (weight: 30%)
1. **River realism** — Do rivers flow downhill? Branch naturally? Carve valleys?
2. **Biome coherence** — Are biomes geographically sensible (snow at high elevation, desert in rain shadow)?
3. **Terrain variety** — Mix of flat, hilly, mountainous? Not monotonous?
4. **Coastal quality** — Clean coastlines, no artifacts, beaches/cliffs where expected?

### Settlement Viability (weight: 30%)
1. **Survival** — Does the settlement survive past year 2?
2. **Growth** — Does population trend upward over time?
3. **Spatial logic** — Farms near settlement, walls facing threats, buildings connected by roads?
4. **Resource balance** — No single resource bottleneck dominating?

### Simulation Health (weight: 20%)
1. **Water conservation** — Total water roughly conserved over time?
2. **Moisture cycle** — Evaporation, transport, precipitation all functioning?
3. **Vegetation response** — Plants grow where wet, die where dry, respond to seasons?
4. **Day/night/seasons** — Visible, affecting gameplay, not just cosmetic?

### Emergent Behavior (weight: 20%)
1. **Seed variety** — Different seeds produce meaningfully different experiences?
2. **Interesting moments** — Close calls, resource crunches, expansion decisions visible?
3. **Narrative quality** — Could you narrate what happened? Does the game "tell a story"?
4. **Surprise** — Anything unexpected (good or bad) happen?

### Scoring:
- **5**: Excellent — would impress a human player
- **4**: Good — works well, minor issues
- **3**: Acceptable — functional but unexciting
- **2**: Poor — noticeable problems
- **1**: Broken — clearly wrong

**Target**: Average score >= 3.5 across 3 seeds before shipping any change.

---

## Golden Seed Baselines

Check in expected metrics for regression detection.

```json
// tests/baselines/seed_42.json
{
  "seed": 42,
  "tick": 12000,
  "expected": {
    "population": {"min": 5, "max": 25},
    "food": {"min": 10, "max": 200},
    "buildings": {"min": 3, "max": 30},
    "biome_types": {"min": 3},
    "water_coverage_pct": {"min": 5.0, "max": 50.0},
    "survived": true
  },
  "tolerance": 0.20,
  "last_updated": "2026-04-05",
  "notes": "Baseline established after SPL erosion merge"
}
```

**Regression rule:** If any metric falls outside `expected ± tolerance`, the playtest fails. Agent must investigate before committing.

---

## Metrics History (trend tracking)

```json
// docs/metrics_history.json
[
  {
    "date": "2026-04-05",
    "commit": "f0b2b3e",
    "seeds": {
      "42": {"population": 12, "food": 45, "buildings": 8, "survived": true},
      "137": {"population": 8, "food": 23, "buildings": 5, "survived": true},
      "777": {"population": 0, "food": 0, "buildings": 2, "survived": false}
    },
    "rubric_avg": 3.2,
    "notes": "Seed 777 settlement died — bad spawn location near predators"
  }
]
```

Agents can see trends: "population has been declining across the last 5 commits" or "seed 777 has never survived — investigate spawn logic."

---

## Implementation Plan

### Phase 1: Capture Pipeline (scripts + CLI flags)
1. `scripts/capture_eval_frames.sh` — renders screenshots as PNGs for 3 seeds at 4 timepoints
2. `--dump-state` flag — JSON state dump at any tick
3. `tests/baselines/` directory with golden seed JSON files
4. `docs/metrics_history.json` — append-only trend log

### Phase 2: Evaluation Pipeline (agent prompt + rubric)
1. Evaluation prompt template that reads screenshots + JSON + rubric
2. Report card generator in Rust (`generate_report_card()`)
3. Integration into health check agent — daily rubric scoring
4. Regression detection: compare against baselines, flag violations

### Phase 3: Gamedev Agent Loop (autonomous improvement)
1. Gamedev agent reads latest health report + evaluation scores
2. Picks lowest-scoring rubric category
3. Writes sprint contract targeting that category
4. Implements, captures new eval frames, re-scores
5. Commits to branch only if rubric score improved or held steady

### Phase 4: Self-Improving Evaluation
1. When human catches something the rubric missed → add rubric item
2. When baseline is consistently exceeded → tighten the baseline
3. When a rubric item never scores below 4 → consider removing (harness hygiene)
4. Track rubric score trends in metrics_history — the rubric itself should improve the game over time

---

## Key Design Decisions

**Why screenshots, not just metrics?**
Metrics catch quantitative regressions. Vision catches qualitative problems — "the terrain looks flat and boring" can't be reduced to a single number. Both channels together give the agent both precision and gestalt.

**Why a rubric, not just pass/fail tests?**
Tests catch bugs (binary: broken or not). The rubric measures *quality* on a spectrum. A settlement that barely survives (score 2) isn't broken, but it's not good either. The rubric gives the gamedev agent a gradient to optimize against.

**Why golden seeds, not random?**
Reproducibility. If seed 42 always produces a viable settlement and seed 777 always dies, we can track whether changes improve the bad case without regressing the good case. Random seeds add noise.

**Why TITAN-style abstraction?**
Raw numbers require domain knowledge to interpret. "Food: 23" means nothing without knowing population. "Food: Tight (no buffer)" is actionable. The abstraction layer encodes domain knowledge once so every agent benefits.
