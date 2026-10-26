// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Follow and snap** (T11 of the measuring tools spec, owner 2026-10-25):
//! select an object of the line (a span's wire, a jumper, a structure),
//! tick the LiDAR classes it snaps to, then move the cursor along it: for
//! the point of the object under the cursor the nearest **stored** LiDAR
//! point of those classes is found and measured live (to a wire's
//! surface), jumping from point to point as the cursor moves; a click
//! locks it and keeps it in the project. **N** jumps to the closest spot
//! of the whole object.
//!
//! Responsive first (owner): the points of the ticked classes around the
//! object are read once, in the background, at full density (the stored
//! points, not the thinned ones drawn), into a grid of plan cells; each
//! cursor move is then a search of the cells near the point, ring by
//! ring, stopped once no farther ring can hold a nearer point — exact,
//! never a farther guess. Nothing within the search radius: said so.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui;

use crate::measure::{Centrelines, Kind, Measurement, Pick, fraction_along, measure, pick_of};
use crate::model::Model;
use crate::selection::{Selection, Use};
use ohl_session::annotations::Anchor;

/// The class roles left out of the default classes: the line's own
/// points, and noise.
const LINE_ROLES: [&str; 5] = [
    "conductor",
    "shield_wire",
    "insulator",
    "structure",
    "noise",
];

/// The default search radius around the object, m.
pub const DEFAULT_RADIUS: f64 = 30.0;

/// The object followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// A span's wire.
    Span {
        /// Its section.
        section: String,
        /// The span (0: the section's first).
        span: usize,
    },
    /// A jumper.
    Jumper(String),
    /// A structure: its members (or stick drawing) and strings.
    Structure(String),
}

impl Subject {
    /// The object a click picked, if it can be followed (a span point or
    /// a spread: its span).
    pub fn of(s: &Selection) -> Option<Self> {
        Some(match s {
            Selection::Span { section, span } => Subject::Span {
                section: section.clone(),
                span: *span,
            },
            Selection::Jumper(id) => Subject::Jumper(id.clone()),
            Selection::Structure(id) => Subject::Structure(id.clone()),
            _ => return None,
        })
    }

    /// The object `s` is part of, if it can be followed: a span point or a
    /// spread its span, a wire point, string or point its structure.
    pub fn of_part(model: &Model, s: &Selection) -> Option<Self> {
        Self::of(s).or_else(|| match s {
            Selection::SpanPoint { section, label } => {
                let sec = model.line.sections.iter().find(|x| &x.id == section)?;
                let p = sec.span_points.iter().find(|p| &p.label == label)?;
                Some(Subject::Span {
                    section: section.clone(),
                    span: p.span,
                })
            }
            Selection::Spread { section, index } => Some(Subject::Span {
                section: section.clone(),
                span: crate::app::spread_span(&model.line, section, *index)?,
            }),
            Selection::Attachment { structure, .. }
            | Selection::String { structure, .. }
            | Selection::Point { structure, .. } => Some(Subject::Structure(structure.clone())),
            Selection::JumperPoint { jumper, .. } => Some(Subject::Jumper(jumper.clone())),
            _ => None,
        })
    }

    /// As selected (the views highlight it).
    pub fn selection(&self) -> Selection {
        match self {
            Subject::Span { section, span } => Selection::Span {
                section: section.clone(),
                span: *span,
            },
            Subject::Jumper(id) => Selection::Jumper(id.clone()),
            Subject::Structure(id) => Selection::Structure(id.clone()),
        }
    }

    /// In words.
    pub fn name(&self) -> String {
        match self {
            Subject::Span { section, span } => format!("wire {section} span {}", span + 1),
            Subject::Jumper(id) => format!("jumper {id}"),
            Subject::Structure(id) => format!("structure {id}"),
        }
    }

    /// Its lines in weather case `case` (a wire: its axis as solved).
    pub fn lines(&self, model: &Model, case: usize) -> Vec<Vec<[f64; 3]>> {
        match self {
            Subject::Span { section, span } => model
                .shape(section, case)
                .and_then(|s| s.spans.get(*span))
                .map(|s| vec![s.points.clone()])
                .unwrap_or_default(),
            Subject::Jumper(id) => model
                .jumper_shape(id, case)
                .map(|j| vec![j.points.clone()])
                .unwrap_or_default(),
            Subject::Structure(id) => model
                .line
                .structure(id)
                .map(|st| {
                    crate::scene::structure_lines(&model.bodies, st)
                        .into_iter()
                        .map(|(_, l)| l)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The pick at point `p` of it (on line `k` of [`Subject::lines`]).
    fn pick_at(
        &self,
        model: &Model,
        case: usize,
        lines: &[Vec<[f64; 3]>],
        k: usize,
        p: [f64; 3],
    ) -> Option<Pick> {
        let anchor = match self {
            Subject::Span { section, span } => Anchor::Wire {
                section: section.clone(),
                span: *span,
                fraction: fraction_along(&lines[k], p),
            },
            Subject::Jumper(id) => Anchor::Jumper {
                jumper: id.clone(),
                fraction: fraction_along(&lines[k], p),
            },
            Subject::Structure(id) => {
                let base = model.line.structure(id)?.position;
                Anchor::OnStructure {
                    structure: id.clone(),
                    offset: [0, 1, 2].map(|i| p[i] - base[i]),
                }
            }
        };
        pick_of(model, case, &anchor)
    }
}

/// The closest point to `p` on segment `a`–`b` and its distance.
fn on_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3], plan: bool) -> ([f64; 3], f64) {
    let dims = if plan { 2 } else { 3 };
    let ab: Vec<f64> = (0..dims).map(|i| b[i] - a[i]).collect();
    let l2: f64 = ab.iter().map(|v| v * v).sum();
    let u = if l2 > 0.0 {
        ((0..dims).map(|i| (p[i] - a[i]) * ab[i]).sum::<f64>() / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [0, 1, 2].map(|i| a[i] + u * (b[i] - a[i]));
    let d = (0..dims).map(|i| (q[i] - p[i]).powi(2)).sum::<f64>().sqrt();
    (q, d)
}

/// The point of `lines` nearest to `p` (in plan when `plan`): its line
/// and the point.
fn nearest_on(lines: &[Vec<[f64; 3]>], p: [f64; 3], plan: bool) -> Option<(usize, [f64; 3])> {
    let mut best: Option<(f64, usize, [f64; 3])> = None;
    for (k, l) in lines.iter().enumerate() {
        for w in l.windows(2) {
            let (q, d) = on_segment(p, w[0], w[1], plan);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, k, q));
            }
        }
    }
    best.map(|b| (b.1, b.2))
}

/// LiDAR points in plan cells, for nearest-point searches.
#[derive(Debug, Default)]
pub struct Cloud {
    /// The points: where, class.
    pub points: Vec<([f64; 3], u8)>,
    /// Cell size, m.
    cell: f64,
    /// The grid's lower corner.
    origin: [f64; 2],
    /// Cells across x and y.
    size: [usize; 2],
    /// Each cell's points: `start[c]..start[c + 1]` of `order`.
    start: Vec<u32>,
    order: Vec<u32>,
}

impl Cloud {
    /// The points in cells of `cell` m.
    pub fn new(points: Vec<([f64; 3], u8)>, cell: f64) -> Self {
        if points.is_empty() {
            return Self {
                cell,
                ..Self::default()
            };
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for (p, _) in &points {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let size = [0, 1].map(|i| (((hi[i] - lo[i]) / cell).floor() as usize + 1).max(1));
        let cell_of = |p: [f64; 3]| {
            let c = [0, 1].map(|i| (((p[i] - lo[i]) / cell) as usize).min(size[i] - 1));
            c[1] * size[0] + c[0]
        };
        let mut count = vec![0u32; size[0] * size[1] + 1];
        for (p, _) in &points {
            count[cell_of(*p) + 1] += 1;
        }
        for k in 1..count.len() {
            count[k] += count[k - 1];
        }
        let mut fill = count.clone();
        let mut order = vec![0u32; points.len()];
        for (k, (p, _)) in points.iter().enumerate() {
            let c = cell_of(*p);
            order[fill[c] as usize] = k as u32;
            fill[c] += 1;
        }
        Self {
            points,
            cell,
            origin: lo,
            size,
            start: count,
            order,
        }
    }

    /// The point nearest to `p` (3D) within `radius`: its index and
    /// distance. Exact: rings of cells around `p` until no farther ring
    /// can hold a nearer point.
    pub fn nearest(&self, p: [f64; 3], radius: f64) -> Option<(usize, f64)> {
        if self.points.is_empty() {
            return None;
        }
        let c = [0, 1].map(|i| ((p[i] - self.origin[i]) / self.cell).floor() as i64);
        let mut best: Option<(usize, f64)> = None;
        let rings = (radius / self.cell).ceil() as i64 + 1;
        for r in 0..=rings {
            // The nearest a point of ring r can be (in plan).
            let floor = ((r - 1).max(0) as f64) * self.cell;
            if floor > radius || best.is_some_and(|b| floor > b.1) {
                break;
            }
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (x, y) = (c[0] + dx, c[1] + dy);
                    if x < 0 || y < 0 || x >= self.size[0] as i64 || y >= self.size[1] as i64 {
                        continue;
                    }
                    let cell = y as usize * self.size[0] + x as usize;
                    for &k in &self.order[self.start[cell] as usize..self.start[cell + 1] as usize]
                    {
                        let q = self.points[k as usize].0;
                        let d = (0..3).map(|i| (q[i] - p[i]).powi(2)).sum::<f64>().sqrt();
                        if d <= radius && best.is_none_or(|b| d < b.1) {
                            best = Some((k as usize, d));
                        }
                    }
                }
            }
        }
        best
    }
}

/// What is read for: the object, its classes, its radius, the line and
/// the case (their change reads again).
type Key = (Subject, BTreeSet<u8>, u64, u64, usize);

/// What a measurement under the cursor was found for: the point under
/// the cursor, the points read and whether the case was solved.
type Found = (Use, Key, bool);

/// The tool's state.
#[derive(Default)]
pub struct Follow {
    /// The object followed (none: the next click picks it).
    pub subject: Option<Subject>,
    /// The classes snapped to (none chosen yet: the classes shown).
    pub classes: Option<BTreeSet<u8>>,
    /// The search radius, m.
    pub radius: Option<f64>,
    /// The points read, for `key`.
    cloud: Option<(Key, Result<Arc<Cloud>, String>)>,
    /// Points being read.
    loading: Option<(Key, Receiver<Result<Cloud, String>>)>,
    /// The live measurement: the object's point, the LiDAR point, the
    /// value.
    pub live: Option<Measurement>,
    /// What the tool says (loading, none within reach…).
    pub status: String,
    /// The live measurement found by N: kept until the cursor moves.
    pub pinned: bool,
    /// Its panel's tab was closed: the tool is put away.
    pub closed: bool,
    /// The point under the cursor last measured from.
    last_hover: Option<Use>,
    /// The last measurement under the cursor (the live one and what the
    /// tool said) and what it was found for: the point under the cursor,
    /// the points read (the object, classes, radius, line version, case)
    /// and whether the case was solved. Taken again while they stay the
    /// same (frames asked for by something else: points streaming).
    measured: Option<(Found, Option<Measurement>, String)>,
}

impl std::fmt::Debug for Follow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Follow")
            .field("subject", &self.subject)
            .field("classes", &self.classes)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Follow {
    /// The radius used.
    pub fn radius(&self) -> f64 {
        self.radius.unwrap_or(DEFAULT_RADIUS)
    }

    /// The classes snapped to: those chosen, else the classes shown but
    /// the line's own (conductors, shield wires, insulators, structures,
    /// by their roles in the class scheme) and noise — a wire would snap
    /// to its own points.
    pub fn classes(&self, model: &Model) -> BTreeSet<u8> {
        self.classes.clone().unwrap_or_else(|| {
            model
                .class_counts
                .iter()
                .map(|(c, _)| *c)
                .filter(|c| !model.hidden_classes.contains(c))
                .filter(|c| {
                    model
                        .scheme
                        .class(*c)
                        .is_none_or(|d| !d.roles.iter().any(|r| LINE_ROLES.contains(&r.as_str())))
                })
                .collect()
        })
    }

    /// The points of the object's surroundings, read in the background
    /// when what they are for changed; the cloud once read.
    fn cloud(&mut self, model: &Model, case: usize, ctx: &egui::Context) -> Option<Arc<Cloud>> {
        let subject = self.subject.clone()?;
        let classes = self.classes(model);
        let key: Key = (
            subject.clone(),
            classes.clone(),
            self.radius().to_bits(),
            model.line_version,
            case,
        );
        if let Some((k, c)) = &self.cloud
            && *k == key
        {
            return match c {
                Ok(c) => Some(c.clone()),
                Err(e) => {
                    self.status = e.clone();
                    None
                }
            };
        }
        if let Some((k, rx)) = &self.loading {
            if *k == key {
                match rx.try_recv() {
                    Ok(r) => {
                        let r = r.map(Arc::new);
                        self.cloud = Some((key, r.clone()));
                        self.loading = None;
                        return r.ok();
                    }
                    Err(TryRecvError::Empty) => return None,
                    Err(TryRecvError::Disconnected) => self.loading = None,
                }
            } else {
                self.loading = None;
            }
        }
        let Some(spec) = model.cloud.clone() else {
            self.status = "no point cloud in this project".into();
            return None;
        };
        let lines = subject.lines(model, case);
        let r = self.radius();
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in lines.iter().flatten() {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i] - r);
                hi[i] = hi[i].max(p[i] + r);
            }
        }
        if !lo[0].is_finite() {
            self.status = format!("{} is not drawn in this case", subject.name());
            return None;
        }
        let wanted: Vec<u8> = classes.into_iter().collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let read = || -> Result<Cloud, String> {
                let (file, name) = ohl_lidar::cloud_spec(&spec);
                let source =
                    ohl_pointcloud::CloudSource::open(file, name).map_err(|e| e.to_string())?;
                // Exit clause: a minute (a span's surroundings read in well
                // under a second).
                let budget =
                    ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(60));
                let mut points = Vec::new();
                source
                    .store()
                    .visit_rect(
                        source.cloud(),
                        lo,
                        hi,
                        Some(&wanted),
                        &budget,
                        &mut |x, y, z, c| {
                            points.push(([x, y, z], c));
                        },
                    )
                    .map_err(|e| e.to_string())?;
                Ok(Cloud::new(points, 2.0))
            };
            let r = read();
            log::info!(
                "follow and snap: {} points read in {:.1?}",
                r.as_ref().map_or(0, |c| c.points.len()),
                started.elapsed()
            );
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.loading = Some((key, rx));
        self.status = "reading the points around it…".into();
        None
    }

    /// The point under the cursor (`hover`, as a click would find it):
    /// the object's point nearest to it and the nearest LiDAR point of
    /// the classes from there, measured (to a wire's surface).
    pub fn hover(
        &mut self,
        model: &Model,
        case: usize,
        lines_of: &Centrelines,
        case_name: &str,
        hover: Option<&Use>,
        ctx: &egui::Context,
    ) {
        if self.pinned && hover == self.last_hover.as_ref() {
            return;
        }
        self.pinned = false;
        self.last_hover = hover.cloned();
        self.live = None;
        let Some(subject) = self.subject.clone() else {
            self.status = "click the object to follow: a wire, a jumper or a structure".into();
            return;
        };
        let Some(cloud) = self.cloud(model, case, ctx) else {
            return;
        };
        let Some(u) = hover else {
            self.status = format!("{}: move the cursor along it", subject.name());
            return;
        };
        let ready = model.cases.get(case).is_some_and(|c| c.ready);
        let Some((read, _)) = &self.cloud else {
            return;
        };
        let key: Found = (u.clone(), read.clone(), ready);
        if let Some((k, live, status)) = &self.measured
            && *k == key
        {
            self.live = live.clone();
            self.status = status.clone();
            return;
        }
        self.under(model, case, lines_of, case_name, &cloud, &subject, u);
        self.measured = Some((key, self.live.clone(), self.status.clone()));
    }

    /// The measurement from the object's point at `u` (the point under
    /// the cursor).
    #[allow(clippy::too_many_arguments)]
    fn under(
        &mut self,
        model: &Model,
        case: usize,
        lines_of: &Centrelines,
        case_name: &str,
        cloud: &Cloud,
        subject: &Subject,
        u: &Use,
    ) {
        let lines = subject.lines(model, case);
        // On the object itself: the point of it clicked; elsewhere the
        // object's point nearest (in plan) to the point under the cursor.
        let on_it = Subject::of(&u.target).as_ref() == Some(subject);
        let (k, p) = match (on_it, u.on, u.at) {
            (true, Some(on), _) => match nearest_on(&lines, on, false) {
                Some(x) => x,
                None => return,
            },
            (_, _, Some(at)) | (_, Some(at), None) => match nearest_on(&lines, at, true) {
                Some(x) => x,
                None => return,
            },
            _ => return,
        };
        self.measure_from(
            model, case, lines_of, case_name, cloud, subject, &lines, k, p,
        );
    }

    /// The measurement from point `p` of line `k` of the object.
    #[allow(clippy::too_many_arguments)]
    fn measure_from(
        &mut self,
        model: &Model,
        case: usize,
        lines_of: &Centrelines,
        case_name: &str,
        cloud: &Cloud,
        subject: &Subject,
        lines: &[Vec<[f64; 3]>],
        k: usize,
        p: [f64; 3],
    ) {
        let r = self.radius();
        let Some(from) = subject.pick_at(model, case, lines, k, p) else {
            return;
        };
        match cloud.nearest(from.at, r) {
            None => {
                self.status = format!(
                    "{}: no point of the classes within {r:.0} m here",
                    subject.name()
                );
            }
            Some((i, _)) => {
                let (q, class) = cloud.points[i];
                let name = model
                    .scheme
                    .class(class)
                    .map_or(format!("class {class}"), |c| c.name.clone());
                let to = Pick {
                    at: q,
                    what: format!("LiDAR point ({name})"),
                    wire: None,
                    anchor: Anchor::Lidar { at: q, class },
                };
                let m = measure(model, lines_of, Kind::Distance, &[from, to], case_name);
                self.status = format!(
                    "{}: {} to {name} — click to keep, N: the closest spot",
                    subject.name(),
                    m.label()
                );
                self.live = Some(m);
            }
        }
    }

    /// The closest spot of the whole object to the points of the classes
    /// (N): every point within the radius measured to the object's lines,
    /// the nearest pair taken. Exact on the lines as solved.
    pub fn closest(&mut self, model: &Model, case: usize, lines_of: &Centrelines, case_name: &str) {
        let Some(subject) = self.subject.clone() else {
            return;
        };
        let Some((_, Ok(cloud))) = &self.cloud else {
            return;
        };
        let cloud = cloud.clone();
        let lines = subject.lines(model, case);
        let started = std::time::Instant::now();
        // Boxes of 8 segments: a point farther from a box than the best
        // so far skips its segments.
        let mut boxes: Vec<(usize, usize, [f64; 3], [f64; 3])> = Vec::new();
        for (k, l) in lines.iter().enumerate() {
            for s in (0..l.len().saturating_sub(1)).step_by(8) {
                let e = (s + 8).min(l.len() - 1);
                let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                for p in &l[s..=e] {
                    for i in 0..3 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
                boxes.push((k, s, lo, hi));
            }
        }
        let gap = |p: [f64; 3], lo: [f64; 3], hi: [f64; 3]| {
            (0..3)
                .map(|i| (lo[i] - p[i]).max(p[i] - hi[i]).max(0.0).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let mut best: Option<(f64, usize, [f64; 3])> = None;
        for (q, _) in &cloud.points {
            for (k, s, lo, hi) in &boxes {
                if best.is_some_and(|b| gap(*q, *lo, *hi) >= b.0) {
                    continue;
                }
                let l = &lines[*k];
                for w in l[*s..=(*s + 8).min(l.len() - 1)].windows(2) {
                    let (on, d) = on_segment(*q, w[0], w[1], false);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, *k, on));
                    }
                }
            }
        }
        log::info!(
            "follow and snap: closest of {} points in {:.1?}",
            cloud.points.len(),
            started.elapsed()
        );
        match best {
            Some((_, k, p)) => self.measure_from(
                model, case, lines_of, case_name, &cloud, &subject, &lines, k, p,
            ),
            None => self.status = "no point of the classes around it".into(),
        }
    }

    /// The tool's panel: the object, the classes (ticks, staying open),
    /// the radius, what it says.
    pub fn panel(&mut self, ctx: &egui::Context, model: &Model) {
        let mut classes = self.classes(model);
        let mut radius = self.radius();
        let mut clear = false;
        let window = egui::Window::new("Follow and snap").default_width(260.0);
        let shown = crate::dockable::show(
            ctx,
            crate::dockable::Panel::Follow,
            "Follow and snap",
            window,
            None,
            |ui| {
                ui.label(match &self.subject {
                    Some(s) => format!("Following {}", s.name()),
                    None => "Click the object to follow".into(),
                });
                ui.label(egui::RichText::new(&self.status).small());
                ui.separator();
                ui.label(egui::RichText::new("Snap to the LiDAR classes").strong());
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (c, n) in &model.class_counts {
                            let name = model
                                .scheme
                                .class(*c)
                                .map_or(format!("class {c}"), |d| d.name.clone());
                            let mut on = classes.contains(c);
                            if ui.checkbox(&mut on, format!("{name} ({n})")).changed() {
                                if on {
                                    classes.insert(*c);
                                } else {
                                    classes.remove(c);
                                }
                            }
                        }
                        if model.class_counts.is_empty() {
                            ui.label(egui::RichText::new("No LiDAR points loaded").weak());
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label("Search within");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut radius)
                            .range(1.0..=200.0)
                            .suffix(" m"),
                    );
                });
                ui.horizontal(|ui| {
                    if self.subject.is_some() && ui.button("Another object").clicked() {
                        clear = true;
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Move the cursor along the object; a click keeps the measurement; \
                         N: the closest spot; Escape: another object.",
                    )
                    .small()
                    .weak(),
                );
            },
        );
        // Its tab closed: the tool is put away.
        self.closed = shown == crate::dockable::Shown::Closed;
        if classes != self.classes(model) {
            self.classes = Some(classes);
        }
        if (radius - self.radius()).abs() > 1e-9 {
            self.radius = Some(radius);
        }
        if clear {
            self.subject = None;
            self.live = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid's nearest point is the brute force's, within the radius
    /// only (none beyond it).
    #[test]
    fn nearest_is_exact() {
        let mut s = 12345u64;
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        let points: Vec<([f64; 3], u8)> = (0..5000)
            .map(|_| ([rnd() * 200.0, rnd() * 60.0, rnd() * 30.0], 5))
            .collect();
        let cloud = Cloud::new(points.clone(), 2.0);
        for _ in 0..200 {
            let p = [rnd() * 220.0 - 10.0, rnd() * 80.0 - 10.0, rnd() * 40.0];
            for r in [3.0, 15.0] {
                let brute = points
                    .iter()
                    .enumerate()
                    .map(|(k, (q, _))| {
                        (k, (0..3).map(|i| (q[i] - p[i]).powi(2)).sum::<f64>().sqrt())
                    })
                    .filter(|(_, d)| *d <= r)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let got = cloud.nearest(p, r);
                assert_eq!(got.map(|g| g.1), brute.map(|b| b.1), "{p:?} {r}");
            }
        }
    }

    /// The point under the cursor unchanged (a frame asked for by
    /// something else): the measurement found for it is taken again, the
    /// same as measured.
    #[test]
    fn the_same_point_is_taken_again() {
        let m = crate::tests::synthetic();
        let ctx = egui::Context::default();
        let section = m.line.sections[0].id.clone();
        let points = m.shape(&section, 0).unwrap().spans[0].points.clone();
        let mid = crate::measure::point_along(&points, 0.5).unwrap();
        let subject = Subject::Span {
            section: section.clone(),
            span: 0,
        };
        let mut f = Follow {
            subject: Some(subject.clone()),
            ..Follow::default()
        };
        let key: Key = (
            subject,
            f.classes(&m),
            f.radius().to_bits(),
            m.line_version,
            0,
        );
        let cloud = Cloud::new(vec![([mid[0], mid[1], mid[2] - 10.0], 2)], 2.0);
        f.cloud = Some((key, Ok(Arc::new(cloud))));
        let lines = Centrelines::of(&m);
        let u = Use {
            target: Selection::Span { section, span: 0 },
            at: None,
            on: Some(mid),
        };
        f.hover(&m, 0, &lines, "c", Some(&u), &ctx);
        let first = f.live.as_ref().expect("a point within reach").label();
        assert!(f.measured.is_some());
        // Taken away meanwhile (N found another, unpinned): the same
        // point gives its measurement back.
        f.live = None;
        f.hover(&m, 0, &lines, "c", Some(&u), &ctx);
        assert_eq!(f.live.as_ref().map(Measurement::label), Some(first));
    }

    /// The object's point nearest to a point, in plan and in 3D.
    #[test]
    fn nearest_on_lines() {
        let lines = vec![vec![[0.0, 0.0, 20.0], [100.0, 0.0, 20.0]]];
        let (k, p) = nearest_on(&lines, [40.0, 5.0, 0.0], true).unwrap();
        assert_eq!(k, 0);
        assert!((p[0] - 40.0).abs() < 1e-12 && (p[2] - 20.0).abs() < 1e-12);
    }
}
