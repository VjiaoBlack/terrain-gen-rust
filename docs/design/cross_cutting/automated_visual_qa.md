# Automated Visual QA — Keeping Claude in the Loop

**Problem:** Claude makes terrain/rendering changes but can't see the result. The human catches obvious problems in 2 seconds that Claude misses after hours of coding. We need systems that automatically close this feedback loop.

---

## Principle: Every Visual Change Gets Automatic Feedback

When Claude modifies terrain generation, erosion, rendering, or any visual system, the following should happen AUTOMATICALLY — not because Claude remembers to do it, but because the system forces it.

---

## Layer 1: Claude Code Hooks (repo-level, runs on every tool call)

### Post-Edit Hook: Pipeline Health Check

Add to `.claude/hooks.json` (or equivalent):
```json
{
  "PostToolUse": {
    "pattern": "Edit|Write",
    "files": ["src/terrain_pipeline.rs", "src/hydrology.rs", "src/terrain_gen.rs",
              "src/simulation/wind.rs", "src/simulation/moisture.rs",
              "src/game/render/*.rs", "src/simulation/day_night.rs"],
    "command": "cargo test --lib pipeline_health -- --nocapture 2>&1 | tail -20",
    "description": "Auto-run pipeline health check after editing terrain/render code"
  }
}
```

This means: every time Claude edits a terrain or rendering file, the pipeline health test runs automatically. If biome diversity collapsed, if rivers vanished, if discharge flooded — Claude sees it immediately in the hook output, not after the human playtests.

### What the health check tests:
```rust
#[test]
fn pipeline_health() {
    let config = PipelineConfig::default();
    let result = run_pipeline(128, 128, &config);
    let n = 128 * 128;

    // 1. Height distribution
    let water_pct = result.heights.iter()
        .filter(|&&h| h <= config.terrain.water_level).count() as f64 / n as f64;
    assert!(water_pct > 0.05 && water_pct < 0.50,
        "HEALTH FAIL: water={water_pct:.1}% (expected 5-50%)");

    // 2. Biome diversity (no single biome > 60%)
    // ...

    // 3. River coverage (0.5% - 15% of land)
    // ...

    // 4. No coastal spikes
    // ...

    eprintln!("PIPELINE HEALTH: OK");
}
```

---

## Layer 2: CLAUDE.md Rules (agent behavior, always read)

Add to CLAUDE.md:

```markdown
### After ANY terrain/render change:
1. Run `cargo test --lib pipeline_health` and CHECK the output
2. Run `cargo run --release -- --play --seed 100 --ticks 1` and check the [HYDROLOGY] line
3. If discharge visible_tiles = 0: your change broke river rendering
4. If discharge visible_tiles > 10000: your change flooded the map
5. Compare against known-good values before committing:
   - water_pct: 10-30%
   - visible_rivers: 1-10% of land
   - biome types: >= 3
   - max discharge: 0.3-2.0
```

This is the behavioral rule — Claude should internalize "check the numbers" as a reflex, not a conscious decision.

---

## Layer 3: Pre-Commit Hook (git-level, blocks bad commits)

`.git/hooks/pre-commit`:
```bash
#!/bin/bash
# Quick pipeline sanity check before allowing commit
if git diff --cached --name-only | grep -qE 'terrain_pipeline|hydrology|terrain_gen|render/landscape|render/normal|day_night'; then
    echo "Terrain/render files changed — running pipeline health check..."
    cargo test --lib pipeline_health 2>&1
    if [ $? -ne 0 ]; then
        echo "PIPELINE HEALTH CHECK FAILED — fix before committing"
        exit 1
    fi
fi
```

This physically prevents committing broken terrain changes. Even if Claude forgets to check, the commit is blocked.

---

## Layer 4: Visual Snapshot Comparison

### Generate reference frames:
```bash
cargo run --release -- --showcase --seed 100 --inputs "tick:1,ansi" > reference_frame.ansi
```

### After changes, generate new frame and diff:
```bash
cargo run --release -- --showcase --seed 100 --inputs "tick:1,ansi" > new_frame.ansi
diff reference_frame.ansi new_frame.ansi | head -50
```

### Automated comparison metrics:
```rust
fn compare_frames(reference: &str, new: &str) -> FrameDiff {
    // Parse ANSI frames into cell grids
    // Count: tiles that changed color, tiles that changed character
    // Flag: if > 30% of tiles changed, something big happened
    // Flag: if water tile count changed by > 50%, rendering broke
}
```

---

## Layer 5: Claude Memory Feedback Loop

After each session where the human catches a visual bug:

1. **Save to memory:** "When changing [system], always check [metric] because [bug happened]"
2. **Add to test harness:** Write the test that would have caught it
3. **Add to CLAUDE.md:** The behavioral rule that prevents it

Example from this session:
- **Memory:** "Normal scale 40→20 looked like it might fix dark tiles, but the actual cause was Peat soil type misclassification. Always diagnose with data before changing parameters."
- **Test:** `no_excessive_dark_tiles_at_midday()`
- **Rule in CLAUDE.md:** "Don't change rendering constants without first querying (k key) the actual problematic tile to get terrain type, soil, height, moisture data."

---

## Layer 6: Diagnostic Overlay System (in-game)

The Height overlay we added is a start. Extend with:

| Overlay | Shows | Catches |
|---|---|---|
| Height | Grayscale heightmap | Erosion artifacts, coastal spikes |
| Discharge | Blue intensity = river strength | Missing rivers, flooding |
| Moisture | Green intensity = soil moisture | Dead zones, coastal-only moisture |
| Slope | White = steep, dark = flat | Over-smoothing, cliff artifacts |
| Soil Type | Color-coded soil types | Misclassified coastal tiles |
| Biome | Color-coded biome map | Biome collapse |
| Light Map | Brightness = light_map value | Dark tile artifacts |

Each overlay already has a key in the cycle (`o`). Add the missing ones (Discharge, Slope, Soil, Biome, Light Map).

---

## Implementation Priority

1. **pipeline_health test** (write now — catches 80% of bugs)
2. **CLAUDE.md rules** (add now — changes agent behavior immediately)
3. **Pre-commit hook** (add now — blocks bad commits)
4. **Missing diagnostic overlays** (add soon — helps human AND agent debug)
5. **Claude Code hooks** (add when hook system supports it)
6. **Visual snapshot comparison** (add later — needs ANSI parsing infrastructure)

---

## The Meta-Principle

The goal isn't just "write tests." It's: **make the AI agent's development loop include visual feedback by default.**

Human developers look at the screen after every change. AI agents look at test output. If the test output includes "here's what the terrain looks like as numbers" after every terrain change, the AI gets the equivalent of "looking at the screen."

The specific mechanisms (hooks, pre-commit, CLAUDE.md rules, overlays) are all just ways to ensure the numbers are always visible. The principle is: **close the loop automatically, don't rely on the agent remembering to check.**
