# Milestone Notifications

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Last updated: 2026-04-01*

## Problem

The current milestone system has five entries (FirstWinter, TenVillagers, FirstGarrison, FiveYears, TwentyVillagers) and each one reads like a phase announcement: "Milestone: 10 villagers!" This tells the player where they are on a progress bar, not what happened in their settlement's story. The messages are generic -- the same five milestones fire on every seed, in roughly the same order, with no connection to the terrain, the economy, or the choices the simulation made.

Worse, milestones currently drive threat scaling (`threat_level += 0.5`), coupling notification flavor text to game mechanics. The threat_scaling design doc proposes decoupling this by replacing milestone-driven threat_level with a continuous threat score. That frees milestones to be purely narrative -- they mark the game arc (Pillar 3) by naming what the settlement achieved, not what phase it entered.

The goal from `game_design.md` (Pillar 3, Rich tier): "Milestone notifications that name what happened, not what phase you're in. 'First stone deposit discovered' not 'Entering Phase 2'."

## Goals

1. Milestones narrate the settlement's story by naming concrete events: a discovery, a construction, a survival, a threshold crossed.
2. Each milestone fires at most once per game. The full set spans the Explore-Expand-Exploit-Endure arc without enforcing it.
3. Messages are short, specific, and celebratory. They read like chapter titles of this settlement's history.
4. Milestones have a brief visual presentation that stands out from regular event log messages without blocking gameplay.
5. The milestone list covers terrain discovery, population, buildings, production chains, seasonal survival, and threats -- reflecting the full breadth of the simulation.

## Non-Goals

- Milestones do not gate gameplay. No features are locked behind milestones.
- Milestones do not drive threat scaling (threat_scaling.md handles that via continuous threat score).
- No achievement UI, trophy case, or persistent cross-game tracking. Milestones are per-run narrative markers.
- No sound design (future work, possibly when terminal audio is explored).
- No milestone for every building type or resource threshold -- the list should be curated, not exhaustive.

## Design

### Milestone List

18 milestones organized by the game arc phase they typically occur in. The phase labels are for designer reference only -- they never appear in-game.

#### Explore Phase (early game, discovery and first steps)

| # | Milestone | Trigger Condition | Message |
|---|-----------|-------------------|---------|
| 1 | FirstWoodGathered | Settlement stockpile receives its first unit of wood | "First timber hauled back to camp!" |
| 2 | FirstStoneFound | A villager discovers a stone deposit (stone tile enters SettlementKnowledge.known_stone) | "Stone deposit discovered!" |
| 3 | FirstFarm | A Farm building completes construction | "First farm planted -- food from the land!" |
| 4 | FirstHut | A Hut building completes construction | "First hut built -- settlers have shelter!" |
| 5 | FirstWinterSurvived | Year >= 1 (the settlement has passed through winter with at least 1 villager alive) | "First winter survived!" |

#### Expand Phase (mid-early game, growth and infrastructure)

| # | Milestone | Trigger Condition | Message |
|---|-----------|-------------------|---------|
| 6 | PopulationTen | Villager count >= 10 | "Population reached 10 -- a real village now!" |
| 7 | FirstWorkshop | A Workshop building completes construction | "Workshop built -- planks now available!" |
| 8 | FirstSmith | A Smithy building completes construction | "Smithy built -- stone becomes masonry!" |
| 9 | FirstRoad | A Road tile is placed (by auto-build traffic system) | "A footpath has worn into the earth!" |
| 10 | FiveBuildings | Total completed buildings (excluding Stockpile and Road) >= 5 | "Five structures standing -- the village takes shape!" |

#### Exploit Phase (mid-late game, production and prosperity)

| # | Milestone | Trigger Condition | Message |
|---|-----------|-------------------|---------|
| 11 | PopulationTwentyFive | Villager count >= 25 | "Population reached 25!" |
| 12 | FirstGranary | A Granary building completes construction | "Granary built -- grain stores for winter!" |
| 13 | FirstBakery | A Bakery building completes construction | "Bakery built -- bread on the table!" |
| 14 | FirstPlank | Settlement stockpile receives its first plank (workshop output) | "First plank produced -- refined goods!" |
| 15 | HundredFood | Stockpile food count >= 100 | "Food stores reached 100 -- plenty for now!" |

#### Endure Phase (late game, defense and survival under pressure)

| # | Milestone | Trigger Condition | Message |
|---|-----------|-------------------|---------|
| 16 | FirstGarrison | A Garrison building completes construction | "Garrison built -- the village can defend itself!" |
| 17 | RaidSurvived | A wolf surge or bandit raid is repelled (defense_rating >= raid_strength) with no villager deaths during the event | "Raid repelled -- not a single soul lost!" |
| 18 | PopulationFifty | Villager count >= 50 | "Population reached 50 -- a thriving settlement!" |

### Trigger Mechanics

Each milestone is tracked as an enum variant in `Milestone`, stored in `DifficultyState.milestones` (a `Vec<Milestone>`). The existing `check_milestones()` method in `events.rs` runs once per tick after ECS systems complete.

**Check pattern** (unchanged from current code):
```rust
let check = |m: Milestone, milestones: &[Milestone]| !milestones.contains(&m);

if condition && check(Milestone::Variant, &self.difficulty.milestones) {
    self.difficulty.milestones.push(Milestone::Variant);
    self.notify_milestone("Message text here");
}
```

**New triggers that don't exist today:**

- **FirstWoodGathered / FirstPlank / HundredFood**: Check stockpile resource counts. The stockpile entity already has `ResourceStore` component with wood, stone, food, planks, etc. Compare against thresholds.
- **FirstStoneFound**: Check if `SettlementKnowledge.known_stone` transitions from empty to non-empty. Track a `stone_was_unknown: bool` flag in DifficultyState or simply check `known_stone.len() >= 1` against the milestone.
- **FirstRoad**: Check if any tile on the map has `Terrain::Road` type. Or hook into the road auto-build logic in `build.rs`.
- **FiveBuildings**: Count entities with `Building` component, excluding Stockpile and Road types.
- **RaidSurvived**: Set a flag during raid resolution. When a raid event resolves with `defense_rating >= raid_strength`, check villager count before and after. If no deaths occurred during the raid window, fire the milestone.

**Removed from current system:**
- `threat_level += N` lines are removed from milestone checks. Threat scaling is decoupled per threat_scaling.md.
- `FiveYears` milestone is removed. Calendar milestones ("you waited long enough") don't name what happened. The same arc moment is better captured by PopulationFifty or RaidSurvived.

### Visual Presentation

Milestones need to feel distinct from regular event log entries ("Drought has ended", "Wolf surge from the north!") without requiring a modal popup or blocking input.

#### Notification Banner

When a milestone fires, display a single-line banner across the top of the viewport for 120 ticks (~4 seconds at normal speed, ~2 seconds at 2x). The banner replaces the normal top status bar temporarily.

```
+---------------------------------------------------------------------------+
|                  * First farm planted -- food from the land! *             |
+---------------------------------------------------------------------------+
|                          (normal game view below)                         |
```

**Rendering details:**
- Banner background: distinct color from the game viewport. In Map mode, use inverted colors (bright text on dark background). In Landscape mode, use a warm gold/amber tone.
- Asterisk/star decorators (`*`) flanking the message. Simple, not flashy.
- The message also appends to the event log so the player can review it later.
- Banner fades after 120 ticks: first 80 ticks at full brightness, then dims over the remaining 40 ticks (reduce color intensity each tick in Landscape mode; in Map mode, switch from bold to normal text at tick 80).

#### Event Log Entry

In addition to the banner, the milestone message is pushed to `events.event_log` with a `[*]` prefix to distinguish it from regular events:

```
[*] First farm planted -- food from the land!
```

Regular events use no prefix or `[!]` for threats. The `[*]` prefix lets the player scan the log and spot milestones.

#### Implementation: `notify_milestone()`

Add a new method alongside the existing `notify()`:

```rust
pub fn notify_milestone(&mut self, msg: String) {
    // Set banner state for renderer
    self.milestone_banner = Some(MilestoneBanner {
        message: msg.clone(),
        ticks_remaining: 120,
    });
    // Also push to event log with prefix
    self.events.event_log.push(format!("[*] {}", msg));
}
```

```rust
pub struct MilestoneBanner {
    pub message: String,
    pub ticks_remaining: u32,
}
```

The renderer checks `game.milestone_banner` each frame. If `Some` and `ticks_remaining > 0`, draw the banner. Decrement `ticks_remaining` each tick in `step()`. When it hits 0, set to `None`.

### Milestone Enum Update

Replace the current 5-variant enum with the full 18:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Milestone {
    FirstWoodGathered,
    FirstStoneFound,
    FirstFarm,
    FirstHut,
    FirstWinterSurvived,
    PopulationTen,
    FirstWorkshop,
    FirstSmith,
    FirstRoad,
    FiveBuildings,
    PopulationTwentyFive,
    FirstGranary,
    FirstBakery,
    FirstPlank,
    HundredFood,
    FirstGarrison,
    RaidSurvived,
    PopulationFifty,
}
```

Serialization compatibility: existing save files have the old enum variants. The deserializer should map `FirstWinter -> FirstWinterSurvived`, `TenVillagers -> PopulationTen`, `TwentyVillagers -> PopulationTwentyFive`. Add `#[serde(alias = "FirstWinter")]` annotations or handle in `serialize.rs`.

## Implementation Plan

### Phase 1: Expand Milestone Enum and Decouple from Threat Level

- Replace the 5-variant `Milestone` enum with the full 18-variant version.
- Remove all `threat_level += N` lines from `check_milestones()`.
- Update existing milestone checks to use new variant names.
- Add serde aliases for backward compatibility with old save files.
- All existing tests pass with updated variant names.

**Files:** `src/game/mod.rs` (Milestone enum, DifficultyState), `src/game/events.rs` (check_milestones)

### Phase 2: Add New Trigger Conditions

- Add checks for FirstWoodGathered, FirstStoneFound, FirstFarm, FirstHut, FirstWorkshop, FirstSmith, FirstRoad, FiveBuildings, FirstGranary, FirstBakery, FirstPlank, HundredFood, PopulationTwentyFive, PopulationFifty.
- Building-completion milestones: query for Building component with matching BuildingType where construction is complete.
- Resource milestones: query the stockpile entity's ResourceStore.
- RaidSurvived: add a `raid_survived_clean: bool` transient flag to EventSystem, set during raid resolution, checked in `check_milestones()`.

**Files:** `src/game/events.rs` (check_milestones), `src/game/mod.rs` (EventSystem if adding raid flag)

### Phase 3: Milestone Banner Rendering

- Add `MilestoneBanner` struct and `milestone_banner: Option<MilestoneBanner>` field to `Game`.
- Add `notify_milestone()` method.
- Update `step()` to decrement banner ticks.
- Update `draw_panel()` in `render.rs` to render the banner when active.
- Banner uses inverted colors in Map mode, warm amber in Landscape mode.

**Files:** `src/game/mod.rs` (Game struct, step), `src/game/render.rs` (banner drawing)

### Phase 4: Event Log Prefix

- Change `notify_milestone()` to push `[*]` prefixed messages to `event_log`.
- Update `draw_panel()` event log rendering to color `[*]` entries differently from regular entries (gold/yellow vs white).

**Files:** `src/game/render.rs`

## Testing Strategy

**Unit tests:**
- Each milestone fires exactly once (push the same condition twice, verify `milestones.len()` doesn't double).
- Milestone fires when condition is met (create game state with 10 villagers, call `check_milestones()`, verify `PopulationTen` is in the list).
- Milestone does NOT fire when condition is not met (9 villagers, no PopulationTen).
- `threat_level` is unchanged after milestones fire (verify decoupling).
- MilestoneBanner ticks down to zero and becomes None.
- Serde round-trip: serialize a game with old milestone names, deserialize into new enum.

**Integration tests:**
- Run 500-tick simulation, verify at least FirstWoodGathered and FirstHut fire (basic settlement always gathers wood and builds huts).
- Run 2000-tick simulation, verify milestones appear in event_log with `[*]` prefix.
- Verify milestone order is plausible: FirstWoodGathered before FirstPlank, FirstFarm before HundredFood.

**Regression tests:**
- Existing milestone test (`milestone_first_winter_detected`) updated for new variant name `FirstWinterSurvived`.
- Save/load round-trip preserves all milestone state.

## Open Questions

1. **Should milestones pause the simulation briefly?** A 0.5-second pause when a milestone fires would draw attention, similar to how Factorio pauses on research completion. But this conflicts with the "observable simulation" pillar -- pausing breaks the flow of watching systems run. Leaning toward no pause, banner only.

2. **Per-seed milestone variety.** All 18 milestones are the same across seeds. Should there be terrain-conditional milestones? "River crossing bridged!" (only on maps with rivers near settlement), "Mountain pass fortified!" (only on maps with chokepoints). This would make the milestone sequence feel unique per seed but increases complexity. Propose as future work after the base 18 are implemented.

3. **Milestone log as game summary.** At game end (settlement destroyed or player quits), display the milestone list as a timeline: "Tick 45: First timber hauled back to camp. Tick 312: First winter survived. Tick 1540: Raid repelled." This would be a lightweight way to tell the settlement's story. Low-cost addition once milestones track their trigger tick.

4. **Banner duration at high speed.** At 5x speed, 120 ticks is less than 1 second of real time. Should banner duration be real-time (always 4 seconds) rather than tick-based? This means tracking elapsed wall-clock time, which the game doesn't currently do. Alternative: scale banner ticks with game speed (`120 * speed_multiplier`).

## References

- `src/game/mod.rs` -- Milestone enum, DifficultyState, Game struct, `check_milestones()` call site
- `src/game/events.rs` -- `check_milestones()` implementation, `notify()`, event_log
- `src/game/render.rs` -- `draw_panel()`, event log rendering
- `src/ecs/components.rs` -- BuildingType, Species, ResourceStore
- `docs/game_design.md` -- Pillar 3 (Explore-Expand-Exploit-Endure), Pillar 4 (observable simulation)
- `docs/design/threat_scaling.md` -- threat score decoupling from milestones
