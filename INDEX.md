# Documentation Index

Orientation guide for humans and agents working on terrain-gen-rust.

## Start Here

1. **[CLAUDE.md](CLAUDE.md)** — Build commands, module structure, key conventions, controls. Read this first for any code work.
2. **[docs/game_design.md](docs/game_design.md)** — Vision, design pillars (ranked), success criteria, phase roadmap, pillar deep-dives. **This is the north star.** Every feature and fix should trace back to a pillar.
3. **[docs/economy_design.md](docs/economy_design.md)** — Building costs, production chains, resource flow. Reference for balance work.

## Design Pillars (quick reference)

Ranked by priority. When two ideas conflict, the higher pillar wins.

1. **Geography Shapes Everything** — Terrain creates reasons, constraints, and asymmetry. Activity changes terrain over time. ([deep-dive](game_design.md#pillar-1-geography-shapes-everything))
2. **Emergent Complexity from Simple Agents** — Local knowledge, systems chain through simulation, simple rules create complex outcomes. Agent knowledge architecture: see/remember/share/truth layers. ([deep-dive](game_design.md#pillar-2-emergent-complexity-from-simple-agents))
3. **Explore / Expand / Exploit / Endure** — Natural game arc that emerges from simulation state. Gradient, not state machine. No phase gates. ([deep-dive](game_design.md#pillar-3-explore--expand--exploit--endure))
4. **Observable Simulation** — Two rendering modes: Map (symbolic ASCII) and Landscape (painterly terminal). If you can't see it, it doesn't count. ([deep-dive](game_design.md#pillar-4-observable-simulation))
5. **Scale Over Fidelity** — 500+ agents target. Spatial hash grid, path caching, tick budgets, hierarchical pathfinding. ([deep-dive](game_design.md#pillar-5-scale-over-fidelity))

## Key Architectural Insight

The **spatial hash grid** is the single highest-leverage infrastructure piece — it unlocks geography queries (P1), knowledge sharing (P2), per-tile rendering (P4), and AI/pathfinding performance (P5) simultaneously.

The **agent knowledge architecture** (P2) is the keystone system — it makes exploration meaningful (P3), geography matter (P1), simulation observable (P4), and forces scale solutions (P5).

## Anti-Goals

- No micromanagement (no individual villager control)
- No manual roads (emerge from traffic)
- No real-time combat controls (garrison placement is strategy)
- No dialogue or narrative text (simulation tells the story)
- No tech tree UI (building unlocks are implicit)
- No random resource spawning (resources exist at world-gen, discovered through exploration)

## Docs Reference

| Document | Purpose | When to read |
|----------|---------|-------------|
| [docs/game_design.md](docs/game_design.md) | Vision, pillars, phases, deep-dives | Before any feature work or design decision |
| [docs/economy_design.md](docs/economy_design.md) | Resource balance, building costs | When touching economy, buildings, production |
| [docs/agent_autoloop_review.md](docs/agent_autoloop_review.md) | Post-mortem on automated dev agent | Before setting up any automated agent work |
| [docs/playtest_notes.md](docs/playtest_notes.md) | Historical playtest data (15 games) | When investigating balance or regressions |
| [docs/terrain_research_topics.md](docs/terrain_research_topics.md) | Terrain algorithm research backlog | When working on terrain pipeline |
| [docs/research/](docs/research/) | Deep research on terrain algorithms | Reference for terrain pipeline implementation |

## For Agents

If you are an AI agent working on this project:

1. Read `CLAUDE.md` for build/test commands and code conventions.
2. Read `docs/game_design.md` for design pillars — check your work against them.
3. Check `docs/agent_autoloop_review.md` for lessons from previous agent runs. Key takeaways:
   - Don't tweak thresholds without testing 10+ seeds.
   - Don't commit-revert-recommit. Test locally, commit once.
   - Use `--diagnostics` mode for structured telemetry, not screenshot parsing.
   - Separate diagnosis from treatment. Understand the system before changing it.
4. Anti-goals are hard constraints. Do not build features on the anti-goals list.
5. When in doubt between two approaches, the higher-ranked pillar wins.

## Current Status (2026-04-02)

- Terrain pipeline: 7 stages, 14 biomes, working
- Settlement sim: villagers, buildings, production chains, basic economy
- Diagnostics: `--diagnostics` flag emits JSONL telemetry
- Tests: 207 lib tests passing
- Next priorities: precomputed resource map, knowledge architecture, rendering modes
