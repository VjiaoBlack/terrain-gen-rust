# Development Workflow

How we go from idea to shipped code in terrain-gen-rust.

## The Pipeline

```
Vision (game_design.md)
  → Pillar deep-dives (game_design.md sections)
    → Research (docs/research/*.md) — when we don't know how to do something
      → Feature design docs (docs/design/**/*.md) — concrete specs
        → GitHub issues — trackable work items
          → Implementation (agent or human)
            → Tests + playtest → merge
```

## Stages

### 1. Vision & Pillars
- `docs/game_design.md` has the 5 ranked pillars and phase roadmap
- When a new idea comes up, check which pillar it serves
- If it conflicts with a pillar, the higher-ranked pillar wins

### 2. Research (when needed)
- For complex systems (fluid dynamics, erosion, etc.), research BEFORE designing
- Write to `docs/research/topic.md`
- Use Sonnet agents for web search + synthesis
- Research report recommends an approach with performance estimates

### 3. Design Docs
- One doc per feature in `docs/design/{pillar_dir}/feature.md`
- Template: What/Why/Current State/Design (Data Structures + Algorithm + Integration)/Edge Cases/Test Criteria/Dependencies/Estimated Scope
- For cross-cutting features: `docs/design/cross_cutting/`
- Master agent reviews all docs for conflicts → `docs/design/KNOWN_CONFLICTS.md`
- Integration plan → `docs/design/INTEGRATION.md`

### 4. GitHub Issues
- One issue per implementable feature
- Labels: `planning` (needs breakdown), `quick-win` (one session), `design-decision` (resolve conflict), `needs-human` (blocked on human input)
- Milestones for phases
- Issues reference their design doc

### 5. Implementation
- **Serial** for features touching shared files (ai.rs, mod.rs, systems.rs)
- **Parallel worktrees** for features touching different files
- Each agent reads: CLAUDE.md + design doc + relevant source files
- Tests must pass before commit
- Commit references issue number (e.g. `#42`)

### 6. Verification (MANDATORY)

Every change must be verified with DATA, not assumptions.

**Before changing code:**
- Write a diagnostic test that shows the current (broken) state with actual numbers
- Trace the full data flow of the system being changed
- Understand WHY the current behavior exists before modifying it

**After changing code:**
- The diagnostic test must now show the correct state
- `cargo test --lib` must pass
- Run `--diagnostics` on 2+ seeds, check that key metrics didn't regress
- If changing rendering: run `--showcase` and verify visually
- Query mode (`k` key) for per-tile data inspection

**Anti-patterns to avoid:**
- Changing a constant without first checking what values the system produces
- Making change A, discovering it breaks B, making change C to fix B, discovering C breaks D... STOP. Revert. Diagnose. Fix the root cause.
- Adding "magic" fixes (infinite sources, hardcoded overrides) instead of finding the real bug
- Assuming a change works because it compiles and tests pass — check the actual gameplay

## Design Doc Updates
- When implementation reveals the design was wrong: update the doc
- When new research changes the approach: write new doc, keep old in git history
- When two docs conflict: resolve in KNOWN_CONFLICTS.md, update both docs

## Agent Patterns
- **Research agent** (Sonnet): web search + synthesis → docs/research/
- **Design agent** (Opus): reads codebase + design doc → writes feature spec
- **Implementation agent** (Opus): reads design doc + code → implements + tests
- **Review agent** (Opus): reads diff + project context → checks for bugs, design violations, regressions
- **Master orchestrator** (this conversation): plans, delegates, verifies, merges

## Code Review Protocol

Every significant diff gets reviewed before merge:

1. Implementation agent finishes → commits to worktree branch
2. **Review agent** spawns with:
   - The diff (`git diff main..branch`)
   - CLAUDE.md (development rules)
   - docs/ARCHITECTURE.md (data flow, known issues)
   - docs/game_design.md (design pillars)
   - Relevant design docs for the feature
3. Review agent checks:
   - Does the diff match what was requested?
   - Any logic errors or off-by-one bugs?
   - Does it violate any design principles or anti-goals?
   - Does it introduce new "magic" fixes instead of solving root causes?
   - Are there untested code paths?
   - Does the data flow make sense end-to-end?
   - Any performance concerns?
4. Review agent reports: APPROVE / NEEDS CHANGES (with specific issues)
5. Master merges only after APPROVE
