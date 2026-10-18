// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Levels of detail for viewers: a point cloud as a tree of nodes, the
//! nodes a view needs picked under a point budget, and a cache of the
//! nodes read (least recently used out first).
//!
//! - [`Hierarchy`]: nodes with bounds, points and point spacing. A COPC
//!   file's octree adds detail level by level ([`Refine::Add`]); a stored
//!   cloud has, per square of the plan, a chain of its levels of detail,
//!   each replacing the coarser one ([`Refine::Replace`]).
//! - [`Hierarchy::select`]: every frame, the nodes in view, the coarsest
//!   first, refined where their points lie farther apart on the screen
//!   than the error allowed, nearest (largest error) first, until the
//!   point budget is used.
//! - [`NodeCache`]: what was read, evicted least recently used first.
//! - [`LodReader`]: reads a node's points ([`open_lod`]: a COPC file or a
//!   project's cloud).

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ols_core::budget::Budget;

use crate::copc::{CopcFile, VoxelKey};
use crate::{CloudInfo, LOD_LEVELS, Stats, Store};

/// Side of a stored cloud's node squares, m (whole chunks; the 3D view's
/// grid tile).
pub const NODE_SIZE: f64 = 256.0;
/// How far above its block's base a point may lie when the top is not
/// stored (a version 1 file), m.
const ABOVE_BASE: f64 = 300.0;

/// How children refine their parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refine {
    /// Children add points to the parent's (COPC).
    Add,
    /// Children replace the parent's points (a stored cloud's levels).
    Replace,
}

/// Bounds: minimum and maximum corner.
pub type Box3 = [[f64; 3]; 2];

/// One node.
#[derive(Debug, Clone, PartialEq)]
pub struct LodNode {
    /// Bounds of its points.
    pub bounds: Box3,
    /// Points.
    pub points: u64,
    /// Mean distance between its points in plan, m (a node is refined
    /// while this is too large on the screen).
    pub spacing: f64,
    /// Depth (0: a root).
    pub depth: u32,
    /// Parent.
    pub parent: Option<usize>,
    /// Children.
    pub children: Vec<usize>,
}

/// A point cloud's nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Hierarchy {
    /// Nodes.
    pub nodes: Vec<LodNode>,
    /// Nodes without a parent.
    pub roots: Vec<usize>,
    /// How children refine.
    pub refine: Refine,
}

/// The view a selection is made for.
pub struct LodView<'a> {
    /// Distance from the eye to bounds, m.
    pub distance: &'a dyn Fn(&Box3) -> f64,
    /// Whether bounds may be in view.
    pub sees: &'a dyn Fn(&Box3) -> bool,
    /// Pixels per radian of view (view height in pixels / vertical field
    /// of view): spacing / distance × this = spacing on the screen, px.
    pub pixels_per_radian: f64,
    /// Nothing farther, m.
    pub render_distance: f64,
}

/// Limits of a selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodLimits {
    /// Points drawn at most.
    pub point_budget: u64,
    /// A node is refined while its points lie farther apart on the screen
    /// than this, px.
    pub max_error: f64,
    /// Share of a node's points shown (0–1]: the budget counts these.
    pub share: f64,
}

/// What a selection picked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selection {
    /// Nodes picked, in order of priority (read them in this order).
    pub nodes: Vec<usize>,
    /// Nodes to draw: all picked ([`Refine::Add`]), or those not replaced
    /// by their picked children ([`Refine::Replace`]).
    pub draw: Vec<usize>,
    /// Points of the nodes drawn (shares applied).
    pub points: u64,
    /// Whether the budget stopped the refinement.
    pub capped: bool,
}

/// A node waiting in the selection: its error on the screen, then its
/// index (largest error first).
#[derive(Debug, PartialEq)]
struct Queued(f64, usize);

impl Eq for Queued {}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Queued {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .total_cmp(&other.0)
            .then_with(|| other.1.cmp(&self.1))
    }
}

impl Hierarchy {
    /// Points of all nodes (each point once for [`Refine::Add`]; the
    /// finest level for [`Refine::Replace`]).
    pub fn points(&self) -> u64 {
        match self.refine {
            Refine::Add => self.nodes.iter().map(|n| n.points).sum(),
            Refine::Replace => self
                .nodes
                .iter()
                .filter(|n| n.children.is_empty())
                .map(|n| n.points)
                .sum(),
        }
    }

    /// The deepest level.
    pub fn depth(&self) -> u32 {
        self.nodes.iter().map(|n| n.depth).max().unwrap_or(0)
    }

    /// The nodes to draw for this view, within the limits: the roots in
    /// view, then children of nodes whose points lie more than
    /// `max_error` px apart on the screen, the largest error first, until
    /// the next node would exceed the point budget.
    ///
    /// Cost: O(k log k) for the k nodes looked at (those in view down to
    /// the depth the error asks for); each node at most once.
    pub fn select(&self, view: &LodView<'_>, limits: &LodLimits) -> Selection {
        let share = limits.share.clamp(0.0, 1.0);
        let mut heap = BinaryHeap::new();
        let consider = |i: usize, heap: &mut BinaryHeap<Queued>| {
            let n = &self.nodes[i];
            let d = (view.distance)(&n.bounds);
            if d > view.render_distance || !(view.sees)(&n.bounds) {
                return;
            }
            let error = n.spacing / d.max(1e-3) * view.pixels_per_radian;
            heap.push(Queued(error, i));
        };
        for &r in &self.roots {
            consider(r, &mut heap);
        }
        let mut out = Selection::default();
        let mut picked: HashSet<usize> = HashSet::new();
        let mut total = 0.0;
        while let Some(Queued(error, i)) = heap.pop() {
            let n = &self.nodes[i];
            let mut cost = n.points as f64 * share;
            if self.refine == Refine::Replace
                && let Some(p) = n.parent
            {
                // A parent's points go when its children come.
                let siblings = self.nodes[p].children.len().max(1) as f64;
                cost -= self.nodes[p].points as f64 * share / siblings;
            }
            if total + cost > limits.point_budget as f64 {
                out.capped = true;
                break;
            }
            total += cost;
            picked.insert(i);
            out.nodes.push(i);
            if error > limits.max_error {
                for &c in &n.children {
                    consider(c, &mut heap);
                }
            }
        }
        out.draw = match self.refine {
            Refine::Add => out.nodes.clone(),
            Refine::Replace => out
                .nodes
                .iter()
                .copied()
                .filter(|&i| {
                    let c = &self.nodes[i].children;
                    c.is_empty() || !c.iter().all(|k| picked.contains(k))
                })
                .collect(),
        };
        out.points = total.max(0.0).round() as u64;
        out
    }
}

/// Nodes read, with their points, evicted least recently used first.
#[derive(Debug)]
pub struct NodeCache<V> {
    entries: HashMap<usize, (V, u64, u64)>,
    clock: u64,
    points: u64,
}

impl<V> Default for NodeCache<V> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            points: 0,
        }
    }
}

impl<V> NodeCache<V> {
    /// Starts a frame: what is used from now on is newer.
    pub fn tick(&mut self) {
        self.clock += 1;
    }

    /// Keeps node `node` with its `points`.
    pub fn insert(&mut self, node: usize, value: V, points: u64) {
        if let Some((_, old, _)) = self.entries.insert(node, (value, points, self.clock)) {
            self.points -= old;
        }
        self.points += points;
    }

    /// Whether node `node` is in.
    pub fn contains(&self, node: usize) -> bool {
        self.entries.contains_key(&node)
    }

    /// Node `node`, marked used now.
    pub fn get(&mut self, node: usize) -> Option<&V> {
        let clock = self.clock;
        self.entries.get_mut(&node).map(|e| {
            e.2 = clock;
            &e.0
        })
    }

    /// Node `node`, not marked.
    pub fn peek(&self, node: usize) -> Option<&V> {
        self.entries.get(&node).map(|e| &e.0)
    }

    /// Points held.
    pub fn points(&self) -> u64 {
        self.points
    }

    /// Nodes held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drops everything.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.points = 0;
    }

    /// Drops the least recently used nodes, not those of `keep`, until it
    /// holds at most `capacity` points; returns the nodes dropped.
    pub fn evict(&mut self, capacity: u64, keep: &HashSet<usize>) -> Vec<usize> {
        if self.points <= capacity {
            return Vec::new();
        }
        let mut old: Vec<(u64, usize)> = self
            .entries
            .iter()
            .filter(|(k, _)| !keep.contains(k))
            .map(|(k, e)| (e.2, *k))
            .collect();
        old.sort_unstable();
        let mut out = Vec::new();
        for (_, k) in old {
            if self.points <= capacity {
                break;
            }
            if let Some((_, p, _)) = self.entries.remove(&k) {
                self.points -= p;
                out.push(k);
            }
        }
        out
    }
}

/// Reads the points of a cloud's nodes.
pub trait LodReader: Send {
    /// The nodes.
    fn hierarchy(&self) -> &Arc<Hierarchy>;
    /// Calls `visit(x, y, z, class)` for the points of node `node` of
    /// `classes` (all without them); the classes `whole` with every point
    /// where the source can (a stored cloud).
    fn load(
        &mut self,
        node: usize,
        classes: Option<&[u8]>,
        whole: &[u8],
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String>;
    /// Another reader of the same cloud (for another thread), sharing the
    /// hierarchy.
    fn another(&self) -> Result<Box<dyn LodReader>, String>;
    /// In words, for the status line.
    fn describe(&self) -> String;
}

/// Opens the cloud of `spec` for reading by node: a COPC file, or a
/// project file's cloud (`FILE.ols`, `FILE.ols#name`).
///
/// Cost: a COPC file's hierarchy, or one query over a stored cloud's
/// block index (~0.4 s per 10⁵ blocks — 10⁹ points are some 10⁵).
pub fn open_lod(spec: &str) -> Result<Box<dyn LodReader>, String> {
    let (file, name) = ols_lidar::cloud_spec(spec);
    if crate::copc::is_copc(file) {
        return Ok(Box::new(CopcLod::open(file)?));
    }
    Ok(Box::new(StoreLod::open(file, name)?))
}

/// A COPC file read by node: its octree as it is.
pub struct CopcLod {
    file: Arc<CopcFile>,
    hierarchy: Arc<Hierarchy>,
    reader: Option<std::io::BufReader<std::fs::File>>,
}

impl CopcLod {
    /// Opens a COPC file.
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = CopcFile::open(path)?;
        let hierarchy = copc_hierarchy(&file);
        Ok(Self {
            file: Arc::new(file),
            hierarchy: Arc::new(hierarchy),
            reader: None,
        })
    }
}

/// The hierarchy of a COPC file: a node per octree node with points, its
/// parent the nearest ancestor with points.
pub fn copc_hierarchy(file: &CopcFile) -> Hierarchy {
    let index: HashMap<VoxelKey, usize> = file
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key, i))
        .collect();
    let [lo, hi] = file.bounds;
    let mut nodes: Vec<LodNode> = file
        .nodes
        .iter()
        .map(|n| {
            let mut b = file.node_bounds(&n.key);
            // The cube cut to the points' bounds (the header's): tighter
            // distances and culling.
            if lo.iter().zip(&hi).all(|(a, b)| a <= b) {
                for k in 0..3 {
                    b[0][k] = b[0][k].max(lo[k]).min(hi[k]);
                    b[1][k] = b[1][k].min(hi[k]).max(b[0][k]);
                }
            }
            // Spacing from the node's plan area and points (survey points
            // lie on a surface): writers differ in what the COPC info's
            // spacing means.
            let area = ((b[1][0] - b[0][0]) * (b[1][1] - b[0][1])).max(1e-6);
            LodNode {
                bounds: b,
                points: n.points,
                spacing: (area / n.points.max(1) as f64).sqrt(),
                depth: n.key.depth.max(0) as u32,
                parent: None,
                children: Vec::new(),
            }
        })
        .collect();
    let mut roots = Vec::new();
    for (i, n) in file.nodes.iter().enumerate() {
        let mut up = n.key.parent();
        let parent = loop {
            match up {
                None => break None,
                Some(k) => match index.get(&k) {
                    Some(&p) => break Some(p),
                    None => up = k.parent(),
                },
            }
        };
        nodes[i].parent = parent;
        match parent {
            Some(p) => nodes[p].children.push(i),
            None => roots.push(i),
        }
    }
    Hierarchy {
        nodes,
        roots,
        refine: Refine::Add,
    }
}

impl LodReader for CopcLod {
    fn hierarchy(&self) -> &Arc<Hierarchy> {
        &self.hierarchy
    }

    fn load(
        &mut self,
        node: usize,
        classes: Option<&[u8]>,
        _whole: &[u8],
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        budget
            .start()
            .check()
            .map_err(|stop| format!("stopped: {stop}"))?;
        let n = *self
            .file
            .nodes
            .get(node)
            .ok_or_else(|| format!("no node {node}"))?;
        if self.reader.is_none() {
            self.reader = Some(self.file.reader()?);
        }
        let reader = self.reader.as_mut().expect("opened");
        let wanted = crate::copc::wanted_classes(classes);
        let mut stats = Stats {
            read: 1,
            ..Stats::default()
        };
        self.file.read_node(reader, &n, &mut |p| {
            stats.points_read += 1;
            if wanted[p.class as usize] {
                stats.points_visited += 1;
                visit(p.x, p.y, p.z, p.class);
            }
        })?;
        Ok(stats)
    }

    fn another(&self) -> Result<Box<dyn LodReader>, String> {
        Ok(Box::new(Self {
            file: self.file.clone(),
            hierarchy: self.hierarchy.clone(),
            reader: None,
        }))
    }

    fn describe(&self) -> String {
        format!(
            "COPC {} — {} points in {} nodes, depth {}",
            self.file
                .path
                .file_name()
                .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
            self.file.points,
            self.file.nodes.len(),
            self.file.depth()
        )
    }
}

/// A project's cloud read by node: per square of [`NODE_SIZE`] (whole
/// chunks), a chain of its levels of detail — level 3 (about 1/64 of the
/// points) at depth 0, then 2, 1, and every point at depth 3.
pub struct StoreLod {
    path: PathBuf,
    store: Store,
    cloud: CloudInfo,
    hierarchy: Arc<Hierarchy>,
    /// Per node: its square (node x, y) and its level of detail.
    keys: Arc<Vec<([i64; 2], u8)>>,
    /// Chunks per node side.
    per: i64,
    /// Whether every block has its levels stored.
    pub stored_levels: bool,
}

impl StoreLod {
    /// Opens the cloud `name` (or the only one) of a project file.
    pub fn open(path: &Path, name: Option<&str>) -> Result<Self, String> {
        let store = Store::open(path).map_err(|e| e.to_string())?;
        let cloud = store.cloud(name).map_err(|e| e.to_string())?;
        let blocks = store.lod_blocks(&cloud).map_err(|e| e.to_string())?;
        let per = (NODE_SIZE / cloud.chunk_size).round().max(1.0) as i64;
        let stored_levels = blocks.iter().all(|b| b.levels.is_some());
        let (hierarchy, keys) = store_hierarchy(&cloud, &blocks, per);
        Ok(Self {
            path: path.to_path_buf(),
            store,
            cloud,
            hierarchy: Arc::new(hierarchy),
            keys: Arc::new(keys),
            per,
            stored_levels,
        })
    }
}

/// The hierarchy of a stored cloud from its blocks ([`StoreLod`]), and
/// per node its square and level.
pub fn store_hierarchy(
    cloud: &CloudInfo,
    blocks: &[crate::BlockLod],
    per: i64,
) -> (Hierarchy, Vec<([i64; 2], u8)>) {
    let levels = LOD_LEVELS as usize;
    // Per square: points per level (0 = all), lowest, highest, chunks.
    #[derive(Default)]
    struct Square {
        points: Vec<u64>,
        z: Option<[i64; 2]>,
        chunks: HashSet<(i64, i64)>,
    }
    let s = cloud.scale;
    let above = (ABOVE_BASE / s).round() as i64;
    let mut squares: std::collections::BTreeMap<[i64; 2], Square> = Default::default();
    for b in blocks {
        let key = [b.cx.div_euclid(per), b.cy.div_euclid(per)];
        let q = squares.entry(key).or_default();
        if q.points.is_empty() {
            q.points = vec![0; levels + 1];
        }
        q.points[0] += b.count;
        for k in 1..=levels {
            q.points[k] += match b.levels {
                Some(l) => l[k - 1],
                None => b.count >> (2 * k),
            };
        }
        let top = b.z1.unwrap_or(b.z0 + above);
        let z = q.z.get_or_insert([b.z0, top]);
        z[0] = z[0].min(b.z0);
        z[1] = z[1].max(top);
        q.chunks.insert((b.cx, b.cy));
    }
    let c = cloud.chunk_size;
    let mut nodes = Vec::with_capacity(squares.len() * (levels + 1));
    let mut keys = Vec::with_capacity(nodes.capacity());
    let mut roots = Vec::with_capacity(squares.len());
    for (key, q) in squares {
        let z = q.z.unwrap_or([0, 0]);
        let side = per as f64 * c;
        let bounds = [
            [key[0] as f64 * side, key[1] as f64 * side, z[0] as f64 * s],
            [
                (key[0] + 1) as f64 * side,
                (key[1] + 1) as f64 * side,
                z[1] as f64 * s,
            ],
        ];
        let area = q.chunks.len() as f64 * c * c;
        roots.push(nodes.len());
        for depth in 0..=levels {
            let level = levels - depth;
            let i = nodes.len();
            let points = q.points[level];
            nodes.push(LodNode {
                bounds,
                points,
                spacing: (area / points.max(1) as f64).sqrt(),
                depth: depth as u32,
                parent: (depth > 0).then(|| i - 1),
                children: if depth < levels {
                    vec![i + 1]
                } else {
                    Vec::new()
                },
            });
            keys.push((key, level as u8));
        }
    }
    (
        Hierarchy {
            nodes,
            roots,
            refine: Refine::Replace,
        },
        keys,
    )
}

impl LodReader for StoreLod {
    fn hierarchy(&self) -> &Arc<Hierarchy> {
        &self.hierarchy
    }

    fn load(
        &mut self,
        node: usize,
        classes: Option<&[u8]>,
        whole: &[u8],
        budget: &Budget,
        visit: &mut dyn FnMut(f64, f64, f64, u8),
    ) -> Result<Stats, String> {
        let (key, level) = *self
            .keys
            .get(node)
            .ok_or_else(|| format!("no node {node}"))?;
        let min = [key[0] * self.per, key[1] * self.per];
        let max = [min[0] + self.per - 1, min[1] + self.per - 1];
        self.store
            .visit_level(&self.cloud, min, max, level, classes, whole, budget, visit)
            .map_err(|e| e.to_string())
    }

    fn another(&self) -> Result<Box<dyn LodReader>, String> {
        Ok(Box::new(Self {
            path: self.path.clone(),
            store: Store::open(&self.path).map_err(|e| e.to_string())?,
            cloud: self.cloud.clone(),
            hierarchy: self.hierarchy.clone(),
            keys: self.keys.clone(),
            per: self.per,
            stored_levels: self.stored_levels,
        }))
    }

    fn describe(&self) -> String {
        format!(
            "\"{}\" — {} points in {} squares of {:.0} m{}",
            self.cloud.name,
            self.cloud.points,
            self.hierarchy.roots.len(),
            self.per as f64 * self.cloud.chunk_size,
            if self.stored_levels {
                ""
            } else {
                " (levels of detail not stored: run `ohl pointcloud octree`)"
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chain hierarchy: `n` squares in a row along x, 100 m apart, each
    /// 4 levels of 64, 256, 1024, 4096 points over 10 000 m².
    fn chains(n: usize) -> Hierarchy {
        let cloud = CloudInfo {
            id: 1,
            name: "t".into(),
            crs: String::new(),
            scale: 0.001,
            chunk_size: 100.0,
            class_scheme: None,
            points: 0,
            bounds: [[0.0; 3]; 2],
        };
        let blocks: Vec<crate::BlockLod> = (0..n as i64)
            .map(|k| crate::BlockLod {
                cx: k,
                cy: 0,
                class: 2,
                count: 4096,
                z0: 0,
                z1: Some(10_000),
                levels: Some([1024, 256, 64]),
            })
            .collect();
        store_hierarchy(&cloud, &blocks, 1).0
    }

    fn view(eye: [f64; 3]) -> impl Fn(&Box3) -> f64 {
        move |b: &Box3| {
            let d = [0, 1, 2].map(|k| (b[0][k] - eye[k]).max(eye[k] - b[1][k]).max(0.0));
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        }
    }

    #[test]
    fn the_budget_is_respected_and_the_nearest_refined_first() {
        let h = chains(20);
        assert_eq!(h.points(), 20 * 4096);
        let distance = view([-50.0, 50.0, 5.0]);
        let sees = |_: &Box3| true;
        let v = LodView {
            distance: &distance,
            sees: &sees,
            pixels_per_radian: 1000.0,
            render_distance: 1e9,
        };
        // Enough for everything: every square at full detail.
        let all = h.select(
            &v,
            &LodLimits {
                point_budget: 1_000_000,
                max_error: 0.0,
                share: 1.0,
            },
        );
        assert_eq!(all.points, 20 * 4096);
        assert!(!all.capped);
        assert_eq!(all.draw.len(), 20);
        assert!(all.draw.iter().all(|&i| h.nodes[i].children.is_empty()));
        // A tight budget: every square drawn, the near ones finer.
        let limits = LodLimits {
            point_budget: 20_000,
            max_error: 0.0,
            share: 1.0,
        };
        let s = h.select(&v, &limits);
        assert!(s.capped);
        assert!(s.points <= 20_000, "{}", s.points);
        assert_eq!(s.draw.len(), 20, "one level per square");
        let depth_of = |sq: usize| {
            s.draw
                .iter()
                .find(|&&i| h.roots[sq] <= i && i < h.roots[sq] + 4)
                .map(|&i| h.nodes[i].depth)
                .unwrap()
        };
        assert_eq!(depth_of(0), 3, "the nearest at full detail");
        assert_eq!(depth_of(19), 0, "the farthest coarsest");
        assert!((0..19).all(|k| depth_of(k) >= depth_of(k + 1)));
        // Points counted at the share shown.
        let half = h.select(
            &v,
            &LodLimits {
                share: 0.5,
                ..limits
            },
        );
        assert!(half.points <= 20_000);
        assert!(
            half.draw.iter().map(|&i| h.nodes[i].depth).sum::<u32>()
                > s.draw.iter().map(|&i| h.nodes[i].depth).sum::<u32>()
        );
    }

    #[test]
    fn the_error_stops_refinement_and_the_view_culls() {
        let h = chains(5);
        let distance = view([250.0, 50.0, 5000.0]);
        let sees = |b: &Box3| b[0][0] < 300.0;
        let v = LodView {
            distance: &distance,
            sees: &sees,
            pixels_per_radian: 1000.0,
            render_distance: 1e9,
        };
        // 5 km away: spacing 12.5 m at depth 0 is 2.5 px, 6.25 m 1.25 px.
        let s = h.select(
            &v,
            &LodLimits {
                point_budget: u64::MAX,
                max_error: 2.0,
                share: 1.0,
            },
        );
        assert!(!s.capped);
        assert_eq!(s.draw.len(), 3, "squares 0, 1 and 2 in view");
        assert!(s.draw.iter().all(|&i| h.nodes[i].depth == 1));
        // Beyond the render distance: nothing.
        let far = LodView {
            render_distance: 1000.0,
            ..v
        };
        assert!(
            h.select(
                &far,
                &LodLimits {
                    point_budget: u64::MAX,
                    max_error: 2.0,
                    share: 1.0
                }
            )
            .nodes
            .is_empty()
        );
    }

    #[test]
    fn the_cache_evicts_the_least_recently_used() {
        let mut c: NodeCache<&str> = NodeCache::default();
        c.insert(1, "a", 100);
        c.tick();
        c.insert(2, "b", 100);
        c.tick();
        c.insert(3, "c", 100);
        c.tick();
        assert_eq!(c.get(1), Some(&"a"), "1 used again: now the newest");
        assert_eq!(c.points(), 300);
        // Room for 150 points, node 3 kept (in view): 2 goes, then 1.
        let keep: HashSet<usize> = [3].into();
        assert_eq!(c.evict(250, &keep), vec![2]);
        assert_eq!(c.evict(150, &keep), vec![1]);
        assert_eq!(c.points(), 100);
        assert!(c.contains(3) && !c.contains(1));
        // What is kept stays, even over the capacity.
        assert!(c.evict(0, &keep).is_empty());
        c.insert(3, "c2", 40);
        assert_eq!(c.points(), 40);
    }
}
