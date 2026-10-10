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
    /// An alignment point of a route.
    AlignmentPoint {
        /// Route id.
        route: String,
        /// Index in the route's alignment.
        index: usize,
    },
    /// A LiDAR point shown (index in the model's points).
    LidarPoint(usize),
}

/// A right-click in a view: what is there, and the point on the ground
/// under it (when the view knows one).
#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    /// The object right-clicked (or nothing).
    pub target: Selection,
    /// The ground under the pointer (the crosshair in first person).
    pub at: Option<[f64; 3]>,
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
}

impl Snap {
    /// Every kind, in the order of the Snap menu.
    pub const ALL: [Snap; 6] = [
        Snap::Structure,
        Snap::Span,
        Snap::Jumper,
        Snap::Attachment,
        Snap::AlignmentPoint,
        Snap::LidarPoint,
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
        }
    }

    /// Picked before others at about the same distance (higher first):
    /// points over the lines they sit on, structures over wires.
    fn priority(self) -> f64 {
        match self {
            Snap::Attachment | Snap::AlignmentPoint => 6.0,
            Snap::LidarPoint => 4.0,
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
            Selection::AlignmentPoint { .. } => Snap::AlignmentPoint,
            Selection::LidarPoint(_) => Snap::LidarPoint,
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
            Selection::AlignmentPoint { route, index } => {
                format!("route {route} alignment point {}", index + 1)
            }
            Selection::LidarPoint(k) => format!("LiDAR point {k}"),
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
            Selection::AlignmentPoint { route, index } => alignment_point(model, route, *index),
            Selection::LidarPoint(k) => model.points.get(*k).map(|(q, _)| lidar_xyz(model, q)),
        }
    }
}

/// An alignment point's position, draped on the ground when the
/// alignment has no height.
pub fn alignment_point(model: &Model, route: &str, index: usize) -> Option<[f64; 3]> {
    model
        .line
        .effective_routes()
        .into_iter()
        .find(|r| r.id == route)?
        .centreline(&model.line)
        .get(index)
        .map(|p| draped(model, *p))
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
    let strings = snaps.on(Snap::Attachment) || snaps.on(Snap::Structure);
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
                if !drawn {
                    out.push((sel.clone(), w.position, w.position));
                }
                if w.set.is_some() {
                    for x in s.strings_to(&w.label) {
                        if let Some(top) = s.node(&x.top) {
                            out.push((sel.clone(), top.position(), top.position()));
                        }
                    }
                }
            }
        }
    }
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
            }
        }
        if snaps.on(Snap::Jumper) {
            for j in &c.jumpers {
                let sel = Selection::Jumper(j.jumper.clone());
                for w in j.points.windows(2) {
                    out.push((sel.clone(), w[0], w[1]));
                }
            }
        }
    }
    if snaps.on(Snap::AlignmentPoint) {
        for r in model.line.effective_routes() {
            for (index, p) in r.centreline(&model.line).into_iter().enumerate() {
                let p = draped(model, p);
                out.push((
                    Selection::AlignmentPoint {
                        route: r.id.clone(),
                        index,
                    },
                    p,
                    p,
                ));
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
    let mut best = pick_scored(&candidates(model, case, snaps), at, radius, &project);
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
    best.map_or(Selection::None, |b| b.1)
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
