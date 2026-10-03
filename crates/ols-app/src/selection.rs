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

/// Points that select something when clicked: structures (base and
/// attachments) and spans (points along the wire).
pub fn candidates(model: &Model, case: usize) -> Vec<(Selection, [f64; 3])> {
    let mut out = Vec::new();
    for s in &model.line.structures {
        let sel = Selection::Structure(s.id.clone());
        out.push((sel.clone(), s.position));
        for a in &s.attachments {
            out.push((sel.clone(), a.insulator_point));
        }
    }
    if let Some(c) = model.cases.get(case) {
        for shape in &c.shapes {
            for (k, span) in shape.spans.iter().enumerate() {
                let n = span.points.len();
                let step = (n / 12).max(1);
                for p in span.points.iter().step_by(step).skip(1) {
                    out.push((
                        Selection::Span {
                            section: shape.section.clone(),
                            span: k,
                        },
                        *p,
                    ));
                }
            }
        }
    }
    out
}

/// The candidate nearest to `at` on screen within `radius` pixels, given a
/// projection to screen coordinates.
pub fn pick(
    candidates: &[(Selection, [f64; 3])],
    at: [f64; 2],
    radius: f64,
    project: impl Fn([f64; 3]) -> Option<[f64; 2]>,
) -> Selection {
    // Structures win over wires at the same distance (they are smaller).
    candidates
        .iter()
        .filter_map(|(s, p)| {
            let q = project(*p)?;
            let d = (q[0] - at[0]).hypot(q[1] - at[1]);
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
