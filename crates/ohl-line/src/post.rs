// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a **fixed post insulator** carries in a weather case (owner,
//! 2026-11-14): the force on its top from the wires (the sections held
//! there, with the jumpers ending on their clamps, and jumpers ending on
//! its wire point where no section does), the moment of the jumpers held
//! in its clamp's fixed direction, and its own weight and wind at its
//! middle — split into what its ratings are given for ([`PostLoad`]):
//! bending across it (as the load at its top giving the same moment at its
//! base: its cantilever load), tension and compression along it.
//!
//! A hinged (braced) post is a member of its assembly: its axial force is
//! the solve's ([`crate::solve::StringState::force`]).

use ohl_weather::WeatherCase;

use crate::solve::SectionShape;
use crate::{InsulatorString, Line, Point, Structure};

/// The loads on one fixed post in one weather case. Line axes (x east, y
/// north, z up).
#[derive(Debug, Clone, PartialEq)]
pub struct PostLoad {
    /// The structure.
    pub structure: String,
    /// The post's label (a string's).
    pub label: String,
    /// The first section held at its top, and the index of its point
    /// there (none: only jumpers end there).
    pub section: Option<(String, usize)>,
    /// Its base and its top, m.
    pub base: Point,
    /// Its top (its wire point), m.
    pub top: Point,
    /// The force on its top from the wires, N (the whole bundle's).
    pub top_force: Point,
    /// Its own weight and the wind on it, at its middle, N.
    pub own: Point,
    /// The moment on its top's clamp from the jumpers it holds in a fixed
    /// direction, N·m.
    pub clamp_moment: Point,
    /// Jumpers ending at its top that did not solve (their loads and
    /// moments are missing), in words.
    pub missing: Vec<String>,
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: Point) -> f64 {
    dot(a, a).sqrt()
}

impl PostLoad {
    /// Its length, base to top, m.
    pub fn length(&self) -> f64 {
        norm(sub(self.top, self.base))
    }

    /// Its axis, a unit vector from its base to its top.
    pub fn axis(&self) -> Point {
        let r = sub(self.top, self.base);
        let l = norm(r).max(1e-12);
        r.map(|x| x / l)
    }

    /// The moment about its base, N·m: \\( \mathbf r \times \mathbf F +
    /// \tfrac12 \mathbf r \times \mathbf F_{own} + \mathbf C \\).
    pub fn base_moment(&self) -> Point {
        let r = sub(self.top, self.base);
        let half = r.map(|x| 0.5 * x);
        let (a, b) = (cross(r, self.top_force), cross(half, self.own));
        [0, 1, 2].map(|k| a[k] + b[k] + self.clamp_moment[k])
    }

    /// The bending moment at its base (the moment across its axis), N·m.
    pub fn bending(&self) -> f64 {
        let m = self.base_moment();
        let a = self.axis();
        let along = dot(m, a);
        norm([0, 1, 2].map(|k| m[k] - along * a[k]))
    }

    /// The twist about its axis, N·m (not checked: no rating for it).
    pub fn torsion(&self) -> f64 {
        dot(self.base_moment(), self.axis()).abs()
    }

    /// Its **cantilever load**: the load across it at its top that gives
    /// its bending moment at its base, N (what its rating is given for).
    pub fn cantilever(&self) -> f64 {
        self.bending() / self.length().max(1e-9)
    }

    /// The force along it, N, positive pulling its top away from its base
    /// (tension): at its top fitting (the wires alone) and at its base
    /// (with its own weight).
    fn axial(&self) -> [f64; 2] {
        let a = self.axis();
        let top = dot(self.top_force, a);
        [top, top + dot(self.own, a)]
    }

    /// The tension along it, N (the larger of top and base; 0: none).
    pub fn tension(&self) -> f64 {
        let [t, b] = self.axial();
        t.max(b).max(0.0)
    }

    /// The compression along it, N (the larger of top and base; 0: none).
    pub fn compression(&self) -> f64 {
        let [t, b] = self.axial();
        (-t).max(-b).max(0.0)
    }
}

/// A fixed post's own weight and the wind on it in weather case `case`:
/// (its middle, the force there, N). The wind is the line's, across the
/// line at its middle (else across the structure's ahead axis), times the
/// structure's local wind factor — as on the strings; no ice, as on the
/// strings. None where its points are missing.
pub fn own_load(
    line: &Line,
    structure: &Structure,
    post: &InsulatorString,
    case: &WeatherCase,
) -> Option<(Point, Point)> {
    let base = structure.point(&post.top)?.position;
    let top = structure.wire(&post.wire)?.position;
    let middle = [0, 1, 2].map(|k| 0.5 * (base[k] + top[k]));
    let mut f = [0.0, 0.0, -post.weight];
    let ahead = line
        .direction_at([middle[0], middle[1]])
        .unwrap_or_else(|| {
            let a = crate::links::from_structure_axes(structure.orientation_deg, [0.0, 1.0, 0.0]);
            [a[0], a[1]]
        });
    let push = case.wind_pressure.si()
        * line.wind_factor_at_structure(&structure.id)
        * post.wind_area
        * case.wind_from.to_left();
    f[0] -= ahead[1] * push;
    f[1] += ahead[0] * push;
    Some((middle, f))
}

/// The force jumper `jumper` puts on its end `k` (0: `from`) where no
/// section carries it, as the sections take a jumper's load on their
/// clamps: its own end force (`shape`) when it is given by its tension or
/// the line takes the jumpers' own pull ([`crate::JumperLoads::Pull`]),
/// else the estimate (`estimate`, [`crate::jumper::end_loads`]).
pub fn jumper_end_force(
    line: &Line,
    jumper: &crate::Jumper,
    shape: &crate::jumper::JumperShape,
    k: usize,
    estimate: &crate::solve::JumperLoadMap,
) -> Point {
    if jumper.tension.is_some() || line.solver.jumper_loads == crate::JumperLoads::Pull {
        return shape.end_forces[k];
    }
    let end = if k == 0 { &jumper.from } else { &jumper.to };
    end.point()
        .and_then(|p| estimate.get(&crate::solve::LoadAt::Clamp(p.clone())))
        .copied()
        .unwrap_or(shape.end_forces[k])
}

/// The jumpers held in a fixed post's clamp whose moment there is the
/// rod's while their forces on the clamps are estimated (the line's
/// jumper loads not their own pull, [`crate::JumperLoads`]): not one state,
/// NOT ON THE SAFE SIDE in general (the clamp's moment shifts the jumper's
/// load between its ends). Their ids; empty with the jumpers' own pull.
pub fn moment_with_estimate(line: &Line) -> Vec<&str> {
    if line.solver.jumper_loads == crate::JumperLoads::Pull {
        return Vec::new();
    }
    let on_post = |p: Option<&crate::SectionPoint>| {
        p.and_then(|p| {
            let s = line.structure(&p.structure)?;
            s.post_at(&s.wire_at_set(p.set)?.label)
        })
        .is_some()
    };
    line.jumpers
        .iter()
        .filter(|j| {
            j.tension.is_none()
                && !j.rigid
                && j.bending_stiffness != Some(0.0)
                && (on_post(j.from.point()) || on_post(j.to.point()))
        })
        .map(|j| j.id.as_str())
        .collect()
}

/// [`moment_with_estimate`] in words (none: nothing to say).
pub fn moment_with_estimate_note(line: &Line) -> Option<String> {
    let ids = moment_with_estimate(line);
    (!ids.is_empty()).then(|| {
        format!(
            "NOT ON THE SAFE SIDE: the moment of jumpers held in a post's clamp is from their \
             bending while their forces there are estimated (half their weight, ice and wind) \
             — not one state; choose the jumpers' own pull (calculation options) for both: {}",
            ids.join(", ")
        )
    })
}

/// The loads on every fixed post of `line` in weather case `case`, the
/// sections solved there as `shapes`; jumpers ending at a post's top are
/// solved for their clamp moments (and, where no section ends there, their
/// end forces) with `library`'s cables.
pub fn post_loads(
    line: &Line,
    library: &ohl_cables::Library,
    case: &WeatherCase,
    shapes: &[SectionShape],
) -> Vec<PostLoad> {
    let mut out = Vec::new();
    for structure in &line.structures {
        for post in structure.strings.iter().filter(|x| x.post && !x.hinged) {
            let (Some(base), Some(wire)) = (structure.point(&post.top), structure.wire(&post.wire))
            else {
                continue;
            };
            let top = wire.position;
            let mut load = PostLoad {
                structure: structure.id.clone(),
                label: post.label.clone(),
                section: None,
                base: base.position,
                top,
                top_force: [0.0; 3],
                own: own_load(line, structure, post, case).map_or([0.0; 3], |(_, f)| f),
                clamp_moment: [0.0; 3],
                missing: Vec::new(),
            };
            // The sections held at its top: their supports there (labelled
            // with its base, at its top), the jumpers on their clamps in.
            let at_top = |p: &crate::SectionPoint| {
                p.structure == structure.id
                    && structure
                        .wire_at_set(p.set)
                        .is_some_and(|w| w.label == post.wire)
            };
            let mut sectioned = false;
            for shape in shapes {
                let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
                    continue;
                };
                for (k, p) in section.points.iter().enumerate() {
                    if !at_top(p) {
                        continue;
                    }
                    sectioned = true;
                    if load.section.is_none() {
                        load.section = Some((section.id.clone(), k));
                    }
                    for s in shape.attachment_loads.get(k).into_iter().flatten() {
                        if s.label == post.top && norm(sub(s.at, top)) < 1e-3 {
                            for c in 0..3 {
                                load.top_force[c] += s.force[c];
                            }
                        }
                    }
                }
            }
            // The jumpers ending there: their clamp moment; their end force
            // where no section carries it.
            let estimate = std::cell::OnceCell::new();
            for j in &line.jumpers {
                let ends = [j.from.point(), j.to.point()];
                if !ends.iter().any(|e| e.is_some_and(at_top)) {
                    continue;
                }
                let shape = match crate::jumper::solve_one_jumper(line, library, case, shapes, j) {
                    Ok(s) => s,
                    Err(e) => {
                        load.missing.push(format!("jumper {}: {e}", j.id));
                        continue;
                    }
                };
                for (k, end) in ends.iter().enumerate() {
                    if !end.is_some_and(at_top) {
                        continue;
                    }
                    let force = if sectioned {
                        [0.0; 3]
                    } else {
                        let estimate = estimate.get_or_init(|| {
                            crate::jumper::end_loads(line, case, |id| library.get(id))
                        });
                        jumper_end_force(line, j, &shape, k, estimate)
                    };
                    for (c, f) in force.iter().enumerate() {
                        load.clamp_moment[c] += shape.end_moments[k][c];
                        load.top_force[c] += f;
                    }
                }
            }
            out.push(load);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(top_force: Point, own: Point) -> PostLoad {
        PostLoad {
            structure: "T".into(),
            label: "P".into(),
            section: None,
            base: [0.0, 0.0, 10.0],
            top: [0.0, 0.0, 12.0],
            top_force,
            own,
            clamp_moment: [0.0; 3],
            missing: Vec::new(),
        }
    }

    /// An upright post 2 m tall: 1 kN across at its top and its own wind
    /// of 200 N at its middle give 2 kN·m + 0.2 kN·m at its base — a
    /// cantilever load of 1.1 kN; 3 kN down on it and its own 500 N: 3.5
    /// kN of compression at its base; a clamp moment about its axis is
    /// torsion, not bending.
    #[test]
    fn a_post_splits_its_load_into_its_ratings() {
        let mut l = load([1000.0, 0.0, -3000.0], [200.0, 0.0, -500.0]);
        assert!((l.bending() - 2200.0).abs() < 1e-9, "{}", l.bending());
        assert!((l.cantilever() - 1100.0).abs() < 1e-9);
        assert!((l.compression() - 3500.0).abs() < 1e-9);
        assert_eq!(l.tension(), 0.0);
        l.clamp_moment = [0.0, 0.0, 50.0];
        assert!((l.torsion() - 50.0).abs() < 1e-9);
        assert!((l.bending() - 2200.0).abs() < 1e-9);
        // Pulled up by 1 kN more than its weight: tension at its top.
        let l = load([0.0, 0.0, 1500.0], [0.0, 0.0, -500.0]);
        assert!((l.tension() - 1500.0).abs() < 1e-9);
        assert_eq!(l.compression(), 0.0);
    }
}
