# Seasonal Pressure as Rhythm

*Design doc for terrain-gen-rust*
*Status: Proposed*
*Last updated: 2026-04-01*

## Problem

Seasons exist mechanically but do not create distinct gameplay rhythms. The current `SeasonModifiers` struct adjusts five floats (`rain_mult`, `evap_mult`, `veg_growth_mult`, `hunger_mult`, `wolf_aggression`) and the event system gates certain events by season (drought in summer, migration in spring, wolves in winter). But these are invisible multipliers. A player watching the simulation cannot feel the difference between mid-spring and mid-summer without reading the date string. There is no shift in what villagers DO, what the player should WORRY about, or what the settlement LOOKS like.

The Pillar 3 "Rich" tier calls for: *Seasonal pressure creates rhythm within the arc (spring = expand, summer = exploit, autumn = prepare, winter = endure).* The simulation_chains doc establishes that effects should flow through physical world state, not flat multipliers. This doc defines what each season feels like -- mechanically, visually, and emotionally -- so that a player can tell the season by watching the settlement for five seconds.

### Current seasonal effects

| Season | Mechanical | Visual | Player Feeling |
|--------|-----------|--------|----------------|
| Spring | rain 1.5x, veg growth 2.0x, migration event | Green tint (if landscape mode) | Indistinguishable from autumn |
| Summer | rain 0.5x, evap 2.0x, drought event | Warm tint | "Drought happened" or nothing |
| Autumn | veg growth 0.3x, bountiful harvest event | Orange tint | Slightly different tint |
| Winter | rain 0.3x, hunger 2.5x, wolves, blizzard | White/blue tint, snow | "Villagers are dying" |

Winter is the only season with genuine pressure. The other three feel like palette swaps.

## Goals

1. Each season has a distinct dominant activity -- what most villagers are doing changes visibly with the season.
2. Each season has a distinct risk -- the thing that can go wrong is different per season.
3. Each season has a distinct visual signature readable at a glance, even in map mode.
4. Seasonal transitions feel like events, not just modifier swaps.
5. The annual cycle creates a preparation loop: what you do THIS season determines whether you survive NEXT season.

## Non-Goals

- Calendar-gated tech unlocks or building restrictions (seasons constrain through simulation, not rules).
- Weather as a separate complex system (rain, wind, temperature as individual simulated values -- future work).
- Micro-seasons or variable-length seasons (keep the clean 10-day/season structure).
- Biome-specific season behavior (tropical vs tundra seasons -- future work after base rhythm is solid).

## Design

### The Annual Rhythm

Each season lasts 10 days (current system: 10 days * 24 hours/day * 0.02 hours/tick = ~12,000 ticks per season). The rhythm is a four-beat cycle:

```
SPRING        SUMMER        AUTUMN        WINTER
Expand        Exploit       Prepare       Endure
───────────── ───────────── ───────────── ─────────────
Snowmelt      Peak output   Harvest       Cold & dark
New growth    Drought risk  Stockpiling   Scarcity
Flood risk    Fire risk     Migration out Wolves
Migrants in   Long days     Short days    Blizzards
```

### Spring: Expansion

**Theme:** The world wakes up. Water everywhere. Growth everywhere. Opportunity everywhere. Danger: too much water.

**Mechanical changes:**

| System | Spring Behavior | Current | Change Needed |
|--------|----------------|---------|---------------|
| Water | Snowmelt pulse: water levels spike in first 3 days of spring | rain_mult 1.5x (flat) | Add snowmelt burst: water_level += snow_accumulation on spring day 0-2, then normal rain |
| Flooding | River plains flood from snowmelt; low-elevation tiles near water become temporarily unwalkable | No flooding | Requires flood threshold from simulation_chains doc |
| Soil | Flood recession deposits fertility on previously flooded tiles (+0.05 per flood event) | No soil system | Requires SoilFertilityMap from simulation_chains doc |
| Vegetation | Rapid regrowth: veg_growth_mult 2.0x (already exists) | Already works | No change needed |
| Farms | Planting season: farms placed in spring get a 1.2x initial growth bonus (soil is moist) | Flat growth rate | New: spring_planting_bonus applied to newly placed farms |
| Migration | Migrants arrive if housing + food available (already exists, 20% per check) | Already works | Increase to 30% in spring; spring is THE migration season |
| Villager activity | Exploration bias: idle villagers prefer exploring over gathering in spring (new land revealed by snowmelt recession) | No seasonal behavior bias | New: AI behavior weight shift -- exploration priority +0.3 in spring |
| Construction | Building speed normal; ground is soft (no penalty, no bonus) | No seasonal modifier | No change needed |

**Snowmelt mechanic (new):**

Winter accumulates a `snow_level` per tile (see Winter section). On the first three days of spring, snow converts to water:

```
water_level[tile] += snow_level[tile] * 0.33 per day  (melts over 3 days)
snow_level[tile] -= snow_level[tile] * 0.33 per day
```

Tiles at low elevation near rivers receive the most snowmelt runoff. This creates the spring flood pulse. By day 4, snow is gone, floods recede, and the fertile mud remains.

**Risk: Spring Floods**

Heavy snowmelt causes flooding on low-lying tiles near rivers. Flooded tiles destroy unharvested farms and damage buildings (per simulation_chains Chain 2). The tension: river-adjacent land is the most fertile (alluvial soil) AND the most flood-prone. Players who farm the flood plain get the best yields but risk losing spring crops.

**Observable indicators:**
- Water tiles visibly expand along rivers in early spring (WaterMap levels rise above terrain)
- Snow tiles transition to mud/wet terrain over the first few days
- Migrant villagers arrive visibly from map edges and walk toward settlement
- Vegetation "pops" -- trees go from winter bare to full canopy over the season
- Idle villagers fan out from settlement center (exploration bias)

**Emotional beat:** Relief after winter. Optimism. "We survived. Now let's grow."

### Summer: Exploitation

**Theme:** The machine runs at full speed. Long days, peak production, everything works. Danger: it gets too dry and things catch fire.

**Mechanical changes:**

| System | Summer Behavior | Current | Change Needed |
|--------|----------------|---------|---------------|
| Water | Evaporation dominates: streams shrink, ponds dry up, moisture drops | evap_mult 2.0x (already exists) | Amplify: shallow water tiles (depth < 0.3) can fully evaporate, converting to dry terrain temporarily |
| Farms | Peak growth: farms grow at 1.3x base rate (long days, warm soil) | Flat rate modified only by drought event | New: summer_growth_bonus 1.3x applied to all farms |
| Production | Workshops/smithies/bakeries run at full speed (already baseline) | No seasonal modifier | No change -- summer IS the baseline. Other seasons are slower. |
| Day length | Longest days: villagers have more active hours before night rest | No day length variation | New: day_hours modifier -- summer gives 16h daylight vs 12h base |
| Drought | Drought event: rain drops to near zero, water levels crash, moisture plummets | Already exists as yield multiplier | Refactor per simulation_chains: drought modifies rain_rate and evap_rate, effect flows through water->moisture->crops |
| Fire | New: forest fire risk on tiles where vegetation > 0.7 AND moisture < 0.2 | Does not exist | New system: fire_risk check per 50 ticks on eligible tiles, probability scales with dryness |
| Wolf activity | Minimal: wolves retreat to deep forest, rarely seen near settlement | wolf_aggression 0.95 (barely different from spring) | Reduce to 0.5 -- summer is the safe season for predators. Wolves spawn further away. |

**Fire mechanic (new):**

During summer, tiles with high vegetation and low moisture can ignite. Fire spreads to adjacent tiles with vegetation > 0.5. Fire destroys vegetation (sets VegetationMap to 0.0), kills trees (removes tree entities), and threatens buildings.

```
fire_check per 50 ticks:
  for each tile where vegetation > 0.7 AND moisture < 0.2:
    ignition_chance = (1.0 - moisture) * 0.002  // ~0.2% per check for bone-dry forest
    if ignited:
      spread to adjacent tiles with vegetation > 0.5
      fire burns for 20 ticks per tile
      vegetation -> 0.0
      buildings on fire tiles take damage
```

Fire clears land (creating natural clearings), destroys potential wood supply, and threatens settlement edges near forests. But burned land has exposed soil for future farming -- fire is both disaster and opportunity.

**Drought-fire chain:**

```
Drought event fires
  -> rain_rate drops, evaporation spikes
    -> water levels fall, moisture plummets
      -> farms slow (simulation_chains Chain 1)
      -> fire risk spikes (moisture < 0.2 across large areas)
        -> forest fire ignites
          -> wood supply destroyed, buildings threatened
            -> double crisis: food AND wood shortage
```

This is the emergent multi-system crisis that Pillar 2 demands. No special-case "drought causes fire" code -- the chain flows through moisture.

**Observable indicators:**
- Water tiles visibly shrink (ponds become smaller, streams narrow)
- Terrain color shifts warm -- golden-brown tones on grass, deeper greens on irrigated areas
- Farms show dense growth (visual: filled-in crop characters vs sparse spring planting)
- Fire appears as bright red/orange characters spreading through forests
- Long shadows in evening (sun is high and days are long)
- Settlement is busy -- more villagers active during extended daylight

**Emotional beat:** Productivity. Pride. "Look at this machine hum." Then anxiety if drought hits: "Please rain."

### Autumn: Preparation

**Theme:** The countdown begins. Everything you harvest now is what you eat in winter. Migration outward. Frantic stockpiling.

**Mechanical changes:**

| System | Autumn Behavior | Current | Change Needed |
|--------|----------------|---------|---------------|
| Farms | Harvest season: all mature farms auto-harvest with 1.5x yield bonus | Bountiful Harvest event (20% chance, 2.0x) | Replace random event with guaranteed autumn harvest bonus. Autumn IS harvest season, not a lucky roll. |
| Vegetation | Decay begins: veg_growth_mult 0.3x (already exists), forests thin | Already works | No change to growth mult. Add: trees drop foliage (visual only, amber/red palette shift) |
| Granary | Granary production priority increases -- villagers prefer granary work over other processing | No seasonal priority | New: autumn_stockpile_priority -- villagers with access to granary + food prioritize grain processing |
| Migration out | New: some prey animals migrate off-map (deer herds leave), reducing hunting opportunities | Prey exists year-round | New: prey entities have autumn migration behavior -- 30% of prey walk toward map edge and despawn |
| Wolf scouting | Wolves begin approaching: wolf_aggression rises from summer's 0.5 to 0.8 | wolf_aggression 0.8 (already exists) | No change to aggression value. Add: wolves spawn closer to settlement edge (scouting range tightens from 30-35 to 20-30 tiles) |
| Construction | Last chance to build: construction speed normal, but players should feel urgency | No seasonal modifier | No mechanical change -- the urgency comes from knowing winter is next |
| Day length | Days shorten: 10h daylight (vs 16h summer) | No day length variation | New: reduced active hours, villagers return home earlier |

**Harvest mechanic refinement:**

Instead of a random "Bountiful Harvest" event, autumn has a guaranteed harvest window. Farms that reached maturity during spring/summer auto-harvest in autumn with a 1.5x yield bonus. Farms that were planted late (summer) may not be mature yet -- they get no bonus and risk dying in winter unharvested.

This creates the preparation tension: did you plant enough, early enough? A settlement that expanded farms in spring reaps the reward in autumn. One that was distracted by expansion or fire recovery scrambles.

**Prey migration (new):**

Prey animals (deer, rabbits) begin migrating off-map in autumn. Each prey entity gets a 30% chance per autumn of entering migration behavior (walk toward nearest map edge, despawn on arrival). By winter, the map has significantly fewer wild food sources. This forces reliance on stored food and makes the granary-bakery chain critical.

```
autumn prey migration:
  for each prey entity:
    if rng < 0.3 AND not already_migrating:
      set behavior = MigrateOffMap(nearest_edge)
      // prey walks to edge, despawns
  // result: ~30% fewer prey by winter
  // spring: new prey spawn to replenish (existing spawn logic)
```

**Observable indicators:**
- Vegetation palette shifts to amber, orange, red (in landscape mode) or thinner/sparser characters (in map mode)
- Farms show harvest activity -- villagers carrying large food bundles back to stockpile
- Deer herds visibly moving toward map edges in groups
- Wolf silhouettes visible at settlement periphery (scouting, not yet attacking)
- Days noticeably shorter -- night arrives earlier, villagers head home sooner
- Granary and bakery show heavy activity (smoke, worker presence)

**Emotional beat:** Urgency. "Do I have enough? Is the granary full? Are the walls repaired?" The ticking clock is visceral because winter's consequences are known.

### Winter: Endurance

**Theme:** Survive. Everything contracts. The world is hostile. Every stored resource matters.

**Mechanical changes:**

| System | Winter Behavior | Current | Change Needed |
|--------|----------------|---------|---------------|
| Hunger | 2.5x hunger rate (already exists) | Already works | No change to multiplier |
| Farms | Farms do not grow. Period. Any unharvested crop is destroyed by frost. | veg_growth_mult 0.0 (farms still have flat growth in code) | Enforce: farm growth_rate = 0.0 in winter regardless of other factors |
| Snow | New: snow accumulates on tiles, reduces movement speed, creates snowmelt fuel for spring | Snow terrain exists at gen-time but no runtime snow | New: snow_level per tile increases by 0.1/day in winter; tiles with snow_level > 0.3 get snow movement penalty (0.4x speed) |
| Movement | Blizzard halves speed (already exists as event); base winter movement 0.7x | Blizzard is 0.5x event, no base winter penalty | New: base_movement_mult 0.7x in winter (stacks with blizzard to 0.35x) |
| Wolves | Peak aggression: wolf_aggression 0.6 means they attack from closer, more frequently | Already exists | Increase wolf surge chance from 25% to 35% per 100-tick check; wolves spawn closer (15-25 tiles) |
| Day length | Shortest days: 8h daylight | No day length variation | New: minimal active hours, long dangerous nights |
| Food | No farms, no prey (migrated), only stockpile. Granary/bakery chain is the lifeline. | Hunger multiplier exists but food sources don't actually dry up | Prey migration (autumn) + farm freeze (winter) means stored food is the ONLY source |
| Building damage | New: buildings without maintenance slowly degrade in winter (frost damage) | Buildings are permanent | New: each building loses 1 durability per 5 winter days if no villager visits it. Abandoned outposts decay. |
| Villager behavior | Hunker mode: villagers stay close to huts and stockpile, minimize outdoor time | No seasonal behavior bias | New: AI behavior weight shift -- gathering range contracts by 50%, villagers prefer tasks near buildings |

**Snow accumulation (new):**

Snow is a per-tile float that builds during winter and melts in spring. It serves three purposes: movement penalty, visual indicator, and spring flood fuel.

```
pub struct SnowMap {
    width: usize,
    height: usize,
    snow: Vec<f64>,  // 0.0 (no snow) to 1.0 (deep snow)
}

winter tick:
  for each land tile:
    snow[tile] += 0.01 per tick  // ~1.2 per season at current tick rate
    snow[tile] = snow[tile].min(1.0)
    // elevation bonus: higher tiles accumulate faster
    snow[tile] += elevation_factor * 0.005

spring tick:
  // snowmelt (described in Spring section)
  water[tile] += snow[tile] * melt_rate
  snow[tile] -= snow[tile] * melt_rate
```

Snow accumulation is higher at higher elevations (mountain passes become impassable in deep winter) and lower near buildings (villagers clear paths -- traffic map reduces local snow).

**Wolf winter behavior:**

Winter wolves are the primary threat. They are hungrier (prey has migrated), bolder (aggression 0.6 = attack range increases), and more numerous (surge chance 35%). The wolf-winter pressure creates the core endurance challenge:

```
Winter wolf pressure chain:
  Prey migrated in autumn -> wolves have no wild food
    -> wolves approach settlement (hunger-driven, not scripted)
      -> wolf packs probe settlement edges
        -> garrison buildings auto-defend within range
          -> undefended edges lose villagers
            -> fewer gatherers -> resource collection slows
              -> starvation pressure compounds wolf pressure
```

A well-prepared settlement (garrisons at chokepoints, full granary, walls) weathers winter. An unprepared one enters a death spiral.

**Observable indicators:**
- Snow blankets the map -- white characters / white overlay progressively covering terrain
- Water tiles freeze (visual change: `~` becomes `=` or solid blue-white)
- Villagers cluster near buildings, short trips only
- Wolf pack silhouettes visible at settlement edges, red threat indicators
- Night dominates -- most of the day-night cycle is dark, torchlight from buildings is prominent
- Stockpile visibly shrinks (resource count drops, visual: pile becomes smaller)
- Smoke from huts and bakery (warmth indicators)
- Dead villagers appear as markers if starvation or wolf attacks occur

**Emotional beat:** Dread. Tension. "Three villagers died last night. Food runs out in four days. The wolves are circling." Then, when spring arrives: catharsis.

### Season Transitions

Season changes should not be invisible modifier swaps. Each transition has a visible event.

| Transition | Event | Observable |
|------------|-------|------------|
| Winter -> Spring | Snowmelt begins | Water rises visibly over 2-3 days; snow tiles transition to mud; first green appears |
| Spring -> Summer | Flood recession | Water retreats to normal channels; exposed mud dries to fertile soil; full canopy |
| Summer -> Autumn | First frost | One-tick event: vegetation at high elevation turns amber; temperature notification |
| Autumn -> Winter | First snow | Snow begins accumulating; remaining unharvested farms flash warning then die; wolves howl (notification) |

### Day Length System (new)

Day length varies by season, affecting how many active hours villagers have. Villagers return to huts when it gets dark (existing night behavior). Shorter days = less productive time = more pressure.

| Season | Daylight Hours | Active Ticks/Day | Relative Productivity |
|--------|---------------|------------------|----------------------|
| Spring | 13h | ~650 | 108% |
| Summer | 16h | ~800 | 133% |
| Autumn | 10h | ~500 | 83% |
| Winter | 8h | ~400 | 67% |

Implementation: `DayNightCycle` gains a `daylight_hours()` method that returns season-dependent values. The sunrise/sunset thresholds shift accordingly. Villager AI already checks `is_night()` -- this just moves the threshold.

```
pub fn daylight_hours(&self) -> f64 {
    match self.season {
        Season::Spring => 13.0,
        Season::Summer => 16.0,
        Season::Autumn => 10.0,
        Season::Winter => 8.0,
    }
}

pub fn is_night(&self) -> bool {
    let sunrise = 12.0 - self.daylight_hours() / 2.0;
    let sunset = 12.0 + self.daylight_hours() / 2.0;
    self.hour < sunrise || self.hour >= sunset
}
```

Summer villagers get twice the productive hours of winter villagers. This compounds all other seasonal effects -- winter pressure is not just hunger, it is hunger with less time to address it.

### SeasonModifiers Expansion

The current `SeasonModifiers` struct gains new fields:

```
pub struct SeasonModifiers {
    // Existing
    pub rain_mult: f64,
    pub evap_mult: f64,
    pub veg_growth_mult: f64,
    pub hunger_mult: f64,
    pub wolf_aggression: f64,
    // New
    pub farm_growth_mult: f64,      // summer bonus, winter zero
    pub movement_mult: f64,         // winter penalty
    pub exploration_weight: f64,    // spring bias toward exploring
    pub gathering_range_mult: f64,  // winter contraction
    pub fire_risk_enabled: bool,    // summer only
    pub snow_accumulation: f64,     // winter only
    pub prey_migration: bool,       // autumn only
    pub daylight_hours: f64,        // varies by season
}
```

Updated values:

| Field | Spring | Summer | Autumn | Winter |
|-------|--------|--------|--------|--------|
| rain_mult | 1.5 | 0.5 | 1.0 | 0.3 |
| evap_mult | 1.0 | 2.0 | 1.0 | 0.5 |
| veg_growth_mult | 2.0 | 1.5 | 0.3 | 0.0 |
| hunger_mult | 1.0 | 0.8 | 1.0 | 2.5 |
| wolf_aggression | 0.95 | 0.5 | 0.8 | 0.6 |
| farm_growth_mult | 1.2 | 1.3 | 1.5 (harvest) | 0.0 |
| movement_mult | 0.9 (mud) | 1.0 | 1.0 | 0.7 |
| exploration_weight | 0.3 | 0.0 | 0.0 | -0.3 |
| gathering_range_mult | 1.0 | 1.0 | 1.0 | 0.5 |
| fire_risk_enabled | false | true | false | false |
| snow_accumulation | -0.33/day (melt) | 0.0 | 0.0 | +0.1/day |
| prey_migration | false | false | true | false |
| daylight_hours | 13.0 | 16.0 | 10.0 | 8.0 |

## Visual Design Per Season

Each season must be instantly recognizable. Both rendering modes (Map and Landscape) need seasonal signatures.

### Map Mode (ASCII)

| Season | Terrain Characters | Entity Colors | Ambient Indicator |
|--------|-------------------|---------------|-------------------|
| Spring | `~` water expands, `.` mud tiles near rivers | Green villager names | `[SPRING]` in status bar, green |
| Summer | Normal terrain, `~` water shrinks | Yellow/gold villager names | `[SUMMER]` in status bar, yellow; `*` fire characters in red |
| Autumn | `'` fallen leaves on forest tiles, thinner tree chars | Orange villager names | `[AUTUMN]` in status bar, orange |
| Winter | `*` snow on all land tiles, `=` frozen water | Blue/white villager names | `[WINTER]` in status bar, blue; wolf `W` characters prominent |

### Landscape Mode (Painterly)

| Season | Palette | Texture | Particles |
|--------|---------|---------|-----------|
| Spring | Bright greens, blue water, brown mud | Dense vegetation emerging, water textures active | Rain drops, occasional mist |
| Summer | Golden greens, warm browns, deep blue water | Full dense vegetation, dry grass textures | Heat shimmer (character jitter), fire embers if burning |
| Autumn | Orange, red, amber, fading green | Sparse vegetation, leaf litter texture | Falling leaves (particles drifting down) |
| Winter | White, blue-grey, dark blue water->ice | Snow texture overlay, bare tree silhouettes | Snowfall particles, breath puffs near villagers |

## Migration Path

This feature spans multiple systems. Implementation order minimizes breakage and delivers visible value early.

### Step 1: Day length variation (small, high impact)

- Add `daylight_hours()` to `DayNightCycle`
- Modify `is_night()` to use season-dependent sunrise/sunset
- Immediate visible effect: winter days are short, summer days are long
- All existing tests pass (night behavior triggers more in winter, less in summer)

### Step 2: Snow accumulation and melt

- Add `SnowMap` to simulation (same pattern as `MoistureMap`)
- Winter: snow accumulates per tile per tick
- Spring: snow converts to water (snowmelt)
- Visual: snow overlay in both rendering modes
- Depends on: nothing new (WaterMap already exists for melt target)

### Step 3: Farm seasonal modifiers

- Add `farm_growth_mult` to `SeasonModifiers`
- Winter farms produce nothing (growth = 0)
- Summer farms get 1.3x bonus
- Autumn harvest bonus replaces random Bountiful Harvest event
- Depends on: nothing new (farms already read season)

### Step 4: Fire system

- Summer-only fire risk check on high-veg low-moisture tiles
- Fire spreads to adjacent vegetated tiles
- Fire destroys vegetation, damages buildings
- Depends on: simulation_chains moisture refactor (Step 2 of that doc) for moisture-driven fire risk

### Step 5: Prey migration

- Autumn: prey entities get migration behavior (walk to map edge, despawn)
- Spring: prey spawn rate increases to replenish
- Depends on: nothing new (prey entities and spawn logic exist)

### Step 6: Winter movement and behavior contraction

- Base movement penalty in winter (0.7x, stacks with snow and blizzard)
- Villager AI: gathering range contracts by 50% in winter
- Villager AI: exploration weight bonus in spring
- Depends on: snow system (Step 2) for snow movement penalty

### Step 7: Season transition events

- Visual/notification events at each transition point
- Snowmelt visible over 3 days (spring start)
- First frost notification (autumn start)
- First snow event (winter start)
- Depends on: snow system (Step 2), farm modifiers (Step 3)

## Testing Strategy

| Test | Validates |
|------|-----------|
| `winter_farms_produce_nothing` | farm_growth_mult = 0.0 in winter stops all farm growth |
| `summer_farms_grow_faster` | farm_growth_mult = 1.3 in summer increases growth rate |
| `snow_accumulates_in_winter` | SnowMap values increase each winter tick |
| `snow_melts_in_spring` | SnowMap values decrease, WaterMap values increase in spring |
| `snowmelt_causes_flooding` | Low-elevation tiles near rivers flood when snow melts |
| `fire_ignites_dry_forest` | Tile with veg > 0.7 and moisture < 0.2 can ignite in summer |
| `fire_spreads_to_neighbors` | Burning tile ignites adjacent tile with veg > 0.5 |
| `fire_destroys_vegetation` | Burned tile has vegetation = 0.0 after fire |
| `prey_migrates_in_autumn` | Prey count decreases during autumn |
| `prey_replenishes_in_spring` | Prey count increases during spring |
| `winter_movement_penalty` | Villager movement speed is 0.7x base in winter |
| `snow_further_slows_movement` | Tiles with snow_level > 0.3 apply additional speed penalty |
| `daylight_hours_vary_by_season` | Summer has 16h, winter has 8h |
| `villagers_sleep_earlier_in_winter` | Night behavior triggers earlier when daylight_hours = 8 |
| `gathering_range_contracts_in_winter` | Villagers seek resources within 50% of normal range in winter |
| `autumn_harvest_bonus` | Farms harvested in autumn yield 1.5x food |

## Performance Notes

- `SnowMap` is a flat `Vec<f64>`, same cost as `MoistureMap`. ~512KB for 256x256. Negligible.
- Fire spread iterates only burning tiles and their 4-8 neighbors. Even a large fire affecting 100 tiles checks ~800 neighbors per tick. Negligible.
- Prey migration adds a behavior state to existing entities. No new per-tick allocation.
- Day length change modifies a threshold comparison, not a new computation. Zero cost.
- Season modifier expansion adds ~8 floats to a struct copied once per season change. Zero cost.

## Open Questions

- **Snow-elevation interaction:** Should high-elevation tiles accumulate snow faster? This would make mountain passes truly impassable in deep winter, creating seasonal geography changes. Strong design signal but needs tuning to avoid trapping villagers.
- **Fire suppression:** Can villagers fight fires? If so, it is a new behavior type. If not, fire is purely destructive and the only defense is clearing vegetation near buildings (firebreak). Firebreak-by-clearing is more emergent (Pillar 2) but fire-fighting is more observable (Pillar 4).
- **Autumn harvest window:** Should the harvest bonus apply only in the first 5 days of autumn (early harvest) or all 10? A narrow window creates more urgency but punishes players who were still recovering from summer drought.
- **Winter building decay:** Is durability loss per winter day too punishing for distant outposts? Alternative: decay only applies to buildings with no villager visit in the last 20 ticks (truly abandoned structures).
- **Prey migration percentage:** 30% attrition per autumn feels right for mid-game. Early game with only 2-3 prey entities, losing one is devastating. Should migration scale with total prey count (skip migration if prey < 5)?
- **Season length tuning:** 10 days per season at current tick rate gives ~48,000 ticks per year. Is this the right cadence? Too fast and players cannot react to seasonal pressure. Too slow and the rhythm drags. Needs playtesting.
