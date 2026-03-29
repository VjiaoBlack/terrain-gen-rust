# Hydrology (Depression Filling, Flow Accumulation, Rivers)

Goal: Connected river networks that reach the sea, merge naturally, and are 2-5 tiles wide.

## Algorithm

### Step A: Depression Filling (Priority-Flood)

Ensures every cell can drain (no tiny basins trapping flow unless you explicitly want lakes).

```pseudocode
// Priority-Flood: fill depressions so every cell drains to boundary
priority_queue PQ  // min-heap by elevation
visited = set()

// seed: all boundary cells
for each boundary cell c:
    PQ.push(c, elevation[c])
    visited.add(c)

while PQ not empty:
    c = PQ.pop_min()
    for each neighbor n of c:
        if n not in visited:
            visited.add(n)
            if elevation[n] < elevation[c]:
                elevation[n] = elevation[c]  // fill the depression
            PQ.push(n, max(elevation[n], elevation[c]))
```

Complexity: O(n log n) for floating-point elevations, O(n) for integer. Trivial at 65k cells.

### Step B: Flow Direction (D8)

```pseudocode
for each cell c:
    best_neighbor = argmax over 8 neighbors n of (elevation[c] - elevation[n]) / distance(c, n)
    flow_dir[c] = best_neighbor
    // distance is 1.0 for cardinal, sqrt(2) for diagonal
```

D8 gives tree-like rivers (single downstream direction per cell). Use MFD (below) if you want broader flow patterns.

**MFD alternative** (for moisture/erosion, not river centerlines):
```pseudocode
for each cell c:
    for each downslope neighbor n:
        weight[n] = max(0, elevation[c] - elevation[n])^p  // p=1.0 to 1.1
    normalize weights to sum to 1.0
    flow_fraction[c -> n] = weight[n]
```

### Step C: Flow Accumulation

```pseudocode
// Process cells in decreasing elevation order (highest first)
sort cells by elevation descending
A[all] = 1  // each cell contributes 1 unit of area

for each cell c in sorted order:
    n = flow_dir[c]  // downstream neighbor
    A[n] += A[c]
```

### Step D: Channel Extraction

```pseudocode
for each cell c:
    if A[c] >= A_river:
        mark c as river
```

With D8, the river network is inherently a directed forest that merges downstream.

### Step E: River Width from Discharge

```pseudocode
Q[c] = A[c] * rain_norm[c] * cell_area

// Power-law width scaling (Leopold relation)
width_tiles[c] = clamp(min_w, max_w, round(k * Q[c]^b))
```

Calibration: pick reference discharge Q* (90th percentile of Q among river cells), set `k = target_width / (Q*^b)`.

### Step F: Valley Carving

```pseudocode
for each river cell r:
    w = width_tiles[r]
    for each cell p within radius (w/2 + bank_zone):
        d = distance(p, r)
        if d < w / 2:
            target = H[r] - bed_depth(Q[r])
        else:
            t = (d - w/2) / bank_zone
            target = H[r] - bed_depth(Q[r]) * falloff(t)  // e.g., (1-t)^2
        H[p] = min(H[p], target)

// Then run a short thermal erosion pass to smooth bank edges
```

`bed_depth(Q)` = `depth0 + depth1 * log(1 + Q)`, capped to avoid cutting canyons everywhere.

## Recommended Parameters for 256x256

| Parameter | Starting Value | Notes |
|-----------|---------------|-------|
| `A_river` (accumulation threshold) | 200-800 | Lower = more rivers, higher = fewer/larger |
| Width exponent `b` | 0.5 (square root) | Classical Leopold scaling |
| `min_w` | 2 tiles | Minimum visible river |
| `max_w` | 5 tiles | Art/gameplay cap |
| `k` | Calibrate from 90th percentile Q | Target median main river = 3 tiles |
| `bed_depth0` | 0.005 (normalized height) | Tune visually |
| `bed_depth1` | 0.002 | Log scaling factor |
| `bank_zone` | 2-4 tiles | Transition width from riverbed to terrain |

## Implementation Priority

1. Depression filling (Priority-Flood) -- absolutely critical, without it rivers die in pits
2. D8 flow direction + flow accumulation -- gives the river network
3. Channel extraction with threshold
4. River width from discharge + valley carving
5. Later: MFD for moisture/erosion fields (not needed for river centerlines)

## Key Pitfalls

- **Rivers stop after 5-10 tiles**: Local minima trap flow. Depression filling MUST come before flow routing.
- **Parallel "ladder" rivers on flats**: D8 on flat regions creates artifacts. Fix: add slight random perturbation to flat elevations before flow routing, or use flat-area tie-breaking heuristics.
- **Rivers are 1 pixel wide**: Drawing only centerlines. Must render width field AND apply valley carving from discharge.
- **D8 concentrates flow into narrow lines**: This is correct for river centerlines but bad for erosion/moisture. Use MFD for those fields.
