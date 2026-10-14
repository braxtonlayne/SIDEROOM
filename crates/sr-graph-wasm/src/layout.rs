//! Force-directed graph layout with Barnes-Hut repulsion (ADM-GRAPH-002).
//!
//! The Invite Graph can hold thousands of accounts. A naive force layout compares every pair of
//! nodes each step, O(n^2); here far-away groups of nodes are treated as one heavier body found
//! through a quadtree, O(n log n) per step. The result is deterministic for a given graph and
//! seed, so the same tree always opens looking the same.
//!
//! Forces (Fruchterman-Reingold): every pair repels with `k^2 / d`, every edge pulls with
//! `d^2 / k`, a weak gravity keeps disconnected pieces near the middle, and a cooling
//! temperature limits how far a node may move per step.

/// What the layout needs to know besides the graph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Steps to run. Around 300 settles a few thousand nodes.
    pub iterations: u32,
    /// Seed of the starting jitter; the same seed gives the same picture.
    pub seed: u32,
    /// Barnes-Hut opening angle: a cell is one body when `size / distance < theta`.
    /// 0 is exact; 0.5 to 1.0 is the usual range.
    pub theta: f32,
    /// Ideal edge length.
    pub spring_length: f32,
    /// Pull towards the origin, as a fraction of the distance.
    pub gravity: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            iterations: 300,
            seed: 1,
            theta: 0.8,
            spring_length: 40.0,
            gravity: 0.02,
        }
    }
}

/// Why a layout could not be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutError {
    #[error("edge {edge} points at node {node}, but there are only {count} nodes")]
    BadEdge {
        edge: usize,
        node: u32,
        count: usize,
    },
    #[error("the edge list must hold pairs of node indices")]
    OddEdgeList,
}

/// Small deterministic generator (xorshift32): the layout must not depend on a platform RNG.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed.max(1) ^ 0x9E37_79B9)
    }

    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[derive(Clone, Copy, Default)]
struct Vec2 {
    x: f32,
    y: f32,
}

/// One cell of the quadtree: the square `(cx, cy, half)`, the total mass and centre of mass of
/// the nodes below it, and either four children or (for a leaf) the single node inside.
struct Cell {
    cx: f32,
    cy: f32,
    half: f32,
    mass: f32,
    mx: f32,
    my: f32,
    /// Index of the first of four consecutive children, or 0 for a leaf.
    children: usize,
    /// The node held by a leaf, if any.
    body: Option<usize>,
}

struct Tree {
    cells: Vec<Cell>,
}

/// Points closer than this are the same place; the tree stops splitting there.
const MIN_HALF: f32 = 1e-4;

impl Tree {
    fn build(pos: &[Vec2]) -> Self {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pos {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        let half = ((max_x - min_x).max(max_y - min_y) / 2.0).max(1.0) * 1.001;
        let mut t = Self {
            cells: Vec::with_capacity(pos.len() * 3),
        };
        t.cells.push(Cell {
            cx: (min_x + max_x) / 2.0,
            cy: (min_y + max_y) / 2.0,
            half,
            mass: 0.0,
            mx: 0.0,
            my: 0.0,
            children: 0,
            body: None,
        });
        for (i, p) in pos.iter().enumerate() {
            t.insert(i, *p, pos);
        }
        t
    }

    /// Child index (0 to 3) of the quadrant of `c` that holds `p`: lower bound inclusive.
    fn quadrant(c: &Cell, p: Vec2) -> usize {
        usize::from(p.x >= c.cx) + 2 * usize::from(p.y >= c.cy)
    }

    fn split(&mut self, at: usize) {
        let (cx, cy, half) = {
            let c = &self.cells[at];
            (c.cx, c.cy, c.half / 2.0)
        };
        let first = self.cells.len();
        for q in 0..4 {
            let dx = if q % 2 == 1 { half } else { -half };
            let dy = if q / 2 == 1 { half } else { -half };
            self.cells.push(Cell {
                cx: cx + dx,
                cy: cy + dy,
                half,
                mass: 0.0,
                mx: 0.0,
                my: 0.0,
                children: 0,
                body: None,
            });
        }
        self.cells[at].children = first;
    }

    /// Add one unit of mass at `p` to cell `at` (keeps the centre of mass).
    fn add(&mut self, at: usize, p: Vec2) {
        let c = &mut self.cells[at];
        let total = c.mass + 1.0;
        c.mx = (c.mx * c.mass + p.x) / total;
        c.my = (c.my * c.mass + p.y) / total;
        c.mass = total;
    }

    fn insert(&mut self, node: usize, p: Vec2, pos: &[Vec2]) {
        let mut at = 0;
        loop {
            let was_empty = self.cells[at].mass == 0.0;
            self.add(at, p);
            if self.cells[at].children == 0 {
                if was_empty {
                    self.cells[at].body = Some(node);
                    return;
                }
                // Cells this small hold several nodes at one place: they stay one body.
                if self.cells[at].half <= MIN_HALF {
                    return;
                }
                // A leaf already holds one node: split, move that node down, go on with this one.
                self.split(at);
                if let Some(old) = self.cells[at].body.take() {
                    let child = self.cells[at].children + Self::quadrant(&self.cells[at], pos[old]);
                    self.add(child, pos[old]);
                    self.cells[child].body = Some(old);
                }
            }
            at = self.cells[at].children + Self::quadrant(&self.cells[at], p);
        }
    }

    /// Does `p` lie in this cell (lower bounds inclusive, upper bounds exclusive)?
    fn contains(c: &Cell, p: Vec2) -> bool {
        p.x >= c.cx - c.half && p.x < c.cx + c.half && p.y >= c.cy - c.half && p.y < c.cy + c.half
    }

    /// Repulsion on `p` from everything in the tree, except `p` itself.
    fn repulsion(&self, p: Vec2, theta: f32, k2: f32) -> Vec2 {
        let mut f = Vec2::default();
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let c = &self.cells[i];
            if c.mass == 0.0 {
                continue;
            }
            let inside = Self::contains(c, p);
            let leaf = c.children == 0;
            let size = c.half * 2.0;
            let (dx, dy) = (p.x - c.mx, p.y - c.my);
            // A cell that holds `p` is always opened, so the node never pushes on itself.
            let far = !leaf && !inside && size * size < theta * theta * (dx * dx + dy * dy);
            if !(leaf || far) {
                for q in 0..4 {
                    stack.push(c.children + q);
                }
                continue;
            }
            let (mut mass, mut bx, mut by) = (c.mass, c.mx, c.my);
            if inside {
                // A leaf that holds the node: take the node out of its mass.
                if mass <= 1.0 {
                    continue;
                }
                bx = (c.mx * mass - p.x) / (mass - 1.0);
                by = (c.my * mass - p.y) / (mass - 1.0);
                mass -= 1.0;
            }
            let (ddx, ddy) = (p.x - bx, p.y - by);
            let dist2 = (ddx * ddx + ddy * ddy).max(0.01);
            // k^2 / d, pointing away from the body: (k^2 / d^2) * delta, once per unit of mass.
            let s = k2 * mass / dist2;
            f.x += ddx * s;
            f.y += ddy * s;
        }
        f
    }
}

/// Exact repulsion by comparing all pairs; used by tests as the reference.
pub fn repulsion_exact(pos: &[(f32, f32)], k: f32) -> Vec<(f32, f32)> {
    let k2 = k * k;
    pos.iter()
        .enumerate()
        .map(|(i, &(px, py))| {
            let (mut fx, mut fy) = (0.0, 0.0);
            for (j, &(qx, qy)) in pos.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (dx, dy) = (px - qx, py - qy);
                let d2 = (dx * dx + dy * dy).max(0.01);
                let s = k2 / d2;
                fx += dx * s;
                fy += dy * s;
            }
            (fx, fy)
        })
        .collect()
}

/// Repulsion through the Barnes-Hut tree with opening angle `theta`.
pub fn repulsion_barnes_hut(pos: &[(f32, f32)], k: f32, theta: f32) -> Vec<(f32, f32)> {
    let v: Vec<Vec2> = pos.iter().map(|&(x, y)| Vec2 { x, y }).collect();
    if v.is_empty() {
        return Vec::new();
    }
    let tree = Tree::build(&v);
    v.iter()
        .map(|p| {
            let f = tree.repulsion(*p, theta, k * k);
            (f.x, f.y)
        })
        .collect()
}

/// Lay out `node_count` nodes joined by `edges` (a flat list of node index pairs).
/// Returns `2 * node_count` floats: `x0, y0, x1, y1, ...`, centred on the origin.
pub fn layout(node_count: usize, edges: &[u32], opts: Options) -> Result<Vec<f32>, LayoutError> {
    let (pairs, rest) = edges.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(LayoutError::OddEdgeList);
    }
    for (e, pair) in pairs.iter().enumerate() {
        for &n in pair {
            if n as usize >= node_count {
                return Err(LayoutError::BadEdge {
                    edge: e,
                    node: n,
                    count: node_count,
                });
            }
        }
    }
    match node_count {
        0 => return Ok(Vec::new()),
        1 => return Ok(vec![0.0, 0.0]),
        _ => {}
    }

    let k = opts.spring_length.max(1.0);
    let theta = opts.theta.max(0.0);
    let mut rng = Rng::new(opts.seed);

    // Start on a golden-angle spiral (even spread, no coincident points) plus a little jitter.
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    let spread = k * (node_count as f32).sqrt() * 0.5;
    let mut pos: Vec<Vec2> = (0..node_count)
        .map(|i| {
            let r = spread * ((i as f32 + 0.5) / node_count as f32).sqrt();
            let a = i as f32 * golden;
            Vec2 {
                x: r * a.cos() + (rng.next_f32() - 0.5) * k * 0.1,
                y: r * a.sin() + (rng.next_f32() - 0.5) * k * 0.1,
            }
        })
        .collect();

    let mut disp = vec![Vec2::default(); node_count];
    let start_temp = spread.max(k);
    let steps = opts.iterations.max(1);
    for step in 0..steps {
        let temp = start_temp * (1.0 - step as f32 / steps as f32).max(0.01);
        let tree = Tree::build(&pos);
        for (i, d) in disp.iter_mut().enumerate() {
            let f = tree.repulsion(pos[i], theta, k * k);
            // Gravity: a pull towards the origin keeps unrelated components together.
            *d = Vec2 {
                x: f.x - pos[i].x * opts.gravity * k,
                y: f.y - pos[i].y * opts.gravity * k,
            };
        }
        for pair in pairs {
            let (a, b) = (pair[0] as usize, pair[1] as usize);
            if a == b {
                continue;
            }
            let dx = pos[a].x - pos[b].x;
            let dy = pos[a].y - pos[b].y;
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
            // d^2 / k along the edge.
            let s = dist / k;
            disp[a].x -= dx * s;
            disp[a].y -= dy * s;
            disp[b].x += dx * s;
            disp[b].y += dy * s;
        }
        for (p, d) in pos.iter_mut().zip(&disp) {
            let len = (d.x * d.x + d.y * d.y).sqrt();
            if len > 0.0 && len.is_finite() {
                let step = len.min(temp);
                p.x += d.x / len * step;
                p.y += d.y / len * step;
            }
        }
    }

    // Centre on the origin.
    let n = node_count as f32;
    let (cx, cy) = pos
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p.x / n, y + p.y / n));
    Ok(pos.iter().flat_map(|p| [p.x - cx, p.y - cy]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(out: &[f32], a: usize, b: usize) -> f32 {
        let (dx, dy) = (out[2 * a] - out[2 * b], out[2 * a + 1] - out[2 * b + 1]);
        (dx * dx + dy * dy).sqrt()
    }

    /// A random-looking but deterministic cloud.
    fn cloud(n: usize) -> Vec<(f32, f32)> {
        let mut r = Rng::new(7);
        (0..n)
            .map(|_| (r.next_f32() * 500.0, r.next_f32() * 500.0))
            .collect()
    }

    #[test]
    fn barnes_hut_matches_the_exact_forces_within_a_few_percent() {
        let pts = cloud(400);
        let exact = repulsion_exact(&pts, 40.0);
        let bh = repulsion_barnes_hut(&pts, 40.0, 0.5);
        let (mut err, mut norm) = (0.0_f64, 0.0_f64);
        for (e, b) in exact.iter().zip(&bh) {
            err += f64::from((e.0 - b.0).powi(2) + (e.1 - b.1).powi(2)).sqrt();
            norm += f64::from(e.0.powi(2) + e.1.powi(2)).sqrt();
        }
        assert!(err / norm < 0.05, "relative error {}", err / norm);
        // theta = 0 opens every cell, so it is exact (up to rounding).
        let zero = repulsion_barnes_hut(&pts, 40.0, 0.0);
        for (e, b) in exact.iter().zip(&zero) {
            assert!(
                (e.0 - b.0).abs() <= 1e-2 * e.0.abs().max(1.0),
                "{e:?} vs {b:?}"
            );
            assert!((e.1 - b.1).abs() <= 1e-2 * e.1.abs().max(1.0));
        }
    }

    #[test]
    fn the_same_graph_and_seed_give_the_same_picture() {
        let edges = [0, 1, 1, 2, 2, 3, 3, 0, 3, 4];
        let a = layout(5, &edges, Options::default()).unwrap();
        let b = layout(5, &edges, Options::default()).unwrap();
        assert_eq!(a, b);
        let c = layout(
            5,
            &edges,
            Options {
                seed: 99,
                ..Options::default()
            },
        )
        .unwrap();
        assert_ne!(a, c);
        assert_eq!(a.len(), 10);
    }

    #[test]
    fn connected_nodes_end_up_closer_than_strangers() {
        // Two cliques of five joined by one edge.
        let mut edges = Vec::new();
        for base in [0u32, 5] {
            for i in 0..5 {
                for j in (i + 1)..5 {
                    edges.extend([base + i, base + j]);
                }
            }
        }
        edges.extend([4, 5]);
        let out = layout(10, &edges, Options::default()).unwrap();
        let within: f32 = (0..5).map(|i| dist(&out, i, (i + 1) % 5)).sum::<f32>() / 5.0;
        let across: f32 = (0..5).map(|i| dist(&out, i, 5 + i)).sum::<f32>() / 5.0;
        assert!(within * 1.5 < across, "within {within}, across {across}");
        // Nothing sits on top of anything else.
        for a in 0..10 {
            for b in (a + 1)..10 {
                assert!(dist(&out, a, b) > 1.0, "{a} and {b} overlap");
            }
        }
    }

    #[test]
    fn a_tree_spreads_out_without_collapsing() {
        // A binary tree of 255 nodes.
        let n = 255usize;
        let mut edges = Vec::new();
        for i in 1..n {
            edges.extend([((i - 1) / 2) as u32, i as u32]);
        }
        let out = layout(n, &edges, Options::default()).unwrap();
        assert!(out.iter().all(|v| v.is_finite()));
        // Edges stay near the spring length: no huge stretching, no collapse.
        let mean: f32 = (1..n).map(|i| dist(&out, (i - 1) / 2, i)).sum::<f32>() / (n - 1) as f32;
        assert!((10.0..160.0).contains(&mean), "mean edge {mean}");
        // Centred.
        let cx: f32 = out.iter().step_by(2).sum::<f32>() / n as f32;
        assert!(cx.abs() < 1.0);
    }

    #[test]
    fn awkward_input_never_produces_nan() {
        // Self loops, repeated edges, isolated nodes.
        let edges = [0, 0, 1, 2, 1, 2, 2, 1];
        let out = layout(6, &edges, Options::default()).unwrap();
        assert!(out.iter().all(|v| v.is_finite()));
        // No edges at all.
        let out = layout(50, &[], Options::default()).unwrap();
        assert!(out.iter().all(|v| v.is_finite()) && out.len() == 100);
        // A coincident cloud through the tree directly.
        let same = vec![(5.0, 5.0); 30];
        let f = repulsion_barnes_hut(&same, 40.0, 0.8);
        assert!(f.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn tiny_graphs_and_bad_edges() {
        assert_eq!(
            layout(0, &[], Options::default()).unwrap(),
            Vec::<f32>::new()
        );
        assert_eq!(layout(1, &[], Options::default()).unwrap(), vec![0.0, 0.0]);
        assert_eq!(
            layout(3, &[0, 5], Options::default()),
            Err(LayoutError::BadEdge {
                edge: 0,
                node: 5,
                count: 3
            })
        );
        assert_eq!(
            layout(3, &[0, 1, 2], Options::default()),
            Err(LayoutError::OddEdgeList)
        );
    }

    #[test]
    fn a_large_graph_finishes() {
        let n = 4000usize;
        let mut edges = Vec::new();
        for i in 1..n {
            edges.extend([((i - 1) / 3) as u32, i as u32]);
        }
        let out = layout(
            n,
            &edges,
            Options {
                iterations: 40,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(out.len(), 2 * n);
        assert!(out.iter().all(|v| v.is_finite()));
    }
}
