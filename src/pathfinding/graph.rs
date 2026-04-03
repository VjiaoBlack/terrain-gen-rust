use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::pathfinding::region::{
    NavRegion, REGION_SIZE, Transition, find_horizontal_transitions, find_vertical_transitions,
};
use crate::tilemap::TileMap;

/// Maximum number of dirty regions to recompute per tick.
pub const MAX_REGION_UPDATES_PER_TICK: usize = 8;

/// Distance threshold: paths longer than this use the hierarchy.
pub const HIERARCHY_DISTANCE_THRESHOLD: f64 = 32.0;

/// Navigation graph for hierarchical (two-level) pathfinding.
/// Precomputed at world-gen; updated incrementally when terrain changes.
#[derive(Debug, Clone)]
pub struct NavGraph {
    /// All regions, indexed by (ry * regions_w + rx).
    pub regions: Vec<NavRegion>,
    /// Number of regions in the X direction.
    pub regions_w: usize,
    /// Number of regions in the Y direction.
    pub regions_h: usize,
    /// All transition points.
    pub transitions: Vec<Transition>,
    /// Adjacency list: for each transition ID, a list of (neighbor_id, cost).
    pub edges: Vec<Vec<(usize, f64)>>,
    /// Lookup: (rx, ry, zone) -> list of transition IDs that touch that region-zone.
    pub region_transitions: HashMap<(usize, usize, u8), Vec<usize>>,
    /// Regions that need recomputation.
    pub dirty_regions: HashSet<(usize, usize)>,
}

impl Default for NavGraph {
    fn default() -> Self {
        NavGraph {
            regions: Vec::new(),
            regions_w: 0,
            regions_h: 0,
            transitions: Vec::new(),
            edges: Vec::new(),
            region_transitions: HashMap::new(),
            dirty_regions: HashSet::new(),
        }
    }
}

impl NavGraph {
    /// Build the full NavGraph from a TileMap.
    pub fn build(map: &TileMap) -> Self {
        let regions_w = (map.width + REGION_SIZE - 1) / REGION_SIZE;
        let regions_h = (map.height + REGION_SIZE - 1) / REGION_SIZE;

        // Compute all regions
        let mut regions = Vec::with_capacity(regions_w * regions_h);
        for ry in 0..regions_h {
            for rx in 0..regions_w {
                regions.push(NavRegion::compute(rx, ry, map));
            }
        }

        let mut graph = NavGraph {
            regions,
            regions_w,
            regions_h,
            transitions: Vec::new(),
            edges: Vec::new(),
            region_transitions: HashMap::new(),
            dirty_regions: HashSet::new(),
        };

        graph.rebuild_transitions(map);
        graph
    }

    /// Rebuild all transitions and edges from scratch (used at init and can be used for full rebuild).
    fn rebuild_transitions(&mut self, map: &TileMap) {
        self.transitions.clear();
        self.edges.clear();
        self.region_transitions.clear();

        // Find all border transitions between adjacent regions
        for ry in 0..self.regions_h {
            for rx in 0..self.regions_w {
                // Horizontal: (rx, ry) <-> (rx+1, ry)
                if rx + 1 < self.regions_w {
                    let left_idx = ry * self.regions_w + rx;
                    let right_idx = ry * self.regions_w + rx + 1;
                    let ts = find_horizontal_transitions(
                        &self.regions[left_idx],
                        &self.regions[right_idx],
                        map,
                    );
                    for t in ts {
                        self.add_transition(t);
                    }
                }
                // Vertical: (rx, ry) <-> (rx, ry+1)
                if ry + 1 < self.regions_h {
                    let top_idx = ry * self.regions_w + rx;
                    let bot_idx = (ry + 1) * self.regions_w + rx;
                    let ts = find_vertical_transitions(
                        &self.regions[top_idx],
                        &self.regions[bot_idx],
                        map,
                    );
                    for t in ts {
                        self.add_transition(t);
                    }
                }
            }
        }

        // Build intra-region edges: connect transitions that share the same (region, zone)
        self.build_intra_region_edges(map);
    }

    /// Add a transition and register it in the lookup maps (bidirectional).
    fn add_transition(&mut self, t: Transition) {
        let id = self.transitions.len();

        // Register for from side
        self.region_transitions
            .entry((t.from_region.0, t.from_region.1, t.from_zone))
            .or_default()
            .push(id);
        // Register for to side
        self.region_transitions
            .entry((t.to_region.0, t.to_region.1, t.to_zone))
            .or_default()
            .push(id);

        self.transitions.push(t);
        self.edges.push(Vec::new());
    }

    /// Build intra-region edges: for each pair of transitions in the same (region, zone),
    /// compute the local A* cost and add bidirectional edges.
    /// Also add cross-border edges (cost = cross_cost).
    fn build_intra_region_edges(&mut self, map: &TileMap) {
        // First, cross-border edges: each transition connects from_tile <-> to_tile
        // We need pairs: the same border creates two transition entries (one per direction)
        // Actually, we store one Transition per border crossing. The graph edges connect
        // the from-side and to-side implicitly. But we need node pairs.
        //
        // Our model: each transition ID is a single node in the graph, representing the
        // crossing point. We add edges between nodes that share a region-zone.

        // Cross-border edges: find pairs of transitions at the same border but different regions.
        // Actually, each transition represents a border crossing. Two transitions that form
        // a "reverse" pair (A->B and B->A at the same point) don't exist in our model --
        // we have one transition per border run. The transition is registered in both regions'
        // lookup. So a transition `t` is a node reachable from both from_region and to_region.
        // Cross-border cost is just the cross_cost (already implicit -- no separate edge needed
        // because the node itself represents crossing). But we need edges *between* transitions.

        // Intra-region edges: for each (region, zone), get all transition IDs touching it.
        // For each pair, compute local A* cost between their tile positions within the region.
        let keys: Vec<(usize, usize, u8)> = self.region_transitions.keys().cloned().collect();
        for key in &keys {
            let ids = match self.region_transitions.get(key) {
                Some(ids) => ids.clone(),
                None => continue,
            };
            if ids.len() < 2 {
                continue;
            }
            let (rx, ry, _zone) = *key;
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let id_a = ids[i];
                    let id_b = ids[j];
                    let pos_a = self.transition_tile_in_region(id_a, rx, ry);
                    let pos_b = self.transition_tile_in_region(id_b, rx, ry);
                    if let (Some((ax, ay)), Some((bx, by))) = (pos_a, pos_b) {
                        if let Some(cost) = local_astar_cost(map, ax, ay, bx, by, rx, ry) {
                            self.edges[id_a].push((id_b, cost));
                            self.edges[id_b].push((id_a, cost));
                        }
                    }
                }
            }
        }
    }

    /// Get the tile position of a transition within a specific region.
    /// Returns from_tile if the transition's from_region matches, else to_tile.
    fn transition_tile_in_region(
        &self,
        tid: usize,
        rx: usize,
        ry: usize,
    ) -> Option<(usize, usize)> {
        let t = &self.transitions[tid];
        if t.from_region == (rx, ry) {
            Some(t.from_tile)
        } else if t.to_region == (rx, ry) {
            Some(t.to_tile)
        } else {
            None
        }
    }

    /// Get the tile position of a transition that is closest to a given point.
    fn transition_tile_nearest(&self, tid: usize, x: usize, y: usize) -> (usize, usize) {
        let t = &self.transitions[tid];
        let d_from =
            (t.from_tile.0 as i32 - x as i32).abs() + (t.from_tile.1 as i32 - y as i32).abs();
        let d_to = (t.to_tile.0 as i32 - x as i32).abs() + (t.to_tile.1 as i32 - y as i32).abs();
        if d_from <= d_to {
            t.from_tile
        } else {
            t.to_tile
        }
    }

    /// Mark a region as dirty (needs recomputation after terrain change).
    /// Also marks adjacent regions if the tile is on a border.
    pub fn mark_dirty(&mut self, tx: usize, ty: usize) {
        let rx = tx / REGION_SIZE;
        let ry = ty / REGION_SIZE;
        self.dirty_regions.insert((rx, ry));
        // If tile is on a region border, also dirty the neighbor
        let lx = tx % REGION_SIZE;
        let ly = ty % REGION_SIZE;
        if lx == 0 && rx > 0 {
            self.dirty_regions.insert((rx - 1, ry));
        }
        if lx == REGION_SIZE - 1 && rx + 1 < self.regions_w {
            self.dirty_regions.insert((rx + 1, ry));
        }
        if ly == 0 && ry > 0 {
            self.dirty_regions.insert((rx, ry - 1));
        }
        if ly == REGION_SIZE - 1 && ry + 1 < self.regions_h {
            self.dirty_regions.insert((rx, ry + 1));
        }
    }

    /// Process dirty regions, recomputing at most `MAX_REGION_UPDATES_PER_TICK`.
    /// Returns the set of region coordinates that were recomputed.
    pub fn process_dirty(&mut self, map: &TileMap) -> Vec<(usize, usize)> {
        if self.dirty_regions.is_empty() {
            return Vec::new();
        }

        let to_update: Vec<(usize, usize)> = self
            .dirty_regions
            .iter()
            .take(MAX_REGION_UPDATES_PER_TICK)
            .cloned()
            .collect();

        for &(rx, ry) in &to_update {
            self.dirty_regions.remove(&(rx, ry));
            self.recompute_region(rx, ry, map);
        }

        to_update
    }

    /// Recompute a single region: re-flood-fill, rebuild transitions for its edges,
    /// and update the graph.
    fn recompute_region(&mut self, rx: usize, ry: usize, map: &TileMap) {
        if rx >= self.regions_w || ry >= self.regions_h {
            return;
        }

        // Re-flood-fill the region
        let idx = ry * self.regions_w + rx;
        self.regions[idx] = NavRegion::compute(rx, ry, map);

        // Remove old transitions that touch this region
        let _old_tids: Vec<usize> = (0..self.transitions.len())
            .filter(|&tid| {
                let t = &self.transitions[tid];
                t.from_region == (rx, ry) || t.to_region == (rx, ry)
            })
            .collect();

        // Rebuild from scratch (simple and correct; region recomputes are rare)
        // For simplicity, do a full rebuild of transitions.
        // This is O(regions) but only happens on terrain changes.
        self.rebuild_transitions(map);
    }

    /// High-level A* on the NavGraph. Returns a list of tile-level waypoints
    /// (transition midpoints) from `start` to `goal`, or None if unreachable.
    pub fn find_path(
        &self,
        sx: f64,
        sy: f64,
        gx: f64,
        gy: f64,
        map: &TileMap,
    ) -> Option<Vec<(f64, f64)>> {
        let six = sx.round() as usize;
        let siy = sy.round() as usize;
        let gix = gx.round() as usize;
        let giy = gy.round() as usize;

        let srx = six / REGION_SIZE;
        let sry = siy / REGION_SIZE;
        let grx = gix / REGION_SIZE;
        let gry = giy / REGION_SIZE;

        // Same region: no hierarchy needed
        if srx == grx && sry == gry {
            return Some(Vec::new());
        }

        // Find which zone the start/goal are in
        if srx >= self.regions_w
            || sry >= self.regions_h
            || grx >= self.regions_w
            || gry >= self.regions_h
        {
            return None;
        }
        let start_region = &self.regions[sry * self.regions_w + srx];
        let goal_region = &self.regions[gry * self.regions_w + grx];
        let start_zone = start_region.zone_at(six, siy);
        let goal_zone = goal_region.zone_at(gix, giy);

        if start_zone == 0 || goal_zone == 0 {
            return None; // start or goal is unwalkable
        }

        // Get transition IDs for start/goal region-zones
        let start_tids = self
            .region_transitions
            .get(&(srx, sry, start_zone))
            .cloned()
            .unwrap_or_default();
        let goal_tids = self
            .region_transitions
            .get(&(grx, gry, goal_zone))
            .cloned()
            .unwrap_or_default();

        if start_tids.is_empty() || goal_tids.is_empty() {
            return None; // isolated region
        }

        // A* on the transition graph.
        // Virtual nodes: start (node_count) and goal (node_count+1)
        let n = self.transitions.len();
        let start_node = n;
        let goal_node = n + 1;

        // dist[i] = best cost to reach node i
        let mut dist: Vec<f64> = vec![f64::INFINITY; n + 2];
        let mut parent: Vec<usize> = vec![usize::MAX; n + 2];
        dist[start_node] = 0.0;

        #[derive(PartialEq)]
        struct Entry {
            f: f64,
            node: usize,
        }
        impl Eq for Entry {}
        impl PartialOrd for Entry {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for Entry {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                other
                    .f
                    .partial_cmp(&self.f)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        }

        let heuristic = |node: usize| -> f64 {
            if node == goal_node {
                return 0.0;
            }
            let (tx, ty) = if node == start_node {
                (six, siy)
            } else {
                self.transition_tile_nearest(node, gix, giy)
            };
            ((tx as f64 - gix as f64).abs() + (ty as f64 - giy as f64).abs()) * 0.7
        };

        let mut heap = BinaryHeap::new();
        heap.push(Entry {
            f: heuristic(start_node),
            node: start_node,
        });

        // Edges from virtual start to start-region transitions
        let mut virtual_start_edges: Vec<(usize, f64)> = Vec::new();
        for &tid in &start_tids {
            let tpos = self.transition_tile_in_region(tid, srx, sry);
            if let Some((tx, ty)) = tpos {
                if let Some(cost) = local_astar_cost(map, six, siy, tx, ty, srx, sry) {
                    virtual_start_edges.push((tid, cost));
                }
            }
        }

        // Edges from goal-region transitions to virtual goal
        let mut virtual_goal_edges: Vec<(usize, f64)> = Vec::new();
        for &tid in &goal_tids {
            let tpos = self.transition_tile_in_region(tid, grx, gry);
            if let Some((tx, ty)) = tpos {
                if let Some(cost) = local_astar_cost(map, tx, ty, gix, giy, grx, gry) {
                    virtual_goal_edges.push((tid, cost));
                }
            }
        }

        let max_iters = (n + 2) * 4; // safety cap
        let mut iters = 0;

        while let Some(Entry { node, .. }) = heap.pop() {
            iters += 1;
            if iters > max_iters {
                return None;
            }

            if node == goal_node {
                // Reconstruct path
                let mut path_nodes = Vec::new();
                let mut cur = goal_node;
                while cur != start_node && cur != usize::MAX {
                    path_nodes.push(cur);
                    cur = parent[cur];
                }
                if cur == usize::MAX {
                    return None;
                }
                path_nodes.reverse();

                // Convert transition nodes to tile waypoints (skip virtual goal)
                let mut waypoints = Vec::new();
                for &nd in &path_nodes {
                    if nd < n {
                        // Use the tile that's on the path direction
                        let t = &self.transitions[nd];
                        // Add both from and to tiles as waypoints for smooth crossing
                        waypoints.push((t.from_tile.0 as f64, t.from_tile.1 as f64));
                        waypoints.push((t.to_tile.0 as f64, t.to_tile.1 as f64));
                    }
                }
                return Some(waypoints);
            }

            let cur_cost = dist[node];

            // Get neighbors
            let neighbors: Vec<(usize, f64)> = if node == start_node {
                virtual_start_edges.clone()
            } else if node < n {
                let mut nbrs = self.edges[node].clone();
                // Check if this node connects to goal
                for &(tid, cost) in &virtual_goal_edges {
                    if tid == node {
                        nbrs.push((goal_node, cost));
                    }
                }
                nbrs
            } else {
                Vec::new()
            };

            for (next, edge_cost) in neighbors {
                let new_cost = cur_cost + edge_cost;
                if new_cost < dist[next] {
                    dist[next] = new_cost;
                    parent[next] = node;
                    heap.push(Entry {
                        f: new_cost + heuristic(next),
                        node: next,
                    });
                }
            }
        }

        None // no path found
    }
}

/// Run local A* within a single region to compute the cost between two tiles.
/// Constrains the search to tiles within the region bounds (allows 1-tile margin for border tiles).
/// Returns None if no path exists.
fn local_astar_cost(
    map: &TileMap,
    sx: usize,
    sy: usize,
    gx: usize,
    gy: usize,
    rx: usize,
    ry: usize,
) -> Option<f64> {
    if sx == gx && sy == gy {
        return Some(0.0);
    }

    // Region bounds (allow tiles on borders of adjacent regions for transition tiles)
    let min_x = (rx * REGION_SIZE).saturating_sub(1);
    let min_y = (ry * REGION_SIZE).saturating_sub(1);
    let max_x = ((rx + 1) * REGION_SIZE).min(map.width);
    let max_y = ((ry + 1) * REGION_SIZE).min(map.height);

    #[derive(PartialEq)]
    struct Node {
        f: f64,
        cost: f64,
        x: usize,
        y: usize,
    }
    impl Eq for Node {}
    impl PartialOrd for Node {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Node {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other
                .f
                .partial_cmp(&self.f)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    }

    let w = max_x - min_x;
    let h = max_y - min_y;
    let local_idx = |x: usize, y: usize| -> usize { (y - min_y) * w + (x - min_x) };

    let mut visited = vec![false; w * h];
    let mut costs = vec![f64::INFINITY; w * h];
    let mut heap = BinaryHeap::new();

    let heuristic =
        |x: usize, y: usize| -> f64 { (x as f64 - gx as f64).abs() + (y as f64 - gy as f64).abs() };

    costs[local_idx(sx, sy)] = 0.0;
    heap.push(Node {
        f: heuristic(sx, sy),
        cost: 0.0,
        x: sx,
        y: sy,
    });

    let max_steps = w * h * 2; // generous budget for local search
    let mut steps = 0;

    while let Some(Node { cost, x, y, .. }) = heap.pop() {
        steps += 1;
        if steps > max_steps {
            return None;
        }

        if x == gx && y == gy {
            return Some(cost);
        }

        let li = local_idx(x, y);
        if visited[li] {
            continue;
        }
        visited[li] = true;

        for &(dx, dy) in &[(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < min_x as i32 || ny < min_y as i32 || nx >= max_x as i32 || ny >= max_y as i32 {
                continue;
            }
            let nxu = nx as usize;
            let nyu = ny as usize;
            let nli = local_idx(nxu, nyu);
            if visited[nli] {
                continue;
            }
            if let Some(t) = map.get(nxu, nyu) {
                if !t.is_walkable() {
                    continue;
                }
                let new_cost = cost + t.move_cost();
                if new_cost < costs[nli] {
                    costs[nli] = new_cost;
                    heap.push(Node {
                        f: new_cost + heuristic(nxu, nyu),
                        cost: new_cost,
                        x: nxu,
                        y: nyu,
                    });
                }
            }
        }
    }

    None
}

/// A hierarchical path: sequence of region-level waypoints for an entity to follow.
#[derive(Debug, Clone, Default)]
pub struct HierarchicalPath {
    /// High-level path: transition tile positions to pass through.
    pub region_waypoints: Vec<(f64, f64)>,
    /// Index of the next region waypoint to reach.
    pub region_cursor: usize,
    /// Tick when the high-level path was computed.
    pub computed_tick: u64,
    /// Destination this path leads to.
    pub dest_x: f64,
    pub dest_y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tilemap::Terrain;

    #[test]
    fn build_simple_graph() {
        // 2x2 regions (32x32 map), all grass
        let map = TileMap::new(32, 32, Terrain::Grass);
        let graph = NavGraph::build(&map);
        assert_eq!(graph.regions_w, 2);
        assert_eq!(graph.regions_h, 2);
        assert_eq!(graph.regions.len(), 4);
        // Should have transitions between all adjacent region pairs (4 pairs)
        // Each pair has 1 transition (full walkable edge)
        assert_eq!(graph.transitions.len(), 4);
    }

    #[test]
    fn find_path_same_region() {
        let map = TileMap::new(16, 16, Terrain::Grass);
        let graph = NavGraph::build(&map);
        // Same region: returns empty waypoints
        let result = graph.find_path(1.0, 1.0, 10.0, 10.0, &map);
        assert!(result.is_some());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn find_path_across_regions() {
        let map = TileMap::new(64, 64, Terrain::Grass);
        let graph = NavGraph::build(&map);
        // Path from (2,2) to (50,50) crosses multiple regions
        let result = graph.find_path(2.0, 2.0, 50.0, 50.0, &map);
        assert!(result.is_some());
        let waypoints = result.unwrap();
        assert!(!waypoints.is_empty());
    }

    #[test]
    fn find_path_unreachable() {
        let mut map = TileMap::new(32, 32, Terrain::Grass);
        // Block the entire border between regions (0,0) and (1,0)
        for y in 0..32 {
            map.set(15, y, Terrain::Water);
            map.set(16, y, Terrain::Water);
        }
        // Also block vertical border
        for x in 0..32 {
            map.set(x, 15, Terrain::Water);
            map.set(x, 16, Terrain::Water);
        }
        let graph = NavGraph::build(&map);
        // Try to path from region (0,0) to region (1,1)
        let result = graph.find_path(2.0, 2.0, 25.0, 25.0, &map);
        assert!(result.is_none());
    }

    #[test]
    fn find_path_around_obstacle() {
        let mut map = TileMap::new(48, 32, Terrain::Grass);
        // Block the direct horizontal border between region (0,0) and (1,0)
        // by placing water across the bottom half of the border
        for y in 8..16 {
            map.set(15, y, Terrain::Water);
            map.set(16, y, Terrain::Water);
        }
        let graph = NavGraph::build(&map);
        // Should still find a path (through the top part of the border)
        let result = graph.find_path(2.0, 2.0, 30.0, 2.0, &map);
        assert!(result.is_some());
    }

    #[test]
    fn incremental_update() {
        let mut map = TileMap::new(32, 32, Terrain::Grass);
        let mut graph = NavGraph::build(&map);

        // Place a building wall at (15, 8) -- on the border between regions (0,0) and (1,0)
        map.set(15, 8, Terrain::BuildingWall);
        graph.mark_dirty(15, 8);

        // Process dirty regions
        let updated = graph.process_dirty(&map);
        assert!(!updated.is_empty());
    }

    #[test]
    fn dirty_region_cap() {
        let map = TileMap::new(256, 256, Terrain::Grass);
        let mut graph = NavGraph::build(&map);

        // Dirty many regions
        for i in 0..20 {
            graph.dirty_regions.insert((i, 0));
        }

        let updated = graph.process_dirty(&map);
        assert_eq!(updated.len(), MAX_REGION_UPDATES_PER_TICK);
        // Remaining should still be dirty
        assert_eq!(graph.dirty_regions.len(), 20 - MAX_REGION_UPDATES_PER_TICK);
    }

    #[test]
    fn local_astar_cost_simple() {
        let map = TileMap::new(16, 16, Terrain::Grass);
        // Cost from (0,0) to (5,0) = 5 tiles * 1.0 cost = 5.0
        let cost = local_astar_cost(&map, 0, 0, 5, 0, 0, 0);
        assert!(cost.is_some());
        assert!((cost.unwrap() - 5.0).abs() < 0.01);
    }

    #[test]
    fn local_astar_cost_blocked() {
        let mut map = TileMap::new(16, 16, Terrain::Grass);
        // Wall across the region
        for y in 0..16 {
            map.set(8, y, Terrain::Water);
        }
        // Cannot cross from (0,0) to (12,0) within this region
        let cost = local_astar_cost(&map, 0, 0, 12, 0, 0, 0);
        assert!(cost.is_none());
    }

    #[test]
    fn local_astar_cost_terrain_weights() {
        let mut map = TileMap::new(16, 16, Terrain::Grass);
        // Make a road path at y=0
        for x in 0..10 {
            map.set(x, 0, Terrain::Road);
        }
        let road_cost = local_astar_cost(&map, 0, 0, 9, 0, 0, 0);
        // Grass path at y=2
        let grass_cost = local_astar_cost(&map, 0, 2, 9, 2, 0, 0);
        assert!(road_cost.is_some());
        assert!(grass_cost.is_some());
        // Road should be cheaper
        assert!(road_cost.unwrap() < grass_cost.unwrap());
    }

    #[test]
    fn region_transitions_lookup() {
        let map = TileMap::new(32, 32, Terrain::Grass);
        let graph = NavGraph::build(&map);
        // Region (0,0) zone 1 should have transitions to adjacent regions
        let tids = graph.region_transitions.get(&(0, 0, 1));
        assert!(tids.is_some());
        assert!(!tids.unwrap().is_empty());
    }

    #[test]
    fn three_region_straight_path() {
        // 3 regions wide, 1 tall (48x16 map)
        let map = TileMap::new(48, 16, Terrain::Grass);
        let graph = NavGraph::build(&map);
        // Path from left to right
        let result = graph.find_path(2.0, 8.0, 45.0, 8.0, &map);
        assert!(result.is_some());
        let waypoints = result.unwrap();
        // Should have transition waypoints (each transition generates 2 waypoints)
        assert!(waypoints.len() >= 2);
    }
}
