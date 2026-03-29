# Pipeline Overview

## Generation Pipeline (ordered stages)

```
1. Base height        -- fBm + ridged noise + domain warping for macroforms
2. Height shaping     -- Terrace/plateau shaping (masked to ridges + high elev), light smoothing
3. Stability pass     -- Thermal talus relaxation (remove spikes, form scree slopes)
4. Hydrology precomp  -- Depression filling -> flow direction -> flow accumulation -> river candidates
5. River carving      -- Carve riverbeds/valleys from flow+discharge, optional droplet erosion for detail
6. Climate + biomes   -- Temperature + rainfall/moisture (inc. rain shadows) -> Whittaker classification
7. Soils + groundwater -- Soil types from slope+alluvial+wetness; lightweight water-table for springs/fertility
```

## Scaling Strategy

### At 256x256 (~65k cells)
- Can afford multiple full-grid passes (dozens to hundreds of iterations)
- Tens of thousands of droplet erosion steps in under a few seconds in optimized Rust
- No need for multi-resolution tricks

### At 8k x 8k (future)
- Compute rivers/climate on a coarse grid (512-2048), then upsample + add local detail noise
- Expensive erosion only in "interesting bands" (near rivers, steep slopes, mountain belts)
- Naive grid erosion can scale cubically in grid dimension -- avoid full-resolution simulation passes

## Recommended Parameters for 256x256

- Thermal erosion: 15-120 iterations, all cheap at this size
- Droplet erosion: up to tens of thousands of droplets, still fast
- Depression filling: O(n log n) with Priority-Flood, trivial at 65k cells
- Moisture diffusion: full BFS from water tiles is fast

## Key Pitfalls

- Do NOT try to make one erosion pass produce everything (rivers, valleys, cliffs). Use explicit maps per feature.
- Information propagation across grid can be slow in iterative erosion -- not a problem at 256x256 but will matter if map size increases.
- Keep pipeline stages independent so they can be tuned/debugged separately.

## Implementation Priority

This is the foundation. The pipeline ordering IS the implementation order. Get stages 1-3 working first (base height + cliffs), then hydrology, then climate/soil.
