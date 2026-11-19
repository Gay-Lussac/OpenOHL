// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tile packager of a network's server (plan §5.2): threads making
//! the light, read-only context of a tile — per structure its id,
//! position, orientation, type, height and a simplified stick; per section
//! its wires solved in the context case; the alignments as polylines; the
//! routes' names — from **the part of each model's line the tile reads**,
//! taken by the authority when the tile is asked for
//! ([`ohl_session::Session::drawing_part`]: the tile's objects and what
//! solving each of its sections alone reads; never a copy of the whole
//! line, phase 6), so the authority goes on meanwhile.
//!
//! Kept between packages, each bounded (the server's settings):
//!
//! - **solved section shapes** ([`Caches::shapes`], least recently used
//!   first beyond a size in MB): keyed like the solve memo's
//!   (`ohl_line::solve::memo`) — a 128-bit fingerprint of everything the
//!   solve reads: the section, its structures, the sections ending at
//!   them, its spacers, the line's direction at its middle, the line's
//!   settings, the wire type and the weather case — so an edit anywhere
//!   else keeps it, and any change of what it reads is another key;
//! - **resolved structures**: per model and library revision, the
//!   libraries read once and the structures merged with their types kept
//!   (`ohl_line::library::ResolveCache`, the application's idea; least
//!   recently used models first beyond a count); a tile's part is resolved
//!   from it (what it holds only).
//!
//! The context's wires are a **drawing**, not a result: each section is
//! solved alone in the context case (`ohl_line::solve::solve_one`) — the
//! jumpers' loads, the coupled solve of sections sharing an assembly and
//! the structures' flexibility are left out (the checks and OHL-Design
//! take them); a span is thinned to [`crate::web::SCENE_SPAN_POINTS`]
//! points at millimetres. Each section is solved on **the part of its
//! line it reads** ([`sub_line`]: its structures, the sections and
//! jumpers at them, its spacers, the alignments near it, the line's
//! settings), so a solve costs the same in a line of a hundred structures
//! as in a model of the whole country — the same shape as on the whole
//! line, bit for bit (tested on the example lines and a synthetic
//! country).

use std::collections::{BTreeMap, HashMap};
use std::fmt::{Debug, Write as _};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ohl_core::grid::Bounds;
use ohl_line::{Line, Structure};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::server::Job;
use crate::{
    ContextLabel, ContextRoute, ContextStructure, ContextWire, Position, SceneSpan, ServerMsg,
    TileKey, TilePackage,
};

/// A least-recently-used store bounded in bytes (or entries).
pub(crate) struct Lru<K, V> {
    map: HashMap<K, (u64, usize, V)>,
    order: BTreeMap<u64, K>,
    clock: u64,
    bytes: usize,
    max_bytes: usize,
    max_entries: usize,
    /// Taken from it / asked.
    pub(crate) hits: u64,
    pub(crate) asked: u64,
}

impl<K: Clone + Eq + Hash, V: Clone> Lru<K, V> {
    pub(crate) fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: BTreeMap::new(),
            clock: 0,
            bytes: 0,
            max_bytes,
            max_entries: max_entries.max(1),
            hits: 0,
            asked: 0,
        }
    }

    pub(crate) fn get(&mut self, k: &K) -> Option<V> {
        self.asked += 1;
        self.clock += 1;
        let clock = self.clock;
        let e = self.map.get_mut(k)?;
        self.order.remove(&e.0);
        e.0 = clock;
        self.order.insert(clock, k.clone());
        self.hits += 1;
        Some(e.2.clone())
    }

    pub(crate) fn put(&mut self, k: K, v: V, bytes: usize) {
        self.clock += 1;
        if let Some((tick, b, _)) = self.map.insert(k.clone(), (self.clock, bytes, v)) {
            self.order.remove(&tick);
            self.bytes -= b;
        }
        self.order.insert(self.clock, k);
        self.bytes += bytes;
        while self.map.len() > self.max_entries
            || (self.bytes > self.max_bytes && self.map.len() > 1)
        {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            if let Some((_, b, _)) = self.map.remove(&oldest) {
                self.bytes -= b;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.bytes = 0;
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

/// A 128-bit fingerprint of values' `Debug` text (as the solve memo's
/// keys: every field, the numbers exactly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Key(u64, u64);

struct Fingerprint(DefaultHasher, DefaultHasher);

impl std::fmt::Write for Fingerprint {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        self.1.write(s.as_bytes());
        Ok(())
    }
}

pub(crate) fn key(parts: &[&dyn Debug]) -> Key {
    let mut f = Fingerprint(DefaultHasher::new(), DefaultHasher::new());
    f.1.write_u8(0xA5);
    for p in parts {
        let _ = write!(f, "{p:?}\u{1f}");
    }
    Key(f.0.finish(), f.1.finish())
}

/// A model's library files a package is made with (by kind).
#[derive(Debug, Default)]
pub(crate) struct LibFiles {
    pub(crate) files: BTreeMap<String, Vec<(String, String)>>,
}

/// One model's part of a tile.
pub(crate) struct Part {
    pub(crate) model: String,
    /// The part of the model's line the tile reads, at the model's
    /// revision ([`ohl_session::Session::drawing_part`]).
    pub(crate) line: Arc<Line>,
    /// A structure of the model has a type (the structure libraries are
    /// read).
    pub(crate) typed: bool,
    pub(crate) libs_rev: u64,
    pub(crate) libs: Arc<LibFiles>,
    pub(crate) structures: Vec<String>,
    pub(crate) sections: Vec<String>,
    pub(crate) alignments: Vec<String>,
}

/// A tile to make.
pub(crate) struct BuildJob {
    pub(crate) tile: TileKey,
    pub(crate) rev: Position,
    pub(crate) bounds: Bounds,
    pub(crate) parts: Vec<Part>,
    pub(crate) case: Arc<ohl_weather::WeatherCase>,
    /// Threads solving its sections side by side.
    pub(crate) threads: usize,
}

/// A tile made.
pub(crate) struct Built {
    pub(crate) tile: TileKey,
    pub(crate) rev: Position,
    pub(crate) text: Utf8Bytes,
    pub(crate) seconds: f64,
    /// Sections solved / taken from the cache.
    pub(crate) solved: usize,
    pub(crate) cached: usize,
}

/// A model's libraries read, and its structures merged with their types.
struct Libs {
    cables: ohl_cables::Library,
    types: Option<(
        ohl_structures::StructureLibrary,
        ohl_structures::InsulatorLibrary,
    )>,
    cache: ohl_line::library::ResolveCache,
    notes: Vec<String>,
}

/// A model's line resolved at a revision, with its lookups.
struct Resolved {
    line: Arc<Line>,
    structures: HashMap<String, usize>,
    sections: HashMap<String, usize>,
    alignments: HashMap<String, usize>,
    points: HashMap<String, usize>,
    routes: HashMap<String, usize>,
    /// Structure → the sections with a point on it.
    at_structure: HashMap<String, Vec<usize>>,
    /// Section → its spacers.
    spacers: HashMap<String, Vec<usize>>,
    /// Structure → the jumpers ending or held at it.
    jumpers: HashMap<String, Vec<usize>>,
    /// Each alignment's plan bounds (its points), by place.
    alignment_bounds: Vec<Option<Bounds>>,
    /// The line's settings (everything but its elements).
    settings: Key,
    notes: Vec<String>,
}

/// A section's spans in the context case, or why it did not solve.
type Shape = Arc<(Vec<SceneSpan>, Option<String>)>;

/// A section's spans, why it did not solve, whether from the cache.
type Solved = (Vec<SceneSpan>, Option<String>, bool);

/// What the packagers keep between packages.
pub(crate) struct Caches {
    pub(crate) shapes: Mutex<Lru<Key, Shape>>,
    libs: Mutex<Lru<(String, u64, bool), Arc<Libs>>>,
}

impl Caches {
    pub(crate) fn new(shapes_mb: usize, models: usize) -> Self {
        Self {
            shapes: Mutex::new(Lru::new(shapes_mb << 20, usize::MAX)),
            libs: Mutex::new(Lru::new(usize::MAX, models)),
        }
    }

    /// Entries and estimated bytes of the solved shapes.
    pub(crate) fn shapes_size(&self) -> (usize, usize, u64, u64) {
        let s = self.shapes.lock().unwrap_or_else(|e| e.into_inner());
        (s.len(), s.bytes(), s.hits, s.asked)
    }

    pub(crate) fn clear(&self) {
        self.shapes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.libs.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// The packagers: threads taking [`BuildJob`]s.
pub(crate) struct Packager {
    tx: Option<std_mpsc::Sender<BuildJob>>,
    threads: Vec<std::thread::JoinHandle<()>>,
    pub(crate) caches: Arc<Caches>,
}

impl Packager {
    /// `n` threads giving what they make to the authority (`jobs`); their
    /// library files written under `work` to be read.
    pub(crate) fn start(
        n: usize,
        caches: Arc<Caches>,
        jobs: std_mpsc::Sender<Job>,
        work: PathBuf,
    ) -> Result<Self, String> {
        let (tx, rx) = std_mpsc::channel::<BuildJob>();
        let rx = Arc::new(Mutex::new(rx));
        let mut threads = Vec::new();
        for k in 0..n.max(1) {
            let (rx, jobs, caches) = (rx.clone(), jobs.clone(), caches.clone());
            let work = work.join(format!("packager-{k}"));
            threads.push(
                std::thread::Builder::new()
                    .name(format!("ohl-packager-{k}"))
                    .spawn(move || {
                        loop {
                            let job = {
                                let rx = rx.lock().unwrap_or_else(|e| e.into_inner());
                                rx.recv()
                            };
                            let Ok(job) = job else { break };
                            let built =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    build(&job, &caches, &work)
                                }))
                                .unwrap_or_else(|_| failed(&job));
                            if jobs.send(Job::Built(Box::new(built))).is_err() {
                                break;
                            }
                        }
                    })
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(Self {
            tx: Some(tx),
            threads,
            caches,
        })
    }

    pub(crate) fn send(&self, job: BuildJob) -> Result<(), String> {
        self.tx
            .as_ref()
            .ok_or("the packagers stopped")?
            .send(job)
            .map_err(|_| "the packagers stopped".to_owned())
    }
}

impl Drop for Packager {
    fn drop(&mut self) {
        self.tx = None;
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// A package saying it could not be made (a bug in the packager).
fn failed(job: &BuildJob) -> Built {
    let pkg = TilePackage {
        tile: job.tile,
        rev: job.rev,
        bounds: bounds4(&job.bounds),
        case: job.case.name.clone(),
        structures: Vec::new(),
        wires: Vec::new(),
        routes: Vec::new(),
        labels: Vec::new(),
        notes: vec!["this tile could not be made (an internal error of the server)".into()],
    };
    encode(pkg, 0.0, 0, 0)
}

fn bounds4(b: &Bounds) -> [f64; 4] {
    [b.min[0], b.min[1], b.max[0], b.max[1]]
}

fn encode(pkg: TilePackage, seconds: f64, solved: usize, cached: usize) -> Built {
    let (tile, rev) = (pkg.tile, pkg.rev);
    let text = serde_json::to_string(&ServerMsg::Tile(Box::new(pkg))).unwrap_or_else(|e| {
        serde_json::to_string(&ServerMsg::Notice {
            text: format!("tile {tile}: {e}"),
        })
        .unwrap_or_default()
    });
    Built {
        tile,
        rev,
        text: Utf8Bytes::from(text),
        seconds,
        solved,
        cached,
    }
}

/// Whether point `p` lies in `b` (its lower and left borders included).
fn inside(b: &Bounds, p: &[f64; 3]) -> bool {
    p[0] >= b.min[0] && p[0] < b.max[0] && p[1] >= b.min[1] && p[1] < b.max[1]
}

/// Centimetres (a stick drawing).
fn cm(p: &[f64; 3]) -> [f64; 3] {
    p.map(|x| (x * 100.0).round() / 100.0)
}

/// An empty tile's package (made by the authority itself).
pub(crate) fn empty(tile: TileKey, rev: Position, bounds: &Bounds, case: &str) -> Utf8Bytes {
    encode(
        TilePackage {
            tile,
            rev,
            bounds: bounds4(bounds),
            case: case.to_owned(),
            structures: Vec::new(),
            wires: Vec::new(),
            routes: Vec::new(),
            labels: Vec::new(),
            notes: Vec::new(),
        },
        0.0,
        0,
        0,
    )
    .text
}

/// Makes a tile's package.
pub(crate) fn build(job: &BuildJob, caches: &Caches, work: &std::path::Path) -> Built {
    let started = Instant::now();
    let mut pkg = TilePackage {
        tile: job.tile,
        rev: job.rev,
        bounds: bounds4(&job.bounds),
        case: job.case.name.clone(),
        structures: Vec::new(),
        wires: Vec::new(),
        routes: Vec::new(),
        labels: Vec::new(),
        notes: Vec::new(),
    };
    let (mut solved, mut cached) = (0, 0);
    for part in &job.parts {
        let libs = libs_of(part, caches, work);
        let r = resolved_of(part, &libs);
        for n in libs.notes.iter().chain(&r.notes) {
            let n = format!("{}: {n}", part.model);
            if !pkg.notes.contains(&n) {
                pkg.notes.push(n);
            }
        }
        let line = &*r.line;
        for id in &part.structures {
            if let Some(&k) = r.structures.get(id) {
                pkg.structures
                    .push(context_structure(&part.model, &line.structures[k]));
            }
        }
        // The sections' shapes, solved side by side (each alone).
        let sections: Vec<&ohl_line::Section> = part
            .sections
            .iter()
            .filter_map(|id| r.sections.get(id).map(|&k| &line.sections[k]))
            .collect();
        let threads = job.threads.clamp(1, sections.len().max(1));
        let shapes: Vec<Solved> = if threads == 1 {
            sections
                .iter()
                .map(|s| shape(part, &r, &libs, s, &job.case, caches))
                .collect()
        } else {
            let mut out: Vec<Option<Solved>> = vec![None; sections.len()];
            std::thread::scope(|scope| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let (sections, r, libs, case) = (&sections, &r, &libs, &job.case);
                        scope.spawn(move || {
                            sections
                                .iter()
                                .enumerate()
                                .skip(t)
                                .step_by(threads)
                                .map(|(i, s)| (i, shape(part, r, libs, s, case, caches)))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                for h in handles {
                    for (i, x) in h.join().unwrap_or_default() {
                        out[i] = Some(x);
                    }
                }
            });
            out.into_iter()
                .map(|x| {
                    x.unwrap_or_else(|| {
                        (
                            Vec::new(),
                            Some("not solved (an internal error of the server)".into()),
                            false,
                        )
                    })
                })
                .collect()
        };
        for (section, (spans, error, was_cached)) in sections.into_iter().zip(shapes) {
            if was_cached {
                cached += 1;
            } else {
                solved += 1;
            }
            pkg.wires.push(ContextWire {
                model: part.model.clone(),
                section: section.id.clone(),
                color: line.section_color(section),
                spans,
                error,
            });
        }
        for id in &part.alignments {
            let Some(&k) = r.alignments.get(id) else {
                continue;
            };
            let a = &line.alignments[k];
            let points: Vec<[f64; 3]> = a
                .points
                .iter()
                .filter_map(|p| r.points.get(p).map(|&i| line.alignment_points[i].position))
                .map(|p| crate::web::mm(&p))
                .collect();
            // Its name at its middle point, in the tile holding that point.
            if let Some(mid) = points.get(points.len() / 2)
                && inside(&job.bounds, mid)
            {
                let name = r
                    .routes
                    .get(&a.route)
                    .map(|&i| line.routes[i].name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| a.route.clone());
                pkg.labels.push(ContextLabel {
                    text: name,
                    at: *mid,
                });
            }
            // Its segments through the tile (runs of them).
            let mut run: Vec<[f64; 3]> = Vec::new();
            let mut runs: Vec<Vec<[f64; 3]>> = Vec::new();
            for w in points.windows(2) {
                let b = Bounds::new([w[0][0], w[0][1]], [w[1][0], w[1][1]]);
                if b.intersects(&job.bounds) {
                    if run.is_empty() {
                        run.push(w[0]);
                    }
                    run.push(w[1]);
                } else if !run.is_empty() {
                    runs.push(std::mem::take(&mut run));
                }
            }
            if !run.is_empty() {
                runs.push(run);
            }
            for points in runs {
                pkg.routes.push(ContextRoute {
                    model: part.model.clone(),
                    route: a.route.clone(),
                    alignment: a.id.clone(),
                    points,
                });
            }
        }
    }
    encode(pkg, started.elapsed().as_secs_f64(), solved, cached)
}

/// The model's libraries at its library revision (read once).
fn libs_of(part: &Part, caches: &Caches, work: &std::path::Path) -> Arc<Libs> {
    let k = (part.model.clone(), part.libs_rev, part.typed);
    if let Some(l) = caches
        .libs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&k)
    {
        return l;
    }
    use ohl_core::files::shipped_library;
    let mut notes = Vec::new();
    let project = |kind: &str| part.libs.files.get(kind).filter(|f| !f.is_empty());
    let mut cables = ohl_cables::Library::default();
    match project("cables") {
        Some(files) => {
            for (name, text) in files {
                if let Err(e) = cables.add_toml(std::path::Path::new(name), text) {
                    notes.push(format!("wire library {name}: {e}"));
                }
            }
        }
        // The shipped library, read once for every model.
        None => {
            static SHIPPED: std::sync::OnceLock<Result<ohl_cables::Library, String>> =
                std::sync::OnceLock::new();
            match SHIPPED.get_or_init(|| {
                ohl_cables::Library::load(&shipped_library("cables")).map_err(|e| e.to_string())
            }) {
                Ok(l) => cables = l.clone(),
                Err(e) => notes.push(format!("wire library: {e}")),
            }
        }
    }
    let types = if part.typed {
        let read = (|| -> Result<_, String> {
            let dir = |kind: &str| -> Result<PathBuf, String> {
                Ok(
                    crate::web::library_dir(part.libs.files.get(kind), work, kind)?
                        .unwrap_or_else(|| shipped_library(kind)),
                )
            };
            let structures = ohl_structures::StructureLibrary::load_dir(&dir("structures")?)
                .map_err(|e| e.to_string())?;
            let insulators = ohl_structures::InsulatorLibrary::load_dir(&dir("insulators")?)
                .map_err(|e| e.to_string())?;
            Ok((structures, insulators))
        })();
        let _ = std::fs::remove_dir_all(work);
        match read {
            Ok(t) => Some(t),
            Err(e) => {
                notes.push(format!("structure types: {e}"));
                None
            }
        }
    } else {
        None
    };
    let libs = Arc::new(Libs {
        cables,
        types,
        cache: ohl_line::library::ResolveCache::default(),
        notes,
    });
    caches
        .libs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(k, libs.clone(), 0);
    libs
}

/// The line's settings: every field but its elements (a new field of the
/// line must be listed here: it may change a solve).
fn settings_key(line: &Line) -> Key {
    let Line {
        format,
        version,
        name,
        crs,
        height_datum,
        alignment: _,
        routes: _,
        alignments: _,
        alignment_points: _,
        structures: _,
        sections: _,
        jumpers: _,
        spacers: _,
        systems,
        circuits,
        display,
        creep_stretch,
        bimetallic,
        solver,
        defaults,
        circuit_starts,
        structure_loads,
        structure_groups,
        electrical,
        reference,
        feet_state,
        condition_cases,
    } = line;
    key(&[
        format,
        version,
        name,
        crs,
        height_datum,
        systems,
        circuits,
        display,
        creep_stretch,
        bimetallic,
        solver,
        defaults,
        circuit_starts,
        structure_loads,
        structure_groups,
        electrical,
        reference,
        feet_state,
        condition_cases,
    ])
}

/// The tile's part of a model's line resolved (its structures merged with
/// their types, kept per model and library revision in `libs.cache`).
fn resolved_of(part: &Part, libs: &Libs) -> Resolved {
    let mut notes = Vec::new();
    let line: Arc<Line> = match &libs.types {
        Some((structures, insulators)) => {
            match part
                .line
                .resolve_with(structures, insulators, Some(&libs.cache))
            {
                Ok(l) => Arc::new(l),
                Err(e) => {
                    notes.push(format!("structure types: {e} (structures as drawn)"));
                    part.line.clone()
                }
            }
        }
        None => part.line.clone(),
    };
    indexed(line, notes)
}

/// A line resolved, with its lookups.
fn indexed(line: Arc<Line>, notes: Vec<String>) -> Resolved {
    let index = |ids: &mut dyn Iterator<Item = String>| -> HashMap<String, usize> {
        let mut m = HashMap::new();
        for (k, id) in ids.enumerate() {
            m.entry(id).or_insert(k);
        }
        m
    };
    let mut at_structure: HashMap<String, Vec<usize>> = HashMap::new();
    for (k, s) in line.sections.iter().enumerate() {
        for p in &s.points {
            let v = at_structure.entry(p.structure.clone()).or_default();
            if v.last() != Some(&k) {
                v.push(k);
            }
        }
    }
    let mut spacers: HashMap<String, Vec<usize>> = HashMap::new();
    for (k, s) in line.spacers.iter().enumerate() {
        spacers.entry(s.a.section.clone()).or_default().push(k);
        spacers.entry(s.b.section.clone()).or_default().push(k);
    }
    let jumpers = jumpers_by_structure(&line);
    let points: HashMap<&str, [f64; 3]> = line
        .alignment_points
        .iter()
        .map(|p| (p.id.as_str(), p.position))
        .collect();
    let alignment_bounds = line
        .alignments
        .iter()
        .map(|a| Bounds::of_points(a.points.iter().filter_map(|p| points.get(p.as_str()))))
        .collect();
    Resolved {
        structures: index(&mut line.structures.iter().map(|x| x.id.clone())),
        sections: index(&mut line.sections.iter().map(|x| x.id.clone())),
        alignments: index(&mut line.alignments.iter().map(|x| x.id.clone())),
        points: index(&mut line.alignment_points.iter().map(|x| x.id.clone())),
        routes: index(&mut line.routes.iter().map(|x| x.id.clone())),
        at_structure,
        spacers,
        jumpers,
        alignment_bounds,
        settings: settings_key(&line),
        notes,
        line,
    }
}

/// A section's spans in the context case: from the cache when nothing it
/// reads changed, else solved (and kept).
fn shape(
    part: &Part,
    r: &Resolved,
    libs: &Libs,
    section: &ohl_line::Section,
    case: &ohl_weather::WeatherCase,
    caches: &Caches,
) -> Solved {
    // What the solve reads, and nothing else: its key.
    let sub = sub_line(r, section);
    let cable = libs.cables.get(&section.cable);
    let k = key(&[
        &part.model,
        &r.settings,
        &sub.routes,
        &sub.alignments,
        &sub.alignment_points,
        &sub.structures,
        &sub.sections,
        &sub.jumpers,
        &sub.spacers,
        &sub.alignment,
        section,
        &cable,
        case,
    ]);
    if let Some(s) = caches
        .shapes
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&k)
    {
        return (s.0.clone(), s.1.clone(), true);
    }
    let (spans, error) = match ohl_line::solve::solve_one(&sub, section, &libs.cables, case) {
        Ok(shape) => (
            shape
                .spans
                .iter()
                .map(|s| SceneSpan {
                    points: crate::web::thin(&s.points, crate::web::SCENE_SPAN_POINTS)
                        .iter()
                        .map(crate::web::mm)
                        .collect(),
                    sag: s.sag,
                    max_tension: s.max_tension,
                })
                .collect::<Vec<_>>(),
            None,
        ),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let bytes = 256
        + spans
            .iter()
            .map(|s: &SceneSpan| 64 + 24 * s.points.len())
            .sum::<usize>()
        + error.as_ref().map_or(0, String::len);
    let kept = Arc::new((spans, error));
    caches
        .shapes
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(k, kept.clone(), bytes);
    (kept.0.clone(), kept.1.clone(), false)
}

/// The jumpers of `line` by the structures they end or are held at.
fn jumpers_by_structure(line: &Line) -> HashMap<String, Vec<usize>> {
    let mut out: HashMap<String, Vec<usize>> = HashMap::new();
    for (k, j) in line.jumpers.iter().enumerate() {
        let mut at: Vec<String> = Vec::new();
        for end in [&j.from, &j.to] {
            at.extend(line.end_structures(end));
        }
        for c in j.supports() {
            at.push(c.structure_id(j).to_owned());
        }
        at.sort();
        at.dedup();
        for s in at {
            out.entry(s).or_default().push(k);
        }
    }
    out
}

/// How far around a section the alignments are kept in its sub-line, m:
/// the line's direction at the section is read from the nearest one (its
/// own, under its structures), and a span point placed by station from
/// its route.
pub(crate) const NEAR_ALIGNMENTS: f64 = 500.0;

/// The part of `r`'s line that solving `section` alone reads: the line's
/// settings; the section's structures; every section and jumper at them
/// (the dead ends and clamps are told by them); the section's spacers and
/// the sections they join; the alignments within [`NEAR_ALIGNMENTS`] of
/// it, with their points and routes. Each kept in the line's order.
fn sub_line(r: &Resolved, section: &ohl_line::Section) -> Line {
    let line = &*r.line;
    let mut out = line.settings_only();
    let mut structures: Vec<usize> = section
        .points
        .iter()
        .filter_map(|p| r.structures.get(&p.structure).copied())
        .collect();
    structures.sort_unstable();
    structures.dedup();
    let mut sections: Vec<usize> = Vec::new();
    let mut jumpers: Vec<usize> = Vec::new();
    for &k in &structures {
        let id = &line.structures[k].id;
        sections.extend(r.at_structure.get(id).into_iter().flatten().copied());
        jumpers.extend(r.jumpers.get(id).into_iter().flatten().copied());
    }
    let mut spacers: Vec<usize> = r
        .spacers
        .get(&section.id)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    for &k in &spacers {
        let s = &line.spacers[k];
        for id in [&s.a.section, &s.b.section] {
            if let Some(&k) = r.sections.get(id) {
                sections.push(k);
            }
        }
    }
    if let Some(&k) = r.sections.get(&section.id) {
        sections.push(k);
    }
    for v in [&mut sections, &mut jumpers, &mut spacers] {
        v.sort_unstable();
        v.dedup();
    }
    // The structures of every section kept (their points must be there).
    let mut more: Vec<usize> = sections
        .iter()
        .flat_map(|&k| line.sections[k].points.iter())
        .filter_map(|p| r.structures.get(&p.structure).copied())
        .collect();
    more.extend(structures);
    more.sort_unstable();
    more.dedup();
    let structures = more;
    // The alignments near it.
    let near = Bounds::of_points(structures.iter().map(|&k| &line.structures[k].position))
        .map(|b| b.grown(NEAR_ALIGNMENTS));
    let alignments: Vec<usize> = match near {
        Some(near) => r
            .alignment_bounds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_some_and(|b| b.intersects(&near)))
            .map(|(k, _)| k)
            .collect(),
        None => Vec::new(),
    };
    let mut points: Vec<usize> = Vec::new();
    let mut routes: Vec<usize> = Vec::new();
    for &k in &alignments {
        let a = &line.alignments[k];
        points.extend(a.points.iter().filter_map(|p| r.points.get(p).copied()));
        if let Some(&k) = r.routes.get(&a.route) {
            routes.push(k);
        }
    }
    for &k in &structures {
        routes.extend(
            line.structures[k]
                .routes
                .iter()
                .filter_map(|id| r.routes.get(id).copied()),
        );
    }
    for v in [&mut points, &mut routes] {
        v.sort_unstable();
        v.dedup();
    }
    out.routes = routes.iter().map(|&k| line.routes[k].clone()).collect();
    out.alignments = alignments
        .iter()
        .map(|&k| line.alignments[k].clone())
        .collect();
    out.alignment_points = points
        .iter()
        .map(|&k| line.alignment_points[k].clone())
        .collect();
    out.structures = structures
        .iter()
        .map(|&k| line.structures[k].clone())
        .collect();
    out.sections = sections.iter().map(|&k| line.sections[k].clone()).collect();
    out.jumpers = jumpers.iter().map(|&k| line.jumpers[k].clone()).collect();
    out.spacers = spacers.iter().map(|&k| line.spacers[k].clone()).collect();
    out.alignment = line.alignment.clone();
    out
}

/// A structure as the context draws it: a stick — its drawing when its
/// type gives one, else a mast to its highest point with an arm to each
/// point — and its strings.
pub(crate) fn context_structure(model: &str, s: &Structure) -> ContextStructure {
    let base = s.position;
    let mut top = base[2];
    let mut stick: Vec<[[f64; 3]; 2]> = Vec::new();
    for m in &s.members {
        for w in m.windows(2) {
            top = top.max(w[0][2]).max(w[1][2]);
            stick.push([cm(&w[0]), cm(&w[1])]);
        }
    }
    let mut at: HashMap<&str, [f64; 3]> = HashMap::new();
    for p in &s.points {
        top = top.max(p.position[2]);
        at.insert(&p.label, p.position);
    }
    for w in &s.wires {
        top = top.max(w.position[2]);
        at.insert(&w.label, w.position);
    }
    if s.members.is_empty() {
        stick.push([cm(&base), cm(&[base[0], base[1], top])]);
        for p in &s.points {
            stick.push([cm(&[base[0], base[1], p.position[2]]), cm(&p.position)]);
        }
    }
    for st in &s.strings {
        if let (Some(a), Some(b)) = (at.get(st.top.as_str()), at.get(st.wire.as_str())) {
            stick.push([cm(a), cm(b)]);
        }
    }
    ContextStructure {
        model: model.to_owned(),
        id: s.id.clone(),
        position: crate::web::mm(&base),
        orientation_deg: s.orientation_deg,
        type_name: s.structure_type.clone(),
        height: ((top - base[2]) * 100.0).round() / 100.0,
        stick,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every section solved on its sub-line gives the shape it gets on its
    /// whole line, bit for bit (the `Debug` text of the results compared):
    /// the example lines (the typed one resolved with its libraries, the
    /// crossing line) and a synthetic country of 300 pylons as one line of
    /// several routes with shared towers.
    #[test]
    fn a_section_solves_on_its_sub_line_as_on_its_whole_line() {
        use ohl_core::files::shipped_library;
        let ex =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let cables = ohl_cables::Library::load(&shipped_library("cables")).unwrap();
        let case = ohl_weather::WeatherCase::still(
            "15 °C, no wind",
            ohl_core::units::Temperature::from_celsius(15.0),
        );
        let mut lines: Vec<(String, Line, usize)> = Vec::new();
        for name in ["synthetic-line.ohl.json", "crossing-line.ohl.json"] {
            lines.push((name.into(), Line::load(&ex.join(name)).unwrap(), 1));
        }
        let typed = Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
        let structures =
            ohl_structures::StructureLibrary::load_dir(&ex.join("Structures")).unwrap();
        let insulators =
            ohl_structures::InsulatorLibrary::load_dir(&ex.join("Insulators")).unwrap();
        lines.push((
            "typed".into(),
            typed.resolve(&structures, &insulators).unwrap(),
            1,
        ));
        let country = ohl_synth::country(&ohl_synth::Settings {
            pylons: 300,
            ..Default::default()
        })
        .unwrap();
        lines.push(("country".into(), country.line, 11));
        let mut compared = 0;
        for (name, line, every) in lines {
            let r = indexed(Arc::new(line), Vec::new());
            let line = &*r.line;
            // The whole line's lookups made once for its sections (O42:
            // the same results as searched, bit for bit; each sub-line is
            // searched).
            ohl_line::lookups::indexed(line, Some(&cables), || {
                for (k, section) in line.sections.iter().enumerate() {
                    if k % every != 0 {
                        continue;
                    }
                    let whole = ohl_line::solve::solve_one(line, section, &cables, &case);
                    let sub = sub_line(&r, section);
                    assert!(
                        sub.structures.len() < line.structures.len() || line.structures.len() < 12
                    );
                    let alone = ohl_line::solve::solve_one(&sub, section, &cables, &case);
                    assert_eq!(
                        format!("{whole:?}"),
                        format!("{alone:?}"),
                        "{name}: section {}",
                        section.id
                    );
                    compared += 1;
                }
            });
        }
        assert!(compared > 100, "{compared}");
    }

    /// A tile's part of a line ([`ohl_session::Session::drawing_part`],
    /// phase 6: what the packager gets instead of a copy of the whole line)
    /// gives each of its sections the sub-line the whole line gives, bit
    /// for bit, resolved as the whole line is: the example lines (the
    /// typed one with its libraries) and a synthetic country of 300 pylons
    /// as one line, by tile of 1 024 m (every third tile).
    #[test]
    fn a_tiles_part_gives_the_whole_lines_sub_lines() {
        // A tile's structures, sections and alignments.
        type Ids = (Vec<String>, Vec<String>, Vec<String>);
        let ex =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let structures =
            ohl_structures::StructureLibrary::load_dir(&ex.join("Structures")).unwrap();
        let insulators =
            ohl_structures::InsulatorLibrary::load_dir(&ex.join("Insulators")).unwrap();
        let types = (structures, insulators);
        let mut lines: Vec<(String, Line, bool, usize)> = Vec::new();
        for name in ["synthetic-line.ohl.json", "crossing-line.ohl.json"] {
            lines.push((name.into(), Line::load(&ex.join(name)).unwrap(), false, 1));
        }
        lines.push((
            "typed".into(),
            Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap(),
            true,
            1,
        ));
        let country = ohl_synth::country(&ohl_synth::Settings {
            pylons: 300,
            ..Default::default()
        })
        .unwrap();
        lines.push(("country".into(), country.line, false, 3));
        let dir = std::env::temp_dir().join(format!("ohl-tile-part-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let grid = ohl_core::grid::Grid::default();
        let mut compared = 0;
        for (n, (name, line, typed, every)) in lines.into_iter().enumerate() {
            let mut s = ohl_session::Session::open(&dir.join(format!("{n}.ohl"))).unwrap();
            s.import(&line, "ana").unwrap();
            let line = s.line().clone();
            let resolve = |l: &Line| -> Line {
                if typed {
                    l.resolve_with(&types.0, &types.1, None).unwrap()
                } else {
                    l.clone()
                }
            };
            let whole = indexed(Arc::new(resolve(&line)), Vec::new());
            // The line's objects by tile, as the server's tile index keeps
            // them (by their bounds).
            let mut by_tile: BTreeMap<(i64, i64), Ids> = BTreeMap::new();
            let mut put = |b: Option<Bounds>, f: &mut dyn FnMut(&mut Ids)| {
                let Some(b) = b else { return };
                let (t0, t1) = (grid.tile_at(2, b.min), grid.tile_at(2, b.max));
                for x in t0.x..=t1.x {
                    for y in t0.y..=t1.y {
                        f(by_tile.entry((x, y)).or_default());
                    }
                }
            };
            for st in &line.structures {
                put(Bounds::of_points([&st.position]), &mut |e| {
                    e.0.push(st.id.clone())
                });
            }
            for sec in &line.sections {
                let b = Bounds::of_points(
                    sec.points
                        .iter()
                        .filter_map(|p| line.structure(&p.structure).map(|s| &s.position)),
                );
                put(b, &mut |e| e.1.push(sec.id.clone()));
            }
            for a in &line.alignments {
                let b = Bounds::of_points(
                    a.points
                        .iter()
                        .filter_map(|p| line.alignment_point(p).map(|p| &p.position)),
                );
                put(b, &mut |e| e.2.push(a.id.clone()));
            }
            for (k, (_, (sts, secs, als))) in by_tile.iter().enumerate() {
                if k % every != 0 {
                    continue;
                }
                let part = s.drawing_part(sts, secs, als, NEAR_ALIGNMENTS);
                assert!(
                    part.structures.len() <= line.structures.len(),
                    "{name}: a part"
                );
                let r = indexed(Arc::new(resolve(&part)), Vec::new());
                for id in secs {
                    let sec = &whole.line.sections[whole.sections[id]];
                    assert_eq!(
                        format!("{:?}", sub_line(&whole, sec)),
                        format!("{:?}", sub_line(&r, sec)),
                        "{name}: section {id}"
                    );
                    compared += 1;
                }
                for id in sts {
                    assert!(r.structures.contains_key(id), "{name}: structure {id}");
                }
                for id in als {
                    assert!(r.alignments.contains_key(id), "{name}: alignment {id}");
                }
            }
        }
        assert!(compared > 200, "{compared}");
    }
}
