// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What is selected, shared by all views; what can be clicked (snaps);
//! picking.

use std::collections::BTreeSet;

use crate::model::Model;

/// The selected object.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selection {
    /// Nothing.
    #[default]
    None,
    /// A structure, by id.
    Structure(String),
    /// A jumper, by id.
    Jumper(String),
    /// A span: section id and span index.
    Span {
        /// Section id.
        section: String,
        /// Span index in the section.
        span: usize,
    },
    /// A wire point of a structure where wires end (with the strings
    /// holding it), by its set.
    Attachment {
        /// Structure id.
        structure: String,
        /// Set.
        set: u32,
    },
    /// A string of a structure (one of a section's, or a helping string
    /// of a jumper), by its label.
    String {
        /// Structure id.
        structure: String,
        /// The string's label.
        label: String,
    },
    /// A point of a structure strings hang from: a structure point or a
    /// wire point, by its label (picked while Add string is held).
    Point {
        /// Structure id.
        structure: String,
        /// The point's label.
        label: String,
    },
    /// An alignment point (on any number of alignments).
    AlignmentPoint {
        /// Its id.
        id: String,
    },
    /// A point on a span where spacers are clamped, by its section and
    /// label.
    SpanPoint {
        /// Section id.
        section: String,
        /// The span point's label.
        label: String,
    },
    /// A spread object on a span, by its section and its index among
    /// the section's spreads (their labels may be empty).
    Spread {
        /// Section id.
        section: String,
        /// Index in the section's spreads.
        index: usize,
    },
    /// An interphase spacer, by id.
    Spacer(String),
    /// A point along a jumper, by its jumper and label.
    JumperPoint {
        /// Jumper id.
        jumper: String,
        /// The point's label.
        label: String,
    },
    /// A LiDAR point shown (index in the model's points).
    LidarPoint(usize),
    /// A point of the ground surface (survey; index in its points).
    SurveyPoint(usize),
}

/// A right-click in a view: what is there, and the point on the ground
/// under it (when the view knows one).
#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    /// The object right-clicked (or nothing).
    pub target: Selection,
    /// The ground under the pointer (the crosshair in first person).
    pub at: Option<[f64; 3]>,
    /// On a span: the point of its wire clicked ([`on_span`]).
    pub on: Option<[f64; 3]>,
}

/// The point of span `target` (as drawn in weather case `case`) nearest to
/// `at` on screen: on its wire, between the drawn points.
pub fn on_span(
    model: &Model,
    case: usize,
    target: &Selection,
    at: [f64; 2],
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Option<[f64; 3]> {
    let points = match target {
        Selection::Span { section, span } => &model.shape(section, case)?.spans.get(*span)?.points,
        Selection::Jumper(id) => &model.jumper_shape(id, case)?.points,
        // A spread: on the span it lies on (the tools other than Delete
        // act on that span).
        Selection::Spread { section, index } => {
            let span = crate::app::spread_span(&model.line, section, *index)?;
            &model.shape(section, case)?.spans.get(span)?.points
        }
        _ => return None,
    };
    let mut best: Option<(f64, [f64; 3])> = None;
    for w in points.windows(2) {
        let (Some(a), Some(b)) = (project(w[0]), project(w[1])) else {
            continue;
        };
        let d = [b[0] - a[0], b[1] - a[1]];
        let len2 = d[0] * d[0] + d[1] * d[1];
        let u = if len2 > 0.0 {
            (((at[0] - a[0]) * d[0] + (at[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = [a[0] + u * d[0], a[1] + u * d[1]];
        let e = (q[0] - at[0]).hypot(q[1] - at[1]);
        if best.is_none_or(|b| e < b.0) {
            best = Some((e, [0, 1, 2].map(|k| w[0][k] + u * (w[1][k] - w[0][k]))));
        }
    }
    best.map(|b| b.1)
}

/// What can be clicked (snapped to), switched on and off in the Snap menu (Edit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Snap {
    /// Structures (anywhere along their drawing).
    Structure,
    /// Spans (anywhere along the wire).
    Span,
    /// Jumpers (anywhere along them).
    Jumper,
    /// Attachment points: insulator top and wire clamp.
    Attachment,
    /// Alignment points of the routes.
    AlignmentPoint,
    /// LiDAR points shown.
    LidarPoint,
    /// Points of the ground surface (survey) shown.
    SurveyPoint,
    /// Structure points and wire points, by label (only while a tool
    /// that hangs strings from them is held; not in the Snap menu).
    Point,
}

impl Snap {
    /// Every kind, in the order of the Snap menu.
    pub const ALL: [Snap; 7] = [
        Snap::Structure,
        Snap::Span,
        Snap::Jumper,
        Snap::Attachment,
        Snap::AlignmentPoint,
        Snap::LidarPoint,
        Snap::SurveyPoint,
    ];

    /// Name in the Snap menu (Edit).
    pub fn name(self) -> &'static str {
        match self {
            Snap::Structure => "Structures",
            Snap::Span => "Spans (wires)",
            Snap::Jumper => "Jumpers",
            Snap::Attachment => "Strings and attachment points",
            Snap::AlignmentPoint => "Alignment points",
            Snap::LidarPoint => "LiDAR points",
            Snap::SurveyPoint => "Survey points (ground surface)",
            Snap::Point => "Structure and wire points",
        }
    }

    /// Key for saving.
    pub fn key(self) -> &'static str {
        match self {
            Snap::Structure => "structure",
            Snap::Span => "span",
            Snap::Jumper => "jumper",
            Snap::Attachment => "attachment",
            Snap::AlignmentPoint => "alignment_point",
            Snap::LidarPoint => "lidar_point",
            Snap::SurveyPoint => "survey_point",
            Snap::Point => "point",
        }
    }

    /// Picked before others at about the same distance (higher first):
    /// points over the lines they sit on, structures over wires.
    fn priority(self) -> f64 {
        match self {
            Snap::Point => 7.0,
            Snap::Attachment | Snap::AlignmentPoint => 6.0,
            Snap::LidarPoint | Snap::SurveyPoint => 4.0,
            Snap::Structure => 3.0,
            Snap::Jumper => 1.0,
            Snap::Span => 0.0,
        }
    }
}

/// The kinds that can be clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snaps(pub BTreeSet<Snap>);

impl Default for Snaps {
    fn default() -> Self {
        Self(
            [Snap::Structure, Snap::Span, Snap::Jumper]
                .into_iter()
                .collect(),
        )
    }
}

impl Snaps {
    /// Whether `kind` can be clicked.
    pub fn on(&self, kind: Snap) -> bool {
        self.0.contains(&kind)
    }

    /// Saved as a comma-separated list of keys.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let keys: Vec<&str> = self.0.iter().map(|k| k.key()).collect();
        storage.set_string("snaps", keys.join(","));
    }

    /// As saved by [`Snaps::save`], if saved.
    pub fn load(storage: &dyn eframe::Storage) -> Option<Snaps> {
        let text = storage.get_string("snaps")?;
        Some(Snaps(
            Snap::ALL
                .into_iter()
                .filter(|k| text.split(',').any(|x| x == k.key()))
                .collect(),
        ))
    }
}

impl Selection {
    /// The kind of object.
    pub fn kind(&self) -> Option<Snap> {
        Some(match self {
            Selection::None => return None,
            Selection::Structure(_) => Snap::Structure,
            Selection::Jumper(_) => Snap::Jumper,
            Selection::Span { .. } => Snap::Span,
            Selection::Attachment { .. } | Selection::String { .. } => Snap::Attachment,
            Selection::Point { .. } => Snap::Point,
            Selection::SpanPoint { .. } | Selection::Spread { .. } | Selection::Spacer(_) => {
                Snap::Span
            }
            Selection::JumperPoint { .. } => Snap::Jumper,
            Selection::AlignmentPoint { .. } => Snap::AlignmentPoint,
            Selection::LidarPoint(_) => Snap::LidarPoint,
            Selection::SurveyPoint(_) => Snap::SurveyPoint,
        })
    }

    /// Text for the status bar.
    pub fn describe(&self) -> String {
        match self {
            Selection::None => "nothing selected".into(),
            Selection::Structure(id) => format!("structure {id}"),
            Selection::Span { section, span } => format!("{section} span {}", span + 1),
            Selection::Jumper(id) => format!("jumper {id}"),
            Selection::Attachment { structure, set } => format!("{structure} set {set}"),
            Selection::String { structure, label } => format!("{structure} string {label}"),
            Selection::Point { structure, label } => format!("{structure} point {label}"),
            Selection::SpanPoint { section, label } => format!("{section} span point {label}"),
            Selection::Spread { section, index } => format!("{section} spread {}", index + 1),
            Selection::Spacer(id) => format!("spacer {id}"),
            Selection::JumperPoint { jumper, label } => format!("jumper {jumper} point {label}"),
            Selection::AlignmentPoint { id } => {
                format!("alignment point {id}")
            }
            Selection::LidarPoint(k) => format!("LiDAR point {k}"),
            Selection::SurveyPoint(k) => format!("survey point {}", k + 1),
        }
    }

    /// A point to look at.
    pub fn focus(&self, model: &Model, case: usize) -> Option<[f64; 3]> {
        match self {
            Selection::None => None,
            Selection::Structure(id) => model.line.structure(id).map(|s| {
                let top = s
                    .wires
                    .iter()
                    .map(|w| w.position[2])
                    .fold(s.position[2], f64::max);
                [s.position[0], s.position[1], 0.5 * (s.position[2] + top)]
            }),
            Selection::Jumper(id) => model
                .cases
                .get(case)?
                .jumpers
                .iter()
                .find(|j| &j.jumper == id)
                .map(|j| j.points[j.points.len() / 2]),
            Selection::JumperPoint { jumper, label } => model
                .jumper_shape(jumper, case)?
                .point_positions
                .iter()
                .find(|(l, _)| l == label)
                .map(|(_, p)| *p),
            Selection::Span { section, span } => model
                .shape(section, case)?
                .spans
                .get(*span)
                .map(|s| s.mid_point),
            Selection::Attachment { structure, set } => model
                .line
                .wire_point(&ols_line::SectionPoint::new(structure.clone(), *set))
                .map(|w| w.position),
            Selection::String { structure, label } => {
                let (a, b) = string_pieces(model, case)
                    .remove(&(structure.clone(), label.clone()))
                    .and_then(|v| v.first().copied())
                    .or_else(|| {
                        let s = model.line.structure(structure)?;
                        let x = s.string(label)?;
                        Some((s.node(&x.top)?.position(), s.wire(&x.wire)?.position))
                    })?;
                Some([0, 1, 2].map(|k| 0.5 * (a[k] + b[k])))
            }
            Selection::Point { structure, label } => model
                .line
                .structure(structure)?
                .node(label)
                .map(|n| n.position()),
            Selection::SpanPoint { section, label } => model
                .shape(section, case)?
                .span_points
                .iter()
                .find(|(l, _)| l == label)
                .map(|(_, p)| *p),
            Selection::Spread { section, index } => {
                let stretch = spread_stretch(model, case, section, *index)?;
                Some(stretch[stretch.len() / 2])
            }
            Selection::Spacer(id) => {
                spacer_ends(model, case, id).map(|[a, b]| [0, 1, 2].map(|k| 0.5 * (a[k] + b[k])))
            }
            Selection::AlignmentPoint { id } => alignment_point(model, id),
            Selection::LidarPoint(_) | Selection::SurveyPoint(_) => position(model, self),
        }
    }
}

/// The stretch of wire spread `index` of section `section` lies on, as
/// solved in weather case `case`.
pub fn spread_stretch(
    model: &Model,
    case: usize,
    section: &str,
    index: usize,
) -> Option<Vec<[f64; 3]>> {
    let shape = model.shape(section, case)?;
    let sec = model.line.sections.iter().find(|s| s.id == section)?;
    crate::scene::spreads_drawn(&model.line.section_with_fractions(sec), shape)
        .into_iter()
        .find(|(k, _)| *k == index)
        .map(|(_, s)| s)
}

/// A spacer's ends as solved in weather case `case` (reported by the
/// section at its end `a`).
pub fn spacer_ends(model: &Model, case: usize, id: &str) -> Option<[[f64; 3]; 2]> {
    spacer_state(model, case, id).map(|x| x.ends)
}

/// A spacer as solved in weather case `case`.
pub fn spacer_state<'a>(
    model: &'a Model,
    case: usize,
    id: &str,
) -> Option<&'a ols_line::solve::SpacerState> {
    let s = model.line.spacers.iter().find(|s| s.id == id)?;
    model
        .shape(&s.a.section, case)?
        .spacers
        .iter()
        .find(|x| x.id == id)
}

/// The exact coordinates of a LiDAR or survey point.
pub fn position(model: &Model, s: &Selection) -> Option<[f64; 3]> {
    match s {
        Selection::LidarPoint(k) => model.points.get(*k).map(|(q, _)| lidar_xyz(model, q)),
        Selection::SurveyPoint(k) => model.ground.as_ref()?.points().get(*k).copied(),
        _ => None,
    }
}

/// An alignment point's position as drawn (on the ground).
pub fn alignment_point(model: &Model, id: &str) -> Option<[f64; 3]> {
    model
        .line
        .alignment_point(id)
        .map(|p| draped(model, p.position))
}

/// An alignment point as drawn: on the ground where there is ground.
pub fn draped(model: &Model, p: [f64; 3]) -> [f64; 3] {
    let z = model
        .ground
        .as_ref()
        .and_then(|g| g.height(p[0], p[1]))
        .unwrap_or(p[2]);
    [p[0], p[1], z + 0.3]
}

/// A LiDAR point's project coordinates.
pub fn lidar_xyz(model: &Model, q: &[i32; 3]) -> [f64; 3] {
    [0, 1, 2].map(|k| model.origin[k] + f64::from(q[k]) / 1000.0)
}

/// A piece that selects something when clicked: a segment (`a` = `b` for
/// a point).
pub type Candidate = (Selection, [f64; 3], [f64; 3]);

/// Strings as drawn: (top, bottom) pieces by (structure, label).
pub type StringPieces = std::collections::HashMap<(String, String), Vec<([f64; 3], [f64; 3])>>;

/// Where each string is drawn in weather case `case` (and its mirror, the
/// wind from the other side): (top, bottom) by (structure, label) — a
/// section's strings as solved, a jumper's helping strings to their
/// clamps.
pub fn string_pieces(model: &Model, case: usize) -> StringPieces {
    let mut out = StringPieces::new();
    for solved in model.cases.get(case).into_iter().flat_map(|c| c.drawn()) {
        for shape in &solved.shapes {
            let Some(sec) = model.line.sections.iter().find(|s| s.id == shape.section) else {
                continue;
            };
            for (p, strings) in sec.points.iter().zip(&shape.strings) {
                for x in strings {
                    out.entry((p.structure.clone(), x.label.clone()))
                        .or_default()
                        .push((x.top, x.bottom));
                }
            }
        }
        for j in &solved.jumpers {
            for s in &j.supports {
                for x in &s.strings {
                    out.entry((s.structure.clone(), x.label.clone()))
                        .or_default()
                        .push((x.top, s.clamp));
                }
            }
        }
    }
    out
}

/// The wire points where the strings hold them in `pieces` (as given
/// when not drawn): as attachments (with `tops`, the tops of the strings
/// holding a set) and as labelled points, of the kinds switched on.
fn wire_point_candidates(
    model: &Model,
    pieces: &StringPieces,
    snaps: &Snaps,
    tops: bool,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for s in &model.line.structures {
        if snaps.on(Snap::Attachment) {
            // Every wire point and string top, for the set(s) it holds
            // (the first one of a TAK's); a clamp where it is drawn.
            for w in &s.wires {
                let c = s.component(&w.label);
                let Some(set) = w
                    .set
                    .or_else(|| c.wires.iter().find_map(|&i| s.wires[i].set))
                else {
                    continue;
                };
                let sel = Selection::Attachment {
                    structure: s.id.clone(),
                    set,
                };
                let mut drawn = false;
                for x in s.strings_to(&w.label) {
                    for (_, bottom) in pieces
                        .get(&(s.id.clone(), x.label.clone()))
                        .into_iter()
                        .flatten()
                    {
                        out.push((sel.clone(), *bottom, *bottom));
                        drawn = true;
                    }
                }
                if !drawn && tops {
                    out.push((sel.clone(), w.position, w.position));
                }
                if tops && w.set.is_some() {
                    for x in s.strings_to(&w.label) {
                        if let Some(top) = s.node(&x.top) {
                            out.push((sel.clone(), top.position(), top.position()));
                        }
                    }
                }
            }
        }
        if snaps.on(Snap::Point) {
            // Every structure point, and every wire point where it is
            // drawn.
            let sel = |label: &str| Selection::Point {
                structure: s.id.clone(),
                label: label.to_owned(),
            };
            if tops {
                for p in &s.points {
                    out.push((sel(&p.label), p.position, p.position));
                }
            }
            for w in &s.wires {
                let at = s.strings_to(&w.label).find_map(|x| {
                    pieces
                        .get(&(s.id.clone(), x.label.clone()))
                        .and_then(|v| v.last().map(|(_, b)| *b))
                });
                match at {
                    Some(at) => out.push((sel(&w.label), at, at)),
                    None if tops => out.push((sel(&w.label), w.position, w.position)),
                    None => {}
                }
            }
        }
    }
    out
}

/// A solved jumper (its pieces) and its points.
fn jumper_candidates(j: &ols_line::jumper::JumperShape) -> Vec<Candidate> {
    let sel = Selection::Jumper(j.jumper.clone());
    let mut out: Vec<Candidate> = j
        .points
        .windows(2)
        .map(|w| (sel.clone(), w[0], w[1]))
        .collect();
    for (label, p) in &j.point_positions {
        out.push((
            Selection::JumperPoint {
                jumper: j.jumper.clone(),
                label: label.clone(),
            },
            *p,
            *p,
        ));
    }
    out
}

/// The span points, spread objects (all along their stretch) and spacers
/// of `shape` (as solved in one case).
fn span_object_candidates(model: &Model, shape: &ols_line::solve::SectionShape) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Some(sec) = model.line.sections.iter().find(|s| s.id == shape.section) {
        let sec = model.line.section_with_fractions(sec);
        for (index, stretch) in crate::scene::spreads_drawn(&sec, shape) {
            let sel = Selection::Spread {
                section: shape.section.clone(),
                index,
            };
            for w in stretch.windows(2) {
                out.push((sel.clone(), w[0], w[1]));
            }
        }
    }
    for (label, p) in &shape.span_points {
        out.push((
            Selection::SpanPoint {
                section: shape.section.clone(),
                label: label.clone(),
            },
            *p,
            *p,
        ));
    }
    for s in &shape.spacers {
        out.push((Selection::Spacer(s.id.clone()), s.ends[0], s.ends[1]));
    }
    out
}

/// What can be clicked, of the kinds switched on: structures (every line
/// they are drawn with but their strings), spans and jumpers (every
/// piece), strings (all along them, where they are drawn: as solved in
/// the case shown and its mirror) and attachment points (string tops and
/// clamps, where drawn), alignment points. LiDAR points are picked
/// separately ([`pick_in_model`]).
pub fn candidates(model: &Model, case: usize, snaps: &Snaps) -> Vec<Candidate> {
    use crate::scene::Part;
    let mut out = Vec::new();
    // Strings are clicked with the structures or the attachment points.
    let strings = snaps.on(Snap::Attachment) || snaps.on(Snap::Structure) || snaps.on(Snap::Point);
    let pieces = if strings {
        string_pieces(model, case)
    } else {
        Default::default()
    };
    for s in &model.line.structures {
        if snaps.on(Snap::Structure) {
            let sel = Selection::Structure(s.id.clone());
            out.push((sel.clone(), s.position, s.position));
            for (part, pts) in crate::scene::structure_lines(s) {
                if part == Part::Insulator {
                    continue;
                }
                for w in pts.windows(2) {
                    out.push((sel.clone(), w[0], w[1]));
                }
            }
        }
        if strings {
            // Every string all along it, where drawn (as given when not
            // solved).
            for x in &s.strings {
                let sel = Selection::String {
                    structure: s.id.clone(),
                    label: x.label.clone(),
                };
                match pieces.get(&(s.id.clone(), x.label.clone())) {
                    Some(drawn) => {
                        for (a, b) in drawn {
                            out.push((sel.clone(), *a, *b));
                        }
                    }
                    None => {
                        if let (Some(top), Some(w)) = (s.node(&x.top), s.wire(&x.wire)) {
                            out.push((sel.clone(), top.position(), w.position));
                        }
                    }
                }
            }
        }
    }
    out.extend(wire_point_candidates(model, &pieces, snaps, true));
    if let Some(c) = model.cases.get(case) {
        if snaps.on(Snap::Span) {
            for shape in model
                .line
                .sections
                .iter()
                .filter_map(|s| model.shape(&s.id, case))
            {
                for (k, span) in shape.spans.iter().enumerate() {
                    let sel = Selection::Span {
                        section: shape.section.clone(),
                        span: k,
                    };
                    for w in span.points.windows(2) {
                        out.push((sel.clone(), w[0], w[1]));
                    }
                }
                // Its span points and the spacers it holds.
                out.extend(span_object_candidates(model, shape));
            }
        }
        if snaps.on(Snap::Jumper) {
            for j in &c.jumpers {
                out.extend(jumper_candidates(j));
            }
        }
    }
    if snaps.on(Snap::AlignmentPoint) {
        for a in &model.line.alignment_points {
            let p = draped(model, a.position);
            out.push((Selection::AlignmentPoint { id: a.id.clone() }, p, p));
        }
    }
    out
}

/// The weather cases drawn: those shown for the line, and every
/// section's display cases.
pub fn drawn_cases(model: &Model, case: usize) -> std::collections::BTreeSet<usize> {
    let mut out: std::collections::BTreeSet<usize> = model.shown(case).into_iter().collect();
    for s in &model.line.sections {
        out.extend(model.section_cases(&s.id, case));
    }
    out.retain(|k| *k < model.cases.len());
    out
}

/// The copies of spans, jumpers and strings drawn in the other weather
/// cases (and their wind's other side) than [`candidates`] has, each with
/// its case: a click on any of them picks the object.
fn other_case_candidates(model: &Model, case: usize, snaps: &Snaps) -> Vec<(Candidate, usize)> {
    let mut out = Vec::new();
    let strings = snaps.on(Snap::Attachment) || snaps.on(Snap::Structure) || snaps.on(Snap::Point);
    for k in drawn_cases(model, case) {
        let Some(c) = model.cases.get(k) else {
            continue;
        };
        for (side, solved) in c.drawn().enumerate() {
            if snaps.on(Snap::Span) {
                for shape in &solved.shapes {
                    let cases = model.section_cases(&shape.section, case);
                    // Drawn in this case; the first case's own side is
                    // among the candidates already.
                    if !cases.contains(&k) || (k == cases[0] && side == 0) {
                        continue;
                    }
                    for (n, span) in shape.spans.iter().enumerate() {
                        let sel = Selection::Span {
                            section: shape.section.clone(),
                            span: n,
                        };
                        for w in span.points.windows(2) {
                            out.push(((sel.clone(), w[0], w[1]), k));
                        }
                    }
                    out.extend(
                        span_object_candidates(model, shape)
                            .into_iter()
                            .map(|c| (c, k)),
                    );
                }
            }
            if snaps.on(Snap::Jumper) && !(k == case && side == 0) && model.shown(case).contains(&k)
            {
                for j in &solved.jumpers {
                    out.extend(jumper_candidates(j).into_iter().map(|c| (c, k)));
                }
            }
        }
        if k != case && (strings || snaps.on(Snap::Attachment)) {
            let pieces = string_pieces(model, k);
            // The wire points where they hang in this case.
            out.extend(
                wire_point_candidates(model, &pieces, snaps, false)
                    .into_iter()
                    .map(|c| (c, k)),
            );
            if strings {
                for ((structure, label), pieces) in pieces {
                    let sel = Selection::String { structure, label };
                    for (a, b) in pieces {
                        out.push(((sel.clone(), a, b), k));
                    }
                }
            }
        }
    }
    out
}

/// Picks among everything switched on in `snaps`: the [`candidates`], and
/// the LiDAR points straight from the model (millions of them: not made
/// into candidates).
pub fn pick_in_model(
    model: &Model,
    case: usize,
    snaps: &Snaps,
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Selection {
    pick_in_model_case(model, case, snaps, at, radius, project).0
}

/// [`pick_in_model`], also in every other weather case drawn: what was
/// picked, and the case of the copy clicked where it is not the one shown
/// (`None`: the case shown, or nothing case-bound).
pub fn pick_in_model_case(
    model: &Model,
    case: usize,
    snaps: &Snaps,
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> (Selection, Option<usize>) {
    let mut best = pick_scored(&candidates(model, case, snaps), at, radius, &project);
    let mut picked_case = None;
    let others = other_case_candidates(model, case, snaps);
    let pieces: Vec<Candidate> = others.iter().map(|x| x.0.clone()).collect();
    if let Some((score, i)) = pick_scored_index(&pieces, at, radius, &project)
        && best.as_ref().is_none_or(|b| score < b.0)
    {
        best = Some((score, pieces[i].0.clone()));
        picked_case = Some(others[i].1);
    }
    let found = best.as_ref().map(|b| b.1.clone());
    if snaps.on(Snap::SurveyPoint)
        && let Some(g) = model.ground.as_ref()
    {
        let bonus = Snap::SurveyPoint.priority();
        for (k, p) in g.points().iter().enumerate() {
            let Some(s) = project(*p) else {
                continue;
            };
            let d = (s[0] - at[0]).hypot(s[1] - at[1]);
            if d <= radius && best.as_ref().is_none_or(|b| d - bonus < b.0) {
                best = Some((d - bonus, Selection::SurveyPoint(k)));
            }
        }
    }
    if snaps.on(Snap::LidarPoint) {
        let bonus = Snap::LidarPoint.priority();
        for (k, (q, class)) in model.points.iter().enumerate() {
            if model.hidden_classes.contains(class) {
                continue;
            }
            let Some(s) = project(lidar_xyz(model, q)) else {
                continue;
            };
            let d = (s[0] - at[0]).hypot(s[1] - at[1]);
            if d <= radius && best.as_ref().is_none_or(|b| d - bonus < b.0) {
                best = Some((d - bonus, Selection::LidarPoint(k)));
            }
        }
    }
    let chosen = best.map_or(Selection::None, |b| b.1);
    // The case only for the copy chosen (a point may have won after).
    let case_of = (found.as_ref() == Some(&chosen))
        .then_some(picked_case)
        .flatten();
    (chosen, case_of)
}

/// The candidate nearest to `at` within `radius` (as [`pick_scored`]):
/// its score and index.
fn pick_scored_index(
    candidates: &[Candidate],
    at: [f64; 2],
    radius: f64,
    project: &impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Option<(f64, usize)> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(i, (s, a, b))| {
            let (pa, pb) = (project(*a)?, project(*b)?);
            let d = ols_core::geometry::distance_to_segment(at, pa, pb);
            let bonus = match s {
                Selection::String { .. } => 2.5,
                // Points on the wire over the wire; a spacer over both
                // wires it joins.
                Selection::SpanPoint { .. } | Selection::JumperPoint { .. } => 7.0,
                // A spread over the wire it lies on, under its points.
                Selection::Spread { .. } => 2.0,
                Selection::Spacer(_) => 1.0,
                _ => s.kind().map_or(0.0, Snap::priority),
            };
            (d <= radius).then_some((d - bonus, i))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// The candidate nearest to `at` on screen within `radius` pixels, given a
/// projection to screen coordinates (`None`: not visible). At about the
/// same distance, points win over the lines they sit on, structures over
/// wires ([`Snap`] priorities).
#[cfg(test)]
pub fn pick(
    candidates: &[Candidate],
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Selection {
    pick_scored(candidates, at, radius, &project).map_or(Selection::None, |b| b.1)
}

/// The best candidate with its score (distance less priority).
fn pick_scored(
    candidates: &[Candidate],
    at: [f64; 2],
    radius: f64,
    project: &impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Option<(f64, Selection)> {
    candidates
        .iter()
        .filter_map(|(s, a, b)| {
            let (pa, pb) = (project(*a)?, project(*b)?);
            let d = ols_core::geometry::distance_to_segment(at, pa, pb);
            // A string's body under the structure it hangs on (where it
            // crosses the mast or an arm) and the points at its ends, over
            // the wires.
            let bonus = match s {
                Selection::String { .. } => 2.5,
                // Points on the wire over the wire; a spacer over both
                // wires it joins.
                Selection::SpanPoint { .. } | Selection::JumperPoint { .. } => 7.0,
                // A spread over the wire it lies on, under its points.
                Selection::Spread { .. } => 2.0,
                Selection::Spacer(_) => 1.0,
                _ => s.kind().map_or(0.0, Snap::priority),
            };
            (d <= radius).then_some((d - bonus, s))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(d, s)| (d, s.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_are_picked_along_their_length() {
        let c = vec![
            (
                Selection::Structure("T1".into()),
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 30.0],
            ),
            (
                Selection::Span {
                    section: "S1".into(),
                    span: 0,
                },
                [0.0, 0.0, 25.0],
                [100.0, 0.0, 25.0],
            ),
        ];
        // Screen: x → x, z → −y (a side view, 1 px per m).
        let project = |p: [f64; 3]| Some([p[0], -p[2]]);
        // Halfway up the mast: the structure.
        assert_eq!(
            pick(&c, [1.0, -15.0], 5.0, project),
            Selection::Structure("T1".into())
        );
        // On the wire far from the structure: the span.
        assert!(matches!(
            pick(&c, [60.0, -24.0], 5.0, project),
            Selection::Span { .. }
        ));
        // Near both (where the wire meets the mast): the structure.
        assert_eq!(
            pick(&c, [1.0, -25.0], 5.0, project),
            Selection::Structure("T1".into())
        );
        assert_eq!(pick(&c, [50.0, -60.0], 5.0, project), Selection::None);
    }
}
