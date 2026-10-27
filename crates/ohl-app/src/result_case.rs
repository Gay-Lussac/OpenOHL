// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A check result's weather case drawn on its spans (owner, 2026-10-27):
//! a finding of a clearance window or of the Clearances window, a result of
//! the Distance between conductors, a row of Wire tensions or Galloping,
//! once clicked, has the sections it
//! involves drawn in the weather case that gave it — first, the cases
//! otherwise drawn for them after it — and the case (and the wind) said
//! beside the finding in the views, in the wire's highlight colour without
//! a box. When the finding was at a wind the case as drawn does not show
//! (a step of the wind, still air in a windy case, a swing angle, the
//! other side), the span's wire is drawn on top at that wind: the section
//! solved again in that state on a background thread (the solve of the
//! check reused when it is in the memo), a swing angle as the still-air
//! wire turned about its clamps (a drawing, not a measurement: the check
//! turns it about the string tops).
//!
//! It is temporary: it goes when the selection changes from the result's
//! object, or when the window (page) that set it closes. Display only;
//! nothing is computed from it.

use std::sync::mpsc::{Receiver, channel};

use crate::selection::Selection;

/// Who drew a [`ResultCase`]: it goes when that window closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum From {
    /// A Vertical or Radial clearances window.
    Zone(ohl_clearance::Measure),
    /// The Clearances window.
    Clearances,
    /// A Distance between conductors window.
    PhaseDistance(crate::phase_distance::Kind),
    /// Another check window with a weather case per result (Wire
    /// tensions, Galloping; 2026-10-27).
    Check(crate::dockable::Panel),
}

/// A wire as drawn on top: its points.
pub type Wire = Vec<[f64; 3]>;

/// The weather case of a result, drawn on its sections.
#[derive(Debug)]
pub struct ResultCase {
    /// Who drew it.
    pub from: From,
    /// The object the result selected: another selection takes it off.
    pub selection: Selection,
    /// The sections drawn in its case.
    pub sections: Vec<String>,
    /// The weather case (index in the model's cases; `None`: not in the
    /// model's cases, only said).
    pub case: Option<usize>,
    /// What is said beside it: the case, and the wind.
    pub says: String,
    /// Where it is said: the finding's point on the wire.
    pub at: [f64; 3],
    /// The colour it is said and its dashed wire drawn in: the object's —
    /// its wire's display colour (owner, 2026-10-27: view notes in the
    /// object's colour, no box); set by [`ResultCase::colour_from`].
    pub colour: [u8; 3],
    /// The span's wire at the finding's wind, when the case as drawn does
    /// not show it (empty until solved, or when not needed).
    pub wires: Vec<Wire>,
    /// That wire being solved.
    pending: Option<Receiver<Vec<Wire>>>,
}

impl ResultCase {
    /// A result of `from` that selected `selection`, found in sections
    /// `sections` in the case called `case` (`None` when not in the
    /// model's `cases`), said `says` at `at`.
    pub fn new(
        from: From,
        selection: Selection,
        sections: Vec<String>,
        case: Option<usize>,
        says: String,
        at: [f64; 3],
    ) -> Self {
        Self {
            from,
            selection,
            sections,
            case,
            says,
            at,
            colour: [255, 255, 255],
            wires: Vec::new(),
            pending: None,
        }
    }

    /// Takes its colour from its object as `line` draws it (Line ›
    /// Sections › Display options, `coloring`): the jumper's, else the
    /// first section's.
    pub fn colour_from(&mut self, line: &ohl_line::Line, coloring: crate::wire_style::Coloring) {
        let styles = crate::wire_style::Styles::new(line, coloring);
        let style = match &self.selection {
            Selection::Jumper(id) => Some(styles.jumper(id)),
            _ => self.sections.first().map(|s| styles.section(s)),
        };
        if let Some(s) = style {
            self.colour = s.color;
        }
    }

    /// The span's wire at the finding's wind to be solved on a background
    /// thread: span `span` of `section` of `line` in `case` (its wind
    /// pressure and side set), turned by `arc` degrees about its clamps
    /// (to the right looking along it) when on a swing arc.
    pub fn solve_wire(
        &mut self,
        line: &ohl_line::Line,
        library: &ohl_cables::Library,
        section: &str,
        span: usize,
        case: ohl_weather::WeatherCase,
        arc: Option<f64>,
    ) {
        let (tx, rx) = channel();
        let part = coupled_part(line, section);
        let library = library.clone();
        let section = section.to_owned();
        std::thread::spawn(move || {
            let solved = crate::model::Solved::new_memo(&part, &library, case);
            let wire = solved
                .shapes
                .iter()
                .find(|s| s.section == section)
                .and_then(|s| s.spans.get(span))
                .map(|s| s.points.clone())
                .filter(|p| p.len() >= 2);
            let wires = wire
                .map(|w| match arc {
                    Some(a) => vec![turned(&w, a)],
                    None => vec![w],
                })
                .unwrap_or_default();
            let _ = tx.send(wires);
        });
        self.pending = Some(rx);
    }

    /// Takes the wire solved, if it came: true when it did.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.pending else {
            return false;
        };
        match rx.try_recv() {
            Ok(w) => {
                self.wires = w;
                self.pending = None;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                false
            }
        }
    }

    /// Its wire still being solved.
    pub fn solving(&self) -> bool {
        self.pending.is_some()
    }
}

/// `line` with only section `section` and those solved with it (a
/// common assembly), no jumpers: what its solve needs.
fn coupled_part(line: &ohl_line::Line, section: &str) -> ohl_line::Line {
    let mut keep: Vec<bool> = line.sections.iter().map(|s| s.id == section).collect();
    for group in ohl_line::solve::coupled_groups(line) {
        if group.iter().any(|&k| keep[k]) {
            for k in group {
                keep[k] = true;
            }
        }
    }
    let mut part = line.clone();
    let mut k = 0;
    part.sections.retain(|_| {
        k += 1;
        keep[k - 1]
    });
    part.jumpers.clear();
    part
}

/// The wire `w` turned about the line through its ends so that its point
/// farthest from that line lies `arc` degrees from the downward vertical
/// (positive to the right, looking from its first end to its last).
pub fn turned(w: &[[f64; 3]], arc: f64) -> Wire {
    let (Some(&a), Some(&b)) = (w.first(), w.last()) else {
        return w.to_vec();
    };
    let Some(axis) = ohl_clearance::positions::Axis::new(a, b) else {
        return w.to_vec();
    };
    let dot = |p: [f64; 3], q: [f64; 3]| p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
    // Each point: along the axis, and (down, right) across it.
    let parts: Vec<(f64, f64, f64)> = w
        .iter()
        .map(|p| {
            let v = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
            (axis.along(*p), dot(v, axis.down), dot(v, axis.right))
        })
        .collect();
    let deepest = parts
        .iter()
        .max_by(|p, q| p.1.hypot(p.2).total_cmp(&q.1.hypot(q.2)))
        .map_or(0.0, |p| p.2.atan2(p.1));
    let turn = arc.to_radians() - deepest;
    let (s, c) = turn.sin_cos();
    parts
        .iter()
        .map(|&(along, x, y)| {
            let (x, y) = (x * c - y * s, x * s + y * c);
            [0, 1, 2].map(|i| a[i] + along * axis.u[i] + x * axis.down[i] + y * axis.right[i])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wire hanging below its ends, turned 30° to the right: its lowest
    /// point 30° from the vertical, on the right looking along it; the
    /// ends kept.
    #[test]
    fn a_wire_turns_about_its_ends() {
        // Along +x: right is −y.
        let w = vec![[0.0, 0.0, 10.0], [50.0, 0.0, 5.0], [100.0, 0.0, 10.0]];
        let t = turned(&w, 30.0);
        assert!((t[0][2] - 10.0).abs() < 1e-9 && (t[2][0] - 100.0).abs() < 1e-9);
        let (dy, dz) = (t[1][1], t[1][2] - 10.0);
        assert!(dy < 0.0, "to the right: {dy}");
        let angle = (-dy).atan2(-dz).to_degrees();
        assert!((angle - 30.0).abs() < 1e-6, "{angle}");
        assert!(((dy * dy + dz * dz).sqrt() - 5.0).abs() < 1e-9);
    }
}
