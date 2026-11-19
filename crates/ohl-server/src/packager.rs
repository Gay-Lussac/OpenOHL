// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tile packager of a network's server (plan §5.2): threads making
//! the light, read-only context of a tile — per structure its id,
//! position, orientation, type, height and a simplified stick; per section
//! its wires solved in the context case; the alignments as polylines; the
//! routes' names — from a copy of each model's line taken when the tile
//! was asked for (the authority goes on meanwhile).
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
//!   (`ohl_line::library::ResolveCache`, the application's idea), the
//!   resolved line of the model's revision (least recently used models
//!   first beyond a count).
//!
//! The context's wires are a **drawing**, not a result: each section is
//! solved alone in the context case (`ohl_line::solve::solve_one`) — the
//! jumpers' loads, the coupled solve of sections sharing an assembly and
//! the structures' flexibility are left out (the checks and OHL-Design
//! take them); a span is thinned to [`crate::web::SCENE_SPAN_POINTS`]
//! points at millimetres.

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
    /// The model's revision (of its main branch) and its line then.
    pub(crate) rev: u64,
    pub(crate) line: Arc<Line>,
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
    /// The line's settings (everything but its elements).
    settings: Key,
    notes: Vec<String>,
}

/// A section's spans in the context case, or why it did not solve.
type Shape = Arc<(Vec<SceneSpan>, Option<String>)>;

/// What the packagers keep between packages.
pub(crate) struct Caches {
    pub(crate) shapes: Mutex<Lru<Key, Shape>>,
    libs: Mutex<Lru<(String, u64), Arc<Libs>>>,
    resolved: Mutex<Lru<(String, u64, u64), Arc<Resolved>>>,
}

impl Caches {
    pub(crate) fn new(shapes_mb: usize, models: usize) -> Self {
        Self {
            shapes: Mutex::new(Lru::new(shapes_mb << 20, usize::MAX)),
            libs: Mutex::new(Lru::new(usize::MAX, models)),
            resolved: Mutex::new(Lru::new(usize::MAX, models)),
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
        self.resolved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
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
        let r = resolved_of(part, &libs, caches);
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
        for id in &part.sections {
            let Some(&k) = r.sections.get(id) else {
                continue;
            };
            let section = &line.sections[k];
            let (spans, error, was_cached) = shape(part, &r, &libs, section, &job.case, caches);
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
    let k = (part.model.clone(), part.libs_rev);
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
        None => match ohl_cables::Library::load(&shipped_library("cables")) {
            Ok(l) => cables = l,
            Err(e) => notes.push(format!("wire library: {e}")),
        },
    }
    let typed = part
        .line
        .structures
        .iter()
        .any(|s| s.structure_type.is_some());
    let types = if typed {
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

/// The model's line resolved at its revision (once per revision).
fn resolved_of(part: &Part, libs: &Libs, caches: &Caches) -> Arc<Resolved> {
    let k = (part.model.clone(), part.rev, part.libs_rev);
    if let Some(r) = caches
        .resolved
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&k)
    {
        return r;
    }
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
    let r = Arc::new(Resolved {
        structures: index(&mut line.structures.iter().map(|x| x.id.clone())),
        sections: index(&mut line.sections.iter().map(|x| x.id.clone())),
        alignments: index(&mut line.alignments.iter().map(|x| x.id.clone())),
        points: index(&mut line.alignment_points.iter().map(|x| x.id.clone())),
        routes: index(&mut line.routes.iter().map(|x| x.id.clone())),
        at_structure,
        spacers,
        settings: settings_key(&line),
        notes,
        line,
    });
    caches
        .resolved
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .put(k, r.clone(), 0);
    r
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
) -> (Vec<SceneSpan>, Option<String>, bool) {
    let line = &*r.line;
    let mut structures: Vec<&Structure> = Vec::new();
    let mut others: Vec<usize> = Vec::new();
    for p in &section.points {
        if let Some(&k) = r.structures.get(&p.structure)
            && !structures.iter().any(|s| s.id == p.structure)
        {
            structures.push(&line.structures[k]);
        }
        if let Some(v) = r.at_structure.get(&p.structure) {
            others.extend(v.iter().copied());
        }
    }
    others.sort_unstable();
    others.dedup();
    let ends: Vec<(&str, &Vec<ohl_line::SectionPoint>)> = others
        .iter()
        .map(|&k| (line.sections[k].id.as_str(), &line.sections[k].points))
        .collect();
    let spacers: Vec<&ohl_line::Spacer> = r
        .spacers
        .get(&section.id)
        .into_iter()
        .flatten()
        .map(|&k| &line.spacers[k])
        .collect();
    let mid = match (section.points.first(), section.points.last()) {
        (Some(a), Some(b)) => match (line.wire_point(a), line.wire_point(b)) {
            (Some(a), Some(b)) => Some([
                0.5 * (a.position[0] + b.position[0]),
                0.5 * (a.position[1] + b.position[1]),
            ]),
            _ => None,
        },
        _ => None,
    };
    let direction = mid.and_then(|m| line.direction_at(m));
    let cable = libs.cables.get(&section.cable);
    let k = key(&[
        &part.model,
        section,
        &structures,
        &ends,
        &spacers,
        &direction,
        &r.settings,
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
    let (spans, error) = match ohl_line::solve::solve_one(line, section, &libs.cables, case) {
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
