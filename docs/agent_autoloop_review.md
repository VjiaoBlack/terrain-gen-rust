# Agent Autoloop Review

Post-mortem on the automated dev agent experiment (2026-03-31 to 2026-04-01).

## Setup

- **Playtester agent**: Ran hourly, played 3 games (seeds 42/137/999), wrote reports to `docs/playtest_notes.md`. 5 runs, 15 games total.
- **Dev agent**: Same hourly trigger, combined playtest + fix loop. Produced 73 commits (52 code, 5 reverts, 16 reports/merges).
- **Model**: claude-sonnet-4-6
- **Tools**: Bash, Read, Write, Edit, Glob, Grep
- **Repo access**: Fresh clone each run, no persistent state between sessions.

## What the Agent Did Well

### Diagnostics
- Consistent, structured reports with per-game tables, cross-run comparisons, priority rankings.
- Built a statistical picture across 15 games that no human would manually produce.
- Correctly identified stone depletion (15/15 desert runs), rabbit absence (15/15), wolf spawn failure (event fires, 0 entities).
- Root cause chains were often correct: stone=0 -> buildings blocked -> wood has no sink -> 47K wood piles up.

### Code Fixes (genuinely good, survived merge)
- Wolf surge now spawns actual wolves (1-4 scaled to settlement size)
- Initial prey/den seeding (fixes permanent 0 rabbits)
- Workshop/Smithy/Granary auto-build priorities
- Food-gated births (population stops growing into starvation)
- Pre-built Granary (food->grain before winter)
- Spawn location validation (rejects narrow corridors, requires 8+ buildable 3x3 zones)
- Stone/timber auto-discovery when critically low
- Drought rebalancing (was instant-death, now survivable)
- Winter food decay cap
- Build-site flee loop fix
- Starvation override for shelter seeking (hungry villagers eat before sleeping)

## What the Agent Did Poorly

### Thrashing (the core problem)
- 24 of 52 code commits (~46%) mention "deadlock," "ceiling," "starvation," or "regression."
- WoodToPlanks threshold: 2 -> 7 -> 12 -> 8 -> 12 -> 2 -> 12 (7 changes, landed where it started)
- Farming interrupt condition: || -> && -> reverted -> changed -> reverted -> settled on &&
- 5 explicit reverts, ~10 more implicit ones.
- The agent was playing whack-a-mole: fix Workshop -> breaks food -> revert -> fix food -> breaks Workshop.

### Missed the biggest bug
- Villagers permanently stuck in Farming/Working with lease=0 (the agent itself introduced this by removing the lease field from constructors). Never detected because it only read aggregate resource counts, not per-villager state.

### No experimentation discipline
- Same 3 seeds every run. Overfits to 42/137/999.
- Never varied parameters, never tested edge cases.
- Never ran A/B comparisons (before vs after a change across many seeds).

### No memory between sessions
- Each run starts fresh. Rediscovers the same problems, tries the same fixes, makes the same mistakes.
- playtest_notes.md is append-only observations, not lessons or decisions.

### Repetitive reporting
- Run 5 says the same things as Run 1 with incrementing confirmation counts (6/6 -> 9/9 -> 12/12 -> 15/15). After 3x confirmation, re-reporting known bugs has zero diagnostic value.

## Why It Thrashes: Root Causes

1. **No mental model of the system.** Each run reads code, sees a symptom, makes a local fix without understanding downstream effects. Next run sees the new symptom and makes the opposite fix.

2. **Single-variable tuning on a coupled system.** The economy is interconnected: wood thresholds affect hut building -> population -> food consumption -> farming priority -> wood gathering. Tweaking one number and observing one metric (pop at tick 36K) can't capture the cascade.

3. **No rollback discipline.** Commits freely — fix, revert, fix again. Git history becomes a scratchpad. A human tests 5 things locally, keeps the one that works, commits once.

4. **No cost to committing.** No gate says "prove this is better across 10 seeds before you ship."

## Structural Limitations

### Frame-snapshot analysis is fundamentally limited
- Reads 3 screenshots per game (tick 12K, 24K, 36K).
- Sees aggregate numbers but not behavior. Can't see a villager stuck in Farming{lease:0} for 10K ticks.
- Like diagnosing a car engine by looking at 3 photos of the dashboard.

### No design vision
- Prompt says "find what's broken and fix it" — that's QA, not game design.
- Can't say "the mid-game feels flat" or "building placement creates ugly grids."
- Optimizes for survival (population > 0) rather than interesting gameplay.

### No collaboration
- Works alone, commits 52 things, presents a merge conflict.
- Never filed an issue saying "I think X should be Y because [data], thoughts?"

### No sense of player experience
- Has never watched a human play. Doesn't know what's confusing, satisfying, or tense.
- Says "54 food / 147 pop = 0.37 food/villager" instead of "this would feel like panic."

## Concrete Improvements

### 1. Structured telemetry (highest impact)
Add `--diagnostics` flag that dumps JSON every 1000 ticks:
```json
{"tick": 5000, "pop": 12, "food": 45, "wood": 230, "stone": 8,
 "villager_states": {"Farming": 3, "Gathering": 2, "Idle": 4, "Sleeping": 3},
 "buildings": {"Hut": 2, "Farm": 1, "Stockpile": 1},
 "events": ["Drought started at tick 4200"]}
```
Agent reads data, not pixels.

### 2. Persistent decision log
A file like `docs/agent_decisions.md` recording: "Changed WoodToPlanks from 7 to 12. Reason: [data]. Risk: [prediction]. Test: [verification criteria]." Next session reads this before making changes. Prevents repeating the same experiments.

### 3. Regression gates
Before any commit, run 10+ seeds and compare key metrics against a baseline. If any seed regresses >20% on population or food, don't commit — write a note about why and move on.

### 4. Separate roles with handoffs
Instead of one agent doing everything:
- **Playtester**: Runs games, produces structured telemetry. Never touches code.
- **Designer**: Reads telemetry + design docs, proposes changes as issues with rationale. Never touches code.
- **Dev**: Picks up approved issues, implements, runs tests + regression suite. Only one that commits.

### 5. Design prompts, not bug prompts
Instead of "find broken, fix it": "Watch these playthroughs. What's the most boring part? Where would a player quit? What would make them say 'one more year'? Propose one system-level change."

### 6. Broader test coverage
- 10+ seeds, not 3. Include pathological maps (islands, all-mountain, tiny).
- Run for 3 in-game years, not 1. Late-game is invisible at tick 36K.
- Vary auto-build on/off, different starting resources.

### 7. Reference points for taste
Give the agent screenshots/descriptions of DF, Songs of Syx, RimWorld. "Here's what good looks like." Without references, it optimizes for survival, not fun.

### 8. PR-based workflow
Agent opens a PR with description: "Seed 137 always hits stone=0 by autumn. Proposing mountain-edge mining. Here's implementation + before/after data across 10 seeds." We review, discuss, merge. Collaborator, not cowboy.

## Key Metrics

| Metric | Value |
|--------|-------|
| Total agent commits | 73 |
| Code commits | 52 |
| Reverts | 5 |
| Thrash commits (deadlock/ceiling/regression) | 24 (46%) |
| Genuinely good fixes | ~10-12 |
| Signal-to-noise ratio | ~20% |
| Biggest bug missed | lease=0 stuck state (agent introduced it) |
| Playtest games run | 15 |
| Unique seeds tested | 3 |

## Summary

The agent delivered ~10 genuinely good fixes and ~40 commits of thrashing. Net positive — the game is better after merging — but the 10 good fixes could have been done in 5 focused commits instead of 52. The core issue: an hourly stateless agent can't tune a coupled economic system. It needs memory, experimentation discipline, regression gates, and broader test coverage. And to think bigger, it needs design vision — not just "what's broken" but "what would be fun."
