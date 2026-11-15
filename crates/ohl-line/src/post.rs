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

/// The hinged posts of `line` whose top holds nothing — no section ends at
/// a wire point moving with it, no jumper ends there or is held there
/// (owner, 2026-11-15: "while nothing is attached to it it should just move
/// freely"): (structure, post label). Nothing gives them a load path: their
/// loads (their own and their braces' weight and wind) are not computed,
/// never made up with a stiffness of their own.
pub fn free_hinged_posts(line: &Line) -> Vec<(String, String)> {
    hinged_posts_holding(line, Holds::Nothing)
}

/// What a hinged post's top holds ([`hinged_posts_holding`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// Nothing: free.
    Nothing,
    /// Jumpers only (their ends or their support clamps), no section.
    Jumpers,
    /// A section ends there.
    Section,
}

/// The hinged posts of `line` (structure, post label) whose top — a wire
/// point moving with it — holds what `holds` says.
fn hinged_posts_holding(line: &Line, holds: Holds) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for structure in &line.structures {
        for post in structure.strings.iter().filter(|x| x.post && x.hinged) {
            let component = structure.component(&post.wire);
            let (mut section, mut jumper) = (false, false);
            for &w in &component.wires {
                let wire = &structure.wires[w];
                let at = |p: Option<&crate::SectionPoint>| {
                    p.is_some_and(|p| p.structure == structure.id && Some(p.set) == wire.set)
                };
                section |= line
                    .sections
                    .iter()
                    .any(|s| s.points.iter().any(|p| at(Some(p))));
                jumper |= line.jumpers.iter().any(|j| {
                    at(j.from.point())
                        || at(j.to.point())
                        || j.supports().iter().any(|c| {
                            let on = c
                                .structure
                                .as_deref()
                                .or_else(|| j.from.point().map(|p| p.structure.as_str()));
                            on == Some(structure.id.as_str()) && c.wire == wire.label
                        })
                });
            }
            let is = match (section, jumper) {
                (true, _) => Holds::Section,
                (false, true) => Holds::Jumpers,
                (false, false) => Holds::Nothing,
            };
            if is == holds {
                out.push((structure.id.clone(), post.label.clone()));
            }
        }
    }
    out
}

/// Whether hinged post `post` of `structure` **pivots**: its assembly's
/// strings all stand on the structure and hold its wire point, their bases
/// on one line — it turns about that line freely (one brace, or braces in
/// line with its base); else its braces lock every direction.
pub fn pivots(structure: &Structure, post: &InsulatorString) -> bool {
    if !(post.post && post.hinged) {
        return false;
    }
    let component = structure.component(&post.wire);
    let mut tops = Vec::new();
    for &i in &component.strings {
        let s = &structure.strings[i];
        let Some(top) = structure.point(&s.top) else {
            return false;
        };
        if s.wire != post.wire {
            return false;
        }
        tops.push(nalgebra::Vector3::new(
            top.position[0],
            top.position[1],
            top.position[2],
        ));
    }
    tops.len() >= 2 && crate::solve::pivot_axis(&tops).is_some()
}

/// The pivoting posts ([`pivots`]) a section ends at, in words: free to
/// turn about the line through their bases on a frictionless hinge, held
/// along it by the wires only — NOT ON THE SAFE SIDE for the structure's
/// longitudinal load (a hinge that sticks passes part of the spans' pull
/// difference into it).
pub fn pivoting_notes(line: &Line) -> Vec<String> {
    hinged_posts_holding(line, Holds::Section)
        .into_iter()
        .filter(|(s, p)| {
            line.structure(s)
                .and_then(|s| Some((s, s.string(p)?)))
                .is_some_and(|(s, p)| pivots(s, p))
        })
        .map(|(structure, label)| {
            format!(
                "hinged post {label} at {structure} pivots: it turns freely about the line through \
                 its base and its brace's (a frictionless hinge), held along it by the wires, and \
                 swings until their pulls balance — NOT ON THE SAFE SIDE for {structure}'s \
                 longitudinal load: a hinge that sticks passes part of the spans' pull difference \
                 into the structure"
            )
        })
        .collect()
}

/// The hinged posts whose top holds jumpers only (no section), in words:
/// each swings under the jumpers' pull as solved to its top where given
/// ([`hinged_alone`]); the jumpers are not solved again with it swung — the
/// sizes of its and its brace's forces stand, their directions in plan are
/// not on the safe side where it swings far (a small push along the line
/// turns a free post a long way).
pub fn jumper_held_hinged_notes(line: &Line) -> Vec<String> {
    hinged_posts_holding(line, Holds::Jumpers)
        .into_iter()
        .map(|(structure, label)| {
            format!(
                "hinged post {label} at {structure} holds only jumpers: it swings about its hinge \
                 under their pull as solved to its top where given (the jumpers not solved again \
                 with it swung) — the sizes of its and its brace's forces stand; NOT ON THE SAFE \
                 SIDE: their directions in plan where it swings far"
            )
        })
        .collect()
}

/// [`free_hinged_posts`] in words, one note per post.
pub fn free_hinged_notes(line: &Line) -> Vec<String> {
    free_hinged_posts(line)
        .into_iter()
        .map(|(structure, label)| {
            format!(
                "hinged post {label} at {structure}: nothing holds it (free to pivot); its loads \
                 not computed — NOT ON THE SAFE SIDE: its own and its brace's weight and wind are \
                 missing from {structure}'s loads"
            )
        })
        .collect()
}

/// A member of a hinged post's assembly as solved alone ([`hinged_alone`]).
#[derive(Debug, Clone, PartialEq)]
pub struct HingedMember {
    /// The string's label.
    pub label: String,
    /// Its top (on the structure): label and position, m.
    pub top: String,
    /// Its top's position, m.
    pub at: Point,
    /// Its axial force, N (tension positive; a hinged post may push).
    pub force: f64,
    /// The force it puts on its top, N: its force towards the clamp, half
    /// its weight down.
    pub on_top: Point,
}

/// A hinged post's assembly at wire point `wire` of `structure` solved alone
/// under force `load` on its clamp (N, the whole bundle's; a jumper's end
/// where no section ends, owner 2026-11-15: "pivoting should simply be tied
/// to the jumper or piece it's attached to"): it turns freely about its
/// hinges in every direction its braces leave free, until the load balances
/// — as a string swings. Its members (their weights half at each end, the
/// post pushing as well as pulling) and their forces on their tops. An
/// error where the load does not hold it (it turns without end: no
/// equilibrium, nothing made up). Simplification: the load as given, not
/// as it would change with the clamp's swing (the jumper solved to the
/// wire point as given, as on strings).
pub fn hinged_alone(
    structure: &Structure,
    wire: &str,
    load: Point,
    default_stiffness: f64,
) -> Result<Vec<HingedMember>, String> {
    use nalgebra::Vector3;
    use ohl_wirefe::{Model, Node, TrussElement};
    let component = structure.component(wire);
    // Solved about the clamp (positions less its own): a stiff member's
    // force noise grows with the coordinates' size.
    let origin = structure
        .wire(wire)
        .ok_or_else(|| format!("no wire point {wire} on {}", structure.id))?
        .position;
    let mut model = Model::default();
    let mut labels: Vec<String> = Vec::new();
    let mut node = |label: &str, model: &mut Model| -> Result<usize, String> {
        if let Some(i) = labels.iter().position(|l| l == label) {
            return Ok(i);
        }
        let (position, fixed) = match structure.wire(label) {
            Some(w) => (w.position, false),
            None => (
                structure
                    .point(label)
                    .ok_or_else(|| format!("no point {label} on {}", structure.id))?
                    .position,
                true,
            ),
        };
        let p = Vector3::new(
            position[0] - origin[0],
            position[1] - origin[1],
            position[2] - origin[2],
        );
        model
            .nodes
            .push(if fixed { Node::fixed(p) } else { Node::free(p) });
        labels.push(label.to_owned());
        Ok(labels.len() - 1)
    };
    let clamp = node(wire, &mut model)?;
    model.nodes[clamp].load = Vector3::new(load[0], load[1], load[2]);
    let mut members = Vec::new();
    for &i in &component.strings {
        let s = &structure.strings[i];
        let (a, b) = (node(&s.top, &mut model)?, node(&s.wire, &mut model)?);
        let length = structure.string_length(s).unwrap_or(0.0);
        model.trusses.push(TrussElement {
            i: a,
            j: b,
            unstressed_length: length,
            axial_stiffness: s.stiffness.unwrap_or(default_stiffness),
            tension_only: !(s.post && s.hinged) && s.compression.is_none(),
            weight: s.weight,
        });
        members.push(s);
    }
    // A counterweight on a wire point moving with it.
    for &w in &component.wires {
        let x = &structure.wires[w];
        if let Some(i) = labels.iter().position(|l| *l == x.label) {
            model.nodes[i].load.z -= x.weight;
        }
    }
    // Its members all from the structure to the clamp, their tops on one
    // line: it turns about that line freely. Turned first to where the
    // load's part across the line points away from it (the moment about
    // the line nil, the post hanging in the load as a pendulum: the stable
    // one of the two); a load along the line only turns it nowhere (it
    // stays as given: every turn balances). The members then hold the
    // rest.
    let tops: Vec<Vector3<f64>> = model
        .trusses
        .iter()
        .map(|t| model.nodes[t.i].position)
        .collect();
    let on_clamp = model
        .trusses
        .iter()
        .all(|t| t.j == clamp && model.nodes[t.i].fixed);
    if let (true, Some((o, a))) = (on_clamp, crate::solve::pivot_axis(&tops)) {
        let f = model.nodes[clamp].load
            - Vector3::new(
                0.0,
                0.0,
                model.trusses.iter().map(|t| t.weight / 2.0).sum::<f64>(),
            );
        let r = model.nodes[clamp].position - o;
        let along = a * r.dot(&a);
        let radial = r - along;
        let across = f - a * f.dot(&a);
        if across.norm() > 1e-9 * f.norm().max(1.0) {
            let e = across.normalize();
            if e.dot(&radial) < 0.0 {
                return Err(
                    "the load pushes its top round its free axis, back towards its hinge (a \
                     balance that does not hold): nothing near where it is given holds it"
                        .into(),
                );
            }
            model.nodes[clamp].position = o + along + e * radial.norm();
        }
    }
    let solved = model
        .solve(&ohl_wirefe::default_fe_budget())
        .map_err(|e| format!("it turns without end under the load ({e})"))?;
    Ok(members
        .iter()
        .zip(&model.trusses)
        .zip(&solved.truss_forces)
        .map(|((s, t), &force)| {
            let (top, bottom) = (solved.positions[t.i], solved.positions[t.j]);
            let d = bottom - top;
            let u = if d.norm() > 0.0 { d / d.norm() } else { d };
            let f = u * force - Vector3::new(0.0, 0.0, t.weight / 2.0);
            HingedMember {
                label: s.label.clone(),
                top: s.top.clone(),
                at: [top.x + origin[0], top.y + origin[1], top.z + origin[2]],
                force,
                on_top: [f.x, f.y, f.z],
            }
        })
        .collect())
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

    /// A structure with a hinged post P (450 N) standing level 1.5 m out
    /// (−y) from A1, braced by B (200 N) from A2 1.5 m straight above A1:
    /// it turns freely about the vertical through A1 and A2. Both 10⁹ N
    /// stiff.
    fn braced() -> Structure {
        let mut s = Structure::new("T", [0.0, 0.0, 0.0]);
        s.points
            .push(crate::StructurePoint::at("A1", [0.0, 0.0, 10.0]));
        s.points
            .push(crate::StructurePoint::at("A2", [0.0, 0.0, 11.5]));
        s.wires.push(crate::WirePoint {
            label: "W".into(),
            position: [0.0, -1.5, 10.0],
            ..Default::default()
        });
        s.strings.push(InsulatorString {
            label: "P".into(),
            top: "A1".into(),
            wire: "W".into(),
            weight: 450.0,
            post: true,
            hinged: true,
            stiffness: Some(1e9),
            ..Default::default()
        });
        s.strings.push(InsulatorString {
            label: "B".into(),
            top: "A2".into(),
            wire: "W".into(),
            weight: 200.0,
            stiffness: Some(1e9),
            ..Default::default()
        });
        s.check_strings().unwrap();
        s
    }

    /// Solved alone under a load on its clamp, the braced post swings about
    /// its axis until the load's part across the axis points along it, as
    /// a string swings (owner, 2026-11-15). Hand check: 300 N along the
    /// line (+x), 400 N outwards (−y), 2000 N down: it turns by
    /// atan(300 / 400) = 36.87° towards +x, its top at (0.9, −1.2, 10);
    /// with half the members' weights (325 N) the brace (at 45°) pulls
    /// √2 × 2325 N and the post pushes with 2325 − 500 N (the outward 500 N
    /// along it pulls). Under a vertical load only it stays where it is
    /// given (every turn balances: it is the one given).
    #[test]
    fn a_hinged_post_alone_swings_to_its_load() {
        let s = braced();
        let m = hinged_alone(&s, "W", [300.0, -400.0, -2000.0], 1e9).unwrap();
        let (p, b) = (&m[0], &m[1]);
        assert_eq!((p.label.as_str(), b.label.as_str()), ("P", "B"));
        let v = 2325.0;
        assert!((b.force - 2f64.sqrt() * v).abs() < 0.05, "{}", b.force);
        assert!((p.force + (v - 500.0)).abs() < 0.05, "{}", p.force);
        // The post's force on A1 points from the clamp: along (0.6, −0.8)
        // in plan (it pushes: towards −(0.6, −0.8)).
        let f = p.on_top;
        let plan = norm([f[0], f[1], 0.0]);
        assert!(
            (f[0] / plan + 0.6).abs() < 1e-4 && (f[1] / plan - 0.8).abs() < 1e-4,
            "{f:?}"
        );
        assert!((f[2] + 225.0).abs() < 0.05, "{f:?}");
        // Equilibrium: the loads on A1 and A2 are the load and the weights.
        let sum = [0, 1, 2].map(|k| p.on_top[k] + b.on_top[k]);
        let expected = [300.0, -400.0, -2650.0];
        assert!(
            (0..3).all(|k| (sum[k] - expected[k]).abs() < 1e-3),
            "{sum:?}"
        );
        // A vertical load: where it is given, the brace √2 × (V + 325).
        let m = hinged_alone(&s, "W", [0.0, 0.0, -2000.0], 1e9).unwrap();
        assert!(
            (m[1].force - 2f64.sqrt() * 2325.0).abs() < 0.05,
            "{}",
            m[1].force
        );
        assert!((m[0].force + 2325.0).abs() < 0.05, "{}", m[0].force);
        let f = m[0].on_top;
        assert!(f[0].abs() < 1e-6 && (f[1] - 2325.0).abs() < 0.05, "{f:?}");
    }

    /// A hinged post whose top holds nothing is listed (free to pivot, its
    /// loads not computed); one a section ends at is not.
    #[test]
    fn a_hinged_post_holding_nothing_is_said() {
        let mut line = Line::from_json(
            r#"{"format": "ohl-line", "version": 17, "name": "free", "crs": "local",
 "structures": [
  {"id": "U", "position": [-300, 0, 0],
   "wires": [{"label": "W", "position": [-300, -1.5, 10], "set": 1}]},
  {"id": "T", "position": [0, 0, 0],
   "wires": [{"label": "W1", "position": [0, 0, 10], "set": 1}]}],
 "sections": [{"id": "S", "cable": "c", "bundle": 1,
  "points": [{"structure": "U", "set": 1}, {"structure": "T", "set": 1}],
  "stringing": {"kind": "tension", "horizontal_tension": 20000.0, "temperature": 15.0}}]
}"#,
        )
        .unwrap();
        let mut s = braced();
        s.wires[0].set = Some(2);
        line.structures[1] = s;
        assert_eq!(free_hinged_posts(&line), vec![("T".into(), "P".into())]);
        let notes = free_hinged_notes(&line);
        assert!(
            notes[0].starts_with("hinged post P at T: nothing holds it (free to pivot)")
                && notes[0].contains("NOT ON THE SAFE SIDE"),
            "{notes:?}"
        );
        // The section ends at its top: held.
        line.structures[1].wires[0].set = Some(1);
        assert!(free_hinged_posts(&line).is_empty());
    }
}
