// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clustering building points in plan: an occupancy grid, cells closer
//! than a gap joined (union–find), and the outline of a cluster's cells
//! traced. Deterministic: the result depends on the set of points, not
//! on their order.

use std::collections::{HashMap, HashSet};

use crate::geom::P2;

/// A cell of the plan grid: its column and row (`floor(x / cell)`,
/// `floor(y / cell)`).
pub type Cell = (i64, i64);

/// The cell of a point.
pub fn cell_of(x: f64, y: f64, cell: f64) -> Cell {
    ((x / cell).floor() as i64, (y / cell).floor() as i64)
}

/// A cluster: its points (indices into the points given, ascending) and
/// its cells (ascending).
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Indices of its points.
    pub points: Vec<usize>,
    /// Its cells.
    pub cells: Vec<Cell>,
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    // The smaller index the root: the same partition in any order.
    if ra < rb {
        parent[rb] = ra;
    } else if rb < ra {
        parent[ra] = rb;
    }
}

/// The offsets (forward half) of the cells joined to a cell: its eight
/// neighbours and every cell whose edge is nearer than `gap`.
fn offsets(cell: f64, gap: f64) -> Vec<Cell> {
    let r = (gap / cell).ceil() as i64 + 1;
    let mut out = Vec::new();
    for di in 0..=r {
        for dj in -r..=r {
            if di == 0 && dj <= 0 {
                continue;
            }
            let gx = (di.abs() - 1).max(0) as f64 * cell;
            let gy = (dj.abs() - 1).max(0) as f64 * cell;
            if (di.abs() <= 1 && dj.abs() <= 1) || gx.hypot(gy) < gap {
                out.push((di, dj));
            }
        }
    }
    out
}

/// The clusters of `points` (plan) on a grid of `cell`, cells nearer
/// than `gap` joined; ordered by their first cell.
pub fn clusters(points: &[P2], cell: f64, gap: f64) -> Vec<Cluster> {
    let mut keys: Vec<Cell> = points.iter().map(|p| cell_of(p[0], p[1], cell)).collect();
    let point_cells = keys.clone();
    keys.sort_unstable();
    keys.dedup();
    let index: HashMap<Cell, usize> = keys.iter().enumerate().map(|(k, c)| (*c, k)).collect();
    let mut parent: Vec<usize> = (0..keys.len()).collect();
    let near = offsets(cell, gap);
    for (k, c) in keys.iter().enumerate() {
        for (di, dj) in &near {
            if let Some(&j) = index.get(&(c.0 + di, c.1 + dj)) {
                union(&mut parent, k, j);
            }
        }
    }
    let mut groups: HashMap<usize, Cluster> = HashMap::new();
    for (k, c) in keys.iter().enumerate() {
        let r = find(&mut parent, k);
        groups
            .entry(r)
            .or_insert_with(|| Cluster {
                points: Vec::new(),
                cells: Vec::new(),
            })
            .cells
            .push(*c);
    }
    for (i, c) in point_cells.iter().enumerate() {
        let r = find(&mut parent, index[c]);
        if let Some(g) = groups.get_mut(&r) {
            g.points.push(i);
        }
    }
    let mut out: Vec<Cluster> = groups.into_values().collect();
    for g in &mut out {
        g.cells.sort_unstable();
        g.points.sort_unstable();
    }
    out.sort_by_key(|g| g.cells[0]);
    out
}

/// The outline of `cells` (a cluster's): gaps narrower than `gap` closed
/// (a dilation and an erosion by the cells they span), then the outer
/// boundary traced, counter-clockwise, collinear corners dropped. None
/// when the closed cells are still in several pieces (cells touching
/// only at a corner count as apart).
pub fn trace_cells(cells: &[Cell], cell: f64, gap: f64) -> Option<Vec<P2>> {
    let widest = ((gap / cell).ceil() as i64 - 1).max(0);
    let r = (widest + 1) / 2;
    let set: HashSet<Cell> = cells.iter().copied().collect();
    let closed = if r > 0 {
        let mut dilated: HashSet<Cell> = HashSet::new();
        for c in &set {
            for di in -r..=r {
                for dj in -r..=r {
                    dilated.insert((c.0 + di, c.1 + dj));
                }
            }
        }
        let mut kept: HashSet<Cell> = HashSet::new();
        for c in &dilated {
            let all = (-r..=r).all(|di| (-r..=r).all(|dj| dilated.contains(&(c.0 + di, c.1 + dj))));
            if all {
                kept.insert(*c);
            }
        }
        // The erosion may drop cells of the cluster's own thin parts:
        // they stay.
        kept.extend(set.iter().copied());
        kept
    } else {
        set
    };
    let loops = boundary_loops(&closed);
    let outer: Vec<Vec<(i64, i64)>> = loops.into_iter().filter(|l| doubled_area(l) > 0).collect();
    if outer.len() != 1 {
        return None;
    }
    let ring: Vec<(i64, i64)> = drop_collinear(&outer[0]);
    Some(
        ring.iter()
            .map(|&(i, j)| [i as f64 * cell, j as f64 * cell])
            .collect(),
    )
}

fn doubled_area(ring: &[(i64, i64)]) -> i64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum()
}

fn drop_collinear(ring: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let n = ring.len();
    (0..n)
        .filter(|&i| {
            let (p, c, q) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            (c.0 - p.0) * (q.1 - c.1) - (c.1 - p.1) * (q.0 - c.0) != 0
        })
        .map(|i| ring[i])
        .collect()
}

/// A boundary edge from a corner: where it ends, whether it is taken.
type Edge = ((i64, i64), bool);

/// The boundary loops of a set of cells, each with the cells on its left
/// (outer boundaries counter-clockwise, holes clockwise); at a corner
/// where two cells touch diagonally the loop turns left (the cells stay
/// apart).
fn boundary_loops(set: &HashSet<Cell>) -> Vec<Vec<(i64, i64)>> {
    let mut cells: Vec<Cell> = set.iter().copied().collect();
    cells.sort_unstable();
    // Edges from a corner: (to, used).
    let mut edges: HashMap<(i64, i64), Vec<Edge>> = HashMap::new();
    let mut starts: Vec<(i64, i64)> = Vec::new();
    let mut add = |a: (i64, i64), b: (i64, i64)| {
        edges.entry(a).or_default().push((b, false));
        starts.push(a);
    };
    for &(i, j) in &cells {
        if !set.contains(&(i, j - 1)) {
            add((i, j), (i + 1, j));
        }
        if !set.contains(&(i + 1, j)) {
            add((i + 1, j), (i + 1, j + 1));
        }
        if !set.contains(&(i, j + 1)) {
            add((i + 1, j + 1), (i, j + 1));
        }
        if !set.contains(&(i - 1, j)) {
            add((i, j + 1), (i, j));
        }
    }
    let mut loops = Vec::new();
    for s in starts {
        let Some(k) = edges[&s].iter().position(|e| !e.1) else {
            continue;
        };
        let mut ring = vec![s];
        let mut cur = s;
        let mut next_k = k;
        loop {
            let list = edges.get_mut(&cur).expect("an edge from the corner");
            list[next_k].1 = true;
            let to = list[next_k].0;
            let dir = (to.0 - cur.0, to.1 - cur.1);
            if to == s {
                break;
            }
            ring.push(to);
            let out = &edges[&to];
            // Left turn first, then straight on, then right.
            let left = (-dir.1, dir.0);
            let right = (dir.1, -dir.0);
            let pick = [left, dir, right].iter().find_map(|d| {
                out.iter()
                    .position(|e| !e.1 && (e.0.0 - to.0, e.0.1 - to.1) == *d)
            });
            match pick {
                Some(p) => {
                    cur = to;
                    next_k = p;
                }
                None => break,
            }
        }
        loops.push(ring);
    }
    loops
}
