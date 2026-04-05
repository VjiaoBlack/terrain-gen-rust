# Terrain Test Harness — Closing the Feedback Loop

**Status:** Design — not yet implemented
**Problem:** The human sees obvious visual problems instantly that the AI misses after hours of coding. We need automated tests that catch what the human catches.

---

## What the Human Catches That We Don't

| Human observation | What it means | How to detect automatically |
|---|---|---|
| "Everything is desert" | Biome distribution collapsed to one type | Count biome types, assert diversity > threshold |
| "Everything is river" | Discharge field flooded (too many particles) | Assert visible_discharge_tiles < 15% of land |
| "No rivers at all" | Discharge too low or rendering not wired | Assert visible_discharge_tiles > 1% of land |
| "Dark tiles everywhere" | Lighting normal scale too aggressive, or soil misclassification | Assert < 5% of tiles have light_map < 0.1 during daytime |
| "Terrain looks flat/ugly" | Not enough terrain detail, erosion too aggressive or too weak | Compute height variance, slope distribution, channel depth |
| "Pillars and gouges at coast" | Erosion artifact at ocean boundary | Check height gradient near water_level for spikes |
| "Rivers don't look real" | Channels not deep enough, no valley geometry | Measure cross-section profile at discharge peaks |
| "Peat on the beach" | Soil type priority ordering wrong | Check soil type at coastal tiles matches expectations |

## Test Categories

### 1. Biome Distribution Tests (catch "everything is desert")

```rust
#[test]
fn biome_diversity() {
    let result = run_pipeline(256, 256, &config);
    let mut counts: HashMap<Terrain, usize> = HashMap::new();
    for y in 0..256 {
        for x in 0..256 {
            if let Some(t) = result.map.get(x, y) {
                *counts.entry(*t).or_default() += 1;
            }
        }
    }
    let land_tiles = counts.iter()
        .filter(|(t, _)| **t != Terrain::Water)
        .map(|(_, c)| *c)
        .sum::<usize>();

    // No single biome should dominate > 60% of land
    for (terrain, count) in &counts {
        if *terrain == Terrain::Water { continue; }
        let pct = *count as f64 / land_tiles as f64;
        assert!(pct < 0.6,
            "{:?} is {:.1}% of land — too dominant", terrain, pct * 100.0);
    }
    // At least 3 different land biome types
    let land_types = counts.keys().filter(|t| **t != Terrain::Water).count();
    assert!(land_types >= 3, "only {} land biome types", land_types);
}
```

### 2. River Coverage Tests (catch "everything is river" / "no rivers")

```rust
#[test]
fn river_coverage_reasonable() {
    let result = run_pipeline(256, 256, &config);
    let land_count = result.heights.iter()
        .filter(|&&h| h > config.terrain.water_level).count();
    let visible_rivers = result.discharge.iter()
        .filter(|&&d| erf_approx(0.4 * d) > 0.1).count();
    let pct = visible_rivers as f64 / land_count as f64;

    assert!(pct > 0.005, "no visible rivers ({:.2}%)", pct * 100.0);
    assert!(pct < 0.15, "too many river tiles ({:.1}%) — flooding", pct * 100.0);
}
```

### 3. Channel Depth Tests (catch "rivers don't look real")

```rust
#[test]
fn rivers_carve_valleys() {
    let result = run_pipeline(256, 256, &config);
    // Find top 1% discharge tiles (river channels)
    let mut sorted_discharge: Vec<(usize, f64)> = result.discharge.iter()
        .enumerate()
        .map(|(i, &d)| (i, d))
        .collect();
    sorted_discharge.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let top_1pct = &sorted_discharge[..sorted_discharge.len() / 100];

    // River channel tiles should be LOWER than their non-river neighbors
    let mut channel_lower_count = 0;
    let w = 256;
    for &(i, _) in top_1pct {
        let x = i % w;
        let y = i / w;
        if x == 0 || x == w-1 || y == 0 || y == w-1 { continue; }
        let h = result.heights[i];
        let neighbors = [
            result.heights[i-1], result.heights[i+1],
            result.heights[i-w], result.heights[i+w],
        ];
        let avg_neighbor = neighbors.iter().sum::<f64>() / 4.0;
        if h < avg_neighbor - 0.001 {
            channel_lower_count += 1;
        }
    }
    let pct_lower = channel_lower_count as f64 / top_1pct.len() as f64;
    assert!(pct_lower > 0.3,
        "only {:.0}% of river channels are lower than neighbors — \
         erosion isn't carving valleys", pct_lower * 100.0);
}
```

### 4. Coastal Artifact Tests (catch "pillars and gouges")

```rust
#[test]
fn no_coastal_height_spikes() {
    let result = run_pipeline(256, 256, &config);
    let wl = config.terrain.water_level;
    let w = 256;
    let mut spikes = 0;
    let mut coastal_tiles = 0;

    for y in 1..255 {
        for x in 1..255 {
            let i = y * w + x;
            if result.heights[i] <= wl { continue; }
            // Is this a coastal tile? (adjacent to water)
            let neighbors = [i-1, i+1, i-w, i+w];
            let adjacent_water = neighbors.iter()
                .any(|&ni| result.heights[ni] <= wl);
            if !adjacent_water { continue; }
            coastal_tiles += 1;

            // Check for spikes: much higher than other coastal neighbors
            let land_neighbors: Vec<f64> = neighbors.iter()
                .filter(|&&ni| result.heights[ni] > wl)
                .map(|&ni| result.heights[ni])
                .collect();
            if land_neighbors.is_empty() { continue; }
            let avg_land = land_neighbors.iter().sum::<f64>() / land_neighbors.len() as f64;
            if (result.heights[i] - avg_land).abs() > 0.05 {
                spikes += 1;
            }
        }
    }
    let spike_pct = if coastal_tiles > 0 {
        spikes as f64 / coastal_tiles as f64
    } else { 0.0 };
    assert!(spike_pct < 0.1,
        "{} coastal spikes out of {} coastal tiles ({:.1}%)",
        spikes, coastal_tiles, spike_pct * 100.0);
}
```

### 5. Lighting Sanity Tests (catch "dark tiles everywhere")

```rust
#[test]
fn no_excessive_dark_tiles_at_midday() {
    let mut game = Game::new(60, 100);
    // Advance to midday
    while game.day_night.hour() < 12.0 {
        game.day_night.tick();
    }
    game.day_night.compute_lighting(
        &game.heights, game.map.width, game.map.height,
        0, 0, game.map.width, game.map.height,
    );
    let dark_tiles = game.day_night.light_map.iter()
        .filter(|&&l| l < 0.1).count();
    let total = game.day_night.light_map.len();
    let pct = dark_tiles as f64 / total as f64;
    assert!(pct < 0.05,
        "{:.1}% of tiles are dark at midday — lighting or normal issue",
        pct * 100.0);
}
```

### 6. Visual Regression Snapshot

```rust
#[test]
fn terrain_snapshot_regression() {
    let result = run_pipeline(64, 64, &config_with_seed(42));

    // Compute summary stats that should be stable across code changes
    let avg_height = result.heights.iter().sum::<f64>() / result.heights.len() as f64;
    let water_pct = result.heights.iter()
        .filter(|&&h| h <= config.terrain.water_level).count() as f64
        / result.heights.len() as f64;
    let avg_discharge = result.discharge.iter().sum::<f64>() / result.discharge.len() as f64;

    // These values are from a known-good run. Update when intentionally
    // changing the pipeline, but flag if they drift accidentally.
    // Tolerance: 20% relative change triggers investigation.
    assert!((avg_height - 0.55).abs() < 0.11,
        "avg height drifted: {avg_height:.3} (expected ~0.55)");
    assert!((water_pct - 0.20).abs() < 0.10,
        "water coverage drifted: {water_pct:.2} (expected ~0.20)");
}
```

## Harness Architecture

### Quick visual check (for AI agent — no human needed)

```bash
cargo run --release -- --play --seed 100 --ticks 1 --inputs "ansi"
```

Capture the ANSI frame, parse it, check:
- How many distinct colors are visible? (< 5 = everything looks the same)
- What fraction of tiles are blue? (> 50% = flooded / < 1% = no water)
- What fraction are brown/grey? (> 60% = all desert/tundra)
- Are there any `~` characters? (0 = no visible water)

### Automated diagnostic after every pipeline change

Run before committing any erosion/terrain change:

```rust
fn pipeline_health_check(result: &PipelineResult, w: usize, h: usize) {
    let n = w * h;

    // 1. Height distribution
    let avg_h = result.heights.iter().sum::<f64>() / n as f64;
    let water_pct = result.heights.iter()
        .filter(|&&h| h <= water_level).count() as f64 / n as f64;
    eprintln!("Height: avg={avg_h:.3} water={water_pct:.1}%");
    assert!(water_pct > 0.05 && water_pct < 0.50, "water coverage out of range");

    // 2. Biome diversity
    let biome_counts = count_biomes(&result.map, w, h);
    let max_biome_pct = biome_counts.values().max().unwrap() as f64 / n as f64;
    eprintln!("Biomes: {} types, max={:.1}%", biome_counts.len(), max_biome_pct * 100.0);
    assert!(biome_counts.len() >= 3, "too few biome types");

    // 3. Discharge / rivers
    let max_d = result.discharge.iter().cloned().fold(0.0f64, f64::max);
    let visible = result.discharge.iter()
        .filter(|&&d| erf_approx(0.4 * d) > 0.1).count();
    let visible_pct = visible as f64 / n as f64;
    eprintln!("Discharge: max={max_d:.3} visible={visible} ({visible_pct:.2}%)");
    assert!(visible > 0, "no visible rivers at all");
    assert!(visible_pct < 0.20, "too many river tiles");

    // 4. Soil diversity
    let soil_counts = count_soil_types(&result.soil);
    eprintln!("Soil: {} types", soil_counts.len());

    // 5. Slope distribution (erosion should create variance)
    let avg_slope = result.slope.iter().sum::<f64>() / n as f64;
    let max_slope = result.slope.iter().cloned().fold(0.0f64, f64::max);
    eprintln!("Slope: avg={avg_slope:.4} max={max_slope:.3}");

    eprintln!("--- PIPELINE HEALTH: OK ---");
}
```

### The key principle

**If the human can see it's wrong by looking at it for 2 seconds, there should be a test that catches it without a human.** Every time the human reports a visual problem, we add a test that would have caught it. This is how the harness grows — from actual bugs, not from imagined requirements.

## Current Gaps (tests we should have had)

| Bug we shipped | Test we should have had |
|---|---|
| Everything is desert (scale 0.008) | Biome diversity test |
| Everything is river (7864 particles) | River coverage upper bound test |
| No rivers visible (landscape mode missing rendering) | Assert `~` chars in showcase ANSI output |
| Dark tiles from lighting (scale 40) | Midday dark tile count |
| Peat on beaches (soil priority wrong) | Coastal soil type check |
| Scale mismatch (game 0.02 vs pipeline 0.015) | Snapshot regression on known seed |
