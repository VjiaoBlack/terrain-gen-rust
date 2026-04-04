# Self-Improving Development Harness Research

## Current Problems

Our multi-agent workflow has a core failure mode: agents make rapid changes in parallel worktrees without sufficient verification before handing off. This produces cascades where a fix introduces a regression, triggering more fixes, and the test suite becomes a moving target rather than a safety net. The root causes are:

1. **No pre-merge gate with teeth** — agents can merge code that passes surface tests but fails semantic/integration checks.
2. **No cross-session memory** — each agent starts cold. Mistakes from last week get repeated this week.
3. **No regression signal beyond pass/fail** — 750+ tests tell us *that* something broke, not *why* or *what pattern* caused it.
4. **Parallel worktrees drift** — agents working concurrently can make conflicting assumptions without realizing it.

---

## Industry Approaches

### Multi-agent orchestration

The best-studied pattern (from SWE-Agent, ChatDev, TheBotCompany, and Anthropic's own research system) is **role specialization + phase separation**:

- **Three permanent manager roles**: Planner, Implementer, Verifier. Worker agents are hired/retired per milestone.
- **Three phases per milestone**: Strategy → Execution → Verification. Verification is always a separate agent, never the one that wrote the code.
- **Human input at phase boundaries, not mid-task**. Interruptions that arrive as issues get consumed at the next natural boundary, not injected mid-stream.

SWE-EVO (2025 benchmark) showed that GPT-5 with OpenHands scores 21% on multi-file, multi-step tasks versus 65% on single-issue fixes. The lesson: multi-agent coordination overhead only pays off for problems requiring iterative exploration spanning many files — which is exactly what settlement simulation feature work looks like.

Anthropic's internal research agent found that Claude 4 models are effective "prompt engineers for themselves": when given a flawed tool, a tool-testing subagent can diagnose the failure and rewrite the tool description, yielding a 40% reduction in task completion time for future agents.

### Automated review

The working pattern from production teams in 2025:

1. PR opened → GitHub Action runs LLM review (diff-scoped, structured JSON output with file+line references).
2. Review posted as inline PR comments, grouped by severity (blocker / warning / note).
3. If zero blockers: auto-merge to a staging branch. Human reviews the staging → main merge.
4. Feed every fix and follow-up prompt back as a training signal to improve the review prompt over time.

**What works**: scoping the review strictly to the diff (not the whole codebase), requiring structured output, running two models and reconciling. **What doesn't**: blocking builds immediately — introduce LLM review as non-blocking first to calibrate false-positive rate before giving it gate power.

For our Rust codebase specifically: a hook on `PostToolUse` for file edits can run `cargo check` deterministically (zero LLM cost) before the agent even tries to compile. This catches type errors before the agent enters a fix spiral.

### Regression detection

From game studios (CD Projekt Red, EA, Ubisoft) and from general CI practice:

- **Metric-based quality gates** are more useful than pass/fail for game sims. Track: defect density, mean time to detect, test coverage %, regression rate per PR. Gate on *trend* not just absolute value.
- **Simulation invariant checks**: for a settlement sim, these are things like "population never goes negative," "food surplus is never NaN," "road graph has no orphan nodes after any tick." These are fast, deterministic, and catch the most common cascade failures before the full test suite runs.
- **Snapshot/diffing on diagnostic output**: our `--diagnostics` mode already produces telemetry. Run a reference snapshot before the merge and diff it. If a stat shifts by more than a threshold (e.g., average population growth rate changes by >10%), flag for human review even if tests pass.

### Agent memory and learning

Current state of the art (A-MEM, Memory-R1, 2025):

- **Don't try to fine-tune** the model between sessions — too expensive and brittle.
- **External structured memory** (SQLite or a vector store) is the practical approach. Each agent run appends to it; future agents query it at session start.
- **Three memory types that matter**:
  - *Decision log*: what was decided, why, and what branch/PR it affected.
  - *Anti-pattern DB*: patterns that caused regressions. Written by the Verifier agent after a failed merge, queried by the Planner before implementing.
  - *Episodic memory*: "last time we touched `settlement/economy.rs`, we broke population growth." Stored as structured tags on file paths.

TheBotCompany's architecture stores all of this in a single SQLite DB with three tables: issues, milestone history, agent reports. Coordination happens through the DB, not through agent-to-agent messaging. This is crash-safe and auditable.

---

## Recommended Architecture for Our Project

Build this in four layers, each independently deployable:

**Layer 1 — Deterministic pre-flight hooks (build first)**
- Claude Code `PreToolUse` hook: before any Rust file edit, run `cargo check --message-format=json`. Pipe errors back as context. Zero LLM cost, stops spiral before it starts.
- `PostToolUse` hook: after file edits, run affected test modules only (`cargo test <module>` derived from changed files). Block if failures exceed 0.

**Layer 2 — Structured agent memory (build second)**
- SQLite file at `.claude/memory/agent_memory.db` with tables: `decisions`, `anti_patterns`, `file_history`.
- A skill (`/recall`) that queries this DB and prepends relevant context to any agent session: "Last 3 times `economy.rs` was modified, this broke..."
- A skill (`/remember`) that the Verifier agent calls after any merge with outcome (success/regression/rollback).

**Layer 3 — Automated PR review gate (build third)**
- GitHub Action on PR open: run Claude review (diff-scoped, JSON output, file+line references) and post inline comments.
- Quality gate script: parse the JSON, count blockers. If zero blockers AND all CI checks pass AND no simulation invariant violations, auto-merge to `staging`. Never auto-merge to `main`.
- Start non-blocking; give it two weeks of calibration before enabling as a hard gate.

**Layer 4 — Agent team coordination (build fourth, experimental)**
- Use Claude Code Agent Teams (launched Feb 2026) for feature work: one lead agent, one security/correctness reviewer agent, one test-coverage agent working in parallel.
- Shared task list via the SQLite memory DB. Agents self-assign, write findings to DB, lead agent synthesizes.
- This is experimental — agent teams have higher token cost. Use only for large features, not bug fixes.

---

## Concrete Next Steps (what to build first)

**Week 1 — Deterministic hooks**
1. Write `.claude/hooks/pre_edit.sh`: runs `cargo check` and outputs errors as JSON.
2. Register it as a `PreToolUse` hook for `Write` and `Edit` tools in `.claude/settings.json`.
3. Write `.claude/hooks/post_edit.sh`: derives affected test module from edited file path, runs `cargo test <module>`.
4. Verify the hook fires correctly on a test edit. Commit.

**Week 2 — Anti-pattern memory**
1. Create `.claude/memory/agent_memory.db` schema (decisions, anti_patterns, file_history tables).
2. Write a `/recall` skill that queries anti_patterns for files touched in the current task.
3. Write a `/remember` skill called after any merge: records outcome, affected files, brief cause if regression.
4. Seed the DB manually with the three most common regression patterns we've already seen.

**Week 3 — Simulation invariants**
1. Add a `tests/invariants.rs` file: fast checks on simulation output snapshots (no negative population, no NaN stats, road graph integrity).
2. Run invariants as part of the existing test suite and in the post-edit hook.
3. Capture a `--diagnostics` baseline snapshot; add a CI step that diffs against it and fails if key metrics shift beyond threshold.

**Week 4 — PR review action**
1. Write a GitHub Actions workflow: on PR, run Claude review of diff, post comments.
2. Start in annotation-only mode (no blocking). Review the quality of comments for two weeks.
3. Once calibrated, add a blocker-count gate before auto-merge to staging.

---

## References

- [Extend Claude Code: Hooks, Skills, Subagents, Agent Teams](https://code.claude.com/docs/en/features-overview)
- [Self-Organizing Multi-Agent Systems for Continuous Software Development (arXiv 2603.25928)](https://arxiv.org/html/2603.25928v1)
- [A-MEM: Agentic Memory for LLM Agents (arXiv 2502.12110)](https://arxiv.org/abs/2502.12110)
- [SWE-EVO: Benchmarking Coding Agents (arXiv 2512.18470)](https://arxiv.org/pdf/2512.18470)
- [How Anthropic built its multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)
- [Augment Code: AI Code Review in CI/CD Pipeline](https://www.augmentcode.com/guides/ai-code-review-ci-cd-pipeline)
- [State of AI Code Review Tools 2025](https://www.devtoolsacademy.com/blog/state-of-ai-code-review-tools-2025/)
- [Regression Testing Strategies for Game Development](https://beefed.ai/en/regression-testing-game-development)
- [awesome-claude-code: Hooks, Skills, Orchestrators](https://github.com/hesreallyhim/awesome-claude-code)
- [Claude Code Full Stack Explained (alexop.dev)](https://alexop.dev/posts/understanding-claude-code-full-stack/)
- [QA Wolf: Diagnosis-First Self-Healing Tests](https://www.qawolf.com/blog/self-healing-test-automation-types)
- [2026 Agentic Coding Trends Report (Anthropic)](https://resources.anthropic.com/hubfs/2026%20Agentic%20Coding%20Trends%20Report.pdf)
