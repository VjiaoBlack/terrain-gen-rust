use hecs::{Entity, World};
use std::collections::HashMap;

use super::components::{Behavior, BehaviorState, ResourceType, TickSchedule};
use super::spatial::{SpatialHashGrid, category};

// --- Constants ---

/// Minimum members to form a group.
pub const MIN_GROUP_SIZE: usize = 3;

/// Maximum members per group (prevents mega-groups masking individual issues).
pub const MAX_GROUP_SIZE: usize = 15;

/// Default group detection radius (tiles) for activities without a specific radius.
pub const DEFAULT_GROUP_RADIUS: f64 = 12.0;

/// How often groups are re-detected (in ticks).
pub const GROUP_DETECTION_INTERVAL: u64 = 12;

/// Group radius per activity type.
fn group_radius(activity: GroupActivity) -> f64 {
    match activity {
        GroupActivity::Farming => 8.0,
        GroupActivity::Building => 4.0,
        GroupActivity::GatheringWood => 10.0,
        GroupActivity::GatheringStone => 10.0,
        GroupActivity::GatheringFood => 10.0,
        GroupActivity::Hauling => 6.0,
        GroupActivity::Working => 3.0,
        GroupActivity::Sleeping => 8.0,
        GroupActivity::Exploring => 12.0,
    }
}

// --- GroupActivity ---

/// Coarse activity category for grouping. Derived from BehaviorState discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupActivity {
    Farming,
    Building,
    GatheringWood,
    GatheringStone,
    GatheringFood,
    Hauling,
    Working,
    Sleeping,
    Exploring,
}

/// Map a BehaviorState to a GroupActivity, if groupable.
/// Non-groupable states return None.
pub fn classify_activity(state: &BehaviorState) -> Option<GroupActivity> {
    match state {
        BehaviorState::Farming { .. } => Some(GroupActivity::Farming),
        BehaviorState::Building { .. } => Some(GroupActivity::Building),
        BehaviorState::Gathering {
            resource_type: ResourceType::Wood,
            ..
        } => Some(GroupActivity::GatheringWood),
        BehaviorState::Gathering {
            resource_type: ResourceType::Stone,
            ..
        } => Some(GroupActivity::GatheringStone),
        BehaviorState::Gathering {
            resource_type: ResourceType::Food,
            ..
        } => Some(GroupActivity::GatheringFood),
        BehaviorState::Hauling { .. } => Some(GroupActivity::Hauling),
        BehaviorState::Working { .. } => Some(GroupActivity::Working),
        BehaviorState::Sleeping { .. } => Some(GroupActivity::Sleeping),
        BehaviorState::Exploring { .. } => Some(GroupActivity::Exploring),
        // Non-groupable: Wander, Idle, Seek, FleeHome, Hunting, Captured, Eating, AtHome
        _ => None,
    }
}

// --- AgentGroup ---

/// A temporary group of entities performing the same activity in the same area.
/// Not an ECS entity. Lives in a side structure rebuilt periodically.
#[derive(Debug, Clone)]
pub struct AgentGroup {
    /// Unique ID for this tick's group set. Not stable across ticks.
    pub id: u32,
    /// The shared activity type.
    pub activity: GroupActivity,
    /// Centroid position.
    pub centroid_x: f64,
    pub centroid_y: f64,
    /// Member entity IDs.
    pub members: Vec<Entity>,
    /// Whether a threat was detected near this group's centroid.
    pub threat_nearby: bool,
    /// The tick this group was formed.
    pub formed_tick: u64,
}

// --- GroupManager ---

/// Manages detection and lifecycle of agent groups.
/// Stored in the Game struct alongside the spatial grid.
pub struct GroupManager {
    /// Currently active groups.
    pub groups: Vec<AgentGroup>,
    /// Monotonic ID counter.
    id_counter: u32,
    /// Tick of last full detection pass.
    pub last_detection_tick: u64,
    /// Entity -> group_id lookup for O(1) membership check.
    membership: HashMap<Entity, u32>,
}

impl Default for GroupManager {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            id_counter: 0,
            last_detection_tick: 0,
            membership: HashMap::new(),
        }
    }
}

impl GroupManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if an entity is currently in a group. Returns the group ID if so.
    pub fn group_id_of(&self, entity: Entity) -> Option<u32> {
        self.membership.get(&entity).copied()
    }

    /// Check if an entity is currently in any group.
    pub fn is_grouped(&self, entity: Entity) -> bool {
        self.membership.contains_key(&entity)
    }

    /// Number of active groups.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Total number of grouped entities.
    pub fn grouped_entity_count(&self) -> usize {
        self.membership.len()
    }

    /// Run group detection from spatial grid and world state.
    /// Should be called every GROUP_DETECTION_INTERVAL ticks.
    pub fn detect_groups(&mut self, world: &World, grid: &SpatialHashGrid, current_tick: u64) {
        self.groups.clear();
        self.membership.clear();

        // Step 1: Bucket villager entities by (cell_index, activity).
        // We iterate the spatial grid's cells to find villager entities,
        // then classify their BehaviorState.
        let mut buckets: HashMap<(usize, GroupActivity), Vec<(Entity, f64, f64)>> = HashMap::new();

        let cell_count = grid.cell_count();
        for cell_idx in 0..cell_count {
            let entries = grid.entries_in_cell_by_index(cell_idx);
            for entry in entries {
                // Only consider villagers
                if entry.categories & category::VILLAGER == 0 {
                    continue;
                }
                // Get behavior state
                let Ok(behavior) = world.get::<&Behavior>(entry.entity) else {
                    continue;
                };
                let Some(activity) = classify_activity(&behavior.state) else {
                    continue;
                };
                buckets.entry((cell_idx, activity)).or_default().push((
                    entry.entity,
                    entry.x,
                    entry.y,
                ));
            }
        }

        // Step 2: For each bucket with >= MIN_GROUP_SIZE, form a group.
        for ((_cell_idx, activity), entities) in &buckets {
            if entities.len() < MIN_GROUP_SIZE {
                continue;
            }

            // Compute centroid
            let (sum_x, sum_y) = entities
                .iter()
                .fold((0.0, 0.0), |(sx, sy), (_, x, y)| (sx + x, sy + y));
            let n = entities.len() as f64;
            let cx = sum_x / n;
            let cy = sum_y / n;

            // Filter by radius
            let radius = group_radius(*activity);
            let r_sq = radius * radius;
            let mut members: Vec<(Entity, f64, f64)> = entities
                .iter()
                .filter(|(_, x, y)| {
                    let dx = x - cx;
                    let dy = y - cy;
                    dx * dx + dy * dy <= r_sq
                })
                .copied()
                .collect();

            if members.len() < MIN_GROUP_SIZE {
                continue;
            }

            // Cap at MAX_GROUP_SIZE (keep closest to centroid)
            if members.len() > MAX_GROUP_SIZE {
                members.sort_by(|(_, ax, ay), (_, bx, by)| {
                    let da = (ax - cx) * (ax - cx) + (ay - cy) * (ay - cy);
                    let db = (bx - cx) * (bx - cx) + (by - cy) * (by - cy);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
                members.truncate(MAX_GROUP_SIZE);
            }

            // Recompute centroid with filtered members
            let (sum_x2, sum_y2) = members
                .iter()
                .fold((0.0, 0.0), |(sx, sy), (_, x, y)| (sx + x, sy + y));
            let n2 = members.len() as f64;
            let final_cx = sum_x2 / n2;
            let final_cy = sum_y2 / n2;

            let group_id = self.id_counter;
            self.id_counter = self.id_counter.wrapping_add(1);

            let member_entities: Vec<Entity> = members.iter().map(|(e, _, _)| *e).collect();
            for &e in &member_entities {
                self.membership.insert(e, group_id);
            }

            self.groups.push(AgentGroup {
                id: group_id,
                activity: *activity,
                centroid_x: final_cx,
                centroid_y: final_cy,
                members: member_entities,
                threat_nearby: false,
                formed_tick: current_tick,
            });
        }

        self.last_detection_tick = current_tick;
    }

    /// Run per-group threat detection. For each group, check if any predator
    /// is within the group's activity radius of its centroid. If so, mark
    /// the group as threatened and dissolve it (members revert to individual AI).
    pub fn update_threat_detection(
        &mut self,
        world: &mut World,
        grid: &SpatialHashGrid,
        current_tick: u64,
    ) {
        let mut dissolved_entities: Vec<Entity> = Vec::new();

        self.groups.retain_mut(|group| {
            let radius = group_radius(group.activity);
            let threat = grid.any_within(
                group.centroid_x,
                group.centroid_y,
                radius,
                category::PREDATOR,
            );

            if threat {
                group.threat_nearby = true;
                // Dissolve: schedule immediate AI for all members
                for &member in &group.members {
                    dissolved_entities.push(member);
                }
                false // remove group
            } else {
                true // keep group
            }
        });

        // Remove dissolved entities from membership map and schedule immediate AI
        for entity in &dissolved_entities {
            self.membership.remove(entity);
            if let Ok(mut schedule) = world.get::<&mut TickSchedule>(*entity) {
                schedule.next_ai_tick = current_tick + 1;
            }
        }
    }

    /// Remove a specific entity from its group (e.g., on death or activity change).
    /// If the group drops below MIN_GROUP_SIZE, dissolve it entirely.
    /// Returns true if the entity was in a group and was removed.
    pub fn remove_entity(&mut self, entity: Entity, world: &mut World, current_tick: u64) -> bool {
        let Some(group_id) = self.membership.remove(&entity) else {
            return false;
        };

        // Find the group and remove the member
        let mut dissolve_group_idx = None;
        for (idx, group) in self.groups.iter_mut().enumerate() {
            if group.id == group_id {
                group.members.retain(|&e| e != entity);
                if group.members.len() < MIN_GROUP_SIZE {
                    dissolve_group_idx = Some(idx);
                }
                break;
            }
        }

        // Dissolve if below minimum
        if let Some(idx) = dissolve_group_idx {
            let group = self.groups.remove(idx);
            for &member in &group.members {
                self.membership.remove(&member);
                if let Ok(mut schedule) = world.get::<&mut TickSchedule>(member) {
                    schedule.next_ai_tick = current_tick + 1;
                }
            }
        }

        true
    }

    /// Remove dead entities from all groups. Called after system_death.
    pub fn remove_dead_entities(&mut self, dead: &[Entity], world: &mut World, current_tick: u64) {
        for &entity in dead {
            self.remove_entity(entity, world, current_tick);
        }
    }

    /// Clear all groups (e.g., on save/load).
    pub fn clear(&mut self) {
        self.groups.clear();
        self.membership.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::components::*;
    use crate::ecs::spatial::SpatialHashGrid;
    use hecs::World;

    /// Helper: spawn a villager entity at (x, y) with a given BehaviorState.
    fn spawn_villager_with_state(
        world: &mut World,
        x: f64,
        y: f64,
        state: BehaviorState,
    ) -> Entity {
        world.spawn((
            Position { x, y },
            Velocity { dx: 0.0, dy: 0.0 },
            Creature {
                species: Species::Villager,
                hunger: 0.0,
                home_x: x,
                home_y: y,
                sight_range: 12.0,
            },
            Behavior { state, speed: 1.0 },
            TickSchedule::default(),
            Sprite {
                ch: 'v',
                fg: crate::renderer::Color(255, 255, 255),
            },
        ))
    }

    /// Helper: spawn a predator entity at (x, y).
    fn spawn_predator(world: &mut World, x: f64, y: f64) -> Entity {
        world.spawn((
            Position { x, y },
            Velocity { dx: 0.0, dy: 0.0 },
            Creature {
                species: Species::Predator,
                hunger: 0.0,
                home_x: x,
                home_y: y,
                sight_range: 15.0,
            },
            Behavior {
                state: BehaviorState::Wander { timer: 50 },
                speed: 1.5,
            },
            Sprite {
                ch: 'W',
                fg: crate::renderer::Color(255, 0, 0),
            },
        ))
    }

    fn make_grid(world: &World) -> SpatialHashGrid {
        let mut grid = SpatialHashGrid::new(256, 256, 16);
        grid.populate(world);
        grid
    }

    // --- classify_activity tests ---

    #[test]
    fn classify_farming_is_groupable() {
        let state = BehaviorState::Farming {
            target_x: 10.0,
            target_y: 10.0,
            lease: 100,
        };
        assert_eq!(classify_activity(&state), Some(GroupActivity::Farming));
    }

    #[test]
    fn classify_building_is_groupable() {
        let state = BehaviorState::Building {
            target_x: 10.0,
            target_y: 10.0,
            timer: 50,
        };
        assert_eq!(classify_activity(&state), Some(GroupActivity::Building));
    }

    #[test]
    fn classify_gathering_wood_is_groupable() {
        let state = BehaviorState::Gathering {
            timer: 20,
            resource_type: ResourceType::Wood,
        };
        assert_eq!(
            classify_activity(&state),
            Some(GroupActivity::GatheringWood)
        );
    }

    #[test]
    fn classify_gathering_stone_is_groupable() {
        let state = BehaviorState::Gathering {
            timer: 20,
            resource_type: ResourceType::Stone,
        };
        assert_eq!(
            classify_activity(&state),
            Some(GroupActivity::GatheringStone)
        );
    }

    #[test]
    fn classify_gathering_food_is_groupable() {
        let state = BehaviorState::Gathering {
            timer: 20,
            resource_type: ResourceType::Food,
        };
        assert_eq!(
            classify_activity(&state),
            Some(GroupActivity::GatheringFood)
        );
    }

    #[test]
    fn classify_wander_not_groupable() {
        let state = BehaviorState::Wander { timer: 50 };
        assert_eq!(classify_activity(&state), None);
    }

    #[test]
    fn classify_idle_not_groupable() {
        let state = BehaviorState::Idle { timer: 50 };
        assert_eq!(classify_activity(&state), None);
    }

    #[test]
    fn classify_flee_not_groupable() {
        let state = BehaviorState::FleeHome { timer: 50 };
        assert_eq!(classify_activity(&state), None);
    }

    #[test]
    fn classify_seek_not_groupable() {
        let state = BehaviorState::Seek {
            target_x: 10.0,
            target_y: 10.0,
            reason: SeekReason::Food,
        };
        assert_eq!(classify_activity(&state), None);
    }

    #[test]
    fn classify_captured_not_groupable() {
        assert_eq!(classify_activity(&BehaviorState::Captured), None);
    }

    #[test]
    fn classify_hunting_not_groupable() {
        let state = BehaviorState::Hunting {
            target_x: 10.0,
            target_y: 10.0,
        };
        assert_eq!(classify_activity(&state), None);
    }

    // --- Group detection tests ---

    #[test]
    fn three_farmers_within_radius_form_group() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 52.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 52.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 1);
        assert_eq!(mgr.groups[0].members.len(), 3);
        assert_eq!(mgr.groups[0].activity, GroupActivity::Farming);
    }

    #[test]
    fn two_farmers_do_not_form_group() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 52.0, 50.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 0);
        assert_eq!(mgr.grouped_entity_count(), 0);
    }

    #[test]
    fn farmers_and_builders_form_separate_groups() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let building = BehaviorState::Building {
            target_x: 50.0,
            target_y: 50.0,
            timer: 50,
        };
        // 3 farmers
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 51.0, farming);
        // 3 builders at same location
        spawn_villager_with_state(&mut world, 52.0, 52.0, building);
        spawn_villager_with_state(&mut world, 53.0, 52.0, building);
        spawn_villager_with_state(&mut world, 52.0, 53.0, building);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 2);
        let activities: Vec<GroupActivity> = mgr.groups.iter().map(|g| g.activity).collect();
        assert!(activities.contains(&GroupActivity::Farming));
        assert!(activities.contains(&GroupActivity::Building));
    }

    #[test]
    fn entities_outside_radius_excluded() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        // 3 close together
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 51.0, farming);
        // 1 far away (outside farming radius of 8)
        spawn_villager_with_state(&mut world, 80.0, 80.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        // The group should have 3 members (the far one excluded or forms no group)
        let total_grouped = mgr.grouped_entity_count();
        assert_eq!(total_grouped, 3);
    }

    #[test]
    fn non_groupable_states_produce_no_groups() {
        let mut world = World::new();
        // 5 wandering villagers near each other
        for i in 0..5 {
            spawn_villager_with_state(
                &mut world,
                50.0 + i as f64,
                50.0,
                BehaviorState::Wander { timer: 50 },
            );
        }

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 0);
    }

    #[test]
    fn empty_grid_returns_no_groups() {
        let world = World::new();
        let grid = SpatialHashGrid::new(256, 256, 16);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 0);
    }

    #[test]
    fn group_centroid_is_average_position() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        spawn_villager_with_state(&mut world, 48.0, 48.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 52.0, 52.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 1);
        assert!((mgr.groups[0].centroid_x - 50.0).abs() < 0.01);
        assert!((mgr.groups[0].centroid_y - 50.0).abs() < 0.01);
    }

    #[test]
    fn group_membership_lookup_works() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let e2 = spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        let e3 = spawn_villager_with_state(&mut world, 50.0, 51.0, farming);
        // Ungrouped
        let e4 = spawn_villager_with_state(
            &mut world,
            200.0,
            200.0,
            BehaviorState::Wander { timer: 50 },
        );

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert!(mgr.is_grouped(e1));
        assert!(mgr.is_grouped(e2));
        assert!(mgr.is_grouped(e3));
        assert!(!mgr.is_grouped(e4));

        // All three should have the same group_id
        let gid = mgr.group_id_of(e1).unwrap();
        assert_eq!(mgr.group_id_of(e2), Some(gid));
        assert_eq!(mgr.group_id_of(e3), Some(gid));
    }

    #[test]
    fn max_group_size_enforced() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        // Spawn 20 farmers in a tight cluster
        for i in 0..20 {
            spawn_villager_with_state(
                &mut world,
                50.0 + (i % 5) as f64,
                50.0 + (i / 5) as f64,
                farming,
            );
        }

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 1);
        assert!(mgr.groups[0].members.len() <= MAX_GROUP_SIZE);
    }

    // --- Threat detection tests ---

    #[test]
    fn threat_dissolves_group() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let e2 = spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        let e3 = spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);

        // Now add a predator near the group
        spawn_predator(&mut world, 53.0, 50.0);
        let grid2 = make_grid(&world);
        mgr.update_threat_detection(&mut world, &grid2, 1);

        // Group should be dissolved
        assert_eq!(mgr.group_count(), 0);
        assert!(!mgr.is_grouped(e1));
        assert!(!mgr.is_grouped(e2));
        assert!(!mgr.is_grouped(e3));

        // Members should have next_ai_tick set to soon
        let sched = world.get::<&TickSchedule>(e1).unwrap();
        assert_eq!(sched.next_ai_tick, 2); // current_tick + 1
    }

    #[test]
    fn distant_threat_does_not_dissolve_group() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);

        // Predator far away (farming radius is 8)
        spawn_predator(&mut world, 200.0, 200.0);
        let grid2 = make_grid(&world);
        mgr.update_threat_detection(&mut world, &grid2, 1);

        // Group should survive
        assert_eq!(mgr.group_count(), 1);
    }

    // --- Dissolution tests ---

    #[test]
    fn remove_entity_from_group() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let e2 = spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        let e3 = spawn_villager_with_state(&mut world, 50.0, 51.0, farming);
        let e4 = spawn_villager_with_state(&mut world, 52.0, 52.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);
        assert_eq!(mgr.groups[0].members.len(), 4);

        // Remove one member — group stays (4 - 1 = 3 >= MIN_GROUP_SIZE)
        let removed = mgr.remove_entity(e1, &mut world, 1);
        assert!(removed);
        assert!(!mgr.is_grouped(e1));
        assert_eq!(mgr.group_count(), 1);
        assert_eq!(mgr.groups[0].members.len(), 3);
    }

    #[test]
    fn remove_entity_dissolves_if_below_minimum() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let e2 = spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        let e3 = spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);

        // Remove one — group drops to 2, below minimum, dissolves
        mgr.remove_entity(e1, &mut world, 1);
        assert_eq!(mgr.group_count(), 0);
        assert!(!mgr.is_grouped(e2));
        assert!(!mgr.is_grouped(e3));
    }

    #[test]
    fn remove_ungrouped_entity_returns_false() {
        let mut world = World::new();
        let e =
            spawn_villager_with_state(&mut world, 50.0, 50.0, BehaviorState::Wander { timer: 50 });

        let mut mgr = GroupManager::new();
        assert!(!mgr.remove_entity(e, &mut world, 0));
    }

    #[test]
    fn remove_dead_entities_cleans_groups() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let e2 = spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        let e3 = spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);

        // Simulate death of e1
        let dead = vec![e1];
        mgr.remove_dead_entities(&dead, &mut world, 1);

        // Group dissolved (3 - 1 = 2 < MIN_GROUP_SIZE)
        assert_eq!(mgr.group_count(), 0);
    }

    #[test]
    fn clear_removes_all_groups() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);

        mgr.clear();
        assert_eq!(mgr.group_count(), 0);
        assert_eq!(mgr.grouped_entity_count(), 0);
    }

    #[test]
    fn redetection_rebuilds_groups() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let e2 = spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        let e3 = spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);
        assert_eq!(mgr.group_count(), 1);

        // Change e1 to wandering (non-groupable)
        if let Ok(mut b) = world.get::<&mut Behavior>(e1) {
            b.state = BehaviorState::Wander { timer: 50 };
        }

        // Re-detect — group should dissolve since only 2 farmers remain
        let grid2 = make_grid(&world);
        mgr.detect_groups(&world, &grid2, 12);
        assert_eq!(mgr.group_count(), 0);
    }

    #[test]
    fn different_resource_types_form_separate_groups() {
        let mut world = World::new();
        // 3 gathering wood
        for i in 0..3 {
            spawn_villager_with_state(
                &mut world,
                50.0 + i as f64,
                50.0,
                BehaviorState::Gathering {
                    timer: 20,
                    resource_type: ResourceType::Wood,
                },
            );
        }
        // 3 gathering stone at same location
        for i in 0..3 {
            spawn_villager_with_state(
                &mut world,
                50.0 + i as f64,
                52.0,
                BehaviorState::Gathering {
                    timer: 20,
                    resource_type: ResourceType::Stone,
                },
            );
        }

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 2);
        let activities: Vec<GroupActivity> = mgr.groups.iter().map(|g| g.activity).collect();
        assert!(activities.contains(&GroupActivity::GatheringWood));
        assert!(activities.contains(&GroupActivity::GatheringStone));
    }

    #[test]
    fn detection_records_formed_tick() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 42);

        assert_eq!(mgr.groups[0].formed_tick, 42);
        assert_eq!(mgr.last_detection_tick, 42);
    }

    #[test]
    fn group_ids_are_unique() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let building = BehaviorState::Building {
            target_x: 50.0,
            target_y: 50.0,
            timer: 50,
        };
        for i in 0..3 {
            spawn_villager_with_state(&mut world, 50.0 + i as f64, 50.0, farming);
        }
        for i in 0..3 {
            spawn_villager_with_state(&mut world, 50.0 + i as f64, 52.0, building);
        }

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        assert_eq!(mgr.group_count(), 2);
        assert_ne!(mgr.groups[0].id, mgr.groups[1].id);
    }

    // --- Integration: system_ai skip for grouped entities ---

    #[test]
    fn is_grouped_query_after_detection() {
        let mut world = World::new();
        let farming = BehaviorState::Farming {
            target_x: 50.0,
            target_y: 50.0,
            lease: 100,
        };
        let e1 = spawn_villager_with_state(&mut world, 50.0, 50.0, farming);
        let ungrouped =
            spawn_villager_with_state(&mut world, 200.0, 200.0, BehaviorState::Idle { timer: 50 });

        // Need 3 for a group
        spawn_villager_with_state(&mut world, 51.0, 50.0, farming);
        spawn_villager_with_state(&mut world, 50.0, 51.0, farming);

        let grid = make_grid(&world);
        let mut mgr = GroupManager::new();
        mgr.detect_groups(&world, &grid, 0);

        // Grouped entity should be skipped in system_ai
        assert!(mgr.is_grouped(e1), "farmer in cluster should be grouped");
        assert!(
            !mgr.is_grouped(ungrouped),
            "idle villager should not be grouped"
        );
    }
}
