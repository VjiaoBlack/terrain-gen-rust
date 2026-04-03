use serde::{Deserialize, Serialize};

use crate::renderer::Color;
use crate::tilemap::Terrain;

// --- AI Result ---

/// Skill-derived multipliers passed into AI systems.
pub struct SkillMults {
    pub gather_wood_speed: f64, // multiplier on gathering timer (lower = faster)
    pub gather_stone_speed: f64,
    pub build_speed: u32, // extra progress per tick
}

impl Default for SkillMults {
    fn default() -> Self {
        Self {
            gather_wood_speed: 1.0,
            gather_stone_speed: 1.0,
            build_speed: 0,
        }
    }
}

/// Result of running the AI system for one tick.
pub struct AiResult {
    pub deposited: Vec<ResourceType>,
    pub food_consumed: u32,
    pub grain_consumed: u32,
    pub bread_consumed: u32,
    pub farming_ticks: u32,
    pub mining_ticks: u32,
    pub woodcutting_ticks: u32,
    pub building_ticks: u32,
    /// Positions where wood was harvested (Forest tiles to convert to Stump).
    pub wood_harvest_positions: Vec<(f64, f64)>,
    /// Positions where stone was harvested from Mountain tiles (for mining terrain changes).
    pub stone_harvest_positions: Vec<(f64, f64)>,
    /// Positions where StoneDeposit entities were fully depleted (for ScarredGround conversion).
    pub depleted_stone_positions: Vec<(f64, f64)>,
}

// --- Villager Memory ---

/// Maximum entries per villager memory.
pub const MEMORY_CAPACITY: usize = 32;
/// Below this confidence, entries are evicted.
pub const MEMORY_FORGET_THRESHOLD: f64 = 0.05;
/// Distance (tiles) at which effective decay rate doubles.
pub const DISTANCE_DECAY_SCALE: f64 = 60.0;
/// Ticks a villager pauses in "confused" idle when arriving at a stale memory location.
pub const STALE_ARRIVAL_PAUSE: u32 = 8;
/// Distance threshold for upsert deduplication (same-kind entries within this range are merged).
pub const MEMORY_UPSERT_RADIUS: f64 = 5.0;

/// What kind of thing a villager remembers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MemoryKind {
    FoodSource,
    WoodSource,
    StoneDeposit,
    DangerZone,
    BuildSite,
    ResourceDepleted,
}

impl MemoryKind {
    /// Pinned kinds do not decay (home, stockpile locations are handled separately).
    /// Currently all observation-based kinds decay.
    pub fn decays(&self) -> bool {
        true
    }

    /// Per-kind base decay rate (confidence lost per tick at distance 0).
    /// Derived from half-lives: rate = ln(2) / half_life.
    /// But we use a simpler model: rate per tick such that confidence reaches ~0.5
    /// at the half-life tick count (linear decay with per-kind rates).
    pub fn decay_rate(&self) -> f64 {
        match self {
            MemoryKind::StoneDeposit => 0.00023,   // half-life ~3000 ticks
            MemoryKind::WoodSource => 0.0007,      // half-life ~1000 ticks
            MemoryKind::FoodSource => 0.0017,      // half-life ~400 ticks
            MemoryKind::DangerZone => 0.0028,      // half-life ~250 ticks
            MemoryKind::BuildSite => 0.0005,       // half-life ~1400 ticks
            MemoryKind::ResourceDepleted => 0.001, // half-life ~700 ticks
        }
    }
}

/// A single thing a villager remembers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub kind: MemoryKind,
    pub x: f64,
    pub y: f64,
    pub tick_observed: u64,
    pub confidence: f64, // 0.0-1.0, decays over time
    /// True if the villager observed this directly; false if learned via encounter or board.
    /// Secondhand entries are NOT shared further (prevents gossip pollution)
    /// and are NOT posted to the bulletin board.
    #[serde(default = "default_firsthand")]
    pub firsthand: bool,
}

fn default_firsthand() -> bool {
    true
}

/// What a villager believes the stockpile contains.
/// Updated ONLY when the villager is physically at a stockpile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BelievedStockpile {
    pub food: u32,
    pub wood: u32,
    pub stone: u32,
    pub tick_observed: u64,
}

// --- Encounter Sharing Constants ---

/// Encounter radius in tiles (squared distance check: 3^2 = 9).
pub const ENCOUNTER_RADIUS_SQ: f64 = 9.0;
/// Max memory entries transferred per direction per encounter.
pub const MAX_SHARE_PER_ENCOUNTER: usize = 3;
/// Confidence penalty applied to shared entries (telephone game degradation).
pub const ENCOUNTER_CONFIDENCE_PENALTY: f64 = 0.15;
/// Ticks before two specific villagers can share again.
pub const ENCOUNTER_COOLDOWN_TICKS: u64 = 60;
/// Max entries in the encounter cooldown ring buffer.
pub const MAX_COOLDOWN_ENTRIES: usize = 8;
/// Encounter system runs every N ticks for performance.
pub const ENCOUNTER_SYSTEM_FREQUENCY: u64 = 5;

/// Ring buffer tracking recent encounter partners to prevent per-tick spam.
/// Stores (entity_generation, entity_id, tick_of_encounter) tuples.
/// When full, the oldest entry is overwritten.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncounterCooldowns {
    /// (entity_id as u64, tick_of_encounter) — we store raw id bits for serialization.
    pub entries: Vec<(u64, u64)>,
}

impl EncounterCooldowns {
    /// Check if we have shared with this entity within the cooldown window.
    pub fn on_cooldown(&self, entity_id: u64, current_tick: u64) -> bool {
        self.entries.iter().any(|&(eid, tick)| {
            eid == entity_id && current_tick.saturating_sub(tick) < ENCOUNTER_COOLDOWN_TICKS
        })
    }

    /// Record an encounter with another entity.
    pub fn record(&mut self, entity_id: u64, tick: u64) {
        // Update existing entry for same entity if present
        for entry in &mut self.entries {
            if entry.0 == entity_id {
                entry.1 = tick;
                return;
            }
        }
        // Otherwise add new or overwrite oldest
        if self.entries.len() < MAX_COOLDOWN_ENTRIES {
            self.entries.push((entity_id, tick));
        } else {
            // Overwrite oldest entry
            if let Some(oldest_idx) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.1)
                .map(|(i, _)| i)
            {
                self.entries[oldest_idx] = (entity_id, tick);
            }
        }
    }
}

/// Per-villager knowledge store. Stores personal observations alongside global state.
/// Phase 1: additive (AI still reads globals). Phase 2 will switch AI to read from memory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VillagerMemory {
    pub entries: Vec<MemoryEntry>,
    pub home: Option<(f64, f64)>,
    pub stockpile_loc: Option<(f64, f64)>,
    pub believed_stockpile: Option<BelievedStockpile>,
    /// Tracks recent encounter partners to prevent per-tick sharing spam.
    #[serde(default)]
    pub encounter_cooldowns: EncounterCooldowns,
}

impl VillagerMemory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a memory entry. If an entry of the same kind exists within
    /// MEMORY_UPSERT_RADIUS, refresh it instead of creating a duplicate.
    /// Entries inserted via upsert are firsthand (direct observation).
    pub fn upsert(&mut self, kind: MemoryKind, x: f64, y: f64, tick: u64) {
        // Check for existing nearby entry of same kind
        for entry in &mut self.entries {
            if entry.kind == kind {
                let dx = entry.x - x;
                let dy = entry.y - y;
                let d = (dx * dx + dy * dy).sqrt();
                if d < MEMORY_UPSERT_RADIUS {
                    // Refresh existing entry — re-observation makes it firsthand
                    entry.x = x;
                    entry.y = y;
                    entry.tick_observed = tick;
                    entry.confidence = 1.0;
                    entry.firsthand = true;
                    return;
                }
            }
        }

        // No existing entry — insert new one
        let entry = MemoryEntry {
            kind,
            x,
            y,
            tick_observed: tick,
            confidence: 1.0,
            firsthand: true,
        };

        if self.entries.len() < MEMORY_CAPACITY {
            self.entries.push(entry);
        } else {
            // Evict: first remove entries below forget threshold
            self.entries
                .retain(|e| e.confidence >= MEMORY_FORGET_THRESHOLD);

            if self.entries.len() >= MEMORY_CAPACITY {
                // Still full — remove lowest-confidence entry
                if let Some(min_idx) = self
                    .entries
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.confidence
                            .partial_cmp(&b.confidence)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                {
                    self.entries.swap_remove(min_idx);
                }
            }
            self.entries.push(entry);
        }
    }

    /// Decay all entry confidences using per-kind rates and distance modifier.
    /// Distance from the villager's current position to the memory location
    /// accelerates decay: `effective_rate = base_rate * (1.0 + distance / DISTANCE_DECAY_SCALE)`.
    pub fn decay_tick(&mut self, villager_x: f64, villager_y: f64) {
        for entry in &mut self.entries {
            if entry.kind.decays() {
                let dx = villager_x - entry.x;
                let dy = villager_y - entry.y;
                let distance = (dx * dx + dy * dy).sqrt();
                let distance_modifier = 1.0 + distance / DISTANCE_DECAY_SCALE;
                let effective_rate = entry.kind.decay_rate() * distance_modifier;
                entry.confidence -= effective_rate;
            }
        }
        self.entries
            .retain(|e| e.confidence >= MEMORY_FORGET_THRESHOLD);
    }

    /// Best-known location for a resource type, weighted by confidence and distance.
    /// Returns (x, y, score) where score = confidence - distance/100.
    pub fn best_resource(
        &self,
        kind: MemoryKind,
        from_x: f64,
        from_y: f64,
    ) -> Option<(f64, f64, f64)> {
        self.entries
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| {
                let dx = e.x - from_x;
                let dy = e.y - from_y;
                let d = (dx * dx + dy * dy).sqrt();
                let score = e.confidence - d / 100.0;
                (e.x, e.y, score)
            })
            .max_by(|(_, _, a), (_, _, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Check if there is a danger memory near the given location.
    pub fn danger_near(&self, x: f64, y: f64, radius: f64) -> bool {
        self.entries.iter().any(|e| {
            e.kind == MemoryKind::DangerZone && {
                let dx = e.x - x;
                let dy = e.y - y;
                (dx * dx + dy * dy).sqrt() < radius
            }
        })
    }

    /// Number of entries (for testing/debug).
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

// --- Bulletin Board ---

/// Maximum posts per bulletin board.
pub const BULLETIN_BOARD_CAPACITY: usize = 50;
/// Posts older than this many ticks are pruned.
pub const BULLETIN_BOARD_STALE_TICKS: u64 = 5000;
/// Confidence multiplier for secondhand knowledge learned from the board.
pub const BULLETIN_SECONDHAND_FACTOR: f64 = 0.8;
/// Minimum confidence for a memory entry to be posted to the board.
pub const BULLETIN_POST_MIN_CONFIDENCE: f64 = 0.5;

/// A single report posted to a stockpile's bulletin board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulletinPost {
    pub kind: MemoryKind,
    pub x: f64,
    pub y: f64,
    pub tick_posted: u64,
    pub confidence: f64,
}

/// The bulletin board attached to a stockpile entity.
/// Villagers write firsthand observations when depositing resources,
/// and read posts into personal memory (as secondhand) when idle at the stockpile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BulletinBoard {
    pub posts: Vec<BulletinPost>,
}

impl BulletinBoard {
    /// Check if a post of the given kind already exists near (x, y).
    pub fn has_post_near(&self, kind: MemoryKind, x: f64, y: f64) -> bool {
        self.posts.iter().any(|p| {
            p.kind == kind && {
                let dx = p.x - x;
                let dy = p.y - y;
                (dx * dx + dy * dy).sqrt() < MEMORY_UPSERT_RADIUS
            }
        })
    }

    /// Write a villager's firsthand memories to the board.
    /// Only posts firsthand entries with confidence > BULLETIN_POST_MIN_CONFIDENCE.
    /// Secondhand entries (learned via encounter) are NOT posted — prevents rumor pollution.
    /// ResourceDepleted posts cancel stale ResourceSighting posts at the same location.
    pub fn write_from_memory(&mut self, memory: &VillagerMemory, current_tick: u64) {
        for entry in &memory.entries {
            // Only post firsthand observations (not secondhand gossip)
            if !entry.firsthand {
                continue;
            }
            // Only post high-confidence observations
            if entry.confidence < BULLETIN_POST_MIN_CONFIDENCE {
                continue;
            }
            // ResourceDepleted: cancel matching resource sightings on the board
            if entry.kind == MemoryKind::ResourceDepleted {
                let ex = entry.x;
                let ey = entry.y;
                self.posts.retain(|p| {
                    if matches!(
                        p.kind,
                        MemoryKind::WoodSource | MemoryKind::StoneDeposit | MemoryKind::FoodSource
                    ) {
                        let dx = p.x - ex;
                        let dy = p.y - ey;
                        (dx * dx + dy * dy).sqrt() >= MEMORY_UPSERT_RADIUS
                    } else {
                        true
                    }
                });
            }
            // Don't double-post if already on the board
            if self.has_post_near(entry.kind, entry.x, entry.y) {
                continue;
            }
            self.posts.push(BulletinPost {
                kind: entry.kind,
                x: entry.x,
                y: entry.y,
                tick_posted: current_tick,
                confidence: entry.confidence,
            });
        }
        self.prune(current_tick);
    }

    /// Read board posts into a villager's personal memory as secondhand knowledge.
    /// Confidence is reduced by BULLETIN_SECONDHAND_FACTOR.
    pub fn read_into_memory(&self, memory: &mut VillagerMemory, current_tick: u64) {
        for post in &self.posts {
            // ResourceDepleted: remove contradicted entries from personal memory
            if post.kind == MemoryKind::ResourceDepleted {
                let px = post.x;
                let py = post.y;
                memory.entries.retain(|e| {
                    if matches!(
                        e.kind,
                        MemoryKind::WoodSource | MemoryKind::StoneDeposit | MemoryKind::FoodSource
                    ) {
                        let dx = e.x - px;
                        let dy = e.y - py;
                        (dx * dx + dy * dy).sqrt() >= MEMORY_UPSERT_RADIUS
                    } else {
                        true
                    }
                });
                continue; // Don't add ResourceDepleted to personal memory
            }
            // Skip if villager already knows about this location
            let already_known = memory.entries.iter().any(|e| {
                e.kind == post.kind && {
                    let dx = e.x - post.x;
                    let dy = e.y - post.y;
                    (dx * dx + dy * dy).sqrt() < MEMORY_UPSERT_RADIUS
                }
            });
            if already_known {
                continue;
            }
            // Learn as secondhand (reduced confidence)
            let secondhand_confidence = post.confidence * BULLETIN_SECONDHAND_FACTOR;
            memory.upsert(post.kind, post.x, post.y, current_tick);
            // Adjust confidence down and mark as secondhand for the entry we just inserted
            if let Some(entry) = memory.entries.iter_mut().rev().find(|e| {
                e.kind == post.kind && {
                    let dx = e.x - post.x;
                    let dy = e.y - post.y;
                    (dx * dx + dy * dy).sqrt() < MEMORY_UPSERT_RADIUS
                }
            }) {
                entry.confidence = secondhand_confidence;
                entry.firsthand = false;
            }
        }
    }

    /// Remove stale posts and enforce capacity limit.
    fn prune(&mut self, current_tick: u64) {
        // Remove posts older than the stale threshold
        self.posts
            .retain(|p| current_tick.saturating_sub(p.tick_posted) < BULLETIN_BOARD_STALE_TICKS);
        // Enforce capacity: keep most recent posts
        if self.posts.len() > BULLETIN_BOARD_CAPACITY {
            // Sort by tick_posted descending, keep newest
            self.posts.sort_by(|a, b| b.tick_posted.cmp(&a.tick_posted));
            self.posts.truncate(BULLETIN_BOARD_CAPACITY);
        }
    }
}

// --- Path Caching ---

/// Per-entity cached A* path. Stores waypoints so A* runs once per trip instead of every tick.
/// See docs/design/pillar5_scale/path_caching.md for design details.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathCache {
    /// Waypoints from current position to destination, in order.
    pub waypoints: Vec<(f64, f64)>,
    /// Index of next waypoint to follow (avoids Vec::remove(0) cost).
    pub cursor: usize,
    /// The destination these waypoints lead to (for invalidation).
    pub dest_x: f64,
    pub dest_y: f64,
    /// Tick when this path was computed (for staleness check).
    pub computed_tick: u64,
}

// --- Tick Budgeting ---

/// Priority-based tick budgeting: determines how often an entity runs AI.
/// See docs/design/pillar5_scale/tick_budgeting.md.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TickSchedule {
    /// Which tick this entity next runs AI.
    pub next_ai_tick: u64,
    /// Ticks between AI evaluations (set from priority category).
    pub interval: u8,
}

impl Default for TickSchedule {
    fn default() -> Self {
        Self {
            next_ai_tick: 0,
            interval: 1,
        }
    }
}

/// Maps a BehaviorState to a tick interval for AI scheduling.
/// Timer-based states (Sleeping, Eating, Gathering, Building) must run every tick
/// so their countdowns work correctly. Only decision-making states can be slowed.
/// - Critical (1): All timer-based states + urgent states
/// - Active (2): Movement states (Seek, Hauling, Exploring)
/// - Normal (4): Farming, Working (lease-based, not timer-based)
/// - Idle (8): Wander, Idle, AtHome (just waiting)
pub fn tick_priority(state: &BehaviorState) -> u8 {
    match state {
        // Critical: every tick (timer-based or urgent)
        BehaviorState::FleeHome { .. }
        | BehaviorState::Captured
        | BehaviorState::Hunting { .. }
        | BehaviorState::Building { .. }
        | BehaviorState::Sleeping { .. }
        | BehaviorState::Eating { .. }
        | BehaviorState::Gathering { .. } => 1,

        // Active: every 2 ticks (movement/pathfinding)
        BehaviorState::Seek { .. }
        | BehaviorState::Hauling { .. }
        | BehaviorState::Exploring { .. } => 2,

        // Normal: every 4 ticks (lease-based work)
        BehaviorState::Farming { .. } | BehaviorState::Working { .. } => 4,

        // Idle: every 8 ticks (just waiting, no urgency)
        BehaviorState::Wander { .. }
        | BehaviorState::Idle { .. }
        | BehaviorState::AtHome { .. } => 8,
    }
}

// --- Components ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Velocity {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sprite {
    pub ch: char,
    pub fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Species {
    Prey,
    Predator,
    Villager,
}

// Resource types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResourceType {
    Food,
    Wood,
    Stone,
    Planks,
    Masonry,
    Grain,
}

/// Visual fullness state of a resource in the stockpile.
/// Villagers read this when they can SEE the stockpile, not globally.
/// Phase 0: computed from global Resources counts; later will be per-stockpile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StockpileFullness {
    Empty,  // 0 of this resource
    Low,    // 1..=4
    Medium, // 5..=20
    High,   // > 20
}

impl StockpileFullness {
    pub fn from_count(count: u32) -> Self {
        match count {
            0 => StockpileFullness::Empty,
            1..=4 => StockpileFullness::Low,
            5..=20 => StockpileFullness::Medium,
            _ => StockpileFullness::High,
        }
    }

    /// Returns true if the resource level is critically low (Empty or Low).
    pub fn is_scarce(&self) -> bool {
        matches!(self, StockpileFullness::Empty | StockpileFullness::Low)
    }
}

/// Aggregate visual state of a stockpile's resources.
/// Replaces raw u32 counts for AI decision-making.
#[derive(Debug, Clone, Copy)]
pub struct StockpileState {
    pub food: StockpileFullness,
    pub wood: StockpileFullness,
    pub stone: StockpileFullness,
}

/// Marker for stockpile location (where villagers deposit resources).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Stockpile;

/// Resource carried by a villager or stored at stockpile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CarriedResource {
    pub resource_type: ResourceType,
    pub amount: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SeekReason {
    Food,
    Stockpile,
    BuildSite,
    Wood,
    Stone,
    Hut,
    ExitBuilding,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BehaviorState {
    /// Wander randomly. Timer counts down to next direction change.
    Wander { timer: u32 },
    /// Move toward a target position with a reason for debugging.
    Seek {
        target_x: f64,
        target_y: f64,
        reason: SeekReason,
    },
    /// Stand still. Timer counts down before switching to Wander.
    Idle { timer: u32 },
    /// Prey: eating at a food source.
    Eating { timer: u32 },
    /// Prey/villager: fleeing home because predator is nearby. Timer prevents getting stuck.
    FleeHome { timer: u32 },
    /// Prey: safe at home, resting until hungry.
    AtHome { timer: u32 },
    /// Predator: chasing a prey it spotted.
    Hunting { target_x: f64, target_y: f64 },
    /// Prey: captured by a predator, frozen in place until consumed.
    Captured,
    /// Villager: gathering a resource at a location.
    Gathering {
        timer: u32,
        resource_type: ResourceType,
    },
    /// Villager: hauling gathered resource back to stockpile.
    Hauling {
        target_x: f64,
        target_y: f64,
        resource_type: ResourceType,
    },
    /// Villager: sleeping at night.
    Sleeping { timer: u32 },
    /// Villager: building at a build site.
    Building {
        target_x: f64,
        target_y: f64,
        timer: u32,
    },
    /// Villager: exploring toward frontier to discover new resources.
    Exploring {
        target_x: f64,
        target_y: f64,
        timer: u32,
    },
    /// Villager: tending a farm (standing at farm, advancing growth). Lease expires → idle.
    Farming {
        target_x: f64,
        target_y: f64,
        lease: u32,
    },
    /// Villager: operating a workshop/smithy. Lease expires → idle.
    Working {
        target_x: f64,
        target_y: f64,
        lease: u32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Behavior {
    pub state: BehaviorState,
    pub speed: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Creature {
    pub species: Species,
    pub hunger: f64, // 0.0 = full, 1.0 = starving
    pub home_x: f64,
    pub home_y: f64,
    pub sight_range: f64, // how far this creature can see
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BuildingType {
    Hut,
    Wall,
    Farm,
    Stockpile,
    Workshop,
    Smithy,
    Garrison,
    Road,
    Granary,
    Bakery,
    TownHall,
}

/// Tile layout pattern for a building footprint.
#[derive(Debug, Clone, Copy)]
pub enum TileLayout {
    /// Single tile of the given terrain type (Wall, Road)
    Single(Terrain),
    /// Filled rectangle of BuildingFloor (Farm, Stockpile)
    FilledFloor,
    /// 3x3 walls with center floor and a door on the south side (Hut)
    WallsDoorSouth,
    /// 3x3 walls with center floor and a door on the north side (Workshop, Smithy, Granary, Bakery)
    WallsDoorNorth,
    /// 3x3 walls with center floor, no door (Garrison)
    WallsNoDoor,
}

/// Static definition of a building type's properties.
pub struct BuildingDef {
    pub name: &'static str,
    pub cost: Resources,
    pub build_time: u32,
    pub size: (i32, i32),
    pub layout: TileLayout,
}

impl BuildingType {
    /// Get the static definition for this building type.
    pub fn def(&self) -> BuildingDef {
        match self {
            BuildingType::Hut => BuildingDef {
                name: "Hut",
                cost: Resources {
                    wood: 6,
                    stone: 3,
                    ..DEF_RES
                },
                build_time: 180,
                size: (3, 3),
                layout: TileLayout::WallsDoorSouth,
            },
            BuildingType::Wall => BuildingDef {
                name: "Wall",
                cost: Resources {
                    wood: 2,
                    stone: 2,
                    ..DEF_RES
                },
                build_time: 45,
                size: (1, 1),
                layout: TileLayout::Single(Terrain::BuildingWall),
            },
            BuildingType::Farm => BuildingDef {
                name: "Farm",
                cost: Resources {
                    wood: 5,
                    stone: 1,
                    ..DEF_RES
                },
                build_time: 120,
                size: (3, 3),
                layout: TileLayout::FilledFloor,
            },
            BuildingType::Stockpile => BuildingDef {
                name: "Stockpile",
                cost: Resources { wood: 4, ..DEF_RES },
                build_time: 60,
                size: (2, 2),
                layout: TileLayout::FilledFloor,
            },
            BuildingType::Workshop => BuildingDef {
                name: "Workshop",
                cost: Resources {
                    wood: 5,
                    stone: 3,
                    ..DEF_RES
                },
                build_time: 220,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::Smithy => BuildingDef {
                name: "Smithy",
                cost: Resources {
                    wood: 10,
                    stone: 15,
                    ..DEF_RES
                },
                build_time: 270,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::Garrison => BuildingDef {
                name: "Garrison",
                cost: Resources {
                    wood: 6,
                    stone: 8,
                    ..DEF_RES
                },
                build_time: 180,
                size: (3, 3),
                layout: TileLayout::WallsNoDoor,
            },
            BuildingType::Road => BuildingDef {
                name: "Road",
                cost: Resources {
                    stone: 2,
                    ..DEF_RES
                },
                build_time: 30,
                size: (1, 1),
                layout: TileLayout::Single(Terrain::Road),
            },
            BuildingType::Granary => BuildingDef {
                name: "Granary",
                cost: Resources {
                    wood: 6,
                    stone: 4,
                    ..DEF_RES
                },
                build_time: 240,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::Bakery => BuildingDef {
                name: "Bakery",
                cost: Resources {
                    wood: 8,
                    stone: 6,
                    planks: 5,
                    ..DEF_RES
                },
                build_time: 210,
                size: (3, 3),
                layout: TileLayout::WallsDoorNorth,
            },
            BuildingType::TownHall => BuildingDef {
                name: "Town Hall",
                cost: Resources {
                    wood: 20,
                    stone: 30,
                    masonry: 80,
                    ..DEF_RES
                },
                build_time: 400,
                size: (3, 3),
                layout: TileLayout::WallsNoDoor,
            },
        }
    }

    pub fn cost(&self) -> Resources {
        self.def().cost
    }
    pub fn build_time(&self) -> u32 {
        self.def().build_time
    }
    pub fn size(&self) -> (i32, i32) {
        self.def().size
    }
    pub fn name(&self) -> &'static str {
        self.def().name
    }

    pub fn tiles(&self) -> Vec<(i32, i32, Terrain)> {
        let d = self.def();
        let (w, h) = d.size;
        match d.layout {
            TileLayout::Single(terrain) => vec![(0, 0, terrain)],
            TileLayout::FilledFloor => {
                let mut tiles = Vec::new();
                for dy in 0..h {
                    for dx in 0..w {
                        tiles.push((dx, dy, Terrain::BuildingFloor));
                    }
                }
                tiles
            }
            TileLayout::WallsDoorSouth => {
                // 3x3 hut: walls on top and sides, wide door on south
                // WWW
                // W.W
                // .._  (south side open for easy entry/exit)
                let mut tiles = Vec::new();
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingWall)); // north wall
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor)); // interior
                // South row: open (all floor) for wide doorway
                for dx in 0..3 {
                    tiles.push((dx, 2, Terrain::BuildingFloor));
                }
                tiles
            }
            TileLayout::WallsDoorNorth => {
                // 3x3 hut: walls on bottom and sides, wide door on north
                let mut tiles = Vec::new();
                // North row: open (all floor) for wide doorway
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingFloor));
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor)); // interior
                for dx in 0..3 {
                    tiles.push((dx, 2, Terrain::BuildingWall)); // south wall
                }
                tiles
            }
            TileLayout::WallsNoDoor => {
                let mut tiles = Vec::new();
                for dx in 0..3 {
                    tiles.push((dx, 0, Terrain::BuildingWall));
                    tiles.push((dx, 2, Terrain::BuildingWall));
                }
                tiles.push((0, 1, Terrain::BuildingWall));
                tiles.push((2, 1, Terrain::BuildingWall));
                tiles.push((1, 1, Terrain::BuildingFloor));
                tiles
            }
        }
    }

    pub fn all() -> &'static [BuildingType] {
        &[
            BuildingType::Hut,
            BuildingType::Wall,
            BuildingType::Farm,
            BuildingType::Stockpile,
            BuildingType::Workshop,
            BuildingType::Smithy,
            BuildingType::Garrison,
            BuildingType::Road,
            BuildingType::Granary,
            BuildingType::Bakery,
            BuildingType::TownHall,
        ]
    }
}

/// Zero-valued Resources constant for struct update syntax in const-like contexts.
const DEF_RES: Resources = Resources {
    food: 0,
    wood: 0,
    stone: 0,
    planks: 0,
    masonry: 0,
    grain: 0,
    bread: 0,
};

/// A build site entity — placed by the player, worked on by villagers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BuildSite {
    pub building_type: BuildingType,
    pub progress: u32,
    pub required: u32,
    pub assigned: bool,
    /// Game tick when this site was placed. Used to detect stuck sites.
    #[serde(default)]
    pub queued_at: u64,
}

/// Marker for a completed farm plot — grows crops and produces food.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FarmPlot {
    pub growth: f64, // 0.0 to 1.0
    pub harvest_ready: bool,
    #[serde(default)]
    pub worker_present: bool, // must have villager tending for growth
    #[serde(default)]
    pub pending_food: u32, // harvested food waiting for pickup
    #[serde(default)]
    pub tile_x: usize, // map x coordinate for moisture lookup
    #[serde(default)]
    pub tile_y: usize, // map y coordinate for moisture lookup
}

/// Marker component for berry bushes (food source for prey).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FoodSource;

/// Marker component for completed garrison buildings — provides defense bonus.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GarrisonBuilding {
    pub defense_bonus: f64,
}

/// Marker component for Town Hall — provides housing bonus and extends settlement influence.
/// The Town Hall is a late-game prestige building that sinks accumulated masonry/stone and
/// allows the settlement to house more villagers without building more huts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TownHallBuilding {
    pub housing_bonus: u32,
}

/// Marker component for completed huts — provides shelter for villagers at night.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HutBuilding {
    pub capacity: u32,
    pub occupants: u32,
}

/// Tracks remaining harvests for a resource entity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResourceYield {
    pub remaining: u32,
    pub max: u32,
}

/// Marker component for stone deposits (mineable by villagers).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StoneDeposit;

/// Marker component for dens (safe home for prey).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Den;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Recipe {
    WoodToPlanks,   // 2 Wood -> 1 Planks
    StoneToMasonry, // 2 Stone -> 1 Masonry
    FoodToGrain,    // 3 Food -> 2 Grain
    GrainToBread,   // 2 Grain + 1 Wood -> 3 Bread (highest food value)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProcessingBuilding {
    pub recipe: Recipe,
    pub progress: u32,
    pub required: u32, // ticks per processing cycle
    #[serde(default)]
    pub worker_present: bool, // must have villager operating for progress
}

// --- Resources ---

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Resources {
    pub food: u32,
    pub wood: u32,
    pub stone: u32,
    pub planks: u32,
    pub masonry: u32,
    pub grain: u32,
    #[serde(default)]
    pub bread: u32,
}

impl Resources {
    pub fn can_afford(&self, cost: &Resources) -> bool {
        self.food >= cost.food
            && self.wood >= cost.wood
            && self.stone >= cost.stone
            && self.planks >= cost.planks
            && self.masonry >= cost.masonry
            && self.grain >= cost.grain
            && self.bread >= cost.bread
    }

    pub fn deduct(&mut self, cost: &Resources) {
        self.food -= cost.food;
        self.wood -= cost.wood;
        self.stone -= cost.stone;
        self.planks -= cost.planks;
        self.masonry -= cost.masonry;
        self.grain -= cost.grain;
        self.bread -= cost.bread;
    }
}
