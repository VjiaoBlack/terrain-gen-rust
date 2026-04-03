/// World-space dirty tileset for skipping unchanged tiles during rendering.
///
/// One bit per world tile. When a tile is marked dirty, the render pass will
/// recompute its appearance. Clean tiles are skipped — the terminal double-buffer
/// already retains the previous content.
pub struct DirtyMap {
    /// Bitset: one u64 holds 64 tile flags.
    bits: Vec<u64>,
    width: usize,
    height: usize,
    /// When true, skip per-tile checks — everything is dirty.
    all_dirty: bool,
}

impl DirtyMap {
    pub fn new(width: usize, height: usize) -> Self {
        let num_bits = width * height;
        let num_words = (num_bits + 63) / 64;
        Self {
            bits: vec![0u64; num_words],
            width,
            height,
            // First frame must draw everything.
            all_dirty: true,
        }
    }

    /// Mark a single world tile as needing redraw.
    #[inline]
    pub fn mark(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            self.bits[idx / 64] |= 1u64 << (idx % 64);
        }
    }

    /// Mark a rectangular region of world tiles as dirty.
    pub fn mark_rect(&mut self, x: usize, y: usize, w: usize, h: usize) {
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);
        for ty in y..y_end {
            for tx in x..x_end {
                let idx = ty * self.width + tx;
                self.bits[idx / 64] |= 1u64 << (idx % 64);
            }
        }
    }

    /// Mark everything dirty (e.g. camera scroll, mode toggle).
    #[inline]
    pub fn mark_all(&mut self) {
        self.all_dirty = true;
    }

    /// Check whether a world tile needs redraw.
    #[inline]
    pub fn is_dirty(&self, x: usize, y: usize) -> bool {
        if self.all_dirty {
            return true;
        }
        if x >= self.width || y >= self.height {
            return false;
        }
        let idx = y * self.width + x;
        (self.bits[idx / 64] >> (idx % 64)) & 1 != 0
    }

    /// Reset all dirty bits. Called at the end of each render pass.
    pub fn clear(&mut self) {
        self.all_dirty = false;
        self.bits.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_all_dirty() {
        let dm = DirtyMap::new(16, 16);
        assert!(dm.is_dirty(0, 0));
        assert!(dm.is_dirty(15, 15));
    }

    #[test]
    fn clear_makes_clean() {
        let mut dm = DirtyMap::new(16, 16);
        dm.clear();
        assert!(!dm.is_dirty(0, 0));
        assert!(!dm.is_dirty(8, 8));
    }

    #[test]
    fn mark_single_tile() {
        let mut dm = DirtyMap::new(32, 32);
        dm.clear();
        dm.mark(5, 7);
        assert!(dm.is_dirty(5, 7));
        assert!(!dm.is_dirty(5, 6));
        assert!(!dm.is_dirty(4, 7));
    }

    #[test]
    fn mark_rect() {
        let mut dm = DirtyMap::new(32, 32);
        dm.clear();
        dm.mark_rect(2, 3, 4, 5);
        // Inside rect
        assert!(dm.is_dirty(2, 3));
        assert!(dm.is_dirty(5, 7));
        // Outside rect
        assert!(!dm.is_dirty(1, 3));
        assert!(!dm.is_dirty(6, 3));
        assert!(!dm.is_dirty(2, 8));
    }

    #[test]
    fn mark_all_overrides() {
        let mut dm = DirtyMap::new(10, 10);
        dm.clear();
        assert!(!dm.is_dirty(0, 0));
        dm.mark_all();
        assert!(dm.is_dirty(0, 0));
        assert!(dm.is_dirty(9, 9));
    }

    #[test]
    fn out_of_bounds_safe() {
        let mut dm = DirtyMap::new(8, 8);
        dm.clear();
        // Should not panic
        dm.mark(100, 100);
        assert!(!dm.is_dirty(100, 100));
    }

    #[test]
    fn mark_rect_clamped() {
        let mut dm = DirtyMap::new(8, 8);
        dm.clear();
        // Rect extends beyond map — should not panic and should mark in-bounds tiles
        dm.mark_rect(6, 6, 10, 10);
        assert!(dm.is_dirty(7, 7));
        assert!(!dm.is_dirty(5, 5));
    }

    #[test]
    fn clear_after_mark_all() {
        let mut dm = DirtyMap::new(16, 16);
        dm.mark_all();
        dm.clear();
        assert!(!dm.is_dirty(0, 0));
        assert!(!dm.is_dirty(15, 15));
    }

    #[test]
    fn large_map() {
        let mut dm = DirtyMap::new(256, 256);
        dm.clear();
        dm.mark(255, 255);
        assert!(dm.is_dirty(255, 255));
        assert!(!dm.is_dirty(0, 0));
        assert!(!dm.is_dirty(254, 255));
    }
}
