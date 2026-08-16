//! Neighbourhood queries: which agents are close enough to steer by.
//!
//! Two backends live here and they must be **observationally identical**: the
//! spatial hash is an optimisation, not a different model. Every rule below
//! therefore applies to both, and the equivalence is asserted by a property
//! test rather than assumed.
//!
//! # The rules
//!
//! * **Toroidal distance.** Closeness is measured with [`World::distance_squared`],
//!   the minimum image, so agents at `x=1` and `x=99` in a width-100 world are
//!   2 apart, not 98.
//! * **Inclusive boundary.** An agent at exactly `radius` **is** a neighbour;
//!   the test is `d <= r`, expressed as `d² <= r²`.
//! * **Never yourself.** The query agent is excluded by index, not by
//!   distance, so a coincident twin is still found while the agent itself
//!   never is.
//! * **Ascending index order.** See [`neighbors_naive`] — this is a
//!   correctness constraint, not a style preference.

use crate::config::NeighborBackend;
use crate::vec2::Vec2;
use crate::world::{Agent, World};

/// Squared radius for the inclusive `d² <= r²` test, or `None` if the radius
/// is not a usable distance.
///
/// A negative radius must not be laundered into a positive one by squaring,
/// and `NaN` is not a distance; both mean "no agent qualifies". Both backends
/// funnel through this one function so their comparison semantics cannot
/// drift apart — a subtle disagreement here is exactly what the equivalence
/// property test would report as a set mismatch.
fn radius_squared(radius: f64) -> Option<f64> {
    if radius.is_nan() || radius < 0.0 {
        return None;
    }
    Some(radius * radius)
}

/// The neighbour set of one agent: indices into `agents`.
///
/// Brute force over every agent — O(N²) and the reference implementation that
/// [`SpatialHash`] is checked against.
///
/// Distances are toroidal, the boundary is inclusive (`d <= radius`), and the
/// agent itself is **never** included. An out-of-range `index` yields an empty
/// set rather than a panic, matching the kernel's totality convention.
///
/// # Ordering
///
/// The result is in **ascending index order**, and that is load-bearing:
/// `f64` addition is not associative, so summing the same neighbours in a
/// different order yields a different steering force in the last bits. Since
/// a run's reproducibility contract compares state hashes bit-for-bit, a
/// non-deterministic neighbour order would make identical inputs diverge.
/// Both backends therefore return the same order, not merely the same set.
#[must_use]
pub fn neighbors_naive(agents: &[Agent], world: &World, index: usize, radius: f64) -> Vec<usize> {
    let Some(r2) = radius_squared(radius) else {
        return Vec::new();
    };
    let Some(me) = agents.get(index) else {
        return Vec::new();
    };
    // Ascending by construction: the scan visits indices in order.
    (0..agents.len())
        .filter(|&j| j != index && world.distance_squared(me.pos, agents[j].pos) <= r2)
        .collect()
}

/// Hard ceiling on cells per axis, applied before the `f64 -> usize` cast so
/// a tiny radius in a large world cannot ask for an absurd allocation.
const MAX_AXIS_CELLS: f64 = 4096.0;

/// How much larger than the radius a cell must be.
///
/// `cell >= radius` is the mathematical requirement; the extra part in
/// 10^12 absorbs the rounding of `x / cell_size` when a coordinate sits
/// exactly on a cell boundary. Without it, a point could be filed one cell
/// away from where the covering argument assumes it is, and a neighbour two
/// columns out would be pruned — a defect that appears in perhaps one
/// randomised case in a billion, which is the worst kind.
const CELL_MARGIN: f64 = 1.0 + 1e-12;

/// Number of cells along one axis: as many as fit while keeping each cell at
/// least `radius` (with margin) wide, and never fewer than one.
///
/// A degenerate axis or radius collapses to a single cell, which is always
/// *correct* — one cell means the query degenerates to a full scan — just not
/// fast. That is the right trade for inputs that should not occur.
fn axis_cells(size: f64, radius: f64) -> usize {
    if !size.is_finite() || size <= 0.0 || !radius.is_finite() || radius <= 0.0 {
        return 1;
    }
    // Both operands are finite and positive here, so the quotient is either a
    // positive number or `+inf` (a subnormal radius); never `NaN`.
    let ideal = (size / radius).floor();
    if ideal < 1.0 {
        return 1;
    }
    let mut n = ideal.min(MAX_AXIS_CELLS) as usize;
    // `floor` of a *rounded* quotient can overshoot by one, which would leave
    // the cell a hair narrower than the radius and break the 3x3 covering.
    // Shrink until the invariant provably holds rather than trusting it.
    while n > 1 && size / (n as f64) < radius * CELL_MARGIN {
        n -= 1;
    }
    n
}

/// Shrink a grid until it has no more cells than agents.
///
/// More cells than agents costs memory and cache misses without reducing the
/// candidate count. Halving an axis only ever makes cells *larger*, so the
/// `cell >= radius` invariant survives untouched.
fn trim_to_budget(mut cols: usize, mut rows: usize, budget: usize) -> (usize, usize) {
    while cols.saturating_mul(rows) > budget && (cols > 1 || rows > 1) {
        if cols >= rows {
            cols = (cols / 2).max(1);
        } else {
            rows = (rows / 2).max(1);
        }
    }
    (cols, rows)
}

/// The **distinct** cell indices adjacent to `c` on an axis of `n` cells,
/// returned as a fixed array plus how many of its entries are live.
///
/// The world is a torus, so the neighbourhood wraps: cell `0`'s left
/// neighbour is cell `n-1`. Clamping at the edge instead — the obvious
/// non-toroidal spelling — silently drops every neighbour across the seam.
///
/// Distinctness matters as much as wrapping. With one or two cells on an
/// axis, the offsets `-1, 0, +1` land on the same cell more than once, and a
/// cell visited twice would report every agent in it twice. Small grids are
/// not a corner case here: a world barely larger than the radius, which the
/// property test generates deliberately, produces exactly that shape.
fn axis_span(c: usize, n: usize) -> ([usize; 3], usize) {
    match n {
        0 | 1 => ([0, 0, 0], 1),
        2 => ([0, 1, 0], 2),
        _ => {
            let lo = if c == 0 { n - 1 } else { c - 1 };
            let hi = if c + 1 == n { 0 } else { c + 1 };
            ([lo, c, hi], 3)
        }
    }
}

/// A uniform grid over the toroidal world, bucketing agents by cell.
///
/// Build once per tick, query many times: construction is O(N) and each query
/// inspects only the 3x3 block of cells around the query point.
///
/// # The cell-size invariant
///
/// Every cell is at least `radius` wide and tall. That is what makes 3x3
/// sufficient: a point inside a cell can reach at most `radius <= cell_size`
/// in any direction, so it cannot reach past the immediately adjacent cell.
/// The grid divides the world into a whole number of equal cells rather than
/// laying out fixed-size cells and leaving a remainder strip, so a world that
/// is not a multiple of the radius is tiled exactly — `cols * cell_w == width`
/// — with cells slightly *larger* than the radius rather than a ragged edge.
///
/// # Contract
///
/// A grid describes the `agents` slice it was built from. Rebuild it every
/// tick, after positions change. If it is handed a slice of a different
/// length it degrades to an exhaustive scan rather than returning a wrong
/// answer, and the same happens when queried with a radius larger than the
/// one it was built for, which 3x3 cannot cover.
#[derive(Debug, Clone)]
pub struct SpatialHash {
    /// Cells along x; at least 1.
    cols: usize,
    /// Cells along y; at least 1.
    rows: usize,
    /// Width of one cell: `world.width / cols`.
    cell_w: f64,
    /// Height of one cell: `world.height / rows`.
    cell_h: f64,
    /// The radius this grid was built for; queries may not exceed it.
    radius: f64,
    /// Number of agents seen at build time, used to detect a stale grid.
    agent_count: usize,
    /// CSR row offsets: cell `c` owns `items[cell_start[c]..cell_start[c+1]]`.
    cell_start: Vec<usize>,
    /// Agent indices grouped by cell, ascending within each cell.
    items: Vec<usize>,
}

impl SpatialHash {
    /// Build a grid over `agents` sized for queries of `radius`.
    ///
    /// O(N) time and memory: a counting sort into a CSR layout, no hashing of
    /// floats and no per-cell `Vec`s. Agent positions are wrapped into the
    /// world before bucketing, so an agent that has drifted out of bounds is
    /// filed where it actually is on the torus.
    #[must_use]
    pub fn build(agents: &[Agent], world: &World, radius: f64) -> SpatialHash {
        let (cols, rows) = trim_to_budget(
            axis_cells(world.width, radius),
            axis_cells(world.height, radius),
            agents.len().max(1),
        );
        let mut grid = SpatialHash {
            cols,
            rows,
            cell_w: world.width / cols as f64,
            cell_h: world.height / rows as f64,
            radius,
            agent_count: agents.len(),
            cell_start: vec![0; cols * rows + 1],
            items: vec![0; agents.len()],
        };
        // Counting sort: tally per cell, prefix-sum into offsets, then place.
        let cells: Vec<usize> = agents
            .iter()
            .map(|a| grid.cell_of(world.wrap(a.pos)))
            .collect();
        for &c in &cells {
            grid.cell_start[c + 1] += 1;
        }
        for c in 0..grid.cell_start.len() - 1 {
            grid.cell_start[c + 1] += grid.cell_start[c];
        }
        let mut cursor = grid.cell_start.clone();
        // Ascending agent order in, ascending order within each bucket out.
        for (i, &c) in cells.iter().enumerate() {
            grid.items[cursor[c]] = i;
            cursor[c] += 1;
        }
        grid
    }

    /// The grid dimensions as `(cols, rows)`; both at least 1.
    #[must_use]
    pub fn dims(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    /// The size of one cell. Never smaller than the build radius unless the
    /// axis holds a single cell (where the whole world is the cell).
    #[must_use]
    pub fn cell_size(&self) -> Vec2 {
        Vec2::new(self.cell_w, self.cell_h)
    }

    /// Neighbours of `agents[index]` within `radius`.
    ///
    /// Returns exactly the same set, in the same ascending-index order, as
    /// [`neighbors_naive`] — that equivalence is the whole contract of this
    /// type and is asserted by property test, not assumed.
    #[must_use]
    pub fn neighbors(
        &self,
        agents: &[Agent],
        world: &World,
        index: usize,
        radius: f64,
    ) -> Vec<usize> {
        let Some(r2) = radius_squared(radius) else {
            return Vec::new();
        };
        let Some(me) = agents.get(index) else {
            return Vec::new();
        };
        // A stale grid, or a query wider than the 3x3 block can cover: an
        // exhaustive scan is slower but never wrong, and wrong is not on the
        // menu. A `NaN` build radius means the grid's covering guarantee is
        // meaningless, so it too falls back rather than trusting the cells.
        if agents.len() != self.agent_count || self.radius.is_nan() || radius > self.radius {
            return neighbors_naive(agents, world, index, radius);
        }

        let p = world.wrap(me.pos);
        let cx = self.col_of(p.x);
        let cy = self.row_of(p.y);
        let mut out = Vec::new();
        let (cols, n_cols) = axis_span(cx, self.cols);
        let (rows, n_rows) = axis_span(cy, self.rows);
        for &row in &rows[..n_rows] {
            for &col in &cols[..n_cols] {
                let cell = row * self.cols + col;
                for &j in &self.items[self.cell_start[cell]..self.cell_start[cell + 1]] {
                    if j != index && world.distance_squared(me.pos, agents[j].pos) <= r2 {
                        out.push(j);
                    }
                }
            }
        }
        // Cells are visited in grid order, not index order, so the ascending
        // guarantee has to be restored here. See [`neighbors_naive`] for why
        // it is a correctness requirement.
        out.sort_unstable();
        out
    }

    /// Cell index of an already-wrapped position.
    fn cell_of(&self, wrapped: Vec2) -> usize {
        self.row_of(wrapped.y) * self.cols + self.col_of(wrapped.x)
    }

    /// Column of an already-wrapped x coordinate, clamped into range.
    ///
    /// The `as usize` cast saturates (negatives and `NaN` to 0), and the
    /// `min` closes the top, so a degenerate world cannot produce an
    /// out-of-bounds cell.
    fn col_of(&self, x: f64) -> usize {
        ((x / self.cell_w) as usize).min(self.cols - 1)
    }

    /// Row of an already-wrapped y coordinate, clamped into range.
    fn row_of(&self, y: f64) -> usize {
        ((y / self.cell_h) as usize).min(self.rows - 1)
    }
}

/// Answer a neighbour query with whichever backend the run is configured for.
///
/// This is the seam that makes AC-49 true: a call site names the query it
/// wants, not the data structure that answers it, so switching a run from
/// [`NeighborBackend::Naive`] to [`NeighborBackend::SpatialHash`] is a config
/// change and touches no code and no test. The two backends return the same
/// indices in the same order, so the choice is a performance decision with no
/// observable behaviour attached.
///
/// `grid` is the tick's prebuilt [`SpatialHash`]. It is ignored by the naive
/// backend, and if the spatial-hash backend is selected without one, a grid is
/// built for this single query — correct, but O(N) per call, so callers in a
/// loop should build once per tick and pass it in.
#[must_use]
pub fn neighbors_with(
    backend: NeighborBackend,
    agents: &[Agent],
    world: &World,
    grid: Option<&SpatialHash>,
    index: usize,
    radius: f64,
) -> Vec<usize> {
    match backend {
        NeighborBackend::Naive => neighbors_naive(agents, world, index, radius),
        NeighborBackend::SpatialHash => match grid {
            Some(g) => g.neighbors(agents, world, index, radius),
            None => {
                SpatialHash::build(agents, world, radius).neighbors(agents, world, index, radius)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NeighborBackend;
    use crate::rng::Rng;
    use crate::vec2::Vec2;
    use crate::world::{Agent, World};

    /// A stationary agent at a position; velocity never affects a query.
    fn at(id: u32, x: f64, y: f64) -> Agent {
        Agent {
            id,
            pos: Vec2::new(x, y),
            vel: Vec2::ZERO,
        }
    }

    fn w100() -> World {
        World::new(100.0, 100.0)
    }

    #[test]
    fn naive_returns_only_agents_inside_the_radius() {
        // 0 at the origin; 1 and 2 are within 10; 3 is 40 away.
        let agents = [
            at(0, 50.0, 50.0),
            at(1, 53.0, 54.0), // distance 5
            at(2, 41.0, 50.0), // distance 9
            at(3, 50.0, 90.0), // distance 40
        ];
        assert_eq!(neighbors_naive(&agents, &w100(), 0, 10.0), vec![1, 2]);
    }

    #[test]
    fn naive_never_returns_the_query_agent_itself() {
        let agents = [at(0, 10.0, 10.0), at(1, 10.5, 10.0)];
        let n = neighbors_naive(&agents, &w100(), 0, 50.0);
        assert!(!n.contains(&0), "an agent must never be its own neighbour");
        assert_eq!(n, vec![1]);
    }

    #[test]
    fn naive_boundary_is_inclusive() {
        // Documented decision: the radius test is `d <= r`, so an agent
        // exactly `r` away IS a neighbour.
        let agents = [at(0, 50.0, 50.0), at(1, 60.0, 50.0), at(2, 50.0, 39.0)];
        assert_eq!(neighbors_naive(&agents, &w100(), 0, 10.0), vec![1]);
        // A 3-4-5 triangle puts the boundary on an inexact diagonal too.
        let agents = [at(0, 0.0, 0.0), at(1, 3.0, 4.0)];
        assert_eq!(neighbors_naive(&agents, &w100(), 0, 5.0), vec![1]);
    }

    #[test]
    fn naive_finds_a_neighbour_across_the_wrap_seam() {
        // Euclidean distance is 98; toroidal distance is 2.
        let agents = [at(0, 1.0, 50.0), at(1, 99.0, 50.0), at(2, 50.0, 50.0)];
        assert_eq!(neighbors_naive(&agents, &w100(), 0, 5.0), vec![1]);
        // ... and across the corner, where both axes wrap at once.
        let agents = [at(0, 1.0, 1.0), at(1, 99.0, 99.0)];
        assert_eq!(neighbors_naive(&agents, &w100(), 0, 3.0), vec![1]);
    }

    #[test]
    fn naive_returns_indices_in_ascending_order() {
        // Deterministic order is a correctness constraint, not cosmetics:
        // f64 addition is not associative, so a reordered neighbour list
        // changes the summed steering force.
        let agents = [
            at(0, 10.0, 10.0),
            at(1, 11.0, 10.0),
            at(2, 12.0, 10.0),
            at(3, 13.0, 10.0),
            at(4, 14.0, 10.0),
        ];
        assert_eq!(neighbors_naive(&agents, &w100(), 2, 50.0), vec![0, 1, 3, 4]);
    }

    #[test]
    fn naive_on_an_empty_world_returns_nothing() {
        let agents: [Agent; 0] = [];
        assert!(neighbors_naive(&agents, &w100(), 0, 10.0).is_empty());
    }

    #[test]
    fn naive_with_a_single_agent_returns_empty_not_itself() {
        let agents = [at(0, 10.0, 10.0)];
        assert!(neighbors_naive(&agents, &w100(), 0, 1e9).is_empty());
    }

    #[test]
    fn naive_with_all_agents_coincident_returns_everyone_else() {
        let agents = [
            at(0, 7.0, 7.0),
            at(1, 7.0, 7.0),
            at(2, 7.0, 7.0),
            at(3, 7.0, 7.0),
        ];
        assert_eq!(neighbors_naive(&agents, &w100(), 1, 1.0), vec![0, 2, 3]);
    }

    #[test]
    fn naive_with_zero_radius_returns_only_coincident_agents() {
        let agents = [at(0, 7.0, 7.0), at(1, 7.0, 7.0), at(2, 7.0, 7.000_001)];
        assert_eq!(neighbors_naive(&agents, &w100(), 0, 0.0), vec![1]);
    }

    #[test]
    fn naive_with_a_degenerate_radius_returns_nothing() {
        // A negative radius must not be laundered into a positive one by
        // squaring it, and NaN is not a distance either.
        let agents = [at(0, 7.0, 7.0), at(1, 7.5, 7.0)];
        assert!(neighbors_naive(&agents, &w100(), 0, -5.0).is_empty());
        assert!(neighbors_naive(&agents, &w100(), 0, f64::NAN).is_empty());
        // An infinite radius contains the whole world.
        assert_eq!(neighbors_naive(&agents, &w100(), 0, f64::INFINITY), vec![1]);
    }

    #[test]
    fn naive_with_an_out_of_range_index_returns_nothing() {
        let agents = [at(0, 7.0, 7.0), at(1, 7.5, 7.0)];
        assert!(neighbors_naive(&agents, &w100(), 9, 10.0).is_empty());
    }

    /// A deterministic spread of agents that does not touch any seam, so it
    /// exercises the grid without depending on wrap handling.
    fn interior_flock() -> Vec<Agent> {
        let mut out = Vec::new();
        for i in 0..64u32 {
            let x = 20.0 + f64::from(i % 8) * 7.5;
            let y = 20.0 + f64::from(i / 8) * 7.5;
            out.push(at(i, x, y));
        }
        out
    }

    #[test]
    fn spatial_hash_answers_the_same_query_shape_as_naive() {
        let agents = interior_flock();
        let world = w100();
        let radius = 12.0;
        let grid = SpatialHash::build(&agents, &world, radius);
        for i in 0..agents.len() {
            assert_eq!(
                grid.neighbors(&agents, &world, i, radius),
                neighbors_naive(&agents, &world, i, radius),
                "agent {i}"
            );
        }
    }

    #[test]
    fn spatial_hash_never_returns_the_query_agent_itself() {
        let agents = [at(0, 10.0, 10.0), at(1, 10.5, 10.0)];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 50.0);
        let n = grid.neighbors(&agents, &world, 0, 50.0);
        assert!(!n.contains(&0));
        assert_eq!(n, vec![1]);
    }

    #[test]
    fn spatial_hash_boundary_is_inclusive() {
        let agents = [at(0, 50.0, 50.0), at(1, 60.0, 50.0), at(2, 50.0, 39.0)];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 10.0);
        assert_eq!(grid.neighbors(&agents, &world, 0, 10.0), vec![1]);
    }

    #[test]
    fn spatial_hash_returns_indices_in_ascending_order() {
        // Agents are inserted into cells in an order the grid chooses, so
        // unlike the naive scan this ordering has to be established, not
        // inherited. It is load-bearing: see `neighbors_naive`.
        let agents = interior_flock();
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 30.0);
        let n = grid.neighbors(&agents, &world, 27, 30.0);
        assert!(n.len() > 8, "expected a crowded neighbourhood, got {n:?}");
        assert!(n.windows(2).all(|w| w[0] < w[1]), "not ascending: {n:?}");
    }

    #[test]
    fn spatial_hash_actually_prunes_a_realistically_shaped_run() {
        // A guard rather than a driver. Every correctness assertion in this
        // module would still pass if the grid collapsed to a single cell and
        // silently became the naive scan, so pin the fact that a
        // default-shaped run gets a grid worth having.
        let world = World::new(200.0, 200.0);
        let agents = populated_flock(0xF10C, 80, &world);
        let grid = SpatialHash::build(&agents, &world, 25.0);
        let (cols, rows) = grid.dims();
        assert!(cols >= 4 && rows >= 4, "grid too coarse: {cols}x{rows}");
        let inspected = 9.0 / (cols * rows) as f64;
        assert!(
            inspected <= 0.25,
            "a query inspects {:.0}% of a {cols}x{rows} grid",
            inspected * 100.0
        );
    }

    #[test]
    fn spatial_hash_cell_is_never_smaller_than_the_radius() {
        // The invariant that makes a 3x3 block sufficient: a point can only
        // reach into the immediately adjacent cell, never past it.
        let agents = interior_flock();
        for (w, h) in [(100.0, 100.0), (100.0, 37.0), (7.0, 512.0), (1.0, 1.0)] {
            for radius in [0.5, 3.0, 30.0, 33.4, 99.0, 250.0] {
                let world = World::new(w, h);
                let grid = SpatialHash::build(&agents, &world, radius);
                let (cols, rows) = grid.dims();
                let cell = grid.cell_size();
                assert!(cols >= 1 && rows >= 1, "empty grid for {world:?}");
                assert!(
                    cols == 1 || cell.x >= radius,
                    "cell width {} < radius {radius} in {world:?}",
                    cell.x
                );
                assert!(
                    rows == 1 || cell.y >= radius,
                    "cell height {} < radius {radius} in {world:?}",
                    cell.y
                );
                // The cells must tile the whole world, including a world that
                // is not an exact multiple of the cell size.
                let span_x = cell.x * cols as f64;
                let span_y = cell.y * rows as f64;
                assert!((span_x - w).abs() <= 1e-9 * w, "x span {span_x} != {w}");
                assert!((span_y - h).abs() <= 1e-9 * h, "y span {span_y} != {h}");
            }
        }
    }

    #[test]
    fn spatial_hash_handles_a_world_that_is_not_a_multiple_of_the_cell_size() {
        // width 100 / radius 30 -> 3 columns of 33.333..., height 37 / 30 ->
        // 1 row: neither axis divides evenly.
        let agents = interior_flock();
        let world = World::new(100.0, 37.0);
        let radius = 30.0;
        let grid = SpatialHash::build(&agents, &world, radius);
        for i in 0..agents.len() {
            assert_eq!(
                grid.neighbors(&agents, &world, i, radius),
                neighbors_naive(&agents, &world, i, radius),
                "agent {i}"
            );
        }
    }

    #[test]
    fn spatial_hash_on_an_empty_world_builds_and_returns_nothing() {
        let agents: [Agent; 0] = [];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 10.0);
        assert!(grid.neighbors(&agents, &world, 0, 10.0).is_empty());
    }

    #[test]
    fn spatial_hash_with_a_single_agent_returns_empty_not_itself() {
        let agents = [at(0, 10.0, 10.0)];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 1e9);
        assert!(grid.neighbors(&agents, &world, 0, 1e9).is_empty());
    }

    #[test]
    fn spatial_hash_with_all_agents_coincident_returns_everyone_else() {
        // Every agent lands in one cell: the degenerate case for a grid.
        let agents = [
            at(0, 7.0, 7.0),
            at(1, 7.0, 7.0),
            at(2, 7.0, 7.0),
            at(3, 7.0, 7.0),
        ];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 1.0);
        assert_eq!(grid.neighbors(&agents, &world, 1, 1.0), vec![0, 2, 3]);
    }

    #[test]
    fn spatial_hash_with_zero_radius_returns_only_coincident_agents() {
        let agents = [at(0, 7.0, 7.0), at(1, 7.0, 7.0), at(2, 7.0, 7.000_001)];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 0.0);
        assert_eq!(grid.neighbors(&agents, &world, 0, 0.0), vec![1]);
    }

    #[test]
    fn spatial_hash_with_a_degenerate_radius_returns_nothing() {
        let agents = [at(0, 7.0, 7.0), at(1, 7.5, 7.0)];
        let world = w100();
        for radius in [-5.0, f64::NAN] {
            let grid = SpatialHash::build(&agents, &world, radius);
            assert!(grid.neighbors(&agents, &world, 0, radius).is_empty());
        }
    }

    #[test]
    fn spatial_hash_with_an_out_of_range_index_returns_nothing() {
        let agents = [at(0, 7.0, 7.0), at(1, 7.5, 7.0)];
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 10.0);
        assert!(grid.neighbors(&agents, &world, 9, 10.0).is_empty());
    }

    #[test]
    fn spatial_hash_queried_wider_than_it_was_built_stays_exact() {
        // A grid built for radius 5 cannot answer a radius-40 query from a
        // 3x3 block. Rather than silently under-report, it must degrade to
        // an exhaustive scan and stay equal to the naive backend.
        let agents = interior_flock();
        let world = w100();
        let grid = SpatialHash::build(&agents, &world, 5.0);
        for i in 0..agents.len() {
            assert_eq!(
                grid.neighbors(&agents, &world, i, 40.0),
                neighbors_naive(&agents, &world, i, 40.0),
                "agent {i}"
            );
        }
    }

    /// One randomised configuration, fully reproducible from its seed.
    #[derive(Debug)]
    struct Case {
        seed: u64,
        world: World,
        radius: f64,
        agents: Vec<Agent>,
    }

    /// A coordinate deliberately parked against a wrap seam — within 2% of
    /// either edge, occasionally exactly *on* the far edge (an unwrapped
    /// `size`, which both backends must tolerate).
    fn near_seam(r: &mut Rng, size: f64) -> f64 {
        let margin = size * 0.02;
        if r.next_u64().is_multiple_of(2) {
            r.range(0.0, margin)
        } else {
            size - r.range(0.0, margin)
        }
    }

    /// Build a configuration from a seed, deliberately over-representing the
    /// shapes that break spatial hashes: seams, corners, coincident agents,
    /// radii larger than the world, radii of zero, and worlds smaller than
    /// the radius.
    fn random_case(seed: u64) -> Case {
        let mut r = Rng::seeded(seed);
        let n = match r.next_u64() % 10 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => r.range(3.0, 40.0) as usize,
        };
        let (w, h) = match r.next_u64() % 4 {
            0 => (100.0, 100.0),
            1 => (r.range(1.0, 200.0), r.range(1.0, 200.0)),
            2 => (r.range(0.5, 4.0), r.range(0.5, 4.0)),
            _ => (r.range(20.0, 300.0), r.range(1.0, 10.0)),
        };
        let radius = match r.next_u64() % 6 {
            0 => 0.0,
            1 => r.range(1e-9, 1e-3),
            2 => r.range(w.max(h), 3.0 * (w + h)),
            3 => w / 2.0,
            _ => r.range(0.01, w.max(h) / 2.0),
        };
        let mut agents: Vec<Agent> = Vec::with_capacity(n);
        for i in 0..n {
            let pos = match r.next_u64() % 5 {
                0 => Vec2::new(near_seam(&mut r, w), r.range(0.0, h)),
                1 => Vec2::new(r.range(0.0, w), near_seam(&mut r, h)),
                2 => Vec2::new(near_seam(&mut r, w), near_seam(&mut r, h)),
                3 if !agents.is_empty() => {
                    agents[(r.next_u64() % agents.len() as u64) as usize].pos
                }
                _ => Vec2::new(r.range(0.0, w), r.range(0.0, h)),
            };
            agents.push(Agent {
                id: i as u32,
                pos,
                vel: Vec2::new(r.range(-1.0, 1.0), r.range(-1.0, 1.0)),
            });
        }
        Case {
            seed,
            world: World::new(w, h),
            radius,
            agents,
        }
    }

    /// Distinct seeds, spread by the golden-ratio odd constant so successive
    /// cases are unrelated rather than adjacent states of one stream.
    fn case_seed(i: u64) -> u64 {
        0x5EED_0000_C0FF_EE00u64.wrapping_add(i.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    #[test]
    fn spatial_hash_is_set_equal_to_naive_over_randomised_configurations() {
        // AC-9. Exact set equality, no tolerance: the spatial hash is an
        // optimisation, so any disagreement at all is a behaviour change.
        const CASES: u64 = 256;
        let (mut empties, mut coincident, mut wider_than_world, mut zero_radius) = (0, 0, 0, 0);

        for c in 0..CASES {
            let case = random_case(case_seed(c));
            let Case {
                seed,
                world,
                radius,
                ref agents,
            } = case;

            if agents.is_empty() {
                empties += 1;
            }
            if radius == 0.0 {
                zero_radius += 1;
            }
            if radius > world.width.max(world.height) {
                wider_than_world += 1;
            }
            if agents
                .iter()
                .enumerate()
                .any(|(i, a)| agents[..i].iter().any(|b| b.pos == a.pos))
            {
                coincident += 1;
            }

            let grid = SpatialHash::build(agents, &world, radius);
            for i in 0..agents.len() {
                let mut expected = neighbors_naive(agents, &world, i, radius);
                let mut actual = grid.neighbors(agents, &world, i, radius);
                expected.sort_unstable();
                actual.sort_unstable();
                assert_eq!(
                    expected,
                    actual,
                    "backends disagree — reproduce with random_case({seed:#x})\n\
                     agent {i} at {:?}\n{world:?} radius {radius}\ngrid {:?} cells of {:?}\n\
                     agents: {agents:?}",
                    agents[i].pos,
                    grid.dims(),
                    grid.cell_size(),
                );
            }
        }

        // The corpus must actually contain the shapes it claims to: a
        // generator that quietly stopped producing them would turn this test
        // into an expensive no-op.
        assert!(empties > 0, "no zero-agent cases generated");
        assert!(coincident > 0, "no coincident-agent cases generated");
        assert!(wider_than_world > 0, "no radius-larger-than-world cases");
        assert!(zero_radius > 0, "no zero-radius cases generated");
    }

    /// A populated flock spread over the whole world, seams included.
    fn populated_flock(seed: u64, n: u32, world: &World) -> Vec<Agent> {
        let mut r = Rng::seeded(seed);
        (0..n)
            .map(|id| Agent {
                id,
                pos: Vec2::new(r.range(0.0, world.width), r.range(0.0, world.height)),
                vel: Vec2::new(r.range(-2.0, 2.0), r.range(-2.0, 2.0)),
            })
            .collect()
    }

    #[test]
    fn both_backends_agree_for_every_agent_over_a_sequence_of_queries() {
        // AC-10 (kernel half): not one lucky agent, but every agent in a
        // populated world, queried in sequence against one grid built once —
        // the exact usage pattern a tick has. The multi-tick `state_hash`
        // equivalence lives in `sim`.
        let world = World::new(200.0, 120.0);
        let agents = populated_flock(0xA11CE, 200, &world);
        let mut total = 0usize;

        for radius in [3.0, 12.5, 40.0, 90.0] {
            let grid = SpatialHash::build(&agents, &world, radius);
            for i in 0..agents.len() {
                let naive =
                    neighbors_with(NeighborBackend::Naive, &agents, &world, None, i, radius);
                let hashed = neighbors_with(
                    NeighborBackend::SpatialHash,
                    &agents,
                    &world,
                    Some(&grid),
                    i,
                    radius,
                );
                assert_eq!(naive, hashed, "radius {radius}, agent {i}");
                // Order, not just membership: an identical set in a different
                // order would sum to a different force.
                assert!(naive.windows(2).all(|w| w[0] < w[1]), "not ascending");
                total += naive.len();
            }
            // Querying the same grid again must give the same answers: a
            // grid is read-only state, and a tick queries it N times.
            for i in (0..agents.len()).rev() {
                assert_eq!(
                    grid.neighbors(&agents, &world, i, radius),
                    neighbors_naive(&agents, &world, i, radius),
                    "second pass, radius {radius}, agent {i}"
                );
            }
        }
        assert!(total > 1_000, "flock too sparse to prove anything: {total}");
    }

    #[test]
    fn swapping_the_backend_changes_nothing_at_the_call_site() {
        // AC-49: the call site is byte-identical across backends — only the
        // enum value differs — and so is the answer.
        let world = World::new(64.0, 64.0);
        let agents = populated_flock(0xB0B, 90, &world);
        let radius = 9.0;
        let grid = SpatialHash::build(&agents, &world, radius);
        for backend in [NeighborBackend::Naive, NeighborBackend::SpatialHash] {
            for i in 0..agents.len() {
                assert_eq!(
                    neighbors_with(backend, &agents, &world, Some(&grid), i, radius),
                    neighbors_naive(&agents, &world, i, radius),
                    "{backend:?} disagreed on agent {i}"
                );
            }
        }
    }

    #[test]
    fn neighbors_with_is_exact_even_without_a_prebuilt_grid() {
        // A caller that has no grid to hand must still get the right answer
        // from the spatial-hash backend, just more slowly.
        let world = World::new(50.0, 80.0);
        let agents = populated_flock(0xC0DE, 40, &world);
        let radius = 11.0;
        for i in 0..agents.len() {
            assert_eq!(
                neighbors_with(
                    NeighborBackend::SpatialHash,
                    &agents,
                    &world,
                    None,
                    i,
                    radius
                ),
                neighbors_naive(&agents, &world, i, radius),
                "agent {i}"
            );
        }
    }

    #[test]
    fn the_naive_backend_ignores_a_supplied_grid() {
        // Passing a grid must not smuggle the other backend in; and passing
        // a *stale* grid to the naive backend must be harmless.
        let world = World::new(50.0, 50.0);
        let agents = populated_flock(0xD00D, 30, &world);
        let stale = SpatialHash::build(&populated_flock(0x9999, 7, &world), &world, 1.0);
        for i in 0..agents.len() {
            assert_eq!(
                neighbors_with(
                    NeighborBackend::Naive,
                    &agents,
                    &world,
                    Some(&stale),
                    i,
                    10.0
                ),
                neighbors_naive(&agents, &world, i, 10.0),
                "agent {i}"
            );
        }
    }

    #[test]
    fn a_grid_built_for_a_different_length_slice_falls_back_to_an_exact_scan() {
        // Guards the `agents.len() != self.agent_count` fallback in
        // `neighbors`: a grid built for a *different* flock must not index out
        // of bounds, and must not drop agents it never saw.
        //
        // The added agent sits in the MIDDLE of the flock, at (45,45) — inside
        // the flock's [20, 72.5]^2 span, well within the 12.0 radius of
        // several existing agents. That placement is the whole test. Put it
        // out at (4,4) instead, where it has no neighbours and is nobody's
        // neighbour, and the stale grid answers every query correctly by
        // accident: deleting the length guard outright still passes.
        let world = w100();
        let built_for = interior_flock();
        let grid = SpatialHash::build(&built_for, &world, 12.0);
        let mut moved = interior_flock();
        moved.push(at(99, 45.0, 45.0));

        // The intruder has to matter, or the fallback is not load-bearing.
        let intruder = moved.len() - 1;
        assert!(
            !neighbors_naive(&moved, &world, intruder, 12.0).is_empty(),
            "the added agent has no neighbours; the test would prove nothing"
        );
        assert!(
            (0..built_for.len()).any(|i| neighbors_naive(&moved, &world, i, 12.0)
                .contains(&intruder)),
            "no existing agent sees the added agent; the test would prove nothing"
        );

        for i in 0..moved.len() {
            assert_eq!(
                grid.neighbors(&moved, &world, i, 12.0),
                neighbors_naive(&moved, &world, i, 12.0),
                "agent {i}"
            );
        }
    }

    #[test]
    fn the_length_guard_does_not_detect_a_same_length_flock_that_moved() {
        // The known limitation of the guard above, pinned so nobody mistakes
        // it for more than it is: the check is on the agent **count**, not on
        // the positions the grid was built from. A flock of the same size
        // whose members have moved still takes the fast path, and the fast
        // path reads buckets that describe where those agents *used to be*.
        //
        // Nothing stronger is possible at this seam without re-deriving every
        // bucket, which is exactly the work `build` does. What actually covers
        // this is the "rebuild every tick" contract in `sim::step`, which
        // builds one grid per tick from that tick's positions and never
        // carries one across a step.
        let world = w100();
        let built_for = interior_flock();
        let grid = SpatialHash::build(&built_for, &world, 12.0);

        // Same length, but every agent teleported to the far corner.
        let moved: Vec<Agent> = built_for
            .iter()
            .map(|a| at(a.id, (a.pos.x + 45.0) % 100.0, (a.pos.y + 45.0) % 100.0))
            .collect();
        assert_eq!(moved.len(), built_for.len(), "the guard is length-only");

        let disagreements = (0..moved.len())
            .filter(|&i| {
                grid.neighbors(&moved, &world, i, 12.0) != neighbors_naive(&moved, &world, i, 12.0)
            })
            .count();
        assert!(
            disagreements > 0,
            "a stale grid over a moved same-length flock agreed with an exact \
             scan everywhere; if that is now genuinely true the guard has been \
             strengthened and this test should be replaced, not deleted"
        );
    }
}
