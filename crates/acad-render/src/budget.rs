//! Whole-frame aggregate render budget (RB2, `docs/native-render-budget.md`).
//!
//! Per-owner preflight (`selection_policy`) bounds a single owner; this budget
//! bounds the sum over every owner drawn in one frame, including the
//! selection-highlight pass when a caller shares one budget between passes.
use crate::flatten::Prim;

/// Work units one frame may spend before rendering stops at an owner boundary.
///
/// One unit is one stored-record visit (render walk, hidden LOAD replay or
/// owner preflight), one executed SHP instruction, or one emitted/copied/
/// transformed vertex; every emitted primitive also costs
/// [`PRIMITIVE_SETUP_UNITS`]. Measured debug rasterization costs about 3 µs
/// per unit (release about 0.1 µs); the largest retained corpus drawing
/// (DISC.BAK) spends about 88,000 units.
pub const FRAME_WORK_LIMIT: usize = 1_000_000;

/// Fixed per-primitive cost (path setup in the rasterizer) in vertex units.
pub const PRIMITIVE_SETUP_UNITS: usize = 4;

/// Monotonic work counter for one frame. Charging never succeeds past the
/// limit; the first refused charge latches `exhausted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameBudget {
    limit: usize,
    used: usize,
    exhausted: bool,
}

impl Default for FrameBudget {
    fn default() -> Self {
        Self::new(FRAME_WORK_LIMIT)
    }
}

impl FrameBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            exhausted: false,
        }
    }
    pub fn limit(&self) -> usize {
        self.limit
    }
    /// Units actually spent (never above the limit).
    pub fn used(&self) -> usize {
        self.used
    }
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }
    /// Spend `units`, or latch exhaustion and spend nothing.
    pub(crate) fn charge(&mut self, units: usize) -> bool {
        if self.exhausted {
            return false;
        }
        match self.used.checked_add(units) {
            Some(used) if used <= self.limit => {
                self.used = used;
                true
            }
            _ => {
                self.exhausted = true;
                false
            }
        }
    }
}

/// Bytes of a drawing-supplied name hashed/compared per work unit.
pub(crate) const NAME_BYTES_PER_UNIT: usize = 16;

/// Extra cost of hashing or comparing a drawing-supplied name (block, font or
/// shape library); names up to 15 bytes are covered by the visit itself.
pub(crate) fn name_units(name: &str) -> usize {
    name.len() / NAME_BYTES_PER_UNIT
}

/// Cost of materializing `prims` once.
pub(crate) fn prim_units(prims: &[Prim]) -> usize {
    prims.iter().fold(0usize, |total, prim| {
        let points = match prim {
            Prim::Polyline(points) | Prim::FilledPolygon(points) => points.len(),
            Prim::ColoredPolyline { points, .. } | Prim::ColoredFilledPolygon { points, .. } => {
                points.len()
            }
        };
        total.saturating_add(points.saturating_add(PRIMITIVE_SETUP_UNITS))
    })
}

/// Where and why a frame pass stopped early. Owners before
/// `first_skipped_item` are complete; it and every later owner are not drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetStop {
    pub limit: usize,
    /// One-based drawing item number of the first owner not drawn.
    pub first_skipped_item: usize,
    /// Entity/REPEAT owners from `first_skipped_item` to the end.
    pub skipped_owners: usize,
}
