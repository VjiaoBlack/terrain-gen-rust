# Game Dev Harness Research
*Practical tools and patterns for simulation game development infrastructure*

---

## 1. Terrain Visualization at Multiple Scales

The canonical approach is **heatmap overlays keyed to a scalar field** — Unity's Terrain Tools ship an Altitude Heatmap that colors elevation into banded ranges. For a terminal game the equivalent is an ANSI-colored character grid where the color encodes moisture, elevation, biome, or temperature rather than raw tile glyphs.

**Practical takeaways:**
- Build one `DiagnosticView` trait with implementations per layer (elevation, moisture, biome, settlement density). Switch views with a single keypress.
- Histograms (value distribution over the whole map) complement heatmaps. A one-line sparkline histogram (`▁▂▃▅▆▇`) printed below the map gives instant feedback on whether a generation pass produced a reasonable distribution.
- A dedicated "statistics pass" after generation (min/max/mean/stddev per field) should be part of every pipeline run, not a separate command.

---

## 2. Downsampled Map Rendering

Mipmaps in graphics are pre-computed half-resolution copies of a texture, used for level-of-detail. The same idea works for overview maps:

- **Majority-vote (mode) downsampling**: for categorical data (biomes, terrain types), the output cell takes the most frequent value in its N×N source block. This preserves recognizable biome shapes at small sizes.
- **Average/median downsampling**: for continuous fields (elevation, moisture), average or median over the source block. Median is more robust to cliffs.
- **Max downsampling**: for sparse features (settlements, rivers) you want to see even in small tiles, use max — a settlement present anywhere in the block propagates to the overview.

For a 256×256 → 8×12 summary, each output cell covers roughly 32×21 source cells. A good strategy: majority-vote the biome glyph, overlay a marker if any settlement exists in the block, color by average elevation.

---

## 3. Simulation State Inspection — Lessons from Real Games

**Factorio** (F5/F4 debug menu) is the gold standard:
- Per-chunk overlays: pollution, active entity count, enemy expansion candidates (color-coded by threat level)
- Entity-level overlays: collision boxes, electric network IDs, fluid flow arrows
- Performance counters per tick (FPS, UPS, GPU time, Lua GC stats)
- All toggleable independently — critical for not drowning in noise

**RimWorld** development mode adds inspector panels that show live internal state of any selected entity, plus a log of all AI decisions and mood calculations.

**Pattern to adopt:** a layered debug menu (F-keys or `/debug` subcommands) where each layer is independently toggled. Keep the default view clean; add noise only when diagnosing a specific system.

---

## 4. Automated Architecture Doc Generation for Rust

Two tools that actually work on a Rust terminal project:

**`cargo-modules`** (`cargo install cargo-modules`, crate: `regexident/cargo-modules`)
- `cargo modules generate tree` — hierarchical module tree, shows `pub`/`pub(crate)`/private
- `cargo modules generate graph` — internal dependency graph in Graphviz DOT format
- `cargo modules generate graph --acyclic` — fails with an error if cycles exist (useful as a CI gate)

**`cargo-depgraph`** (`cargo install cargo-depgraph`, crate: `jplatte/cargo-depgraph`)
- Visualizes inter-crate dependencies across a workspace
- Output: Graphviz DOT, pipe to `dot -Tsvg` or an online renderer
- Color-codes dev-deps (blue), build-deps (green), optional deps (dotted)

For keeping `ARCHITECTURE.md` in sync: run `cargo modules generate tree > /tmp/current_tree.txt` in CI and diff against a committed snapshot. A change in the tree fails the check and forces a manual doc update.

---

## 5. Keeping Docs in Sync with Code

**Rust doc-tests** (`cargo test --doc`) compile and run every code block in `///` comments. Any example that becomes wrong breaks the build. This is the highest-fidelity sync mechanism in the Rust ecosystem.

**Intra-doc links** (`[`SomeStruct`]`) are compiler-checked. Broken links are warnings (configurable to errors with `#![deny(broken_intra_doc_links)]`).

**Patterns that work:**
- Replace prose descriptions of function behavior with doc-tested examples — if the example stays green, the description is implicitly still accurate.
- Add a `# Architecture` section to each module's top-level comment. These are visible in `cargo doc` output and live next to the code they describe.
- For higher-level docs (ARCHITECTURE.md, TODO.md), use a CI script that checks word counts or section headers against a manifest — crude but it catches complete deletions.

---

## 6. Test-Driven Simulation Development

**Property-based testing** is the most powerful technique for simulation systems:

- **`proptest`** (preferred over quickcheck for Rust) — define invariants as properties, generate thousands of random inputs. Example: "after any pipeline run, every tile's moisture must be in [0.0, 1.0]" or "no river tile is more than 3 tiles from a water source."
- **`quickcheck`** — simpler API, good for pure functions. MSRV 1.85 as of Feb 2026.

**Patterns from Bevy/game-dev TDD:**
- Test simulation systems in isolation with minimal world state — no rendering, no I/O.
- For determinism testing: run the same seed twice and assert the outputs are identical byte-for-byte.
- For regression testing: commit a known-good output snapshot (`insta` crate) and fail if generation changes unexpectedly.

**What not to do:** testing procedural output for exact values is brittle. Test invariants and distributions instead ("biome X covers 5–20% of the map for seed range Y", not "tile (3,7) is Forest").

---

## Recommended Toolchain for This Project

| Need | Tool |
|---|---|
| Module structure graph | `cargo modules generate tree` |
| Crate dependency graph | `cargo-depgraph \| dot -Tsvg` |
| Simulation invariants | `proptest` |
| Doc freshness | `cargo test --doc` + intra-doc links |
| Multi-scale map view | Custom `DiagnosticView` with majority-vote downsampling |
| Debug layer toggles | F-key or subcommand system modeled on Factorio F4/F5 |
