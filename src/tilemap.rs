use serde::{Deserialize, Serialize};

use crate::renderer::{Color, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Terrain {
    Water,
    Sand,
    Grass,
    Forest,
    Mountain,
    Snow,
    Cliff,
    Marsh,
    Desert,
    Tundra,
    Scrubland,
    Stump,
    Bare,
    Sapling,
    Quarry,
    QuarryDeep,
    ScarredGround,
    BuildingFloor,
    BuildingWall,
    Road,
    /// Shallow river crossing — rare natural ford at narrow river points.
    Ford,
    /// Player-built bridge over water — enables crossing rivers.
    Bridge,
    /// Frozen water — walkable winter crossing, reverts to Water in spring.
    Ice,
    /// Temporary spring flood — impassable, sediment-laden water near rivers.
    FloodWater,
    /// Active fire — walkable but dangerous, very high A* cost.
    Burning,
    /// Burned-out ground — dark grey, walkable, blocks further fire spread.
    Scorched,
}

impl Terrain {
    pub fn is_walkable(&self) -> bool {
        match self {
            Terrain::Water | Terrain::BuildingWall | Terrain::Cliff | Terrain::FloodWater => false,
            _ => true, // Burning and Scorched are walkable
        }
    }

    /// Characters chosen for similar visual density — subtle texture, not brightness.
    pub fn ch(&self) -> char {
        match self {
            Terrain::Water => '~',
            Terrain::Sand => '·',
            Terrain::Grass => '\'',
            Terrain::Forest => ':',
            Terrain::Mountain => '^',
            Terrain::Snow => '·',
            Terrain::Cliff => '#',
            Terrain::Marsh => ',',
            Terrain::Desert => '.',
            Terrain::Tundra => '-',
            Terrain::Scrubland => ';',
            Terrain::Stump => '%',
            Terrain::Bare => '.',
            Terrain::Sapling => '!',
            Terrain::Quarry => 'U',
            Terrain::QuarryDeep => 'V',
            Terrain::ScarredGround => '.',
            Terrain::BuildingFloor => '░',
            Terrain::BuildingWall => '█',
            Terrain::Road => '=',
            Terrain::Ford => '~',
            Terrain::Bridge => '#',
            Terrain::Ice => '=',
            Terrain::FloodWater => '~',
            Terrain::Burning => '*',
            Terrain::Scorched => '`',
        }
    }

    /// Foreground: subtle texture color, close to bg so character density doesn't dominate.
    pub fn fg(&self) -> Color {
        match self {
            Terrain::Water => Color(60, 110, 220),
            Terrain::Sand => Color(190, 165, 90),
            Terrain::Grass => Color(45, 140, 45),
            Terrain::Forest => Color(15, 80, 20),
            Terrain::Mountain => Color(120, 110, 100),
            Terrain::Snow => Color(220, 220, 240),
            Terrain::Cliff => Color(100, 90, 80),
            Terrain::Marsh => Color(40, 90, 60),
            Terrain::Desert => Color(200, 180, 120),
            Terrain::Tundra => Color(160, 170, 180),
            Terrain::Scrubland => Color(130, 120, 60),
            Terrain::Stump => Color(100, 80, 40),
            Terrain::Bare => Color(90, 80, 50),
            Terrain::Sapling => Color(30, 120, 30),
            Terrain::Quarry => Color(140, 130, 115),
            Terrain::QuarryDeep => Color(110, 100, 90),
            Terrain::ScarredGround => Color(145, 135, 120),
            Terrain::BuildingFloor => Color(140, 120, 90),
            Terrain::BuildingWall => Color(160, 140, 110),
            Terrain::Road => Color(160, 130, 80),
            Terrain::Ford => Color(80, 140, 220),
            Terrain::Bridge => Color(140, 100, 50),
            Terrain::Ice => Color(180, 210, 240),
            Terrain::FloodWater => Color(100, 150, 200),
            Terrain::Burning => Color(255, 120, 20),
            Terrain::Scorched => Color(80, 70, 60),
        }
    }

    /// Background: every terrain gets a bg color so lighting controls perceived brightness.
    pub fn bg(&self) -> Option<Color> {
        match self {
            Terrain::Water => Some(Color(20, 40, 100)),
            Terrain::Sand => Some(Color(170, 145, 80)),
            Terrain::Grass => Some(Color(30, 100, 30)),
            Terrain::Forest => Some(Color(10, 60, 15)),
            Terrain::Mountain => Some(Color(95, 85, 75)),
            Terrain::Snow => Some(Color(200, 200, 215)),
            Terrain::Cliff => Some(Color(70, 65, 55)),
            Terrain::Marsh => Some(Color(25, 60, 40)),
            Terrain::Desert => Some(Color(180, 160, 100)),
            Terrain::Tundra => Some(Color(140, 150, 160)),
            Terrain::Scrubland => Some(Color(110, 100, 50)),
            Terrain::Stump => Some(Color(40, 90, 30)),
            Terrain::Bare => Some(Color(55, 90, 40)),
            Terrain::Sapling => Some(Color(35, 95, 30)),
            Terrain::Quarry => Some(Color(90, 80, 70)),
            Terrain::QuarryDeep => Some(Color(65, 58, 50)),
            Terrain::ScarredGround => Some(Color(115, 105, 90)),
            Terrain::BuildingFloor => Some(Color(100, 80, 60)),
            Terrain::BuildingWall => Some(Color(120, 100, 80)),
            Terrain::Road => Some(Color(130, 105, 65)),
            Terrain::Ford => Some(Color(40, 70, 120)),
            Terrain::Bridge => Some(Color(80, 60, 30)),
            Terrain::Ice => Some(Color(120, 150, 180)),
            Terrain::FloodWater => Some(Color(40, 70, 110)),
            Terrain::Burning => Some(Color(180, 40, 10)),
            Terrain::Scorched => Some(Color(40, 35, 25)),
        }
    }

    // --- Map Mode rendering: flat symbolic glyphs, no lighting ---

    /// Map Mode glyph: one semantic symbol per terrain type.
    pub fn map_ch(&self) -> char {
        match self {
            Terrain::Water | Terrain::FloodWater => '~',
            Terrain::Sand => ',',
            Terrain::Grass => '.',
            Terrain::Forest => '\u{2660}',   // ♠
            Terrain::Mountain => '\u{25B2}', // ▲
            Terrain::Snow => '*',
            Terrain::Cliff => '#',
            Terrain::Marsh => '"',
            Terrain::Desert => ':',
            Terrain::Tundra => '-',
            Terrain::Scrubland => ';',
            Terrain::Stump => '%',
            Terrain::Bare => '.',
            Terrain::Sapling => '\'',
            Terrain::Quarry => 'U',
            Terrain::QuarryDeep => 'V',
            Terrain::ScarredGround => '.',
            Terrain::BuildingFloor => '+',
            Terrain::BuildingWall => '#',
            Terrain::Road => '=',
            Terrain::Ford => '~',
            Terrain::Bridge => '#',
            Terrain::Ice => '=',
            Terrain::Burning => '*',
            Terrain::Scorched => '`',
        }
    }

    /// Map Mode foreground: flat, low-saturation color. No lighting applied.
    pub fn map_fg(&self) -> Color {
        match self {
            Terrain::Water | Terrain::FloodWater => Color(70, 120, 220),
            Terrain::Sand => Color(190, 170, 100),
            Terrain::Grass => Color(60, 140, 60),
            Terrain::Forest => Color(20, 100, 25),
            Terrain::Mountain => Color(140, 130, 120),
            Terrain::Snow => Color(220, 225, 240),
            Terrain::Cliff => Color(110, 100, 85),
            Terrain::Marsh => Color(50, 110, 70),
            Terrain::Desert => Color(200, 180, 120),
            Terrain::Tundra => Color(155, 165, 175),
            Terrain::Scrubland => Color(140, 125, 65),
            Terrain::Stump => Color(100, 80, 40),
            Terrain::Bare => Color(90, 80, 50),
            Terrain::Sapling => Color(50, 150, 50),
            Terrain::Quarry => Color(140, 130, 115),
            Terrain::QuarryDeep => Color(110, 100, 90),
            Terrain::ScarredGround => Color(145, 135, 120),
            Terrain::BuildingFloor => Color(150, 130, 100),
            Terrain::BuildingWall => Color(170, 150, 120),
            Terrain::Road => Color(170, 145, 90),
            Terrain::Ford => Color(80, 140, 220),
            Terrain::Bridge => Color(140, 100, 50),
            Terrain::Ice => Color(180, 210, 240),
            Terrain::Burning => Color(255, 120, 20),
            Terrain::Scorched => Color(80, 70, 60),
        }
    }

    /// Map Mode background: muted, paired with map_fg. No lighting.
    pub fn map_bg(&self) -> Color {
        match self {
            Terrain::Water | Terrain::FloodWater => Color(20, 40, 110),
            Terrain::Sand => Color(150, 135, 80),
            Terrain::Grass => Color(30, 80, 30),
            Terrain::Forest => Color(15, 60, 18),
            Terrain::Mountain => Color(90, 82, 75),
            Terrain::Snow => Color(180, 185, 200),
            Terrain::Cliff => Color(65, 60, 50),
            Terrain::Marsh => Color(30, 65, 45),
            Terrain::Desert => Color(160, 140, 90),
            Terrain::Tundra => Color(120, 130, 140),
            Terrain::Scrubland => Color(100, 90, 45),
            Terrain::Stump => Color(40, 60, 30),
            Terrain::Bare => Color(55, 50, 35),
            Terrain::Sapling => Color(30, 80, 30),
            Terrain::Quarry => Color(90, 80, 70),
            Terrain::QuarryDeep => Color(65, 58, 50),
            Terrain::ScarredGround => Color(115, 105, 90),
            Terrain::BuildingFloor => Color(100, 85, 65),
            Terrain::BuildingWall => Color(120, 105, 85),
            Terrain::Road => Color(120, 100, 60),
            Terrain::Ford => Color(40, 70, 120),
            Terrain::Bridge => Color(80, 60, 30),
            Terrain::Ice => Color(120, 150, 180),
            Terrain::Burning => Color(180, 40, 10),
            Terrain::Scorched => Color(40, 35, 25),
        }
    }

    // --- Landscape Mode rendering: texture chars, hand-picked muted palettes ---

    /// Landscape Mode texture pool: characters are surface noise, not semantic.
    /// Returns a slice of characters for deterministic per-tile selection.
    pub fn landscape_texture_pool(&self) -> &'static [char] {
        match self {
            Terrain::Grass | Terrain::Bare | Terrain::Sapling => &['.', '\'', ',', ' '],
            Terrain::Sand => &['.', ':', ',', ' '],
            Terrain::Desert => &['.', '\'', ' ', ' '],
            Terrain::Forest => &['"', ':', ';', '%'],
            Terrain::Scrubland => &[';', '\'', ',', ':'],
            Terrain::Mountain => &['^', ':', '#', '%'],
            Terrain::Cliff => &['#', '%', '|', ':'],
            Terrain::Snow => &['.', ' ', '\'', ','],
            Terrain::Tundra => &['-', '.', '\'', ','],
            Terrain::Marsh => &[',', '~', '.', ';'],
            Terrain::Water | Terrain::FloodWater | Terrain::Ford => &['~', '~', '~', '~'],
            Terrain::Ice => &['=', '-', '=', '-'],
            Terrain::Road => &['=', '-', '=', '-'],
            Terrain::BuildingFloor => &['+', '.', '+', '.'],
            Terrain::BuildingWall => &['#', '#', '#', '#'],
            Terrain::Stump => &['%', '.', '%', '.'],
            Terrain::Quarry | Terrain::QuarryDeep => &[':', '#', '%', ':'],
            Terrain::ScarredGround | Terrain::Scorched => &['.', '`', '.', ','],
            Terrain::Bridge => &['#', '=', '#', '='],
            Terrain::Burning => &['*', '^', '*', '.'],
        }
    }

    /// Landscape Mode character: driven by vegetation density + position noise.
    /// High vegetation = dense chars, low = sparse, zero = bare dirt.
    pub fn landscape_ch(&self, wx: usize, wy: usize, vegetation: f64) -> char {
        // Base position noise (cheap hash, avoids visible diagonal stripes)
        let noise =
            ((wx.wrapping_mul(31) ^ wy.wrapping_mul(17) ^ wx.wrapping_mul(wy).wrapping_add(7))
                % 256) as f64
                / 256.0;

        // For terrain types that don't care about vegetation, use noise-based pool selection
        match self {
            Terrain::Water
            | Terrain::FloodWater
            | Terrain::Ford
            | Terrain::Ice
            | Terrain::Road
            | Terrain::BuildingFloor
            | Terrain::BuildingWall
            | Terrain::Bridge
            | Terrain::Burning
            | Terrain::Quarry
            | Terrain::QuarryDeep
            | Terrain::ScarredGround
            | Terrain::Scorched
            | Terrain::Stump => {
                let pool = self.landscape_texture_pool();
                let idx = (noise * pool.len() as f64) as usize % pool.len();
                return pool[idx];
            }
            _ => {}
        }

        // Vegetation-driven selection for natural terrain
        // Combine vegetation with noise for organic variation
        let v = (vegetation + noise * 0.3 - 0.15).clamp(0.0, 1.0);

        if v < 0.1 {
            // Bare dirt / exposed soil
            match noise {
                n if n < 0.3 => '.',
                n if n < 0.6 => ' ',
                _ => ',',
            }
        } else if v < 0.3 {
            // Sparse — thin grass, scrub
            match noise {
                n if n < 0.25 => '\'',
                n if n < 0.5 => ',',
                n if n < 0.75 => '.',
                _ => ' ',
            }
        } else if v < 0.6 {
            // Medium — grass, light vegetation
            match self {
                Terrain::Forest | Terrain::Sapling => match noise {
                    n if n < 0.3 => ':',
                    n if n < 0.6 => ';',
                    _ => '"',
                },
                _ => match noise {
                    n if n < 0.3 => '\'',
                    n if n < 0.5 => ',',
                    n if n < 0.7 => '.',
                    _ => ':',
                },
            }
        } else {
            // Dense — thick vegetation, forest canopy
            match self {
                Terrain::Forest => match noise {
                    n if n < 0.25 => '%',
                    n if n < 0.5 => '#',
                    n if n < 0.75 => '"',
                    _ => ':',
                },
                Terrain::Mountain | Terrain::Cliff => match noise {
                    n if n < 0.3 => '^',
                    n if n < 0.6 => '#',
                    _ => '%',
                },
                _ => match noise {
                    n if n < 0.2 => '"',
                    n if n < 0.4 => ':',
                    n if n < 0.7 => '\'',
                    _ => ',',
                },
            }
        }
    }

    /// Landscape Mode foreground: close to bg for low character contrast.
    /// Hand-picked per terrain. These are "noon, clear day" base colors.
    pub fn landscape_fg(&self) -> Color {
        match self {
            Terrain::Grass => Color(60, 140, 50),
            Terrain::Sand => Color(190, 170, 100),
            Terrain::Desert => Color(210, 190, 130),
            Terrain::Forest => Color(25, 90, 20),
            Terrain::Scrubland => Color(120, 115, 55),
            Terrain::Mountain => Color(130, 120, 110),
            Terrain::Cliff => Color(100, 92, 82),
            Terrain::Snow => Color(210, 215, 220),
            Terrain::Tundra => Color(155, 165, 172),
            Terrain::Marsh => Color(50, 95, 65),
            Terrain::Water | Terrain::FloodWater => Color(40, 80, 200),
            Terrain::Ford => Color(70, 130, 210),
            Terrain::Road => Color(155, 128, 78),
            Terrain::BuildingFloor => Color(145, 125, 95),
            Terrain::BuildingWall => Color(165, 145, 115),
            Terrain::Stump => Color(100, 80, 45),
            Terrain::Bare => Color(90, 85, 55),
            Terrain::Sapling => Color(50, 130, 45),
            Terrain::Quarry => Color(135, 125, 112),
            Terrain::QuarryDeep => Color(108, 98, 88),
            Terrain::ScarredGround => Color(140, 130, 115),
            Terrain::Ice => Color(180, 205, 230),
            Terrain::Bridge => Color(140, 105, 55),
            Terrain::Burning => Color(255, 140, 40),
            Terrain::Scorched => Color(75, 68, 58),
        }
    }

    /// Landscape Mode background: carries biome identity. Close to fg.
    pub fn landscape_bg(&self) -> Color {
        match self {
            Terrain::Grass => Color(30, 80, 25),
            Terrain::Sand => Color(120, 105, 60),
            Terrain::Desert => Color(195, 175, 115),
            Terrain::Forest => Color(12, 50, 10),
            Terrain::Scrubland => Color(100, 95, 42),
            Terrain::Mountain => Color(80, 75, 70),
            Terrain::Cliff => Color(75, 68, 58),
            Terrain::Snow => Color(170, 175, 185),
            Terrain::Tundra => Color(138, 148, 155),
            Terrain::Marsh => Color(32, 72, 48),
            Terrain::Water | Terrain::FloodWater => Color(20, 40, 120),
            Terrain::Ford => Color(40, 80, 150),
            Terrain::Road => Color(135, 110, 65),
            Terrain::BuildingFloor => Color(115, 95, 72),
            Terrain::BuildingWall => Color(130, 112, 88),
            Terrain::Stump => Color(50, 60, 30),
            Terrain::Bare => Color(60, 55, 38),
            Terrain::Sapling => Color(28, 75, 25),
            Terrain::Quarry => Color(88, 78, 68),
            Terrain::QuarryDeep => Color(62, 55, 48),
            Terrain::ScarredGround => Color(112, 102, 88),
            Terrain::Ice => Color(120, 148, 175),
            Terrain::Bridge => Color(82, 62, 32),
            Terrain::Burning => Color(180, 50, 15),
            Terrain::Scorched => Color(42, 38, 28),
        }
    }

    /// Movement speed multiplier for this terrain.
    pub fn speed_multiplier(&self) -> f64 {
        match self {
            Terrain::Road => 1.5,
            Terrain::Grass | Terrain::BuildingFloor | Terrain::Scrubland => 1.0,
            Terrain::Bridge => 0.9,
            Terrain::ScarredGround => 0.9,
            Terrain::Bare => 0.9,
            Terrain::Desert | Terrain::Tundra => 0.8,
            Terrain::Sand | Terrain::Stump => 0.8,
            Terrain::Sapling | Terrain::Quarry => 0.7,
            Terrain::QuarryDeep => 0.5,
            Terrain::Forest => 0.6,
            Terrain::Snow => 0.4,
            Terrain::Ford => 0.3,
            Terrain::Marsh => 0.3,
            Terrain::Mountain => 0.25,
            Terrain::Ice => 0.5,
            Terrain::Burning => 0.3,
            Terrain::Scorched => 0.9,
            Terrain::Water | Terrain::BuildingWall | Terrain::Cliff | Terrain::FloodWater => 0.0,
        }
    }

    /// Movement cost for A* pathfinding (inverse of speed, higher = harder).
    pub fn move_cost(&self) -> f64 {
        match self {
            Terrain::Road => 0.7,
            Terrain::Grass | Terrain::BuildingFloor | Terrain::Scrubland | Terrain::Bare => 1.0,
            Terrain::Bridge => 1.1,
            Terrain::ScarredGround => 1.1,
            Terrain::Stump => 1.2,
            Terrain::Sand | Terrain::Desert | Terrain::Tundra => 1.3,
            Terrain::Sapling | Terrain::Quarry => 1.4,
            Terrain::QuarryDeep => 2.0,
            Terrain::Forest => 1.7,
            Terrain::Snow => 2.5,
            Terrain::Ford => 3.0,
            Terrain::Marsh => 3.0,
            Terrain::Mountain => 4.0,
            Terrain::Ice => 2.0,
            Terrain::Burning => 10.0,
            Terrain::Scorched => 1.3,
            Terrain::Water | Terrain::BuildingWall | Terrain::Cliff | Terrain::FloodWater => {
                f64::INFINITY
            }
        }
    }

    /// Returns true if this terrain type can catch fire.
    pub fn is_flammable(&self) -> bool {
        matches!(
            self,
            Terrain::Forest | Terrain::Sapling | Terrain::Stump | Terrain::Scrubland
        )
    }

    /// Returns true if this terrain type blocks fire spread (natural firebreak).
    pub fn is_firebreak(&self) -> bool {
        matches!(
            self,
            Terrain::Water
                | Terrain::Ford
                | Terrain::Sand
                | Terrain::Desert
                | Terrain::Mountain
                | Terrain::Snow
                | Terrain::Tundra
                | Terrain::Road
                | Terrain::Bridge
                | Terrain::Scorched
                | Terrain::BuildingWall
                | Terrain::Ice
                | Terrain::FloodWater
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct TileMap {
    pub width: usize,
    pub height: usize,
    tiles: Vec<Terrain>,
    /// Per-tile mining counter for mountain mining progression.
    /// Tracks how many times each tile has been mined.
    #[serde(default)]
    mine_counts: Vec<u8>,
    /// Base terrain grid — stores the "real" terrain under seasonal overlays.
    /// Seasonal effects (Ice, FloodWater) write to `tiles`; when a season ends
    /// the affected tiles revert to their `base_terrain` value.
    #[serde(default)]
    base_terrain: Vec<Terrain>,
}

impl TileMap {
    pub fn new(width: usize, height: usize, fill: Terrain) -> Self {
        Self {
            width,
            height,
            tiles: vec![fill; width * height],
            mine_counts: vec![0u8; width * height],
            base_terrain: Vec::new(), // lazily initialized
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Terrain> {
        if x < self.width && y < self.height {
            Some(&self.tiles[y * self.width + x])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: usize, y: usize, terrain: Terrain) {
        if x < self.width && y < self.height {
            self.tiles[y * self.width + x] = terrain;
        }
    }

    /// Initialize base_terrain as a snapshot of the current tile grid.
    /// Called once after terrain generation, before any seasonal effects.
    pub fn init_base_terrain(&mut self) {
        self.base_terrain = self.tiles.clone();
    }

    /// Get the base (non-seasonal) terrain at a position.
    pub fn get_base(&self, x: usize, y: usize) -> Option<&Terrain> {
        if x < self.width && y < self.height && !self.base_terrain.is_empty() {
            Some(&self.base_terrain[y * self.width + x])
        } else {
            self.get(x, y) // fallback to active tiles
        }
    }

    /// Apply a seasonal overlay: set the active tile but keep base_terrain unchanged.
    pub fn set_seasonal(&mut self, x: usize, y: usize, terrain: Terrain) {
        if x < self.width && y < self.height {
            // Initialize base_terrain lazily (for old saves without it)
            if self.base_terrain.is_empty() {
                self.base_terrain = self.tiles.clone();
            }
            self.tiles[y * self.width + x] = terrain;
        }
    }

    /// Revert a tile to its base terrain (undo seasonal overlay).
    pub fn revert_seasonal(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height && !self.base_terrain.is_empty() {
            self.tiles[y * self.width + x] = self.base_terrain[y * self.width + x];
        }
    }

    /// Revert all Ice tiles back to their base terrain.
    pub fn revert_ice(&mut self) {
        if self.base_terrain.is_empty() {
            return;
        }
        for i in 0..self.tiles.len() {
            if self.tiles[i] == Terrain::Ice {
                self.tiles[i] = self.base_terrain[i];
            }
        }
    }

    /// Revert all FloodWater tiles back to their base terrain.
    /// Returns the list of (x, y) positions that were reverted.
    pub fn revert_flood_water(&mut self) -> Vec<(usize, usize)> {
        let mut reverted = Vec::new();
        if self.base_terrain.is_empty() {
            return reverted;
        }
        for i in 0..self.tiles.len() {
            if self.tiles[i] == Terrain::FloodWater {
                self.tiles[i] = self.base_terrain[i];
                let x = i % self.width;
                let y = i / self.width;
                reverted.push((x, y));
            }
        }
        reverted
    }

    /// Apply winter ice: convert all Water tiles to Ice.
    /// Returns the count of tiles frozen.
    pub fn apply_winter_ice(&mut self) -> usize {
        if self.base_terrain.is_empty() {
            self.base_terrain = self.tiles.clone();
        }
        let mut count = 0;
        for i in 0..self.tiles.len() {
            if self.tiles[i] == Terrain::Water {
                self.tiles[i] = Terrain::Ice;
                count += 1;
            }
        }
        count
    }

    /// Apply spring floods near rivers on low-elevation alluvial tiles.
    /// `river_mask` marks river tiles, `heights` is the elevation grid.
    /// Returns the list of (x, y) positions that were flooded.
    pub fn apply_spring_floods(
        &mut self,
        river_mask: &[bool],
        heights: &[f64],
        soil: &[crate::terrain_pipeline::SoilType],
    ) -> Vec<(usize, usize)> {
        use crate::terrain_pipeline::SoilType;

        if self.base_terrain.is_empty() {
            self.base_terrain = self.tiles.clone();
        }
        let w = self.width;
        let h = self.height;
        let mut flooded = Vec::new();

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let tile = self.tiles[idx];

                // Only flood walkable natural terrain near rivers
                // Don't flood buildings, roads, mountains, water, etc.
                if !matches!(
                    tile,
                    Terrain::Grass
                        | Terrain::Sand
                        | Terrain::Bare
                        | Terrain::Marsh
                        | Terrain::Scrubland
                ) {
                    continue;
                }

                // Must be within 2 tiles of a river
                let mut near_river = false;
                let mut min_river_height = f64::INFINITY;
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                            let ni = ny as usize * w + nx as usize;
                            if ni < river_mask.len() && river_mask[ni] {
                                near_river = true;
                                min_river_height = min_river_height.min(heights[ni]);
                            }
                        }
                    }
                }
                if !near_river {
                    continue;
                }

                // Must be low elevation (at or below nearby river + small margin)
                if heights[idx] > min_river_height + 0.02 {
                    continue;
                }

                // Prefer alluvial soil, but also flood marsh near rivers
                let is_alluvial = idx < soil.len() && soil[idx] == SoilType::Alluvial;
                let is_marsh = tile == Terrain::Marsh;
                if !is_alluvial && !is_marsh {
                    continue;
                }

                self.tiles[idx] = Terrain::FloodWater;
                flooded.push((x, y));
            }
        }
        flooded
    }

    /// Get the mining counter for a tile.
    pub fn mine_count(&self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height && !self.mine_counts.is_empty() {
            self.mine_counts[y * self.width + x]
        } else {
            0
        }
    }

    /// Increment the mining counter for a tile and return the new count.
    pub fn increment_mine_count(&mut self, x: usize, y: usize) -> u8 {
        if x < self.width && y < self.height {
            // Ensure mine_counts is initialized (for old saves with default empty vec)
            if self.mine_counts.is_empty() {
                self.mine_counts = vec![0u8; self.width * self.height];
            }
            let idx = y * self.width + x;
            self.mine_counts[idx] = self.mine_counts[idx].saturating_add(1);
            self.mine_counts[idx]
        } else {
            0
        }
    }

    /// A* pathfinding from (sx, sy) to (gx, gy). Returns next waypoint (not full path).
    /// Returns None if no path found within search budget. max_steps caps exploration.
    pub fn astar_next(
        &self,
        sx: f64,
        sy: f64,
        gx: f64,
        gy: f64,
        max_steps: usize,
    ) -> Option<(f64, f64)> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        let si = sx.round() as i32;
        let sj = sy.round() as i32;
        let gi = gx.round() as i32;
        let gj = gy.round() as i32;

        if si == gi && sj == gj {
            return Some((gx, gy));
        }

        #[derive(Clone)]
        struct Node {
            cost: f64,
            heuristic: f64,
            x: i32,
            y: i32,
            parent: usize,
        }
        impl PartialEq for Node {
            fn eq(&self, o: &Self) -> bool {
                self.cost + self.heuristic == o.cost + o.heuristic
            }
        }
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                // Reverse for min-heap
                let a = self.cost + self.heuristic;
                let b = o.cost + o.heuristic;
                b.partial_cmp(&a).unwrap_or(Ordering::Equal)
            }
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let mut visited = vec![false; self.width * self.height];
        let mut nodes: Vec<Node> = Vec::new();
        // Wrap (Node, usize) so heap sorts by Node (reversed cost+heuristic)
        struct HeapEntry(Node, usize); // (node for ordering, index into nodes vec)
        impl PartialEq for HeapEntry {
            fn eq(&self, o: &Self) -> bool {
                self.0 == o.0
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, o: &Self) -> Ordering {
                self.0.cmp(&o.0)
            }
        }
        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();

        let heuristic =
            |x: i32, y: i32| -> f64 { ((x - gi) as f64).abs() + ((y - gj) as f64).abs() };

        let start = Node {
            cost: 0.0,
            heuristic: heuristic(si, sj),
            x: si,
            y: sj,
            parent: usize::MAX,
        };
        nodes.push(start.clone());
        heap.push(HeapEntry(start, 0));

        const DIRS: [(i32, i32); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];

        let mut steps = 0;
        while let Some(HeapEntry(node, idx)) = heap.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;

            let vx = node.x as usize;
            let vy = node.y as usize;
            if vx >= self.width || vy >= self.height {
                continue;
            }
            if visited[vy * self.width + vx] {
                continue;
            }
            visited[vy * self.width + vx] = true;

            if node.x == gi && node.y == gj {
                // Trace back to find first step
                let mut cur = idx;
                loop {
                    let p = nodes[cur].parent;
                    if p == usize::MAX || (nodes[p].x == si && nodes[p].y == sj) {
                        return Some((nodes[cur].x as f64, nodes[cur].y as f64));
                    }
                    cur = p;
                }
            }

            for &(dx, dy) in &DIRS {
                let nx = node.x + dx;
                let ny = node.y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * self.width + nx as usize;
                if visited[ni] {
                    continue;
                }

                let terrain = &self.tiles[ni];
                if !terrain.is_walkable() {
                    continue;
                }

                let step_cost = terrain.move_cost() * if dx != 0 && dy != 0 { 1.414 } else { 1.0 };
                let new_cost = node.cost + step_cost;
                let new_node = Node {
                    cost: new_cost,
                    heuristic: heuristic(nx, ny),
                    x: nx,
                    y: ny,
                    parent: idx,
                };
                let new_idx = nodes.len();
                nodes.push(new_node.clone());
                heap.push(HeapEntry(new_node, new_idx));
            }
        }
        None // no path found
    }

    /// A* pathfinding returning the full path as waypoints (start excluded, destination included).
    /// Returns None if no path found within search budget.
    pub fn astar_full_path(
        &self,
        sx: f64,
        sy: f64,
        gx: f64,
        gy: f64,
        max_steps: usize,
    ) -> Option<Vec<(f64, f64)>> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        let si = sx.round() as i32;
        let sj = sy.round() as i32;
        let gi = gx.round() as i32;
        let gj = gy.round() as i32;

        if si == gi && sj == gj {
            return Some(vec![(gx, gy)]);
        }

        #[derive(Clone)]
        struct Node {
            cost: f64,
            heuristic: f64,
            x: i32,
            y: i32,
            parent: usize,
        }
        impl PartialEq for Node {
            fn eq(&self, o: &Self) -> bool {
                self.cost + self.heuristic == o.cost + o.heuristic
            }
        }
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                let a = self.cost + self.heuristic;
                let b = o.cost + o.heuristic;
                b.partial_cmp(&a).unwrap_or(Ordering::Equal)
            }
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let mut visited = vec![false; self.width * self.height];
        let mut nodes: Vec<Node> = Vec::new();
        struct HeapEntry(Node, usize);
        impl PartialEq for HeapEntry {
            fn eq(&self, o: &Self) -> bool {
                self.0 == o.0
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, o: &Self) -> Ordering {
                self.0.cmp(&o.0)
            }
        }
        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();

        let heuristic =
            |x: i32, y: i32| -> f64 { ((x - gi) as f64).abs() + ((y - gj) as f64).abs() };

        let start = Node {
            cost: 0.0,
            heuristic: heuristic(si, sj),
            x: si,
            y: sj,
            parent: usize::MAX,
        };
        nodes.push(start.clone());
        heap.push(HeapEntry(start, 0));

        const DIRS: [(i32, i32); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];

        let mut steps = 0;
        while let Some(HeapEntry(node, idx)) = heap.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;

            let vx = node.x as usize;
            let vy = node.y as usize;
            if vx >= self.width || vy >= self.height {
                continue;
            }
            if visited[vy * self.width + vx] {
                continue;
            }
            visited[vy * self.width + vx] = true;

            if node.x == gi && node.y == gj {
                // Trace back full path
                let mut path = Vec::new();
                let mut cur = idx;
                while nodes[cur].parent != usize::MAX {
                    path.push((nodes[cur].x as f64, nodes[cur].y as f64));
                    cur = nodes[cur].parent;
                }
                path.reverse();
                return Some(path);
            }

            for &(dx, dy) in &DIRS {
                let nx = node.x + dx;
                let ny = node.y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * self.width + nx as usize;
                if visited[ni] {
                    continue;
                }

                let terrain = &self.tiles[ni];
                if !terrain.is_walkable() {
                    continue;
                }

                let step_cost = terrain.move_cost() * if dx != 0 && dy != 0 { 1.414 } else { 1.0 };
                let new_cost = node.cost + step_cost;
                let new_node = Node {
                    cost: new_cost,
                    heuristic: heuristic(nx, ny),
                    x: nx,
                    y: ny,
                    parent: idx,
                };
                let new_idx = nodes.len();
                nodes.push(new_node.clone());
                heap.push(HeapEntry(new_node, new_idx));
            }
        }
        None
    }

    /// A* pathfinding with an additional per-tile cost overlay (e.g., danger scent).
    /// `cost_overlay` must have the same length as `self.tiles` (width * height).
    /// Each overlay value is added to the base terrain move cost for that tile.
    /// Falls back to `astar_full_path` when overlay is empty or all-zero.
    pub fn astar_full_path_with_cost_overlay(
        &self,
        sx: f64,
        sy: f64,
        gx: f64,
        gy: f64,
        max_steps: usize,
        cost_overlay: &[f64],
    ) -> Option<Vec<(f64, f64)>> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        // If overlay is wrong size, fall back to regular A*
        if cost_overlay.len() != self.width * self.height {
            return self.astar_full_path(sx, sy, gx, gy, max_steps);
        }

        let si = sx.round() as i32;
        let sj = sy.round() as i32;
        let gi = gx.round() as i32;
        let gj = gy.round() as i32;

        if si == gi && sj == gj {
            return Some(vec![(gx, gy)]);
        }

        #[derive(Clone)]
        struct Node {
            cost: f64,
            heuristic: f64,
            x: i32,
            y: i32,
            parent: usize,
        }
        impl PartialEq for Node {
            fn eq(&self, o: &Self) -> bool {
                self.cost + self.heuristic == o.cost + o.heuristic
            }
        }
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                let a = self.cost + self.heuristic;
                let b = o.cost + o.heuristic;
                b.partial_cmp(&a).unwrap_or(Ordering::Equal)
            }
        }

        let w = self.width as i32;
        let h = self.height as i32;
        let mut visited = vec![false; self.width * self.height];
        let mut nodes: Vec<Node> = Vec::new();
        struct HeapEntry(Node, usize);
        impl PartialEq for HeapEntry {
            fn eq(&self, o: &Self) -> bool {
                self.0 == o.0
            }
        }
        impl Eq for HeapEntry {}
        impl PartialOrd for HeapEntry {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for HeapEntry {
            fn cmp(&self, o: &Self) -> Ordering {
                self.0.cmp(&o.0)
            }
        }
        let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::new();

        let heuristic =
            |x: i32, y: i32| -> f64 { ((x - gi) as f64).abs() + ((y - gj) as f64).abs() };

        let start = Node {
            cost: 0.0,
            heuristic: heuristic(si, sj),
            x: si,
            y: sj,
            parent: usize::MAX,
        };
        nodes.push(start.clone());
        heap.push(HeapEntry(start, 0));

        const DIRS: [(i32, i32); 8] = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ];

        let mut steps = 0;
        while let Some(HeapEntry(node, idx)) = heap.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;

            let vx = node.x as usize;
            let vy = node.y as usize;
            if vx >= self.width || vy >= self.height {
                continue;
            }
            if visited[vy * self.width + vx] {
                continue;
            }
            visited[vy * self.width + vx] = true;

            if node.x == gi && node.y == gj {
                let mut path = Vec::new();
                let mut cur = idx;
                while nodes[cur].parent != usize::MAX {
                    path.push((nodes[cur].x as f64, nodes[cur].y as f64));
                    cur = nodes[cur].parent;
                }
                path.reverse();
                return Some(path);
            }

            for &(dx, dy) in &DIRS {
                let nx = node.x + dx;
                let ny = node.y + dy;
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let ni = ny as usize * self.width + nx as usize;
                if visited[ni] {
                    continue;
                }

                let terrain = &self.tiles[ni];
                if !terrain.is_walkable() {
                    continue;
                }

                let base_cost = terrain.move_cost() * if dx != 0 && dy != 0 { 1.414 } else { 1.0 };
                // Add overlay cost (danger scent penalty, capped at +2.0)
                let overlay_penalty = (cost_overlay[ni] / 20.0).min(2.0);
                let step_cost = base_cost + overlay_penalty;
                let new_cost = node.cost + step_cost;
                let new_node = Node {
                    cost: new_cost,
                    heuristic: heuristic(nx, ny),
                    x: nx,
                    y: ny,
                    parent: idx,
                };
                let new_idx = nodes.len();
                nodes.push(new_node.clone());
                heap.push(HeapEntry(new_node, new_idx));
            }
        }
        None
    }

    /// Find the nearest walkable tile to a position. Returns None if no walkable
    /// tile exists within search radius 50. Used to rescue entities stranded on
    /// impassable tiles (e.g. water after rivers become barriers).
    pub fn find_nearest_walkable(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let cx = x.round() as i32;
        let cy = y.round() as i32;
        for r in 1..50i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx >= 0
                        && ny >= 0
                        && (nx as usize) < self.width
                        && (ny as usize) < self.height
                    {
                        if let Some(t) = self.get(nx as usize, ny as usize) {
                            if t.is_walkable() {
                                return Some((nx as f64, ny as f64));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if a world position is walkable (in-bounds and walkable terrain).
    pub fn is_walkable(&self, x: f64, y: f64) -> bool {
        let ix = x.round() as i64;
        let iy = y.round() as i64;
        if ix < 0 || iy < 0 {
            return false;
        }
        match self.get(ix as usize, iy as usize) {
            Some(t) => t.is_walkable(),
            None => false, // out of bounds = blocked
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Camera {
    pub x: i32,
    pub y: i32,
}

impl Camera {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Clamp camera so viewport stays within map bounds.
    pub fn clamp(&mut self, map_w: usize, map_h: usize, view_w: u16, view_h: u16) {
        let max_x = (map_w as i32) - (view_w as i32);
        let max_y = (map_h as i32) - (view_h as i32);
        self.x = self.x.clamp(0, max_x.max(0));
        self.y = self.y.clamp(0, max_y.max(0));
    }
}

pub fn render_map(map: &TileMap, camera: &Camera, renderer: &mut dyn Renderer) {
    let (vw, vh) = renderer.size();
    for sy in 0..vh {
        for sx in 0..vw {
            let wx = camera.x + sx as i32;
            let wy = camera.y + sy as i32;
            if wx >= 0
                && wy >= 0
                && let Some(terrain) = map.get(wx as usize, wy as usize)
            {
                renderer.draw(sx, sy, terrain.ch(), terrain.fg(), terrain.bg());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_renderer::HeadlessRenderer;

    #[test]
    fn tilemap_new_and_get() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        assert_eq!(*map.get(0, 0).unwrap(), Terrain::Grass);
        assert_eq!(*map.get(9, 9).unwrap(), Terrain::Grass);
        assert!(map.get(10, 10).is_none());
    }

    #[test]
    fn tilemap_set_and_get() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(5, 3, Terrain::Water);
        assert_eq!(*map.get(5, 3).unwrap(), Terrain::Water);
        assert_eq!(*map.get(5, 4).unwrap(), Terrain::Grass);
    }

    #[test]
    fn render_map_draws_terrain() {
        let map = TileMap::new(20, 10, Terrain::Grass);
        let camera = Camera::new(0, 0);
        let mut r = HeadlessRenderer::new(10, 5);
        render_map(&map, &camera, &mut r);

        let frame = r.frame_as_string();
        // every cell should be grass
        for ch in frame.chars() {
            if ch != '\n' {
                assert_eq!(ch, '\'', "expected grass char, got '{}'", ch);
            }
        }
    }

    #[test]
    fn render_map_with_camera_offset() {
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        map.set(5, 3, Terrain::Mountain);

        // camera at (3, 2) means world (5,3) maps to screen (2,1)
        let camera = Camera::new(3, 2);
        let mut r = HeadlessRenderer::new(10, 5);
        render_map(&map, &camera, &mut r);

        assert_eq!(r.get_cell(2, 1).unwrap().ch, '^');
    }

    #[test]
    fn render_map_camera_past_edge_shows_nothing() {
        let map = TileMap::new(5, 5, Terrain::Grass);
        let camera = Camera::new(10, 10);
        let mut r = HeadlessRenderer::new(10, 5);
        render_map(&map, &camera, &mut r);

        // everything should be blank
        let frame = r.frame_as_string();
        assert!(
            !frame.contains(','),
            "should not render grass past map edge"
        );
    }

    #[test]
    fn camera_clamp_keeps_in_bounds() {
        let mut camera = Camera::new(-5, -5);
        camera.clamp(100, 100, 20, 10);
        assert_eq!(camera.x, 0);
        assert_eq!(camera.y, 0);

        camera.x = 200;
        camera.y = 200;
        camera.clamp(100, 100, 20, 10);
        assert_eq!(camera.x, 80);
        assert_eq!(camera.y, 90);
    }

    #[test]
    fn mixed_terrain_renders_correctly() {
        let mut map = TileMap::new(5, 3, Terrain::Water);
        map.set(1, 0, Terrain::Sand);
        map.set(2, 0, Terrain::Forest);

        let camera = Camera::new(0, 0);
        let mut r = HeadlessRenderer::new(5, 3);
        render_map(&map, &camera, &mut r);

        assert_eq!(r.get_cell(0, 0).unwrap().ch, '~');
        assert_eq!(r.get_cell(1, 0).unwrap().ch, '·');
        assert_eq!(r.get_cell(2, 0).unwrap().ch, ':');
    }

    #[test]
    fn road_terrain_properties() {
        assert!(Terrain::Road.is_walkable());
        assert_eq!(Terrain::Road.ch(), '=');
        assert!(Terrain::Road.bg().is_some());
        assert_eq!(Terrain::Road.speed_multiplier(), 1.5);
    }

    #[test]
    fn terrain_speed_multipliers() {
        assert_eq!(Terrain::Grass.speed_multiplier(), 1.0);
        assert_eq!(Terrain::Sand.speed_multiplier(), 0.8);
        assert_eq!(Terrain::Forest.speed_multiplier(), 0.6);
        assert_eq!(Terrain::Mountain.speed_multiplier(), 0.25);
        assert_eq!(Terrain::Road.speed_multiplier(), 1.5);
        assert!(!Terrain::Water.is_walkable()); // impassable barrier
        assert_eq!(Terrain::Water.speed_multiplier(), 0.0);
        assert!(Terrain::Mountain.is_walkable());
    }

    #[test]
    fn ford_terrain_properties() {
        assert!(Terrain::Ford.is_walkable());
        assert_eq!(Terrain::Ford.ch(), '~');
        assert_eq!(Terrain::Ford.speed_multiplier(), 0.3);
        assert_eq!(Terrain::Ford.move_cost(), 3.0);
        assert!(Terrain::Ford.bg().is_some());
    }

    #[test]
    fn bridge_terrain_properties() {
        assert!(Terrain::Bridge.is_walkable());
        assert_eq!(Terrain::Bridge.ch(), '#');
        assert_eq!(Terrain::Bridge.speed_multiplier(), 0.9);
        assert_eq!(Terrain::Bridge.move_cost(), 1.1);
        assert!(Terrain::Bridge.bg().is_some());
    }

    #[test]
    fn water_blocks_movement() {
        // Water is impassable — A* treats it like a wall
        assert!(!Terrain::Water.is_walkable());
        assert_eq!(Terrain::Water.move_cost(), f64::INFINITY);
        assert_eq!(Terrain::Water.speed_multiplier(), 0.0);
    }

    #[test]
    fn astar_paths_around_water() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // Wall of water from (5,0) to (5,8), leaving a gap at (5,9)
        // Water is impassable, so A* must route around it via the gap
        for y in 0..9 {
            map.set(5, y, Terrain::Water);
        }
        // Path from (3,5) to (7,5) must go around the water wall
        let next = map.astar_next(3.0, 5.0, 7.0, 5.0, 500);
        assert!(next.is_some(), "should find a path around water");
        // The first step should move south (toward the gap) not east (into water)
        let (nx, ny) = next.unwrap();
        assert!(
            ny > 5.0 || nx < 5.0,
            "should route around water, not through it"
        );
    }

    #[test]
    fn astar_routes_through_ford() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // River wall from (5,0) to (5,19) — impassable water
        for y in 0..20 {
            map.set(5, y, Terrain::Water);
        }
        // Ford at (5,10) — the only crossing
        map.set(5, 10, Terrain::Ford);
        // Path from (3,5) to (7,5) must route through the ford at (5,10)
        let path = map.astar_full_path(3.0, 5.0, 7.0, 5.0, 2000);
        assert!(path.is_some(), "should find path through ford");
        let path = path.unwrap();
        // Path should pass through or near the ford
        let passes_ford = path.iter().any(|(x, _y)| *x == 5.0);
        assert!(passes_ford, "path should cross at ford x=5");
    }

    #[test]
    fn astar_routes_through_bridge() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // River wall from (5,0) to (5,19) — impassable water
        for y in 0..20 {
            map.set(5, y, Terrain::Water);
        }
        // Bridge at (5,10) — the only crossing
        map.set(5, 10, Terrain::Bridge);
        // Path from (3,10) to (7,10) should go straight through bridge
        let next = map.astar_next(3.0, 10.0, 7.0, 10.0, 500);
        assert!(next.is_some(), "should find path through bridge");
        let (nx, _ny) = next.unwrap();
        assert!(nx > 3.0, "should move toward bridge");
    }

    #[test]
    fn astar_prefers_roads() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // Road along y=3
        for x in 0..20 {
            map.set(x, 3, Terrain::Road);
        }
        // Path from (0,5) to (15,5) — should prefer routing via road
        let next = map.astar_next(0.0, 5.0, 15.0, 5.0, 500);
        assert!(next.is_some());
    }

    #[test]
    fn astar_returns_none_for_unreachable() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Surround target with BuildingWall (truly impassable)
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                map.set((5 + dx) as usize, (5 + dy) as usize, Terrain::BuildingWall);
            }
        }
        let next = map.astar_next(0.0, 0.0, 5.0, 5.0, 500);
        assert!(
            next.is_none(),
            "should return None when target is unreachable"
        );
    }

    #[test]
    fn astar_straight_line_on_open_map() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        // Should find path from (2,2) to (17,17) on open map
        let next = map.astar_next(2.0, 2.0, 17.0, 17.0, 500);
        assert!(next.is_some(), "should find path on open map");
        let (nx, ny) = next.unwrap();
        // First step should move toward target (diagonal)
        assert!(nx > 2.0 || ny > 2.0, "should move toward target");
    }

    #[test]
    fn astar_reaches_target_iteratively() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let mut x = 2.0f64;
        let mut y = 2.0f64;
        let tx = 17.0;
        let ty = 17.0;
        // Simulate walking step by step
        for _ in 0..100 {
            let d = ((x - tx).powi(2) + (y - ty).powi(2)).sqrt();
            if d < 1.5 {
                break;
            }
            let next = map.astar_next(x, y, tx, ty, 500);
            assert!(next.is_some(), "should find path at ({:.1},{:.1})", x, y);
            let (nx, ny) = next.unwrap();
            x = nx;
            y = ny;
        }
        let final_d = ((x - tx).powi(2) + (y - ty).powi(2)).sqrt();
        assert!(
            final_d < 2.0,
            "should reach target iteratively, got dist={}",
            final_d
        );
    }

    #[test]
    fn astar_navigates_corridor() {
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        // Wall across middle with one gap
        for x in 0..20 {
            map.set(x, 5, Terrain::BuildingWall);
        }
        map.set(15, 5, Terrain::Grass); // gap at x=15

        // Path from (2,2) to (2,8) must go through gap at (15,5)
        let next = map.astar_next(2.0, 2.0, 2.0, 8.0, 500);
        assert!(next.is_some(), "should find path through corridor gap");
    }

    #[test]
    fn astar_inside_hut_finds_door() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Build a hut-like structure: walls on 3 sides, open south
        // WWW
        // W.W
        // ...  (open)
        map.set(3, 3, Terrain::BuildingWall);
        map.set(4, 3, Terrain::BuildingWall);
        map.set(5, 3, Terrain::BuildingWall);
        map.set(3, 4, Terrain::BuildingWall);
        // (4,4) = interior floor
        map.set(5, 4, Terrain::BuildingWall);
        // (3,5), (4,5), (5,5) = open (door)

        // From inside (4,4) to outside (4,7)
        let next = map.astar_next(4.0, 4.0, 4.0, 7.0, 100);
        assert!(
            next.is_some(),
            "should find path from inside hut through door"
        );
        let (nx, ny) = next.unwrap();
        // First step should go south toward the open side
        assert!(
            ny > 4.0 || nx != 4.0,
            "should move toward door, got ({},{})",
            nx,
            ny
        );
    }

    #[test]
    fn astar_avoids_water() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Lake in the middle — water is impassable, must route around
        for y in 3..7 {
            for x in 3..7 {
                map.set(x, y, Terrain::Water);
            }
        }
        // Path from (1,5) to (8,5) must go around lake
        let next = map.astar_next(1.0, 5.0, 8.0, 5.0, 500);
        assert!(next.is_some(), "should find path around lake");
    }

    #[test]
    fn astar_same_position_returns_target() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let next = map.astar_next(5.0, 5.0, 5.0, 5.0, 100);
        assert!(next.is_some());
        let (nx, ny) = next.unwrap();
        assert_eq!(nx, 5.0);
        assert_eq!(ny, 5.0);
    }

    #[test]
    fn astar_adjacent_target() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let next = map.astar_next(5.0, 5.0, 6.0, 5.0, 100);
        assert!(next.is_some());
        let (nx, ny) = next.unwrap();
        assert_eq!(nx, 6.0);
        assert_eq!(ny, 5.0);
    }

    #[test]
    fn quarry_terrain_properties() {
        assert!(Terrain::Quarry.is_walkable());
        assert_eq!(Terrain::Quarry.ch(), 'U');
        assert_eq!(Terrain::Quarry.speed_multiplier(), 0.7);
        assert_eq!(Terrain::Quarry.move_cost(), 1.4);
        assert!(Terrain::Quarry.bg().is_some());
    }

    #[test]
    fn quarry_deep_terrain_properties() {
        assert!(Terrain::QuarryDeep.is_walkable());
        assert_eq!(Terrain::QuarryDeep.ch(), 'V');
        assert_eq!(Terrain::QuarryDeep.speed_multiplier(), 0.5);
        assert_eq!(Terrain::QuarryDeep.move_cost(), 2.0);
        assert!(Terrain::QuarryDeep.bg().is_some());
    }

    #[test]
    fn scarred_ground_terrain_properties() {
        assert!(Terrain::ScarredGround.is_walkable());
        assert_eq!(Terrain::ScarredGround.ch(), '.');
        assert_eq!(Terrain::ScarredGround.speed_multiplier(), 0.9);
        assert_eq!(Terrain::ScarredGround.move_cost(), 1.1);
        assert!(Terrain::ScarredGround.bg().is_some());
    }

    #[test]
    fn mine_count_increment_and_get() {
        let mut map = TileMap::new(10, 10, Terrain::Mountain);
        assert_eq!(map.mine_count(5, 5), 0);
        let count = map.increment_mine_count(5, 5);
        assert_eq!(count, 1);
        assert_eq!(map.mine_count(5, 5), 1);
        for _ in 0..5 {
            map.increment_mine_count(5, 5);
        }
        assert_eq!(map.mine_count(5, 5), 6);
    }

    #[test]
    fn mine_count_threshold_transitions() {
        let mut map = TileMap::new(10, 10, Terrain::Mountain);
        // Increment to 6 -> should become Quarry
        for _ in 0..6 {
            let count = map.increment_mine_count(5, 5);
            if count >= 6 {
                map.set(5, 5, Terrain::Quarry);
            }
        }
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Quarry);

        // Increment to 12 -> should become QuarryDeep
        for _ in 0..6 {
            let count = map.increment_mine_count(5, 5);
            if count >= 12 {
                map.set(5, 5, Terrain::QuarryDeep);
            }
        }
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::QuarryDeep);
    }

    #[test]
    fn mine_count_saturates_at_u8_max() {
        let mut map = TileMap::new(5, 5, Terrain::Mountain);
        for _ in 0..260 {
            map.increment_mine_count(2, 2);
        }
        assert_eq!(map.mine_count(2, 2), 255);
    }

    #[test]
    fn astar_routes_through_quarry() {
        let mut map = TileMap::new(20, 5, Terrain::Grass);
        // Wall of mountains from (10,0) to (10,4)
        for y in 0..5 {
            map.set(10, y, Terrain::Mountain);
        }
        // No path through mountains (Mountain is walkable but cost 4.0)
        // Convert one tile to Quarry (cost 1.4) -- A* should prefer it
        map.set(10, 2, Terrain::Quarry);
        let next = map.astar_next(5.0, 2.0, 15.0, 2.0, 500);
        assert!(next.is_some(), "should find path through quarry gap");
    }

    #[test]
    fn mine_count_out_of_bounds_returns_zero() {
        let map = TileMap::new(5, 5, Terrain::Grass);
        assert_eq!(map.mine_count(10, 10), 0);
    }

    // --- astar_full_path tests ---

    #[test]
    fn astar_full_path_same_position() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let path = map.astar_full_path(5.0, 5.0, 5.0, 5.0, 100);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], (5.0, 5.0));
    }

    #[test]
    fn astar_full_path_adjacent() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        let path = map.astar_full_path(5.0, 5.0, 6.0, 5.0, 100);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path[0], (6.0, 5.0));
    }

    #[test]
    fn astar_full_path_straight_line() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let path = map.astar_full_path(2.0, 5.0, 10.0, 5.0, 500);
        assert!(path.is_some());
        let path = path.unwrap();
        // Path should end at or near destination
        let (lx, ly) = path.last().unwrap();
        assert_eq!(*lx, 10.0);
        assert_eq!(*ly, 5.0);
        // Path length should be roughly the manhattan distance
        assert!(path.len() >= 5, "path should have multiple waypoints");
    }

    #[test]
    fn astar_full_path_around_wall() {
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        // Wall across middle with one gap
        for x in 0..20 {
            map.set(x, 5, Terrain::BuildingWall);
        }
        map.set(15, 5, Terrain::Grass); // gap at x=15
        let path = map.astar_full_path(2.0, 2.0, 2.0, 8.0, 500);
        assert!(path.is_some(), "should find path through corridor gap");
        let path = path.unwrap();
        // Path must go through x=15 gap
        assert!(
            path.iter().any(|&(x, _)| x >= 14.0),
            "path should route through gap at x=15"
        );
    }

    #[test]
    fn astar_full_path_unreachable() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Surround target with walls
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                map.set((5 + dx) as usize, (5 + dy) as usize, Terrain::BuildingWall);
            }
        }
        let path = map.astar_full_path(0.0, 0.0, 5.0, 5.0, 500);
        assert!(path.is_none(), "should return None for unreachable target");
    }

    #[test]
    fn astar_full_path_reaches_destination_when_walked() {
        let map = TileMap::new(20, 20, Terrain::Grass);
        let path = map.astar_full_path(2.0, 2.0, 17.0, 17.0, 500);
        assert!(path.is_some());
        let path = path.unwrap();
        let (lx, ly) = path.last().unwrap();
        assert_eq!(*lx, 17.0);
        assert_eq!(*ly, 17.0);
        // Verify monotonic progress
        for i in 1..path.len() {
            let (px, py) = path[i - 1];
            let (cx, cy) = path[i];
            let step = ((cx - px).abs().max((cy - py).abs())) as i32;
            assert!(step <= 2, "each step should be at most 1 tile away");
        }
    }

    // --- Path cache: astar_full_path correctness ---

    #[test]
    fn astar_full_path_simple_3_tile() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        // From (2,5) to (5,5) — 3 tiles apart horizontally
        let path = map.astar_full_path(2.0, 5.0, 5.0, 5.0, 100);
        assert!(path.is_some());
        let path = path.unwrap();
        // Should have waypoints: at least (3,5), (4,5), (5,5) or similar
        assert!(
            path.len() >= 2,
            "3-tile path should have at least 2 waypoints"
        );
        // Last waypoint must be destination
        let (lx, ly) = path.last().unwrap();
        assert_eq!(*lx, 5.0);
        assert_eq!(*ly, 5.0);
    }

    #[test]
    fn astar_full_path_blocked_returns_none() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        // Create a complete wall blocking all paths
        for y in 0..10 {
            map.set(5, y, Terrain::BuildingWall);
        }
        let path = map.astar_full_path(2.0, 5.0, 8.0, 5.0, 500);
        assert!(path.is_none(), "fully blocked path should return None");
    }

    #[test]
    fn astar_full_path_blocked_waypoint_invalidates_cache() {
        // Simulate: compute path, then place building on a waypoint
        let mut map = TileMap::new(20, 10, Terrain::Grass);
        let path = map.astar_full_path(2.0, 5.0, 15.0, 5.0, 500);
        assert!(path.is_some());
        let path = path.unwrap();

        // Block a waypoint along the path
        let (wx, wy) = path[path.len() / 2];
        map.set(wx as usize, wy as usize, Terrain::BuildingWall);

        // The blocked tile is no longer walkable
        assert!(
            !map.is_walkable(wx, wy),
            "blocked waypoint should not be walkable"
        );

        // Recomputing should find an alternate route or return None
        let new_path = map.astar_full_path(2.0, 5.0, 15.0, 5.0, 500);
        if let Some(np) = &new_path {
            // New path should NOT go through the blocked tile
            assert!(
                !np.iter()
                    .any(|&(px, py)| px as usize == wx as usize && py as usize == wy as usize),
                "new path should avoid blocked waypoint"
            );
        }
    }

    // --- Terrain change tests ---

    #[test]
    fn deforestation_forest_becomes_stump() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(5, 5, Terrain::Forest);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Forest);

        // Simulate harvest: convert Forest -> Stump
        map.set(5, 5, Terrain::Stump);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Stump);
    }

    #[test]
    fn mining_progression_zero_to_quarry() {
        let mut map = TileMap::new(10, 10, Terrain::Mountain);
        assert_eq!(map.mine_count(5, 5), 0);

        // Mine 6 times -> should become Quarry
        for _ in 0..6 {
            let count = map.increment_mine_count(5, 5);
            if count >= 6 {
                map.set(5, 5, Terrain::Quarry);
            }
        }
        assert_eq!(map.mine_count(5, 5), 6);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Quarry);
    }

    #[test]
    fn mining_progression_quarry_to_deep() {
        let mut map = TileMap::new(10, 10, Terrain::Mountain);
        // Advance to Quarry
        for _ in 0..6 {
            map.increment_mine_count(5, 5);
        }
        map.set(5, 5, Terrain::Quarry);

        // Mine 6 more -> should become QuarryDeep
        for _ in 0..6 {
            let count = map.increment_mine_count(5, 5);
            if count >= 12 {
                map.set(5, 5, Terrain::QuarryDeep);
            }
        }
        assert_eq!(map.mine_count(5, 5), 12);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::QuarryDeep);
    }

    #[test]
    fn quarry_walkability_and_cost() {
        assert!(Terrain::Quarry.is_walkable());
        assert!(Terrain::QuarryDeep.is_walkable());
        assert!(Terrain::Quarry.move_cost() < Terrain::Mountain.move_cost());
        assert!(Terrain::QuarryDeep.move_cost() < Terrain::Mountain.move_cost());
    }

    #[test]
    fn stump_bare_sapling_speed_multipliers() {
        assert_eq!(Terrain::Stump.speed_multiplier(), 0.8);
        assert_eq!(Terrain::Bare.speed_multiplier(), 0.9);
        assert_eq!(Terrain::Sapling.speed_multiplier(), 0.7);

        // All should be walkable
        assert!(Terrain::Stump.is_walkable());
        assert!(Terrain::Bare.is_walkable());
        assert!(Terrain::Sapling.is_walkable());
    }

    #[test]
    fn building_wall_and_cliff_not_walkable() {
        assert!(!Terrain::BuildingWall.is_walkable());
        assert!(!Terrain::Cliff.is_walkable());
        assert_eq!(Terrain::BuildingWall.speed_multiplier(), 0.0);
        assert_eq!(Terrain::Cliff.speed_multiplier(), 0.0);
    }

    #[test]
    fn is_walkable_method_checks_bounds() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        assert!(map.is_walkable(5.0, 5.0));
        assert!(
            !map.is_walkable(-1.0, 5.0),
            "negative x should not be walkable"
        );
        assert!(
            !map.is_walkable(5.0, -1.0),
            "negative y should not be walkable"
        );
        assert!(
            !map.is_walkable(100.0, 5.0),
            "out of bounds should not be walkable"
        );
    }

    // --- Seasonal terrain tests ---

    #[test]
    fn ice_terrain_properties() {
        assert!(Terrain::Ice.is_walkable());
        assert_eq!(Terrain::Ice.ch(), '=');
        assert_eq!(Terrain::Ice.speed_multiplier(), 0.5);
        assert_eq!(Terrain::Ice.move_cost(), 2.0);
        assert!(Terrain::Ice.bg().is_some());
    }

    #[test]
    fn flood_water_terrain_properties() {
        assert!(!Terrain::FloodWater.is_walkable());
        assert_eq!(Terrain::FloodWater.ch(), '~');
        assert_eq!(Terrain::FloodWater.speed_multiplier(), 0.0);
        assert_eq!(Terrain::FloodWater.move_cost(), f64::INFINITY);
        assert!(Terrain::FloodWater.bg().is_some());
    }

    #[test]
    fn base_terrain_snapshot() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(3, 3, Terrain::Water);
        map.init_base_terrain();

        // Base should match current tiles
        assert_eq!(*map.get_base(3, 3).unwrap(), Terrain::Water);
        assert_eq!(*map.get_base(0, 0).unwrap(), Terrain::Grass);
    }

    #[test]
    fn seasonal_overlay_preserves_base() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(5, 5, Terrain::Water);
        map.init_base_terrain();

        // Apply seasonal overlay
        map.set_seasonal(5, 5, Terrain::Ice);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Ice);
        assert_eq!(*map.get_base(5, 5).unwrap(), Terrain::Water);
    }

    #[test]
    fn revert_seasonal_restores_base() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(5, 5, Terrain::Water);
        map.init_base_terrain();

        map.set_seasonal(5, 5, Terrain::Ice);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Ice);

        map.revert_seasonal(5, 5);
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Water);
    }

    #[test]
    fn apply_winter_ice_freezes_water() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(2, 2, Terrain::Water);
        map.set(3, 3, Terrain::Water);
        map.set(4, 4, Terrain::Water);
        map.init_base_terrain();

        let frozen = map.apply_winter_ice();
        assert_eq!(frozen, 3);
        assert_eq!(*map.get(2, 2).unwrap(), Terrain::Ice);
        assert_eq!(*map.get(3, 3).unwrap(), Terrain::Ice);
        assert_eq!(*map.get(4, 4).unwrap(), Terrain::Ice);
        // Non-water tiles unaffected
        assert_eq!(*map.get(0, 0).unwrap(), Terrain::Grass);
    }

    #[test]
    fn revert_ice_thaws_to_water() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(2, 2, Terrain::Water);
        map.set(3, 3, Terrain::Water);
        map.init_base_terrain();

        map.apply_winter_ice();
        assert_eq!(*map.get(2, 2).unwrap(), Terrain::Ice);

        map.revert_ice();
        assert_eq!(*map.get(2, 2).unwrap(), Terrain::Water);
        assert_eq!(*map.get(3, 3).unwrap(), Terrain::Water);
    }

    #[test]
    fn revert_flood_water_returns_positions() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.init_base_terrain();

        map.set_seasonal(1, 1, Terrain::FloodWater);
        map.set_seasonal(2, 2, Terrain::FloodWater);

        let reverted = map.revert_flood_water();
        assert_eq!(reverted.len(), 2);
        assert_eq!(*map.get(1, 1).unwrap(), Terrain::Grass);
        assert_eq!(*map.get(2, 2).unwrap(), Terrain::Grass);
    }

    #[test]
    fn astar_paths_across_ice() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        // River wall from (5,0) to (5,19) — impassable water
        for y in 0..20 {
            map.set(5, y, Terrain::Water);
        }
        map.init_base_terrain();

        // Without ice, no path across
        let no_path = map.astar_next(3.0, 10.0, 7.0, 10.0, 2000);
        assert!(no_path.is_none(), "should not find path across water");

        // Freeze the river
        map.apply_winter_ice();

        // With ice, should find path
        let path = map.astar_next(3.0, 10.0, 7.0, 10.0, 500);
        assert!(path.is_some(), "should find path across ice");
        let (nx, _ny) = path.unwrap();
        assert!(nx > 3.0, "should move toward target across ice");
    }

    #[test]
    fn flood_water_blocks_astar() {
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        map.init_base_terrain();

        // Wall of FloodWater
        for y in 0..20 {
            map.set_seasonal(10, y, Terrain::FloodWater);
        }

        let path = map.astar_next(5.0, 10.0, 15.0, 10.0, 2000);
        assert!(
            path.is_none(),
            "FloodWater wall should block A* pathfinding"
        );
    }

    #[test]
    fn apply_spring_floods_on_alluvial_near_river() {
        use crate::terrain_pipeline::SoilType;
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        let mut heights = vec![0.5; 20 * 20];
        let mut river_mask = vec![false; 20 * 20];
        let mut soil = vec![SoilType::Loam; 20 * 20];

        // River at x=10
        for y in 0..20 {
            let idx = y * 20 + 10;
            river_mask[idx] = true;
            heights[idx] = 0.3;
            map.set(10, y, Terrain::Water);
        }

        // Alluvial soil at (9, 5) — within 2 tiles of river, low elevation
        let target_idx = 5 * 20 + 9;
        soil[target_idx] = SoilType::Alluvial;
        heights[target_idx] = 0.3; // at river level

        map.init_base_terrain();

        let flooded = map.apply_spring_floods(&river_mask, &heights, &soil);
        assert!(
            flooded.contains(&(9, 5)),
            "alluvial tile near river at river elevation should flood"
        );
        assert_eq!(
            *map.get(9, 5).unwrap(),
            Terrain::FloodWater,
            "flooded tile should be FloodWater"
        );
    }

    #[test]
    fn spring_floods_skip_high_elevation() {
        use crate::terrain_pipeline::SoilType;
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        let mut heights = vec![0.5; 20 * 20];
        let mut river_mask = vec![false; 20 * 20];
        let mut soil = vec![SoilType::Alluvial; 20 * 20]; // all alluvial

        // River at x=10 with low elevation
        for y in 0..20 {
            let idx = y * 20 + 10;
            river_mask[idx] = true;
            heights[idx] = 0.3;
        }

        // Tile at (9, 5) is high elevation — should NOT flood
        heights[5 * 20 + 9] = 0.6; // well above river

        map.init_base_terrain();

        let flooded = map.apply_spring_floods(&river_mask, &heights, &soil);
        assert!(
            !flooded.contains(&(9, 5)),
            "high elevation tile should not flood even if alluvial"
        );
    }

    #[test]
    fn spring_floods_skip_non_alluvial() {
        use crate::terrain_pipeline::SoilType;
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        let mut heights = vec![0.3; 20 * 20]; // all low
        let mut river_mask = vec![false; 20 * 20];
        let soil = vec![SoilType::Rocky; 20 * 20]; // no alluvial

        // River at x=10
        for y in 0..20 {
            river_mask[y * 20 + 10] = true;
        }

        map.init_base_terrain();

        let flooded = map.apply_spring_floods(&river_mask, &heights, &soil);
        assert!(flooded.is_empty(), "rocky soil near river should not flood");
    }

    #[test]
    fn spring_floods_skip_buildings() {
        use crate::terrain_pipeline::SoilType;
        let mut map = TileMap::new(20, 20, Terrain::Grass);
        let heights = vec![0.3; 20 * 20];
        let mut river_mask = vec![false; 20 * 20];
        let mut soil = vec![SoilType::Alluvial; 20 * 20];

        for y in 0..20 {
            river_mask[y * 20 + 10] = true;
        }

        // Place a building near the river
        map.set(9, 5, Terrain::BuildingFloor);
        map.init_base_terrain();

        let flooded = map.apply_spring_floods(&river_mask, &heights, &soil);
        assert!(
            !flooded.contains(&(9, 5)),
            "building tiles should not be flooded"
        );
    }

    #[test]
    fn ice_freeze_thaw_cycle() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(5, 5, Terrain::Water);
        map.init_base_terrain();

        // Freeze
        map.apply_winter_ice();
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Ice);
        assert!(map.is_walkable(5.0, 5.0), "ice should be walkable");

        // Thaw
        map.revert_ice();
        assert_eq!(*map.get(5, 5).unwrap(), Terrain::Water);
        assert!(!map.is_walkable(5.0, 5.0), "water should not be walkable");
    }

    #[test]
    fn flood_water_reverts_to_original_terrain() {
        let mut map = TileMap::new(10, 10, Terrain::Grass);
        map.set(3, 3, Terrain::Marsh);
        map.init_base_terrain();

        map.set_seasonal(3, 3, Terrain::FloodWater);
        assert_eq!(*map.get(3, 3).unwrap(), Terrain::FloodWater);

        map.revert_seasonal(3, 3);
        assert_eq!(
            *map.get(3, 3).unwrap(),
            Terrain::Marsh,
            "should revert to original Marsh, not Grass"
        );
    }

    #[test]
    fn astar_cost_overlay_avoids_high_cost_area() {
        // Create a map where the direct path goes through a danger zone
        let map = TileMap::new(20, 10, Terrain::Grass);
        let w = 20;
        let h = 10;
        let mut overlay = vec![0.0f64; w * h];

        // Place heavy danger scent across the direct path (y=5, x=5..15)
        for x in 5..15 {
            overlay[5 * w + x] = 100.0; // very high danger
        }

        // Path from (0, 5) to (19, 5) — direct path goes through danger zone
        let path_danger =
            map.astar_full_path_with_cost_overlay(0.0, 5.0, 19.0, 5.0, 1000, &overlay);
        let path_normal = map.astar_full_path(0.0, 5.0, 19.0, 5.0, 1000);

        assert!(path_danger.is_some(), "should find path even with danger");
        assert!(path_normal.is_some(), "should find normal path");

        let danger_path = path_danger.unwrap();
        let normal_path = path_normal.unwrap();

        // The danger-aware path should be longer (detours around the scent zone)
        assert!(
            danger_path.len() >= normal_path.len(),
            "danger-aware path should be at least as long as normal: {} vs {}",
            danger_path.len(),
            normal_path.len()
        );

        // Check that the danger-aware path avoids the danger zone (y=5, x=5..15)
        let enters_danger_zone = danger_path
            .iter()
            .any(|(x, y)| *y == 5.0 && *x >= 5.0 && *x < 15.0);
        // With very high danger (100.0 / 20.0 = 5.0 penalty per tile), the path
        // should strongly prefer going around
        assert!(
            !enters_danger_zone || danger_path.len() > normal_path.len(),
            "danger-aware path should avoid or detour around the danger zone"
        );
    }

    #[test]
    fn astar_cost_overlay_wrong_size_falls_back() {
        let map = TileMap::new(10, 10, Terrain::Grass);
        // Wrong-size overlay should fall back to regular A*
        let overlay = vec![0.0; 5]; // wrong size
        let path = map.astar_full_path_with_cost_overlay(0.0, 0.0, 9.0, 9.0, 500, &overlay);
        assert!(
            path.is_some(),
            "should fall back to regular A* with wrong-size overlay"
        );
    }

    // --- Map Mode terrain glyph tests ---

    #[test]
    fn map_mode_water_glyph_is_tilde() {
        assert_eq!(Terrain::Water.map_ch(), '~');
    }

    #[test]
    fn map_mode_grass_glyph_is_dot() {
        assert_eq!(Terrain::Grass.map_ch(), '.');
    }

    #[test]
    fn map_mode_forest_glyph_is_spade() {
        assert_eq!(Terrain::Forest.map_ch(), '\u{2660}'); // ♠
    }

    #[test]
    fn map_mode_mountain_glyph_is_triangle() {
        assert_eq!(Terrain::Mountain.map_ch(), '\u{25B2}'); // ▲
    }

    #[test]
    fn map_mode_road_glyph_is_equals() {
        assert_eq!(Terrain::Road.map_ch(), '=');
    }

    #[test]
    fn map_mode_building_floor_is_plus() {
        assert_eq!(Terrain::BuildingFloor.map_ch(), '+');
    }

    #[test]
    fn map_mode_all_terrains_have_colors() {
        // Every terrain variant must return valid map fg/bg colors.
        let terrains = [
            Terrain::Water,
            Terrain::Sand,
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Mountain,
            Terrain::Snow,
            Terrain::Cliff,
            Terrain::Marsh,
            Terrain::Desert,
            Terrain::Tundra,
            Terrain::Scrubland,
            Terrain::Stump,
            Terrain::Bare,
            Terrain::Sapling,
            Terrain::Quarry,
            Terrain::QuarryDeep,
            Terrain::ScarredGround,
            Terrain::BuildingFloor,
            Terrain::BuildingWall,
            Terrain::Road,
            Terrain::Ford,
            Terrain::Bridge,
            Terrain::Ice,
            Terrain::FloodWater,
            Terrain::Burning,
            Terrain::Scorched,
        ];
        for t in &terrains {
            let _ch = t.map_ch();
            let _fg = t.map_fg();
            let _bg = t.map_bg();
            // Just verify no panic — the match arms are exhaustive
        }
    }

    #[test]
    fn map_mode_terrain_bg_dimmer_than_fg() {
        // Map Mode design: terrain bg should generally be dimmer than fg.
        let terrains = [
            Terrain::Grass,
            Terrain::Forest,
            Terrain::Water,
            Terrain::Mountain,
            Terrain::Sand,
            Terrain::Snow,
        ];
        for t in &terrains {
            let Color(fr, fg, fb) = t.map_fg();
            let Color(br, bg, bb) = t.map_bg();
            let fg_lum = fr as u32 + fg as u32 + fb as u32;
            let bg_lum = br as u32 + bg as u32 + bb as u32;
            assert!(
                fg_lum >= bg_lum,
                "terrain {:?}: map fg luminance ({}) should be >= bg luminance ({})",
                t,
                fg_lum,
                bg_lum,
            );
        }
    }
}
