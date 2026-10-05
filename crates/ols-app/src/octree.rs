// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! LiDAR points streamed by node under a point budget: the project's
//! point cloud (its levels of detail per 256 m square) or a COPC file (its
//! octree), read on background threads as the view needs them
//! ([`ols_pointcloud::octree`]).
//!
//! Every frame the nodes in view are picked — the coarsest first, refined
//! where their points lie farther apart on the screen than the error
//! allowed, nearest first — until the point budget is used; the missing
//! ones are queued (in that order, the queue replaced every frame, so it
//! never lags behind the camera) and read by a few threads, each with
//! its own reader. Nodes read are kept on the GPU, the least recently
//! used dropped beyond 1.5 × the budget. A node not read yet is drawn at
//! a coarser (or finer) level already there.

use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use ols_pointcloud::octree::{
    Hierarchy, LodLimits, LodReader, LodView, NodeCache, Refine, Selection,
};
use ols_render::Scene;

/// What the readers apply to the points of a node.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadSettings {
    /// Classes read (`None`: all).
    pub classes: Option<Vec<u8>>,
    /// Classes kept whole (small ones; where the source can).
    pub whole: Vec<u8>,
    /// Every n-th point of the others kept (the share shown).
    pub every: u64,
    /// Colour per class (`None`: not drawn).
    pub colors: Arc<Vec<Option<[u8; 3]>>>,
}

/// A node read.
struct Done {
    node: usize,
    generation: u64,
    scene: Result<(Scene, u64), String>,
}

/// The queue the readers take from.
#[derive(Default)]
struct Queue {
    /// Nodes to read, in order, with the settings.
    wanted: VecDeque<usize>,
    /// Nodes being read.
    busy: HashSet<usize>,
    settings: Option<(u64, Arc<LoadSettings>)>,
    closed: bool,
}

/// What the streaming did in the last frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Nodes of the cloud.
    pub nodes: usize,
    /// Nodes picked for the view.
    pub selected: usize,
    /// Nodes drawn.
    pub drawn: usize,
    /// Points drawn.
    pub points: u64,
    /// Points the selection asks for.
    pub wanted: u64,
    /// Nodes waiting or being read.
    pub pending: usize,
    /// Nodes on the GPU, and their points.
    pub cached: (usize, u64),
    /// Whether the budget stopped the refinement.
    pub capped: bool,
}

/// A point cloud streamed by node.
pub struct PointOctree {
    hierarchy: Arc<Hierarchy>,
    /// The source, in words.
    pub describe: String,
    queue: Arc<(Mutex<Queue>, Condvar)>,
    rx: Receiver<Done>,
    generation: u64,
    settings: Option<LoadSettings>,
    /// Nodes that failed to read (not asked again until the settings
    /// change).
    failed: HashSet<usize>,
    /// What the last frame did.
    pub stats: StreamStats,
}

impl std::fmt::Debug for PointOctree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointOctree")
            .field("describe", &self.describe)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

impl Drop for PointOctree {
    fn drop(&mut self) {
        let (lock, cv) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.closed = true;
            q.wanted.clear();
        }
        cv.notify_all();
    }
}

/// Reader threads: half the cores, 1 to 4 (decoding a node is single
/// threaded; the rest is left to the view and the other work).
fn reader_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4))
}

impl PointOctree {
    /// Opens the cloud of `spec` (a project file's cloud or a COPC file)
    /// and starts its readers; points are drawn relative to `origin`.
    /// `ctx` is asked to repaint when a node comes in.
    pub fn open(
        spec: &str,
        origin: [f64; 3],
        ctx: Option<eframe::egui::Context>,
    ) -> Result<Self, String> {
        let first = ols_pointcloud::octree::open_lod(spec)?;
        let hierarchy = first.hierarchy().clone();
        let describe = first.describe();
        let queue: Arc<(Mutex<Queue>, Condvar)> = Default::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut readers = vec![first];
        for _ in 1..reader_threads() {
            readers.push(readers[0].another()?);
        }
        for reader in readers {
            let queue = queue.clone();
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || read_nodes(reader, &queue, &tx, origin, ctx.as_ref()));
        }
        log::info!("LiDAR streamed by node: {describe}");
        Ok(Self {
            hierarchy,
            describe,
            queue,
            rx,
            generation: 0,
            settings: None,
            failed: HashSet::new(),
            stats: StreamStats::default(),
        })
    }

    /// The nodes.
    #[cfg(test)]
    pub fn hierarchy(&self) -> &Hierarchy {
        &self.hierarchy
    }

    /// Sets what the readers keep; other settings than before: what was
    /// read goes (returns true: the caller empties its cache).
    pub fn set(&mut self, settings: LoadSettings) -> bool {
        if self.settings.as_ref() == Some(&settings) {
            return false;
        }
        self.generation += 1;
        self.failed.clear();
        let (lock, _) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.wanted.clear();
            q.settings = Some((self.generation, Arc::new(settings.clone())));
        }
        self.settings = Some(settings);
        true
    }

    /// One frame: takes the nodes read since the last one into `cache`
    /// (`make` uploads them), picks the nodes for `view` within `limits`,
    /// queues the missing ones and drops old ones from the cache. Returns
    /// the nodes to draw (in the cache).
    pub fn frame<V>(
        &mut self,
        cache: &mut NodeCache<V>,
        view: &LodView<'_>,
        limits: &LodLimits,
        mut make: impl FnMut(&Scene) -> V,
    ) -> Vec<usize> {
        cache.tick();
        let selection = self.hierarchy.select(view, limits);
        let (lock, cv) = &*self.queue;
        let pending = {
            let Ok(mut q) = lock.lock() else {
                return Vec::new();
            };
            // Under the lock: a node is either back here or still busy.
            while let Ok(d) = self.rx.try_recv() {
                if d.generation != self.generation {
                    continue;
                }
                match d.scene {
                    Ok((scene, points)) => cache.insert(d.node, make(&scene), points),
                    Err(e) => {
                        log::warn!("LiDAR node {}: {e}", d.node);
                        self.failed.insert(d.node);
                    }
                }
            }
            q.wanted = selection
                .nodes
                .iter()
                .copied()
                .filter(|i| !cache.contains(*i) && !q.busy.contains(i) && !self.failed.contains(i))
                .collect();
            q.wanted.len() + q.busy.len()
        };
        if pending > 0 {
            cv.notify_all();
        }
        let draw = self.drawn(cache, &selection);
        let mut keep: HashSet<usize> = selection.nodes.iter().copied().collect();
        keep.extend(draw.iter().copied());
        // Room for 1.5 × the budget: turning back finds nodes still there.
        let capacity = limits.point_budget.saturating_mul(3) / 2;
        cache.evict(capacity, &keep);
        self.stats = StreamStats {
            nodes: self.hierarchy.nodes.len(),
            selected: selection.nodes.len(),
            drawn: draw.len(),
            points: draw
                .iter()
                .map(|&i| self.hierarchy.nodes[i].points)
                .sum::<u64>(),
            wanted: selection.points,
            pending,
            cached: (cache.len(), cache.points()),
            capped: selection.capped,
        };
        if let Some(s) = &self.settings {
            // Points drawn, at the share shown.
            self.stats.points /= s.every.max(1);
        }
        draw
    }

    /// The nodes to draw: those picked that are read; for one not read
    /// yet, a coarser one of its chain read (or a finer one) meanwhile.
    fn drawn<V>(&self, cache: &mut NodeCache<V>, selection: &Selection) -> Vec<usize> {
        let h = &self.hierarchy;
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for &i in &selection.draw {
            let mut pick = None;
            if cache.contains(i) {
                pick = Some(i);
            } else if h.refine == Refine::Replace {
                let mut up = h.nodes[i].parent;
                while let Some(p) = up {
                    if cache.contains(p) {
                        pick = Some(p);
                        break;
                    }
                    up = h.nodes[p].parent;
                }
                if pick.is_none() {
                    let mut down = h.nodes[i].children.first().copied();
                    while let Some(c) = down {
                        if cache.contains(c) {
                            pick = Some(c);
                            break;
                        }
                        down = h.nodes[c].children.first().copied();
                    }
                }
            }
            if let Some(p) = pick
                && seen.insert(p)
            {
                cache.get(p);
                out.push(p);
            }
        }
        out
    }
}

/// A reader thread: takes the first node wanted, reads it, sends it back.
fn read_nodes(
    mut reader: Box<dyn LodReader>,
    queue: &(Mutex<Queue>, Condvar),
    tx: &Sender<Done>,
    origin: [f64; 3],
    ctx: Option<&eframe::egui::Context>,
) {
    let (lock, cv) = queue;
    // Exit clause: a node is ~10⁵ points (~10 ms); a minute is far beyond.
    let budget = ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(60));
    loop {
        let (node, generation, settings) = {
            let Ok(mut q) = lock.lock() else {
                return;
            };
            loop {
                if q.closed {
                    return;
                }
                if let Some((generation, settings)) = q.settings.clone()
                    && let Some(node) = q.wanted.pop_front()
                {
                    q.busy.insert(node);
                    break (node, generation, settings);
                }
                q = match cv.wait(q) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
        };
        let mut scene = Scene::new(origin);
        let mut keep = ols_pointcloud::Every::new(settings.every, &settings.whole);
        let mut kept = 0u64;
        let result = reader
            .load(
                node,
                settings.classes.as_deref(),
                &settings.whole,
                &budget,
                &mut |x, y, z, c| {
                    if keep.keep(c)
                        && let Some(col) = settings.colors[c as usize]
                    {
                        kept += 1;
                        scene.point([x, y, z], [col[0], col[1], col[2], 255], 2.0);
                    }
                },
            )
            .map(|_| (scene, kept));
        {
            let Ok(mut q) = lock.lock() else {
                return;
            };
            q.busy.remove(&node);
            let sent = tx.send(Done {
                node,
                generation,
                scene: result,
            });
            if sent.is_err() {
                return;
            }
        }
        if let Some(c) = ctx {
            c.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic cloud far larger than the budget, streamed from a
    /// view above its middle: what is drawn stays within the budget, the
    /// cache within 1.5 × of it, and moving away loads new nodes and
    /// drops the oldest.
    #[test]
    fn a_large_cloud_stays_within_the_budget() {
        let dir = std::env::temp_dir().join(format!("ols-app-{}-octree", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("p.ols");
        std::fs::remove_file(&db).ok();
        // 1 km × 1 km at 0.5 m: 4 million points.
        let points = (0..2000).flat_map(|i| {
            (0..2000).map(move |j| ols_lidar::LidarPoint {
                x: 2_600_000.0 + f64::from(i) * 0.5,
                y: 1_200_000.0 + f64::from(j) * 0.5,
                z: 500.0 + f64::from((i + j) % 7),
                class: if (i + j) % 3 == 0 { 5 } else { 2 },
                intensity: 0,
                return_number: 1,
                number_of_returns: 1,
            })
        });
        let mut store = ols_pointcloud::Store::open(&db).unwrap();
        store
            .import_points(
                "big",
                "",
                None,
                64.0,
                0.001,
                points,
                &ols_core::budget::Budget::new(),
            )
            .unwrap();
        drop(store);
        let mut stream = PointOctree::open(
            &db.display().to_string(),
            [2_600_000.0, 1_200_000.0, 0.0],
            None,
        )
        .unwrap();
        assert_eq!(stream.hierarchy().points(), 4_000_000);
        stream.set(LoadSettings {
            classes: None,
            whole: Vec::new(),
            every: 1,
            colors: Arc::new(vec![Some([200, 200, 200]); 256]),
        });
        let budget = 300_000;
        let limits = LodLimits {
            point_budget: budget,
            max_error: 0.5,
            share: 1.0,
        };
        let mut cache: NodeCache<u64> = NodeCache::default();
        let run = |stream: &mut PointOctree, cache: &mut NodeCache<u64>, eye: [f64; 3]| {
            let distance = move |b: &ols_pointcloud::octree::Box3| {
                let d = [0, 1, 2].map(|k| (b[0][k] - eye[k]).max(eye[k] - b[1][k]).max(0.0));
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            };
            let sees = |_: &ols_pointcloud::octree::Box3| true;
            let view = LodView {
                distance: &distance,
                sees: &sees,
                pixels_per_radian: 1000.0,
                render_distance: 1e9,
            };
            let mut drawn = Vec::new();
            for _ in 0..2000 {
                drawn = stream.frame(cache, &view, &limits, |s| s.counts().1 as u64);
                if stream.stats.pending == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert_eq!(stream.stats.pending, 0, "every node read");
            drawn.iter().map(|&i| *cache.peek(i).unwrap()).sum::<u64>()
        };
        let shown = run(&mut stream, &mut cache, [2_600_500.0, 1_200_500.0, 600.0]);
        assert!(stream.stats.capped, "the budget limits the view");
        assert!(shown <= budget, "{shown} points drawn");
        assert!(shown > budget / 2, "{shown} points drawn");
        assert!(cache.points() <= budget * 3 / 2);
        let first: HashSet<usize> = (0..stream.hierarchy().nodes.len())
            .filter(|&i| cache.contains(i))
            .collect();
        // To a corner, closer: other nodes, still within the budget.
        let shown = run(&mut stream, &mut cache, [2_600_050.0, 1_200_050.0, 520.0]);
        assert!(shown <= budget, "{shown} points drawn");
        assert!(cache.points() <= budget * 3 / 2, "{}", cache.points());
        let now: HashSet<usize> = (0..stream.hierarchy().nodes.len())
            .filter(|&i| cache.contains(i))
            .collect();
        assert!(now.difference(&first).count() > 0, "new nodes read");
        assert!(first.difference(&now).count() > 0, "old nodes dropped");
        drop(stream);
        std::fs::remove_dir_all(dir).ok();
    }
}
