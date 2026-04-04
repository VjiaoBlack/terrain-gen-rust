# Agent Team Architecture

## Real-Time Development Team (parallel, always-on during coding)

### Coder
- Writes code, runs tests
- Follows the architect's plan
- Does NOT make design decisions — asks architect when unsure

### Watchdog
- Monitors every file write
- Checks: design doc compliance, anti-goal violations, data flow coherence
- Warns immediately, doesn't block
- "You just added an infinite water source — that violates the no-magic-fixes rule"

### Tester
- After every code change, writes targeted diagnostic tests
- Not just `cargo test` — verifies the INTENT of the change
- "You changed moisture decay? Here's moisture values after 100/500/1000 ticks on seeds 42/100/777"
- Catches regressions in real time

### Architect
- Holds the big picture (design docs, ARCHITECTURE.md, data flow)
- Answers design questions from the coder
- Prevents improvised solutions that violate principles
- "Don't make Water tiles infinite sources. The root cause is the decay rate."

## Quality Review Team (periodic, autonomous)

Runs independently to find improvements — not triggered by bugs.

### Unit Test Auditor
- Reviews test quality across the codebase
- Finds: untested code paths, flaky tests, tests that don't test what they claim
- Proposes: new tests, test fixes, test deletions
- "The `bare_adjacent_to_forest_becomes_sapling` test is RNG-dependent with no seed — should use deterministic setup"

### Architecture Reviewer  
- Reviews module boundaries, file sizes, coupling
- Checks ARCHITECTURE.md against actual code
- Finds: files that are too big, circular dependencies, leaky abstractions
- Proposes: splits, extractions, interface simplifications
- "simulation.rs has 11 systems in one file — split into simulation/ module"

### Simplifier
- Proactively looks for code that can be simplified
- Finds: dead code, over-abstractions, duplicate logic, unused features
- Proposes: deletions, merges, simplifications
- "The old WaterMap is no longer used by anything — remove it entirely"
- Goal: less code, not more

### Design Drift Detector
- Compares actual game behavior against design doc expectations
- Runs diagnostics and playtests, checks against success criteria
- "game_design.md says two seeds should produce different settlements, but seeds 42 and 100 have 85% footprint overlap"
- Tracks: which design goals are met, which are drifting, which are blocked

## Workflow Integration

### During Active Development
```
Human picks task → Architect writes plan → 
Coder + Watchdog + Tester run in parallel →
Review agent checks diff → Merge
```

### Weekly/Periodic Quality Review
```
Unit Test Auditor → files issues for test gaps
Architecture Reviewer → files issues for refactors  
Simplifier → files PRs removing dead code
Design Drift Detector → updates design doc status
```

### When Bugs Are Found
```
1. Spawn Diagnostician (ONLY diagnoses, writes failing test)
2. Architect reads diagnosis, writes fix plan
3. Coder implements the plan
4. Tester verifies
5. Reviewer approves
```

Never: coder diagnoses AND fixes in the same breath.

## Implementation

### Phase 1: Use existing Claude Code agent spawning
- Coder = implementation agent (current)
- Reviewer = review agent (just added)
- Diagnostician = new agent role for bugs
- Architect = the main conversation (human + orchestrator)

### Phase 2: Agent Teams (Claude Code feature)
- Parallel agents with shared context
- Real-time watchdog alongside coder
- Tester running diagnostics after each file write

### Phase 3: Autonomous Quality Reviews
- Scheduled agents (like our old hourly playtester, but smarter)
- Each review type runs weekly or on-demand
- Files GitHub issues, not code changes
- Human reviews and approves before any action

## Anti-Patterns to Prevent
- Agent that diagnoses AND fixes (leads to spiral)
- Quality review that auto-fixes without human approval
- Watchdog that blocks instead of warns (slows iteration)
- Too many agents = too much noise. Start with 2 (coder + tester), add more as needed.
