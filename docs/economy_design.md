# Economy & Food System Design

## Current Problems
1. Stone deposits are finite (10 total stone ever) — can't sustain building
2. All food is one type — no crop variety, no strategic food decisions
3. system_assign_workers grabs all idle villagers → nobody gathers
4. Berry bushes deplete fast (12 yield) and respawn randomly/slowly
5. No reason to plant different crops or manage food diversity

## Proposed Food Types

### Raw Foods (gathered from world)
- **Berries** — from berry bushes, fast to gather, small yield, spoils quickly
- **Vegetables** (potatoes) — from farm plots, medium grow time, good yield, stores OK
- **Wheat** — from farm plots, slow grow time, must be processed into bread
- **Meat** — from hunting prey (future), high food value

### Processed Foods
- **Bread** — wheat + wood (bakery), high food value, stores well
- **Preserved Food** — vegetables + salt? (granary), winter-safe

### Farm Types
Instead of one generic "Farm", villagers auto-decide what to plant:
- Low food → plant **vegetables** (fast, reliable)
- Enough food → plant **wheat** (long term, needs bakery)
- Berry bushes could be **plantable** in farms as a crop option

### Food Storage
Separate stockpile counts:
```
Resources {
    berries: u32,      // spoils in 3 days
    vegetables: u32,   // spoils in 7 days
    wheat: u32,        // doesn't spoil (grain)
    bread: u32,        // doesn't spoil
    meat: u32,         // spoils in 2 days unless smoked
    ...
}
```

Villagers prefer: bread > vegetables > berries > wheat (raw) > meat

## Stone/Mining Rework

### Problem
Only 2 stone deposits = 10 stone. Game needs way more.

### Solution
- **Mountains are infinite stone** — villagers mine at mountain edges, slower than deposits
- **Stone deposits** are "rich veins" — faster mining, 20 yield each, spawn 4-6 per map
- **New deposits appear** as exploration expands (discovered in fog)
- **Quarry building** — placed adjacent to mountain, provides steady stone income with worker

## Wood Rework
- Chopping forest tiles works (current) but forest should **regrow over time**
- Forest regrowth: grass tiles adjacent to forest slowly become forest (growth sim)
- **Lumber Mill** building — placed adjacent to forest, sustainable wood income

## Villager Work Priority Rework

Instead of system_assign_workers grabbing everyone:

```
Priority (checked each tick by AI):
1. Flee (danger)
2. Eat (hungry > 0.5)
3. Sleep (night)
4. Critical gather (wood < 5 OR stone < 5)
5. Assigned work (farm/workshop) — but auto-unassign after 200 ticks
6. Build (nearest site)
7. Gather resources (balanced by need)
8. Wander
```

Key change: **work assignments are temporary** (200 tick lease). After lease expires, villager goes idle and re-evaluates. This naturally rotates workers through different tasks.

## Wolf/Threat Rework

Current: wolves only in winter surge events, never otherwise.

Proposed:
- **Year 1**: No wolves (grace period)
- **Year 2+**: Occasional lone wolves wander near settlement (1-2 per season)
- **Winter**: Wolf surge events (current, but spawn 3-5 wolves not just "increased activity")
- **Year 5+**: Wolf packs that establish dens near settlement
- Garrison/walls still defend against raids

## Implementation Priority

1. **Fix worker rotation** — temporary assignments, auto-release
2. **Infinite mountain stone** — biggest resource bottleneck
3. **Forest regrowth** — wood sustainability
4. **More stone deposits** — spawn during game
5. **Food types** — vegetables vs wheat (later)
6. **Wolves year-round** — mild constant threat (later)
