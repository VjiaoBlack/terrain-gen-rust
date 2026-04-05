# Meandering Rivers — Nick McDonald (Dec 2023)

Source: https://nickmcd.me/2023/12/12/meandering-rivers-in-particle-based-hydraulic-erosion-simulations/
Code: SimpleHydrology (CPU) https://github.com/weigert/SimpleHydrology
Latest: soillib (GPU/CUDA) https://github.com/erosiv/soillib

---

## Three Versions

1. **SimpleHydrology** (GitHub: weigert/SimpleHydrology) — CPU, what we ported to `src/hydrology.rs`
2. **soillib** (GitHub: erosiv/soillib) — GPU rewrite with significant algorithmic improvements
3. **Our port** — faithful to SimpleHydrology, missing soillib upgrades

---

## What soillib Changes vs SimpleHydrology

### 1. Velocity Integration (BIGGEST CHANGE)

SimpleHydrology (ours):
```
speed += gravity * normal / volume
speed += momentumTransfer * dot(norm(fspeed), norm(speed)) / (volume + discharge) * fspeed
```

soillib:
```
speed = g * normal + viscosity * average_speed
// Implicit Euler each step:
speed = 1/(1 + ds*(bedShear+viscosity)) * speed + ds*viscosity/(1 + ds*(bedShear+viscosity)) * average_speed
```

Where `average_speed = momentum[cell] / discharge[cell]`. Meandering emerges from viscous
coupling to bulk flow, not from the dot-product "momentum transfer" hack. Implicit Euler
is unconditionally stable.

### 2. Sediment Equilibrium (Stream Power Law)

SimpleHydrology:
```
c_eq = (1 + entrainment * discharge) * (h_here - h_next)
```

soillib:
```
slope = (h1 - h0) / distance
suspend = dt * ks * volume * slope * pow(discharge, 0.4)  // only downhill
deposit = dt * kd * sediment
transfer = deposit + suspend
```

Key: discharge^0.4 (stream power), separate suspend/deposit rates, activation function
(suspension only downhill).

### 3. Separate Bedrock + Sediment Buffers

soillib has `height` (bedrock) and `sediment` (loose) as separate fields. Erosion
removes sediment first, then bedrock. We only have one height buffer.

### 4. 5-Point Gradient Stencil (4th-order accurate)

```
grad = (1*f[-2] - 8*f[-1] + 8*f[+1] - 1*f[+2]) / 12
```

vs our simple 2-point central difference.

### 5. Dynamic Timestep

`ds = cell_distance / speed` — varies per step. Faster particles take shorter time steps.
SimpleHydrology normalizes speed to sqrt(2) (effectively ds=1).

### 6. Debris Flow (replaces cascade)

Path-integral based thermal erosion with bank stability function. Separate particle
system, not 8-neighbor cascade.

### 7. Sampling Probability Normalization

Track accumulation scaled by `1/(P*N)` — normalizes discharge to be independent
of sample count.

### 8. Real Physical Units

soillib uses meters, m/s, m^3/y, etc. SimpleHydrology uses dimensionless units.

---

## River Rendering (CRITICAL FOR US)

Rivers are NOT water tiles. They're rendered from the **discharge field**:

1. Normalize discharge: `river_alpha = erf(0.4 * discharge)`
2. Blend terrain color with water color: `color = mix(terrain, waterColor, river_alpha)`
   - waterColor = RGB(92, 133, 142) — blue-gray
3. Increase specular: `specularStrength = 0.05 + 0.55 * discharge`

In the 2D map view, discharge is shown directly as an overlay.
Momentum can be visualized as `erf(momentum)` mapped to red/green channels.

**We have the discharge field already. We just need to render it.**

---

## Parameters

### SimpleHydrology (what we have):
| Parameter | Value |
|-----------|-------|
| evapRate | 0.001 |
| depositionRate | 0.1 |
| minVol | 0.01 |
| maxAge | 500 |
| entrainment | 10.0 |
| gravity | 1.0 |
| momentumTransfer | 1.0 |
| lrate | 0.1 |
| maxdiff | 0.01 |
| settling | 0.8 |

### soillib (latest):
| Parameter | Value | Unit |
|-----------|-------|------|
| samples | 8192 | count |
| maxage | 128 | steps |
| lrate | 0.2 | - |
| gravity | 9.81 | m/s^2 |
| viscosity | 0.05 | m^2/s |
| bedShear | 0.025 | - |
| depositionRate | 0.01 | - |
| suspensionRate | 0.0007 | - |

---

## Upgrade Priority for Our Port

1. **Render discharge as rivers** — we have the data, just need coloring (QUICK WIN)
2. **Replace momentum transfer with viscosity-based implicit Euler** — key to meandering quality
3. **Separate suspension/deposition with discharge^0.4** — stream power law
4. **Add sediment buffer** separate from bedrock
5. **5-point gradient stencil** — better normals
6. **Dynamic timestep** — physically correct particle speeds
