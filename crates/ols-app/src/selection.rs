// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What is selected, shared by all views, and picking.

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
}

impl Selection {
    /// Text for the status bar.
    pub fn describe(&self) -> String {
        match self {
            Selection::None => "nothing selected".into(),
            Selection::Structure(id) => format!("structure {id}"),
            Selection::Span { section, span } => format!("{section} span {}", span + 1),
            Selection::Jumper(id) => format!("jumper {id}"),
        }
    }

    /// A point to look at.
    pub fn focus(&self, model: &Model, case: usize) -> Option<[f64; 3]> {
        match self {
            Selection::None => None,
            Selection::Structure(id) => model.line.structure(id).map(|s| {
                let top = s
                    .attachments
                    .iter()
                    .map(|a| a.wire_point[2])
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
                .cases
                .get(case)?
                .shapes
                .iter()
                .find(|x| &x.section == section)?
                .spans
                .get(*span)
                .map(|s| s.mid_point),
        }
    }
}

/// A piece that selects something when clicked: a segment (`a` = `b` for
/// a point).
pub type Candidate = (Selection, [f64; 3], [f64; 3]);

/// What can be clicked: every line a structure is drawn with (mast or stick
/// drawing, arms, insulator strings), every piece of every span and jumper.
pub fn candidates(model: &Model, case: usize) -> Vec<Candidate> {
    let mut out = Vec::new();
    for s in &model.line.structures {
        let sel = Selection::Structure(s.id.clone());
        out.push((sel.clone(), s.position, s.position));
        for (_, pts) in crate::scene::structure_lines(s) {
            for w in pts.windows(2) {
                out.push((sel.clone(), w[0], w[1]));
            }
        }
    }
    if let Some(c) = model.cases.get(case) {
        for shape in &c.shapes {
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
        for j in &c.jumpers {
            let sel = Selection::Jumper(j.jumper.clone());
            for w in j.points.windows(2) {
                out.push((sel.clone(), w[0], w[1]));
            }
        }
    }
    out
}

/// Distance on screen from `p` to the segment `a`–`b`.
fn to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a[0] + t * d[0] - p[0]).hypot(a[1] + t * d[1] - p[1])
}

/// The candidate nearest to `at` on screen within `radius` pixels, given a
/// projection to screen coordinates (`None`: not visible).
pub fn pick(
    candidates: &[Candidate],
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Selection {
    // Structures win over wires at about the same distance (thin lines
    // in front of a tower).
    candidates
        .iter()
        .filter_map(|(s, a, b)| {
            let (pa, pb) = (project(*a)?, project(*b)?);
            let d = to_segment(at, pa, pb);
            let bonus = if matches!(s, Selection::Structure(_)) {
                3.0
            } else {
                0.0
            };
            (d <= radius).then_some((d - bonus, s))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(Selection::None, |(_, s)| s.clone())
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
