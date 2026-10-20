// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

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
    /// A LiDAR point drawn: millimetres from the model's origin and its
    /// class, as drawn ([`lidar_xyz`]); the editor finds the stored point
    /// behind it ([`crate::lidar_edit`]).
    LidarPoint(crate::model::LidarPoint),
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
            Selection::LidarPoint((_, c)) => format!("LiDAR point (class {c})"),
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
                .wire_point(&ohl_line::SectionPoint::new(structure.clone(), *set))
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
) -> Option<&'a ohl_line::solve::SpacerState> {
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
        Selection::LidarPoint((q, _)) => Some(lidar_xyz(model, q)),
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
fn jumper_candidates(j: &ohl_line::jumper::JumperShape) -> Vec<Candidate> {
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
fn span_object_candidates(model: &Model, shape: &ohl_line::solve::SectionShape) -> Vec<Candidate> {
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
/// separately ([`pick_shown`]).
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
            for (part, pts) in crate::scene::structure_lines(&model.bodies, s) {
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

/// The member of structure `id` (levels 2 and 3) nearest to `at` on the
/// screen within `radius` pixels: its id (none: no member there, or a
/// structure at level 1). Structures are drawn once, not per weather case:
/// every member drawn is this one.
pub fn picked_member(
    model: &Model,
    id: &str,
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Option<String> {
    let st = model.line.structure(id)?;
    crate::scene::placed_members(&model.bodies, st)
        .into_iter()
        .filter_map(|(m, pts)| {
            let m = m?;
            let d = pts
                .windows(2)
                .filter_map(|w| {
                    let (a, b) = (project(w[0])?, project(w[1])?);
                    Some(ohl_core::geometry::distance_to_segment(at, a, b))
                })
                .fold(f64::INFINITY, f64::min);
            (d <= radius).then_some((d, m))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, m)| m)
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
                // Sections not solved in this case, drawn dashed in their
                // place: every copy picks the span (its properties say why).
                for (id, _) in &solved.unsolved {
                    if !model.section_cases(id, case).contains(&k) {
                        continue;
                    }
                    let Some(drawn) = model.unsolved_drawn(id, k, side) else {
                        continue;
                    };
                    for (n, points) in drawn.spans.iter().enumerate() {
                        let sel = Selection::Span {
                            section: id.clone(),
                            span: n,
                        };
                        for w in points.windows(2) {
                            out.push(((sel.clone(), w[0], w[1]), k));
                        }
                    }
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

/// The LiDAR points a view draws, for picking: every point drawn can be
/// clicked, no other (not those a level of detail thins out, beyond the
/// render distance, of hidden classes or without a colour).
#[derive(Debug)]
pub enum LidarShown<'a> {
    /// None (a view without LiDAR points, or their layer hidden).
    Nothing,
    /// The model's points loaded (classes hidden and those without a
    /// colour left out): a view drawing all of them (tests).
    #[cfg(test)]
    Loaded,
    /// Groups of points drawn, each with its bounds (groups whose bounds
    /// lie away from the click on the screen are skipped).
    Groups(Vec<LidarGroup<'a>>),
}

/// Points drawn together (a node of a streamed cloud, a chunk).
#[derive(Debug)]
pub struct LidarGroup<'a> {
    /// Lowest and highest corner of its points, project coordinates.
    pub bounds: Option<[[f64; 3]; 2]>,
    /// Its points.
    pub points: LidarPoints<'a>,
    /// The same of each run of [`LIDAR_CELL`] of its points in their
    /// order (columns only; empty: not known).
    pub cells: &'a [[[f64; 3]; 2]],
}

/// Points per run whose bounds a click looks at before its points
/// ([`LidarGroup::cells`]).
pub const LIDAR_CELL: usize = 4096;

/// The points of a [`LidarGroup`].
#[derive(Debug)]
pub enum LidarPoints<'a> {
    /// Coordinates (mm from the model's origin) and classes.
    Columns(&'a [[i32; 3]], &'a [u8]),
    /// The model's points of these indices, those divisible by the step.
    Indices(&'a [u32], usize),
}

/// The LiDAR point drawn nearest to `at` on the screen within `radius`
/// pixels: its distance there, px, and the point. Of points within a
/// pixel of each other, the one nearest the eye (`project` gives the
/// screen position and the depth). Groups whose bounds project away from
/// `at` are skipped (their eight corners all in front of the eye: the
/// rectangle around them on the screen).
///
/// Cost: projecting the points of the groups near the click (a few
/// nodes of 10⁵ points: a few ms; the loaded points of the tests: every
/// loaded point, ~10 ns each).
pub fn pick_lidar(
    model: &Model,
    shown: &LidarShown<'_>,
    at: [f64; 2],
    radius: f64,
    project: &dyn Fn([f64; 3]) -> Option<([f64; 2], f64)>,
) -> Option<(f64, crate::model::LidarPoint)> {
    // (distance px, depth, point)
    let mut best: Option<(f64, f64, crate::model::LidarPoint)> = None;
    let mut consider = |q: &[i32; 3], c: u8| {
        let Some((s, depth)) = project(lidar_xyz(model, q)) else {
            return;
        };
        let d = (s[0] - at[0]).hypot(s[1] - at[1]);
        if d > radius {
            return;
        }
        let better = match &best {
            None => true,
            // A pixel apart or less: the one nearer the eye.
            Some((bd, bz, _)) if (d - bd).abs() <= 1.0 => depth < *bz,
            Some((bd, _, _)) => d < *bd,
        };
        if better {
            best = Some((d, depth, (*q, c)));
        }
    };
    let near = |b: &[[f64; 3]; 2]| {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for k in 0..8 {
            let p = [0, 1, 2].map(|i| b[(k >> i) & 1][i]);
            // A corner behind the eye: the rectangle is not known.
            let Some((s, _)) = project(p) else {
                return true;
            };
            for i in 0..2 {
                lo[i] = lo[i].min(s[i]);
                hi[i] = hi[i].max(s[i]);
            }
        }
        (0..2).all(|i| at[i] >= lo[i] - radius && at[i] <= hi[i] + radius)
    };
    match shown {
        LidarShown::Nothing => {}
        #[cfg(test)]
        LidarShown::Loaded => {
            let drawn = |c: u8| {
                !model.hidden_classes.contains(&c)
                    && crate::scene::class_color(&model.scheme, c).is_some()
            };
            for (q, c) in &model.points {
                if drawn(*c) {
                    consider(q, *c);
                }
            }
        }
        LidarShown::Groups(groups) => {
            for g in groups {
                if g.bounds.as_ref().is_some_and(|b| !near(b)) {
                    continue;
                }
                match &g.points {
                    LidarPoints::Columns(xyz, class) => {
                        let runs = if g.cells.len() == xyz.len().div_ceil(LIDAR_CELL) {
                            g.cells.len()
                        } else {
                            1
                        };
                        for k in 0..runs {
                            if runs > 1 && !near(&g.cells[k]) {
                                continue;
                            }
                            let (a, b) = if runs > 1 {
                                (k * LIDAR_CELL, ((k + 1) * LIDAR_CELL).min(xyz.len()))
                            } else {
                                (0, xyz.len())
                            };
                            for (q, c) in xyz[a..b].iter().zip(&class[a..b]) {
                                consider(q, *c);
                            }
                        }
                    }
                    LidarPoints::Indices(indices, step) => {
                        for &k in indices.iter() {
                            if (k as usize).is_multiple_of(*step)
                                && let Some((q, c)) = model.points.get(k as usize)
                            {
                                consider(q, *c);
                            }
                        }
                    }
                }
            }
        }
    }
    best.map(|(d, _, p)| (d, p))
}

/// The LiDAR points drawn inside `area` on the screen (`project` gives
/// the screen position and depth; points behind the eye are left out):
/// the rule of a click — those drawn, no other ([`pick_lidar`]) —, of the
/// classes `only` when given. At most `limit` are kept; the second value
/// counts them all.
///
/// Cost: projecting the points of the groups whose bounds meet the area
/// (~10 ns a point; a lasso tests each point inside its bounds against
/// its edges).
pub fn lidar_in_area(
    model: &Model,
    shown: &LidarShown<'_>,
    area: &crate::area::Area,
    project: &dyn Fn([f64; 3]) -> Option<([f64; 2], f64)>,
    only: Option<&std::collections::BTreeSet<u8>>,
    limit: usize,
) -> (Vec<crate::model::LidarPoint>, usize) {
    let mut out = Vec::new();
    let mut total = 0usize;
    let mut consider = |q: &[i32; 3], c: u8| {
        if only.is_some_and(|o| !o.contains(&c)) {
            return;
        }
        let Some((s, _)) = project(lidar_xyz(model, q)) else {
            return;
        };
        if area.contains(s) {
            total += 1;
            if out.len() < limit {
                out.push((*q, c));
            }
        }
    };
    // Whether a group's bounds may hold points inside (a corner behind
    // the eye: not known, looked at).
    let meets = |b: &[[f64; 3]; 2]| {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for k in 0..8 {
            let p = [0, 1, 2].map(|i| b[(k >> i) & 1][i]);
            let Some((s, _)) = project(p) else {
                return true;
            };
            for i in 0..2 {
                lo[i] = lo[i].min(s[i]);
                hi[i] = hi[i].max(s[i]);
            }
        }
        area.meets(lo, hi)
    };
    match shown {
        LidarShown::Nothing => {}
        #[cfg(test)]
        LidarShown::Loaded => {
            for (q, c) in &model.points {
                if !model.hidden_classes.contains(c)
                    && crate::scene::class_color(&model.scheme, *c).is_some()
                {
                    consider(q, *c);
                }
            }
        }
        LidarShown::Groups(groups) => {
            for g in groups {
                if g.bounds.as_ref().is_some_and(|b| !meets(b)) {
                    continue;
                }
                match &g.points {
                    LidarPoints::Columns(xyz, class) => {
                        let runs = if g.cells.len() == xyz.len().div_ceil(LIDAR_CELL) {
                            g.cells.len()
                        } else {
                            1
                        };
                        for k in 0..runs {
                            if runs > 1 && !meets(&g.cells[k]) {
                                continue;
                            }
                            let (a, b) = if runs > 1 {
                                (k * LIDAR_CELL, ((k + 1) * LIDAR_CELL).min(xyz.len()))
                            } else {
                                (0, xyz.len())
                            };
                            for (q, c) in xyz[a..b].iter().zip(&class[a..b]) {
                                consider(q, *c);
                            }
                        }
                    }
                    LidarPoints::Indices(indices, step) => {
                        for &k in indices.iter() {
                            if (k as usize).is_multiple_of(*step)
                                && let Some((q, c)) = model.points.get(k as usize)
                            {
                                consider(q, *c);
                            }
                        }
                    }
                }
            }
        }
    }
    (out, total)
}

/// Picks among everything switched on in `snaps`: the [`candidates`], and
/// the LiDAR points straight from the model (millions of them: not made
/// into candidates), all loaded (tests; the views pick what they draw:
/// [`pick_shown`]).
#[cfg(test)]
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

/// The structure whose pole (levels 2 and 3) covers `at` on the screen:
/// between two of its outline's heights, inside the convex hull of both
/// outlines as projected.
pub fn pole_at(
    model: &Model,
    at: [f64; 2],
    project: &impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Option<String> {
    model.line.structures.iter().find_map(|s| {
        let rings = crate::scene::placed_pole(&model.bodies, s);
        rings
            .windows(2)
            .any(|w| {
                let pts: Option<Vec<[f64; 2]>> = w.iter().flatten().map(|p| project(*p)).collect();
                pts.is_some_and(|p| inside_hull(at, &p))
            })
            .then(|| s.id.clone())
    })
}

/// Whether `at` lies inside the convex hull of `points` (monotone chain).
fn inside_hull(at: [f64; 2], points: &[[f64; 2]]) -> bool {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return false;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut hull: Vec<[f64; 2]> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 {
            Box::new(p.iter())
        } else {
            Box::new(p.iter().rev())
        };
        for q in iter {
            while hull.len() >= start + 2
                && turn(hull[hull.len() - 2], hull[hull.len() - 1], *q) <= 0.0
            {
                hull.pop();
            }
            hull.push(*q);
        }
        hull.pop();
    }
    hull.len() >= 3 && (0..hull.len()).all(|k| turn(hull[k], hull[(k + 1) % hull.len()], at) >= 0.0)
}

/// [`pick_in_model`], also in every other weather case drawn: what was
/// picked, and the case of the copy clicked where it is not the one shown
/// (`None`: the case shown, or nothing case-bound).
#[cfg(test)]
pub fn pick_in_model_case(
    model: &Model,
    case: usize,
    snaps: &Snaps,
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> (Selection, Option<usize>) {
    pick_shown(
        model,
        case,
        snaps,
        at,
        radius,
        &|p| project(p).map(|q| (q, 0.0)),
        &LidarShown::Loaded,
    )
}

/// What a click at `at` picks among everything switched on in `snaps`:
/// the [`candidates`] (in the case shown), the copies in every other
/// weather case drawn ([`other_case_candidates`]; the case of the copy
/// clicked where it is not the one shown, else `None`), poles by their
/// outline, survey points, and the LiDAR points the view draws, `lidar`:
/// those and no other ([`pick_lidar`]). `project3` gives the screen
/// position and the depth (0 in plan and profile).
pub fn pick_shown(
    model: &Model,
    case: usize,
    snaps: &Snaps,
    at: [f64; 2],
    radius: f64,
    project3: &dyn Fn([f64; 3]) -> Option<([f64; 2], f64)>,
    lidar: &LidarShown<'_>,
) -> (Selection, Option<usize>) {
    let project = |p: [f64; 3]| project3(p).map(|q| q.0);
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
    // Inside a pole's outline on the screen (a pole at level 2 or 3 drawn
    // as a solid): its structure, when no line is near.
    if best.is_none()
        && snaps.on(Snap::Structure)
        && let Some(id) = pole_at(model, at, &project)
    {
        best = Some((-Snap::Structure.priority(), Selection::Structure(id)));
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
    if snaps.on(Snap::LidarPoint)
        && let Some((d, p)) = pick_lidar(model, lidar, at, radius, project3)
    {
        let bonus = Snap::LidarPoint.priority();
        if best.as_ref().is_none_or(|b| d - bonus < b.0) {
            best = Some((d - bonus, Selection::LidarPoint(p)));
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
            let d = ohl_core::geometry::distance_to_segment(at, pa, pb);
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
            let d = ohl_core::geometry::distance_to_segment(at, pa, pb);
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
