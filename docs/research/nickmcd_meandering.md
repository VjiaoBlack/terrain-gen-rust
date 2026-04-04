# Nick McDonald — Meandering Rivers in Particle-Based Hydraulic Erosion

**Source:** https://nickmcd.me/2023/12/12/meandering-rivers-in-particle-based-hydraulic-erosion-simulations/  
**Code:** https://github.com/weigert/SimpleHydrology (C++, ~700 stars, Jan 2023 update adds meandering)  
**Modern library:** https://github.com/erosiv/soillib (consolidating all erosion work, forward target)

---

## 1. How It Works (Particle-Based)

McDonald's system is already particle-based (identical model to ours): particles descend the heightmap, eroding/depositing sediment via a capacity equation. The meandering extension adds a **momentum map** — four floats per cell (`momentumx`, `momentumy`, and their tracking counterparts). At each particle timestep, the particle's velocity is accumulated into the local momentum tracking values. A post-pass exponentially averages these into the persistent momentum map.

Particles then receive a directional force proportional to local stream momentum, scaled by the dot-product of their current velocity against that momentum (so only particles moving roughly with the stream are accelerated, not those crossing it). This is the entire change.

## 2. The Key Insight

The original problem: particles are **decoupled** — no particle affects another's trajectory. Rivers couldn't sustain curves because there was no mechanism for the outer bank to erode faster than the inner bank.

The fix: **momentum conservation makes high-velocity zones self-reinforcing.** A slight bend causes particles to carry momentum into the outer bank. That momentum increases their effective velocity there, which (via the discharge-scaled sediment capacity equation) causes more erosion on the outer bank and deposition on the inner bank. The curve deepens. This continues until a cutoff event. No explicit curve-following logic is needed — it emerges from the capacity equation already in place.

Side effect: streams are also more stable over long distances, reducing the braiding/silt-basin problem.

## 3. Performance and Complexity

- **Memory:** 4 additional floats per cell. Negligible.
- **Compute:** One accumulation step per particle per timestep + one exponential average pass. Essentially free alongside existing discharge tracking.
- **Code invasiveness:** McDonald calls it "quite minimal and non-invasive." The momentum force is injected into the existing particle update loop; no structural changes needed.
- **Implementation risk:** Low. The momentum map is independent of the height/sediment maps and can be toggled off cleanly.

## 4. vs. aparis69/Meandering-rivers (Paris et al. 2023)

| | McDonald (particle erosion) | Paris et al. (vector-based) |
|---|---|---|
| **Mechanism** | Emergent from momentum accumulation in existing erosion particles | Physically-based migration equations with curvature and control terms |
| **Integration** | Modifies existing heightmap erosion | Simulates river path evolution separately from terrain |
| **Speed** | Adds ~0% overhead to erosion pass | Runs at interactive rates, but separate pipeline |
| **Realism source** | Geomorphology emerges from physics | Calibrated against real river sinuosity/wavelength data |
| **Oxbows/cutoffs** | Can emerge naturally | Explicitly modeled as events |
| **Best for** | Terrain that needs rivers and erosion together | Detailed river path design overlaid on existing terrain |

For terrain-gen-rust, McDonald's approach is the better fit — it works within our existing particle erosion pass rather than requiring a separate river simulation layer.

## 5. Compatibility with Analytical SPL Erosion (Tzathas 2024)

Yes, compatible. The momentum map is maintained by the particle pass. The SPL/analytical pass operates on the heightmap directly and does not interact with momentum tracking. They can be run in separate stages:
1. Analytical SPL pass for large-scale valley/ridge shaping
2. Particle pass (with momentum) for fine-scale river channel carving and meandering

The only integration concern: if the analytical pass reshapes terrain between particle passes, momentum maps may briefly point "wrong" directions. Flushing or decaying the momentum map between major analytical passes would prevent artifacts.

## 6. Open Source Code

- **SimpleHydrology** (C++): https://github.com/weigert/SimpleHydrology — the reference implementation; `water.h` contains the particle + momentum logic
- **soillib** (C++): https://github.com/erosiv/soillib — newer unified library; McDonald's active development target; visualization tools included
- License: available for study and adaptation

---

## Verdict

This is a strong fit. The technique slots directly into our existing particle erosion architecture, costs nearly nothing in compute/memory, and produces the outer-bank erosion behavior that makes rivers carve realistic channels rather than fill basins with silt. Implementation path: add `momentum_x/y` fields to the erosion grid, accumulate during the particle descent loop, apply force during velocity update. Estimated integration effort: 1–2 days.
