// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! From the points and the model to the presets: the buildings (clusters
//! of building points, surveyed outlines), their heights and floors,
//! their spans, the cuts across the line at each of them and their places
//! of sensitive use.

use std::collections::{BTreeMap, HashMap};

use crate::cluster::{clusters, trace_cells};
use crate::geom::{
    P2, ccw, centroid, convex_hull, distance_to_area, nearest_on_segment, percentile,
    segment_to_area, signed_area, simplify_ring,
};
use crate::model::{Model, Span};
use crate::params::{OutlineKind, Params, matches_any};

/// A point the host gathered.
#[derive(Debug, Clone, PartialEq)]
pub struct InPoint {
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m.
    pub z: f64,
    /// LiDAR class (0: a survey point).
    pub class: u8,
    /// A survey point's code.
    pub code: String,
    /// A survey point's breakline id.
    pub line: String,
}

/// A building found.
#[derive(Debug, Clone, PartialEq)]
pub struct Building {
    /// `B001`, … in station order along the line.
    pub id: String,
    /// Its points.
    pub points: usize,
    /// Its footprint, counter-clockwise.
    pub ring: Vec<P2>,
    /// The footprint's area, m².
    pub area: f64,
    /// The footprint's centroid.
    pub centroid: P2,
    /// Its ground, eaves and roof elevations, m.
    pub ground: f64,
    /// Eaves, m.
    pub eaves: f64,
    /// Roof, m.
    pub roof: f64,
    /// Floors.
    pub floors: u32,
    /// The span nearest it (index into the model's spans).
    pub span: usize,
    /// Every span it lies beside (its own first).
    pub spans: Vec<usize>,
    /// Its footprint's stations along `span`, from its first structure, m.
    pub stations: (f64, f64),
    /// The smallest plan distance from its footprint to the axis, m.
    pub axis_distance: f64,
    /// The smallest 3D distance from its box to a phase conductor, m.
    pub conductor_distance: f64,
    /// Its cuts' labels.
    pub cuts: Vec<String>,
}

/// A cut across the line.
#[derive(Debug, Clone, PartialEq)]
pub struct CutOut {
    /// Its label: the cuts merged into it (`B001-gov+B001-near`).
    pub label: String,
    /// The span's structure its fraction is measured from.
    pub from: String,
    /// The other.
    pub to: String,
    /// Its place in the span, 0–1.
    pub fraction: f64,
    /// Either side of the axis, m.
    pub width: f64,
    /// Above the highest wire, m.
    pub above: f64,
    /// Between points, m.
    pub step: f64,
}

/// A place of sensitive use.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOut {
    /// `B007-F2-c1-h4.5`: building, floor, place on the footprint
    /// (`c` corner, `m` side's midpoint, `n` nearest the axis), height.
    pub name: String,
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Height above the ground, m.
    pub height: f64,
}

/// A building's outline for the report.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineOut {
    /// The building's id.
    pub id: String,
    /// Its footprint.
    pub ring: Vec<P2>,
    /// Its ground, m.
    pub ground: f64,
    /// Its roof, m.
    pub top: f64,
}

/// A preset as the plugin gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetOut {
    /// Its name.
    pub name: String,
    /// What it is for.
    pub description: String,
    /// The cuts.
    pub cuts: Vec<CutOut>,
    /// The places.
    pub points: Vec<PlaceOut>,
    /// Isoline levels, µT.
    pub levels_ut: Vec<f64>,
    /// Isoline levels, kV/m.
    pub levels_kv_m: Vec<f64>,
    /// The buildings' outlines.
    pub outlines: Vec<OutlineOut>,
    /// Notes: what was assumed, each building's summary, each cut's wires.
    pub notes: Vec<String>,
}

/// Everything found.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// The buildings, in station order.
    pub buildings: Vec<Building>,
    /// The cuts, merged, in span and station order.
    pub cuts: Vec<CutOut>,
    /// The places of sensitive use.
    pub places: Vec<PlaceOut>,
    /// Notes: assumptions, buildings left out, the cuts' wires.
    pub notes: Vec<String>,
    /// The cuts' wire crossings, one note per cut.
    pub crossings: Vec<String>,
}

/// The most cuts of a preset (the host's limit).
pub const MAX_CUTS: usize = 1_000;
/// The most cut points of a preset, as the host estimates them.
pub const MAX_CUT_POINTS: f64 = 4_000_000.0;
/// The height the host's estimate takes from the ground to the highest
/// wire, m.
pub const CUT_HEIGHT_ESTIMATE: f64 = 100.0;
/// The most places of a preset (the host's limit).
pub const MAX_PLACES: usize = 100_000;

/// A candidate before its heights and spans: its points and footprint.
struct Candidate {
    points: Vec<usize>,
    ring: Vec<P2>,
    note: Option<String>,
}

/// A grid of ground points for the lookups around each building.
struct GroundIndex {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<[f64; 3]>>,
}

impl GroundIndex {
    fn new(points: &[[f64; 3]]) -> Self {
        let cell = 10.0;
        let mut cells: HashMap<(i64, i64), Vec<[f64; 3]>> = HashMap::new();
        for p in points {
            cells
                .entry(((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64))
                .or_default()
                .push(*p);
        }
        Self { cell, cells }
    }

    /// The ground points from `near` to `far` around the ring's area.
    fn around(&self, ring: &[P2], near: f64, far: f64) -> Vec<f64> {
        let (lo, hi) = bbox(ring);
        let (i0, j0) = (
            ((lo[0] - far) / self.cell).floor() as i64,
            ((lo[1] - far) / self.cell).floor() as i64,
        );
        let (i1, j1) = (
            ((hi[0] + far) / self.cell).floor() as i64,
            ((hi[1] + far) / self.cell).floor() as i64,
        );
        let mut out = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                for p in self.cells.get(&(i, j)).into_iter().flatten() {
                    let d = distance_to_area([p[0], p[1]], ring);
                    if d >= near && d <= far {
                        out.push(p[2]);
                    }
                }
            }
        }
        out
    }
}

fn bbox(ring: &[P2]) -> (P2, P2) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in ring {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    (lo, hi)
}

/// The plan distance between two boxes (0 when they overlap).
fn box_gap(a: (P2, P2), b: (P2, P2)) -> f64 {
    let dx = (a.0[0] - b.1[0]).max(b.0[0] - a.1[0]).max(0.0);
    let dy = (a.0[1] - b.1[1]).max(b.0[1] - a.1[1]).max(0.0);
    dx.hypot(dy)
}

/// The 3D distance from `p` to the prism of `ring` from `ground` to
/// `roof`.
fn prism_distance(p: [f64; 3], ring: &[P2], ground: f64, roof: f64) -> f64 {
    let plan = distance_to_area([p[0], p[1]], ring);
    let dz = (ground - p[2]).max(p[2] - roof).max(0.0);
    plan.hypot(dz)
}

/// Points along the ring's boundary, `step` apart at most (its corners
/// among them).
fn boundary_samples(ring: &[P2], step: f64) -> Vec<P2> {
    let n = ring.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let k = (len / step).ceil().max(1.0) as usize;
        for j in 0..k {
            let t = j as f64 / k as f64;
            out.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    out
}

/// A building found, its own notes, and per span where a phase
/// conductor comes nearest it (span, distance, station).
type Found = (Building, Vec<String>, Vec<(usize, f64, f64)>);

/// The kinds of cut per building, in priority order (a merged cut is
/// placed at its first member's station).
const TAGS: [&str; 3] = ["gov", "near", "ctr"];

fn fmt_height(h: f64) -> String {
    format!("{h:.1}")
}

/// Finds the buildings, their cuts and places.
pub fn analyse(
    params: &Params,
    model: &Model,
    points: &[InPoint],
    ground: &[f64],
) -> Result<Analysis, String> {
    let mut notes = Vec::new();
    // The ground: the point cloud's and the survey's ground codes.
    let mut ground_points: Vec<[f64; 3]> = ground.as_chunks::<3>().0.to_vec();
    let mut pool: Vec<usize> = Vec::new();
    let mut lines: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let (mut lidar, mut surveyed, mut surveyed_ground) = (0usize, 0usize, 0usize);
    for (i, p) in points.iter().enumerate() {
        // The host gives survey points class 0 (LiDAR classes of the
        // building role are not 0).
        if p.class != 0 {
            lidar += 1;
            pool.push(i);
        } else if matches_any(&params.building_codes, &p.code) {
            surveyed += 1;
            if p.line.is_empty() {
                pool.push(i);
            } else {
                lines.entry(p.line.clone()).or_default().push(i);
            }
        } else if matches_any(&params.ground_codes, &p.code) {
            surveyed_ground += 1;
            ground_points.push([p.x, p.y, p.z]);
        }
    }
    notes.push(format!(
        "building points: {lidar} from the point cloud, {surveyed} surveyed (codes {}; {} outlines \
         by breakline); ground: {} points ({surveyed_ground} surveyed, codes {})",
        params.building_codes.join(" "),
        lines.len(),
        ground_points.len(),
        params.ground_codes.join(" "),
    ));
    let ground_index = GroundIndex::new(&ground_points);
    let xy = |i: usize| [points[i].x, points[i].y];

    // Candidates: the clusters of the pool and the surveyed outlines.
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut dropped = (0usize, 0usize);
    let pool_xy: Vec<P2> = pool.iter().map(|&i| xy(i)).collect();
    for c in clusters(&pool_xy, params.cell, params.gap) {
        let idx: Vec<usize> = c.points.iter().map(|&k| pool[k]).collect();
        let hull = || {
            let pts: Vec<P2> = idx.iter().map(|&i| xy(i)).collect();
            convex_hull(&pts)
        };
        let (ring, note) = match params.outline {
            OutlineKind::Hull => (hull(), None),
            OutlineKind::Cells => match trace_cells(&c.cells, params.cell, params.gap) {
                Some(r) => (r, None),
                None => (
                    hull(),
                    Some("its cells are in several pieces: the convex hull taken".to_owned()),
                ),
            },
        };
        let ring = ccw(simplify_ring(&ring, params.simplify));
        if idx.len() < params.min_points || ring.len() < 3 || signed_area(&ring) < params.min_area {
            dropped.0 += 1;
            dropped.1 += idx.len();
            continue;
        }
        candidates.push(Candidate {
            points: idx,
            ring,
            note,
        });
    }
    for (line, idx) in &lines {
        let mut ring: Vec<P2> = Vec::new();
        for &i in idx {
            let p = xy(i);
            if ring
                .last()
                .is_none_or(|q: &P2| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-6)
            {
                ring.push(p);
            }
        }
        if ring.len() > 1 {
            let (f, l) = (ring[0], ring[ring.len() - 1]);
            if (f[0] - l[0]).hypot(f[1] - l[1]) <= 1e-6 {
                ring.pop();
            }
        }
        let ring = ccw(ring);
        if ring.len() < 3 || signed_area(&ring) < params.min_area {
            notes.push(format!(
                "surveyed outline {line}: fewer than three corners or under {} m²: left out",
                params.min_area
            ));
            continue;
        }
        candidates.push(Candidate {
            points: idx.clone(),
            ring,
            note: Some(format!("surveyed outline {line}")),
        });
    }
    if dropped.0 > 0 {
        notes.push(format!(
            "{} clusters left out (fewer than {} points or under {} m²): {} points",
            dropped.0, params.min_points, params.min_area, dropped.1
        ));
    }

    // Heights, spans, distances.
    let phase_boxes: Vec<(usize, (P2, P2))> = model
        .wires
        .iter()
        .enumerate()
        .filter(|(_, w)| w.phase)
        .map(|(k, w)| {
            let ring: Vec<P2> = w.points.iter().map(|p| [p[0], p[1]]).collect();
            (k, bbox(&ring))
        })
        .collect();
    let mut buildings: Vec<Found> = Vec::new();
    let mut beyond = 0usize;
    for c in candidates {
        let mut own_notes: Vec<String> = c.note.into_iter().collect();
        let mut z: Vec<f64> = c.points.iter().map(|&i| points[i].z).collect();
        let lowest = z.iter().copied().fold(f64::INFINITY, f64::min);
        let eaves0 = percentile(&mut z, params.eaves_percentile).unwrap_or(lowest);
        let roof0 = percentile(&mut z, params.roof_percentile).unwrap_or(lowest);
        let [near, far] = params.ground_ring;
        let mut g = ground_index.around(&c.ring, near, far);
        if g.is_empty() {
            g = ground_index.around(&c.ring, near, 3.0 * far);
            if !g.is_empty() {
                own_notes.push(format!(
                    "no ground points {near}–{far} m around it: those to {} m taken",
                    3.0 * far
                ));
            }
        }
        let ground_z = match percentile(&mut g, params.ground_percentile) {
            Some(v) => v,
            None => {
                own_notes.push(
                    "no ground points near it: its lowest point taken as its ground".to_owned(),
                );
                lowest
            }
        };
        let (mut eaves, mut roof) = (eaves0, roof0.max(eaves0));
        if eaves - ground_z < 2.0 {
            eaves = ground_z + params.default_height;
            roof = roof.max(eaves);
            own_notes.push(format!(
                "its points are less than 2 m above its ground (a footprint): {} m high assumed",
                params.default_height
            ));
        }
        let floors = ((eaves - ground_z) / params.floor_height).ceil().max(1.0) as u32;
        // Spans.
        let mut beside: Vec<(usize, f64)> = Vec::new();
        let mut axis = f64::INFINITY;
        for (k, s) in model.spans.iter().enumerate() {
            let d = segment_to_area(s.a, s.b, &c.ring);
            axis = axis.min(d);
            let (lo, hi) = t_range(s, &c.ring);
            if hi > 0.0 && lo < s.length && d <= params.max_distance {
                beside.push((k, s.distance(centroid(&c.ring))));
            }
        }
        if beside.is_empty() {
            beyond += 1;
            continue;
        }
        beside.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let span = beside[0].0;
        let mut spans: Vec<usize> = beside.iter().map(|b| b.0).collect();
        spans[1..].sort_unstable();
        // The nearest phase conductor: overall, and per span where on it.
        let bb = bbox(&c.ring);
        let mut conductor = f64::INFINITY;
        let mut per_span: Vec<(usize, f64, f64)> = Vec::new(); // span, distance, station
        // What a wire must come nearer than to matter: the best of its
        // span (the governing cut there), else the best overall.
        let bound = |per_span: &[(usize, f64, f64)], conductor: f64, span: Option<usize>| match span
        {
            Some(s) => per_span
                .iter()
                .find(|p| p.0 == s)
                .map_or(f64::INFINITY, |p| p.1),
            None => conductor,
        };
        for &(k, wb) in &phase_boxes {
            let w = &model.wires[k];
            if box_gap(bb, wb) > bound(&per_span, conductor, w.span) {
                continue;
            }
            for seg in w.points.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let sb = bbox(&[[a[0], a[1]], [b[0], b[1]]]);
                if box_gap(bb, sb) > bound(&per_span, conductor, w.span) {
                    continue;
                }
                let len = (b[0] - a[0]).hypot(b[1] - a[1]).hypot(b[2] - a[2]);
                let n = (len / 0.5).ceil().max(1.0) as usize;
                for j in 0..=n {
                    let t = j as f64 / n as f64;
                    let p = [
                        a[0] + t * (b[0] - a[0]),
                        a[1] + t * (b[1] - a[1]),
                        a[2] + t * (b[2] - a[2]),
                    ];
                    let d = prism_distance(p, &c.ring, ground_z, roof);
                    conductor = conductor.min(d);
                    if let Some(s) = w.span {
                        let station = model.spans[s].local([p[0], p[1]]).0;
                        match per_span.iter_mut().find(|q| q.0 == s) {
                            Some(q) if d < q.1 => {
                                q.1 = d;
                                q.2 = station;
                            }
                            Some(_) => {}
                            None => per_span.push((s, d, station)),
                        }
                    }
                }
            }
        }
        let s = &model.spans[span];
        let (lo, hi) = t_range(s, &c.ring);
        let area = signed_area(&c.ring);
        buildings.push((
            Building {
                id: String::new(),
                points: c.points.len(),
                centroid: centroid(&c.ring),
                ring: c.ring,
                area,
                ground: ground_z,
                eaves,
                roof,
                floors,
                span,
                spans,
                stations: (lo.max(0.0), hi.min(s.length)),
                axis_distance: axis,
                conductor_distance: conductor,
                cuts: Vec::new(),
            },
            own_notes,
            per_span,
        ));
    }
    if beyond > 0 {
        notes.push(format!(
            "{beyond} buildings left out: beyond the line's ends or farther than {} m from its axis",
            params.max_distance
        ));
    }
    // Ids in station order.
    buildings.sort_by(|a, b| {
        let key = |x: &Building| {
            let s = &model.spans[x.span];
            s.start + s.local(x.centroid).0.clamp(0.0, s.length)
        };
        key(&a.0)
            .total_cmp(&key(&b.0))
            .then(a.0.centroid[0].total_cmp(&b.0.centroid[0]))
            .then(a.0.centroid[1].total_cmp(&b.0.centroid[1]))
    });
    for (k, b) in buildings.iter_mut().enumerate() {
        b.0.id = format!("B{:03}", k + 1);
    }

    // Cuts: per span, the members (station, priority, building, name).
    let mut members: BTreeMap<usize, Vec<(f64, usize, usize, String)>> = BTreeMap::new();
    for (bi, (b, own, per_span)) in buildings.iter_mut().enumerate() {
        for &k in &b.spans {
            let s = &model.spans[k];
            let (lo, hi) = t_range(s, &b.ring);
            let (lo, hi) = (lo.max(0.0), hi.min(s.length));
            let name = |tag: &str| {
                if b.spans.len() > 1 {
                    format!("{}-{}{}-{tag}", b.id, s.from, s.to)
                } else {
                    format!("{}-{tag}", b.id)
                }
            };
            let list = members.entry(k).or_default();
            match per_span.iter().find(|p| p.0 == k) {
                Some(&(_, _, station)) => {
                    list.push((station.clamp(lo, hi), 0, bi, name(TAGS[0])));
                }
                None => own.push(format!(
                    "no phase conductor in span {}–{}: no governing cut there",
                    s.from, s.to
                )),
            }
            list.push((nearest_station(s, &b.ring, lo, hi), 1, bi, name(TAGS[1])));
            if params.centroid_cut {
                let t = s.local(b.centroid).0.clamp(lo, hi);
                list.push((t, 2, bi, name(TAGS[2])));
            }
        }
        if b.spans.len() > 1 {
            let names: Vec<String> = b
                .spans
                .iter()
                .map(|&k| format!("{}–{}", model.spans[k].from, model.spans[k].to))
                .collect();
            own.push(format!(
                "it lies beside {} spans ({}): cuts in each",
                b.spans.len(),
                names.join(", ")
            ));
        }
    }
    let mut cuts = Vec::new();
    let mut cut_spans = Vec::new();
    for (k, mut list) in members {
        let s = &model.spans[k];
        list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut groups: Vec<Vec<(f64, usize, usize, String)>> = Vec::new();
        for m in list {
            match groups.last_mut() {
                Some(g) if m.0 - g[0].0 <= params.merge => g.push(m),
                _ => groups.push(vec![m]),
            }
        }
        for g in groups {
            let rep = g
                .iter()
                .min_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)))
                .expect("a member");
            let station = rep.0;
            let label = g.iter().map(|m| m.3.as_str()).collect::<Vec<_>>().join("+");
            // Wide enough for every building the cut crosses.
            let mut reach: f64 = 0.0;
            for (b, _, _) in &buildings {
                if !b.spans.contains(&k) {
                    continue;
                }
                let (lo, hi) = t_range(s, &b.ring);
                let ours = g
                    .iter()
                    .any(|m| m.2 < buildings.len() && buildings[m.2].0.id == b.id);
                if ours || (lo - 1e-9..=hi + 1e-9).contains(&station) {
                    for p in &b.ring {
                        reach = reach.max(s.local(*p).1.abs());
                    }
                }
            }
            for m in &g {
                buildings[m.2].0.cuts.push(label.clone());
            }
            cuts.push(CutOut {
                label,
                from: s.from.clone(),
                to: s.to.clone(),
                fraction: (station / s.length).clamp(0.0, 1.0),
                width: params.min_width.max(reach + params.cut_margin),
                above: params.above,
                step: params.step,
            });
            cut_spans.push((k, station));
        }
    }
    if cuts.len() > MAX_CUTS {
        return Err(format!(
            "{} cuts: at most {MAX_CUTS} (raise merge, or run the line in parts)",
            cuts.len()
        ));
    }
    // The host's estimate of the cut points: a coarser step when over.
    let estimate = |step: f64| -> f64 {
        cuts.iter()
            .map(|c| {
                ((2.0 * c.width / step).ceil() + 1.0)
                    * (((c.above + CUT_HEIGHT_ESTIMATE) / step).ceil() + 1.0)
            })
            .sum()
    };
    if estimate(params.step) > MAX_CUT_POINTS {
        let mut step = params.step;
        while estimate(step) > MAX_CUT_POINTS {
            step *= 1.1;
        }
        let step = (step * 100.0).ceil() / 100.0;
        notes.push(format!(
            "the cuts' step raised from {} m to {step} m: the host computes at most {} cut points \
             a preset (estimated with {CUT_HEIGHT_ESTIMATE} m from the ground to the highest wire)",
            params.step, MAX_CUT_POINTS
        ));
        for c in &mut cuts {
            c.step = step;
        }
    }

    // The wires each cut crosses.
    let mut crossings = Vec::new();
    for (c, &(k, station)) in cuts.iter().zip(&cut_spans) {
        let s = &model.spans[k];
        let mut parts = Vec::new();
        for w in model.wires.iter().filter(|w| w.span == Some(k)) {
            for seg in w.points.windows(2) {
                let (ta, tb) = (
                    s.local([seg[0][0], seg[0][1]]).0 - station,
                    s.local([seg[1][0], seg[1][1]]).0 - station,
                );
                if (ta <= 0.0 && tb > 0.0) || (ta >= 0.0 && tb < 0.0) {
                    let f = ta / (ta - tb);
                    let p = [
                        seg[0][0] + f * (seg[1][0] - seg[0][0]),
                        seg[0][1] + f * (seg[1][1] - seg[0][1]),
                        seg[0][2] + f * (seg[1][2] - seg[0][2]),
                    ];
                    parts.push(format!(
                        "wire {} {:.2} {:.2}{}",
                        w.section,
                        s.local([p[0], p[1]]).1,
                        p[2],
                        if w.phase { "" } else { " earth" }
                    ));
                    break;
                }
            }
        }
        crossings.push(format!("cut {}: {}", c.label, parts.join("; ")));
    }

    // Places of sensitive use.
    let mut places = Vec::new();
    for (b, own, _) in &mut buildings {
        let chords: Vec<&Span> = b.spans.iter().map(|&k| &model.spans[k]).collect();
        let nearest_axis = |p: P2| -> (P2, f64) {
            chords
                .iter()
                .map(|s| nearest_on_segment(p, s.a, s.b))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("a span")
        };
        let n = b.ring.len();
        let mut facing = vec![false; n];
        for (i, f) in facing.iter_mut().enumerate() {
            let (a, c) = (b.ring[i], b.ring[(i + 1) % n]);
            let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
            let len = dx.hypot(dy);
            if len < 1e-9 {
                continue;
            }
            let normal = [dy / len, -dx / len];
            let m = [(a[0] + c[0]) / 2.0, (a[1] + c[1]) / 2.0];
            let (q, _) = nearest_axis(m);
            *f = normal[0] * (q[0] - m[0]) + normal[1] * (q[1] - m[1]) > 1e-9;
        }
        if !facing.iter().any(|f| *f) {
            facing = vec![true; n];
            own.push("no side faces the axis (it stands on it): every side taken".to_owned());
        }
        let mut spots: Vec<(String, P2)> = Vec::new();
        let mut corner = vec![false; n];
        for (i, _) in facing.iter().enumerate().filter(|(_, f)| **f) {
            corner[i] = true;
            corner[(i + 1) % n] = true;
        }
        let mut k = 0;
        for (p, _) in b.ring.iter().zip(&corner).filter(|(_, c)| **c) {
            k += 1;
            spots.push((format!("c{k}"), *p));
        }
        let mut k = 0;
        for (i, _) in facing.iter().enumerate().filter(|(_, f)| **f) {
            k += 1;
            let (a, c) = (b.ring[i], b.ring[(i + 1) % n]);
            spots.push((format!("m{k}"), [(a[0] + c[0]) / 2.0, (a[1] + c[1]) / 2.0]));
        }
        let nearest = boundary_samples(&b.ring, 0.25)
            .into_iter()
            .map(|p| (p, nearest_axis(p).1))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0);
        if let Some(p) = nearest
            && !spots
                .iter()
                .any(|(_, q)| (q[0] - p[0]).hypot(q[1] - p[1]) < 0.05)
        {
            spots.push(("n".to_owned(), p));
        }
        for f in 0..b.floors {
            let h = params.first_height + f as f64 * params.floor_height;
            for (tag, p) in &spots {
                places.push(PlaceOut {
                    name: format!("{}-F{}-{tag}-h{}", b.id, f + 1, fmt_height(h)),
                    x: p[0],
                    y: p[1],
                    height: h,
                });
            }
        }
    }
    if places.len() > MAX_PLACES {
        return Err(format!(
            "{} places of sensitive use: at most {MAX_PLACES} (raise simplify or min_area, or run \
             the line in parts)",
            places.len()
        ));
    }
    if model.routed {
        notes.push(
            "the line has routes: its spans are taken in the direction they are drawn; where the \
             routes run the other way a cut lands at 1 - its fraction (the report says so)"
                .to_owned(),
        );
    }
    if model.no_systems {
        notes.push("no wire has a system: every wire taken as a phase conductor".to_owned());
    }
    notes.push(format!(
        "{} buildings, {} cuts, {} places of sensitive use ({} m + {} m a floor above the ground; \
         floors = ceil((eaves - ground) / {} m), at least 1)",
        buildings.len(),
        cuts.len(),
        places.len(),
        params.first_height,
        params.floor_height,
        params.floor_height
    ));
    let mut out_buildings = Vec::new();
    for (b, own, _) in buildings {
        for n in own {
            notes.push(format!("{}: {n}", b.id));
        }
        out_buildings.push(b);
    }
    Ok(Analysis {
        buildings: out_buildings,
        cuts,
        places,
        notes,
        crossings,
    })
}

/// The footprint's stations along the span's chord: least and largest.
fn t_range(s: &Span, ring: &[P2]) -> (f64, f64) {
    ring.iter()
        .map(|p| s.local(*p).0)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| {
            (lo.min(t), hi.max(t))
        })
}

/// The station of the footprint's point nearest the axis within `lo`–`hi`
/// (a side parallel to the axis: its middle).
fn nearest_station(s: &Span, ring: &[P2], lo: f64, hi: f64) -> f64 {
    let samples: Vec<(f64, f64)> = boundary_samples(ring, 0.25)
        .into_iter()
        .map(|p| s.local(p))
        .filter(|(t, _)| (lo - 1e-9..=hi + 1e-9).contains(t))
        .map(|(t, d)| (t, d.abs()))
        .collect();
    let least = samples.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
    if !least.is_finite() {
        return (lo + hi) / 2.0;
    }
    let near: Vec<f64> = samples
        .iter()
        .filter(|x| x.1 <= least + 0.01)
        .map(|x| x.0)
        .collect();
    let (a, b) = near
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), t| {
            (a.min(*t), b.max(*t))
        });
    (a + b) / 2.0
}

/// A building's summary as a note (the report reads it back).
pub fn summary_note(b: &Building, model: &Model) -> String {
    let s = &model.spans[b.span];
    format!(
        "building {}: points {}; area {:.1} m²; centroid {:.2} {:.2}; ground {:.2} m; eaves {:.2} m; \
         roof {:.2} m; floors {}; span {}–{}; stations {:.1} {:.1} m; axis {:.2} m; conductor {:.2} m",
        b.id,
        b.points,
        b.area,
        b.centroid[0],
        b.centroid[1],
        b.ground,
        b.eaves,
        b.roof,
        b.floors,
        s.from,
        s.to,
        b.stations.0,
        b.stations.1,
        b.axis_distance,
        b.conductor_distance
    )
}

/// The presets: `buildings` (the cuts and the places) and `places` (the
/// places alone, quicker).
pub fn presets(
    params: &Params,
    model: &Model,
    points: &[InPoint],
    ground: &[f64],
) -> Result<Vec<PresetOut>, String> {
    let a = analyse(params, model, points, ground)?;
    let mut notes = a.notes.clone();
    notes.extend(a.buildings.iter().map(|b| summary_note(b, model)));
    let outlines: Vec<OutlineOut> = a
        .buildings
        .iter()
        .map(|b| OutlineOut {
            id: b.id.clone(),
            ring: b.ring.clone(),
            ground: b.ground,
            top: b.roof,
        })
        .collect();
    let mut with_cuts = notes.clone();
    with_cuts.extend(a.crossings.iter().cloned());
    Ok(vec![
        PresetOut {
            name: "buildings".into(),
            description: format!(
                "{} buildings near the line: the cuts across the line at each and their places of \
                 sensitive use",
                a.buildings.len()
            ),
            cuts: a.cuts.clone(),
            points: a.places.clone(),
            levels_ut: params.levels_ut.clone(),
            levels_kv_m: params.levels_kv_m.clone(),
            outlines: outlines.clone(),
            notes: with_cuts,
        },
        PresetOut {
            name: "places".into(),
            description: format!(
                "{} buildings near the line: their places of sensitive use alone",
                a.buildings.len()
            ),
            cuts: Vec::new(),
            points: a.places,
            levels_ut: params.levels_ut.clone(),
            levels_kv_m: params.levels_kv_m.clone(),
            outlines,
            notes,
        },
    ])
}
