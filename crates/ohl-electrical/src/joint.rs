// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The electric field of several lines **solved together** (ROADMAP O17,
//! 2026-11-24; built, in active development, not approved yet).
//!
//! A line near another changes its charges: the other line's conductors,
//! held at their own voltages, and its earth wires, held at zero, take
//! charges of their own, and every wire's charge depends on all the
//! others'. Two lines' fields computed each on its own and added are
//! therefore not a proven bound of their field together. Here every
//! line's wires are one system:
//!
//! - **Charge simulation** (Singer, Steinbigler and Weiss 1974): every wire
//!   (a bundle as its equivalent wire, [`crate::matrices::bundle`]) cut
//!   into straight pieces of uniform charge per metre, each piece's
//!   potential held at its wire's voltage at one point on the wire's
//!   surface at its middle; the earth a perfect conductor, each charge's
//!   image in the ground plane fitted under the point where the potential
//!   or field is taken ([`GroundPlane::fitted`], as the electric field's
//!   quick ground model: exact for level or evenly sloping ground).
//!   \\( P\,q = V \\) for the pieces' charges, \\( P \\) real.
//! - **By parts**: the system is linear, so the charges and the field are
//!   the sum of each **part**'s alone — a part is one line's phases of one
//!   frequency at their voltages, every other wire at zero (the other
//!   lines' conductors and every earth wire). Each part's field at a point
//!   is a vector of three complex rms phasors \\( \vec E_k \\). The phase
//!   angles of two parts to each other are not known (two lines'), so the
//!   field is \\( \vec E = \sum_k e^{j\theta_k}\vec E_k \\) for unknown
//!   \\( \theta_k \\), its rms \\( |\vec E|^2 = \sum_k |\vec E_k|^2 + 2\sum_{k<l}
//!   \operatorname{Re}\big(e^{j(\theta_l-\theta_k)}\langle\vec E_k,\vec E_l\rangle\big) \\)
//!   with \\( \langle a,b\rangle = \sum_c \bar a_c b_c \\).
//! - **The worst over the angles** ([`worst`]): each cross term is at most
//!   \\( |\langle\vec E_k,\vec E_l\rangle| \\), so
//!   \\( |\vec E|^2 \le \sum_k |\vec E_k|^2 + 2\sum_{k<l}|\langle\vec E_k,\vec E_l\rangle| \\)
//!   — **exact for two parts** (one angle: the cross term reaches its bound
//!   at \\( \theta = -\arg\langle\vec E_k,\vec E_l\rangle \\)), **an upper bound
//!   for three or more** (the angles cannot always align every pair at
//!   once). Never above the parts added by size
//!   \\( \big(\sum_k|\vec E_k|\big)^2 \\) (Cauchy–Schwarz): the parts are
//!   solved together, each with every other wire in place.
//!
//! Parts of different frequencies have no fixed angle; their rms adds as
//! \\( \sum_k|\vec E_k|^2 \\), below the bound: conservative.

use std::f64::consts::PI;

use nalgebra::Complex;
use serde::{Deserialize, Serialize};

use crate::cross_section::PhaseData;
use crate::earth::EPS0;
use crate::electric::{
    CircuitVoltage, GroundPlane, VoltagesUsed, phase_voltage, segment_field, voltage_notes,
};
use crate::matrices;

/// A complex number.
pub type C = Complex<f64>;

/// \\( 1/(4\pi\varepsilon_0) \\), m/F.
const K: f64 = 1.0 / (4.0 * PI * EPS0);

/// A wire of the joint system.
#[derive(Debug, Clone, PartialEq)]
pub struct JointWire {
    /// Its centre line as solved (a bundle's centre), m.
    pub points: Vec<[f64; 3]>,
    /// Its radius (a bundle's equivalent radius), m.
    pub radius: f64,
    /// Its part (one line's phases of one frequency); `None`: earthed.
    pub part: Option<usize>,
    /// Its voltage to earth within its part (rms phasor at its own phase
    /// angle), V; zero when earthed.
    pub volts: C,
}

/// How the joint system is cut into pieces and where it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JointSettings {
    /// The wires within this distance (in plan) of the place computed are
    /// in the system, m; farther: left out (said).
    pub reach: f64,
    /// The shortest piece (near the place computed), m.
    pub min_piece: f64,
    /// Pieces grow with their distance from the place computed: at most
    /// this fraction of it long.
    pub grading: f64,
    /// The longest piece, m.
    pub max_piece: f64,
    /// The ground plane under a point is fitted over this distance either
    /// side, m.
    pub ground_radius: f64,
}

impl Default for JointSettings {
    fn default() -> Self {
        Self {
            reach: 500.0,
            min_piece: 4.0,
            grading: 0.15,
            max_piece: 60.0,
            ground_radius: 10.0,
        }
    }
}

/// A straight piece of uniform charge.
#[derive(Debug, Clone, Copy)]
struct Piece {
    a: [f64; 3],
    b: [f64; 3],
    radius: f64,
    wire: usize,
}

impl Piece {
    fn mid(&self) -> [f64; 3] {
        std::array::from_fn(|k| 0.5 * (self.a[k] + self.b[k]))
    }

    /// Where its potential is held: on its surface at its middle, across
    /// it and level when it can be.
    fn surface(&self) -> [f64; 3] {
        let d: [f64; 3] = std::array::from_fn(|k| self.b[k] - self.a[k]);
        let mut n = [d[1], -d[0], 0.0];
        let l = n[0].hypot(n[1]);
        n = if l < 1e-9 {
            [1.0, 0.0, 0.0]
        } else {
            n.map(|x| x / l)
        };
        let m = self.mid();
        std::array::from_fn(|k| m[k] + self.radius * n[k])
    }

    /// The potential at `p` of 1 C/m along it, V (no nearer than its
    /// radius: a thin wire).
    fn potential(&self, p: [f64; 3]) -> f64 {
        let d: [f64; 3] = std::array::from_fn(|k| self.b[k] - self.a[k]);
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < 1e-9 {
            return 0.0;
        }
        let u = d.map(|x| x / len);
        let r: [f64; 3] = std::array::from_fn(|k| p[k] - self.a[k]);
        let t = r[0] * u[0] + r[1] * u[1] + r[2] * u[2];
        let rho = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] - t * t)
            .max(0.0)
            .sqrt()
            .max(self.radius);
        K * (((len - t) / rho).asinh() + (t / rho).asinh())
    }
}

/// The parts' fields at the points (V/m, rms phasors), solved together.
#[derive(Debug, Clone, Default)]
pub struct JointField {
    /// Per point, per part: the vector \\( \vec E_k \\) (three phasors).
    pub parts: Vec<Vec<[C; 3]>>,
    /// The pieces solved.
    pub pieces: usize,
    /// The wires in the system (within the reach).
    pub wires: usize,
}

/// `wires` cut into pieces graded with their distance (in plan) from
/// `centre`, those within `s.reach` kept.
fn pieces(wires: &[JointWire], centre: [f64; 2], s: &JointSettings) -> Vec<Piece> {
    let target = |p: [f64; 3]| {
        let d = (p[0] - centre[0]).hypot(p[1] - centre[1]);
        (s.grading * d).clamp(s.min_piece, s.max_piece.max(s.min_piece))
    };
    let mut out = Vec::new();
    for (w, wire) in wires.iter().enumerate() {
        if wire.points.len() < 2 {
            continue;
        }
        // Walk the polyline, cutting a piece each time the length walked
        // since its start reaches the target at its start.
        let mut start = wire.points[0];
        let mut walked = 0.0;
        let mut want = target(start);
        for seg in wire.points.windows(2) {
            let d: [f64; 3] = std::array::from_fn(|k| seg[1][k] - seg[0][k]);
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let mut used = 0.0;
            while len - used > 1e-9 {
                let rest = want - walked;
                if len - used >= rest {
                    used += rest;
                    let f = used / len;
                    let end: [f64; 3] = std::array::from_fn(|k| seg[0][k] + f * d[k]);
                    out.push(Piece {
                        a: start,
                        b: end,
                        radius: wire.radius,
                        wire: w,
                    });
                    start = end;
                    walked = 0.0;
                    want = target(start);
                } else {
                    walked += len - used;
                    used = len;
                }
            }
        }
        let last = wire.points[wire.points.len() - 1];
        // A short rest joins the wire's last piece (no sliver of a piece:
        // its coefficient would be far below the others').
        let joins = out
            .last()
            .is_some_and(|p: &Piece| p.wire == w && walked < 0.5 * want);
        if joins {
            if let Some(p) = out.last_mut() {
                p.b = last;
            }
        } else if walked > 1e-6 {
            out.push(Piece {
                a: start,
                b: last,
                radius: wire.radius,
                wire: w,
            });
        }
    }
    out.retain(|p| {
        let m = p.mid();
        (m[0] - centre[0]).hypot(m[1] - centre[1]) <= s.reach
    });
    out
}

/// The field of `parts` parts of `wires` at `points` (V/m), every wire
/// within `s.reach` (in plan) of `centre` solved together (see the
/// module's notes); `ground`: the ground's elevation at a plan point.
///
/// Expected cost: the pieces' potential coefficients (n², n some 50 per
/// wire of a kilometre with the defaults) and their LU on all cores
/// (n³/3), then n per point and part.
pub fn solve(
    wires: &[JointWire],
    parts: usize,
    centre: [f64; 2],
    points: &[[f64; 3]],
    ground: &(dyn Fn(f64, f64) -> f64 + Sync),
    s: &JointSettings,
) -> Result<JointField, String> {
    let system = System::solve(wires, parts, centre, ground, s)?;
    Ok(JointField {
        parts: system.field(points, ground),
        pieces: system.pieces(),
        wires: system.wires,
    })
}

/// The lines' charges solved together around one place
/// ([`System::solve`]), their field taken at any point after
/// ([`System::field`]): one system serves every point near its place (a
/// cut's, a volume's stretch's; 2026-11-25). [`solve`] is one and its
/// field at the points given.
#[derive(Debug, Clone)]
pub struct System {
    ps: Vec<Piece>,
    /// Per part, each piece's charge (C/m, rms phasor).
    q: Vec<Vec<C>>,
    parts: usize,
    /// The wires in the system (within the reach).
    pub wires: usize,
    ground_radius: f64,
}

impl System {
    /// The pieces solved.
    pub fn pieces(&self) -> usize {
        self.ps.len()
    }

    /// The pieces' ends (each a straight piece of a wire), m.
    pub fn segments(&self) -> Vec<[[f64; 3]; 2]> {
        self.ps.iter().map(|p| [p.a, p.b]).collect()
    }

    /// Every wire of `parts` parts of `wires` within `s.reach` (in plan) of
    /// `centre` solved together (see the module's notes); `ground`: the
    /// ground's elevation at a plan point.
    ///
    /// Expected cost: the pieces' potential coefficients (n², n some 50
    /// per wire of a kilometre with the defaults) and their LU on all
    /// cores (n³/3).
    pub fn solve(
        wires: &[JointWire],
        parts: usize,
        centre: [f64; 2],
        ground: &(dyn Fn(f64, f64) -> f64 + Sync),
        s: &JointSettings,
    ) -> Result<Self, String> {
        solve_system(wires, parts, centre, ground, s)
    }

    /// Its field at `points` (V/m), per point per part \\( \vec E_k \\): each
    /// piece's and its image's in the ground plane fitted under the point
    /// (`ground`: the ground's elevation, as the system was solved with).
    ///
    /// Expected cost: n per point and part (on all cores).
    pub fn field(
        &self,
        points: &[[f64; 3]],
        ground: &(dyn Fn(f64, f64) -> f64 + Sync),
    ) -> Vec<Vec<[C; 3]>> {
        if self.ps.is_empty() {
            return vec![vec![[C::new(0.0, 0.0); 3]; self.parts]; points.len()];
        }
        let threads = crate::field_views::threads();
        let one = |p: &[f64; 3]| self.field_at(*p, ground);
        std::thread::scope(|scope| {
            let chunk = points.len().div_ceil(threads).max(1);
            let one = &one;
            let handles: Vec<_> = points
                .chunks(chunk)
                .map(|c| scope.spawn(move || c.iter().map(one).collect::<Vec<_>>()))
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().expect("a point's field"))
                .collect()
        })
    }
}

impl System {
    /// Its field at point `p` alone (V/m), per part — [`System::field`]'s
    /// for one point, on the calling thread (for callers that spread the
    /// points over the cores themselves).
    pub fn field_at(&self, p: [f64; 3], ground: &(dyn Fn(f64, f64) -> f64 + Sync)) -> Vec<[C; 3]> {
        let (ps, q, parts) = (&self.ps, &self.q, self.parts);
        let mut out = vec![[C::new(0.0, 0.0); 3]; parts];
        if ps.is_empty() {
            return out;
        }
        let plane = GroundPlane::fitted(ground, p[0], p[1], self.ground_radius);
        let mirror = plane.mirror(p);
        for (j, piece) in ps.iter().enumerate() {
            let d = segment_field(piece.a, piece.b, p);
            let m = plane.reflect(segment_field(piece.a, piece.b, mirror));
            let g: [f64; 3] = std::array::from_fn(|c| d[c] - m[c]);
            for (k, e) in out.iter_mut().enumerate() {
                for c in 0..3 {
                    e[c] += q[k][j] * g[c];
                }
            }
        }
        out
    }
}

fn solve_system(
    wires: &[JointWire],
    parts: usize,
    centre: [f64; 2],
    ground: &(dyn Fn(f64, f64) -> f64 + Sync),
    s: &JointSettings,
) -> Result<System, String> {
    if !(s.reach > 0.0 && s.min_piece > 0.0 && s.grading >= 0.0 && s.ground_radius > 0.0) {
        return Err("a reach, shortest piece and ground radius above 0".into());
    }
    let ps = pieces(wires, centre, s);
    let n = ps.len();
    let mut used: Vec<usize> = ps.iter().map(|p| p.wire).collect();
    used.dedup();
    used.sort_unstable();
    used.dedup();
    if n == 0 {
        return Ok(System {
            ps,
            q: vec![Vec::new(); parts],
            parts,
            wires: 0,
            ground_radius: s.ground_radius,
        });
    }
    // Each piece's surface point and the ground plane under it.
    let at: Vec<([f64; 3], GroundPlane)> = ps
        .iter()
        .map(|p| {
            let m = p.surface();
            (m, GroundPlane::fitted(ground, m[0], m[1], s.ground_radius))
        })
        .collect();
    for (p, (m, plane)) in ps.iter().zip(&at) {
        let h = (0..3)
            .map(|k| (m[k] - plane.point[k]) * plane.normal[k])
            .sum::<f64>();
        if h <= p.radius {
            return Err(format!(
                "a wire at or below the ground near ({:.1}, {:.1}): the lines cannot be solved \
                 together there",
                m[0], m[1]
            ));
        }
    }
    // The potential coefficients column by column on all cores
    // (column-major): the potential at piece i's surface point of piece j
    // and its image in the plane under that point.
    let mut table = vec![0.0; n * n];
    let threads = crate::field_views::threads();
    std::thread::scope(|scope| {
        let per = n.div_ceil(threads).max(1);
        let (ps, at) = (&ps, &at);
        for (c, cols) in table.chunks_mut(per * n).enumerate() {
            scope.spawn(move || {
                for (k, col) in cols.chunks_mut(n).enumerate() {
                    let j = c * per + k;
                    for (i, v) in col.iter_mut().enumerate() {
                        let (m, plane) = &at[i];
                        *v = ps[j].potential(*m) - ps[j].potential(plane.mirror(*m));
                    }
                }
            });
        }
    });
    let lu = faer::MatRef::from_column_major_slice(&table, n, n).partial_piv_lu();
    drop(table);
    // Each part's voltages: the real and imaginary parts as two
    // right-hand sides each.
    let rhs = faer::Mat::<f64>::from_fn(n, 2 * parts, |i, c| {
        let w = &wires[ps[i].wire];
        match w.part {
            Some(k) if k == c / 2 => {
                if c % 2 == 0 {
                    w.volts.re
                } else {
                    w.volts.im
                }
            }
            _ => 0.0,
        }
    });
    let x = faer::linalg::solvers::Solve::solve(&lu, &rhs);
    let mut q = vec![vec![C::new(0.0, 0.0); n]; parts];
    for (k, qk) in q.iter_mut().enumerate() {
        for (i, v) in qk.iter_mut().enumerate() {
            *v = C::new(x[(i, 2 * k)], x[(i, 2 * k + 1)]);
            if !(v.re.is_finite() && v.im.is_finite()) {
                return Err("the lines' system of charges could not be solved".into());
            }
        }
    }
    Ok(System {
        ps,
        q,
        parts,
        wires: used.len(),
        ground_radius: s.ground_radius,
    })
}

/// \\( \langle a, b\rangle = \sum_c \bar a_c b_c \\).
fn inner(a: &[C; 3], b: &[C; 3]) -> C {
    (0..3).map(|c| a[c].conj() * b[c]).sum()
}

/// The rms of a vector of phasors.
pub fn rms(e: &[C; 3]) -> f64 {
    e.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt()
}

/// The largest rms of \\( \sum_k e^{j\theta_k}\vec E_k \\) over the parts'
/// unknown angles (see the module's notes), V/m: exact for one or two
/// parts, an upper bound for more.
pub fn worst(parts: &[[C; 3]]) -> f64 {
    let mut sum: f64 = parts
        .iter()
        .map(|e| e.iter().map(|c| c.norm_sqr()).sum::<f64>())
        .sum();
    for k in 0..parts.len() {
        for l in k + 1..parts.len() {
            sum += 2.0 * inner(&parts[k], &parts[l]).norm();
        }
    }
    sum.max(0.0).sqrt()
}

/// What [`worst`] is: exact or a bound.
pub fn method_words(parts: usize) -> String {
    if parts <= 2 {
        format!(
            "solved together ({parts} part(s): exact over the unknown phase angle between them, \
             the worst taken)"
        )
    } else {
        format!(
            "solved together ({parts} parts: an upper bound over their unknown phase angles — \
             each pair's cross term at its largest, |E|² ≤ Σ|E_k|² + 2Σ|<E_k, E_l>|; never above \
             the parts added by size)"
        )
    }
}

/// A line's wires for the joint system: every solved span of `shapes` of
/// `line` (a bundle as its equivalent wire), its phases at their circuits'
/// voltages ([`crate::electric::circuit_voltage`]; `given` first), its
/// earth wires earthed — and **its jumpers** (`jumpers`, as solved;
/// 2026-11-25): each held at the voltage of the section ending where it
/// starts (else where it ends), a bundle of that section's as its
/// equivalent wire with the jumper's own conductor (the section's when the
/// library has not the jumper's), as the line alone gives them its
/// phase's charge. Gives each wire with its frequency (`None`: earthed),
/// and adds to `notes` the voltages used (`label` before them: the line's
/// name) and what was left out.
pub fn line_wires(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    shapes: &[ohl_line::solve::SectionShape],
    jumpers: &[ohl_line::jumper::JumperShape],
    given: &[CircuitVoltage],
    label: &str,
    notes: &mut Vec<String>,
) -> Vec<(Option<f64>, JointWire)> {
    let phases = match line.section_phases() {
        Ok(p) => p,
        Err(e) => {
            notes.push(format!(
                "{label}: its phases could not be read ({e}): left out"
            ));
            return Vec::new();
        }
    };
    let mut used = VoltagesUsed::new();
    let mut out = Vec::new();
    let mut dc = false;
    for shape in shapes {
        let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
            continue;
        };
        let Some(cable) = library.get(&section.cable) else {
            notes.push(format!(
                "{label}: section {}: wire type {} not in the wire library, left out",
                section.id, section.cable
            ));
            continue;
        };
        let r = 0.5 * cable.diameter.si();
        let (_, radius) = matrices::bundle(section.bundle.max(1), section.bundle_spacing, r, r);
        let phase: Option<PhaseData> = phases
            .get(&section.id)
            .copied()
            .flatten()
            .map(|at| crate::cross_section::phase_data(line, section, at));
        let volts = match &phase {
            Some(p) => match phase_voltage(p, given, &mut used) {
                Some(v) => Some(v),
                None => {
                    dc = true;
                    continue;
                }
            },
            None => None,
        };
        for sp in &shape.spans {
            if sp.points.len() < 2 {
                continue;
            }
            out.push((
                volts.map(|v| v.0),
                JointWire {
                    points: sp.points.clone(),
                    radius,
                    part: None,
                    volts: volts.map_or(C::new(0.0, 0.0), |v| v.1),
                },
            ));
        }
    }
    // The jumpers: each at the voltage of the section at one of its ends.
    let mut no_section = Vec::new();
    for js in jumpers {
        let Some(j) = line.jumpers.iter().find(|j| j.id == js.jumper) else {
            continue;
        };
        if js.points.len() < 2 {
            continue;
        }
        let section = [&j.from, &j.to].into_iter().find_map(|e| {
            let p = e.point()?;
            line.sections.iter().find(|sec| {
                let n = sec.points.len();
                n >= 2 && (sec.points[0] == *p || sec.points[n - 1] == *p)
            })
        });
        let Some(section) = section else {
            no_section.push(j.id.clone());
            continue;
        };
        let Some(cable) = library
            .get(&j.cable)
            .or_else(|| library.get(&section.cable))
        else {
            notes.push(format!(
                "{label}: jumper {}: wire types {} and {} not in the wire library, left out",
                j.id, j.cable, section.cable
            ));
            continue;
        };
        let r = 0.5 * cable.diameter.si();
        let (_, radius) = matrices::bundle(section.bundle.max(1), section.bundle_spacing, r, r);
        let phase: Option<PhaseData> = phases
            .get(&section.id)
            .copied()
            .flatten()
            .map(|at| crate::cross_section::phase_data(line, section, at));
        let volts = match &phase {
            Some(p) => match phase_voltage(p, given, &mut used) {
                Some(v) => Some(v),
                None => {
                    dc = true;
                    continue;
                }
            },
            None => None,
        };
        out.push((
            volts.map(|v| v.0),
            JointWire {
                points: js.points.clone(),
                radius,
                part: None,
                volts: volts.map_or(C::new(0.0, 0.0), |v| v.1),
            },
        ));
    }
    if !no_section.is_empty() {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: {label}: jumper(s) {} end at no section's end, so their \
             voltage is not known: left out of the system — a conductor near the ground missing, \
             the field near them can be higher; join them to a section's end",
            no_section.join(", ")
        ));
    }
    if dc {
        notes.push(format!(
            "{label}: DC poles left out (a static field and space charge: another calculation)"
        ));
    }
    notes.extend(voltage_notes(&used, "field").into_iter().map(|t| {
        match t.strip_prefix("NOT ON THE SAFE SIDE: ") {
            Some(rest) => format!("NOT ON THE SAFE SIDE: {label}: {rest}"),
            None => format!("{label}: {t}"),
        }
    }));
    out
}

/// The ground of several lines where no terrain gives it: straight
/// between the structures' feet and level across, by the nearest span of
/// any of `lines` (each line's own as [`crate::electric::feet_ground`]).
pub fn feet_ground_of(
    lines: &[&ohl_line::Line],
) -> impl Fn(f64, f64) -> f64 + Send + Sync + 'static {
    let axes: Vec<crate::cross_section::Axis> = lines
        .iter()
        .flat_map(|line| {
            crate::cross_section::line_spans(line)
                .iter()
                .filter_map(|s| crate::cross_section::Axis::of(line, s).ok())
                .collect::<Vec<_>>()
        })
        .collect();
    move |x: f64, y: f64| {
        let mut best: Option<(f64, f64)> = None;
        for a in &axes {
            let (t, s) = a.local(&[x, y, 0.0]);
            let tc = t.clamp(0.0, a.length);
            let d = (t - tc).hypot(s);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, a.ground(None, tc, 0.0)));
            }
        }
        best.map_or(0.0, |b| b.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two long parallel single wires over level ground at z = 0, against
    /// the image method in 2D by hand: the potential coefficients
    /// \\( p_{ii} = \frac{1}{2\pi\varepsilon_0}\ln\frac{2h_i}{r} \\),
    /// \\( p_{ij} = \frac{1}{2\pi\varepsilon_0}\ln\frac{D'_{ij}}{D_{ij}} \\),
    /// \\( q = P^{-1}V \\), the field of each line charge and its image.
    #[test]
    fn two_wires_against_the_image_method() {
        let (h1, h2, y2, r) = (12.0, 15.0, 9.0, 0.015);
        let long = |y: f64, h: f64| -> Vec<[f64; 3]> {
            (0..=400)
                .map(|i| [-2000.0 + 10.0 * f64::from(i), y, h])
                .collect()
        };
        let v1 = C::from_polar(220e3 / 3f64.sqrt(), 0.0);
        let v2 = C::from_polar(132e3 / 3f64.sqrt(), 0.0);
        let wires = vec![
            JointWire {
                points: long(0.0, h1),
                radius: r,
                part: Some(0),
                volts: v1,
            },
            JointWire {
                points: long(y2, h2),
                radius: r,
                part: Some(1),
                volts: v2,
            },
        ];
        let points: Vec<[f64; 3]> = (0..=20)
            .map(|i| [0.0, -10.0 + 1.5 * f64::from(i), 1.0])
            .collect();
        let s = JointSettings {
            reach: 2000.0,
            min_piece: 2.0,
            grading: 0.05,
            ..JointSettings::default()
        };
        let got = solve(&wires, 2, [0.0, 0.0], &points, &|_, _| 0.0, &s).expect("solved");
        // By hand.
        let k2 = 1.0 / (2.0 * PI * EPS0);
        let d = y2.hypot(h2 - h1);
        let di = y2.hypot(h2 + h1);
        let p = [
            [k2 * (2.0 * h1 / r).ln(), k2 * (di / d).ln()],
            [k2 * (di / d).ln(), k2 * (2.0 * h2 / r).ln()],
        ];
        let det = p[0][0] * p[1][1] - p[0][1] * p[1][0];
        let inv = [
            [p[1][1] / det, -p[0][1] / det],
            [-p[1][0] / det, p[0][0] / det],
        ];
        for (pt, e) in points.iter().zip(&got.parts) {
            for (part, v) in [(0, v1), (1, v2)] {
                let q = [inv[0][part] * v, inv[1][part] * v];
                let f1 = crate::electric::line_field(0.0, h1, pt[1], pt[2]);
                let f2 = crate::electric::line_field(y2, h2, pt[1], pt[2]);
                let hand = [
                    C::new(0.0, 0.0),
                    q[0] * f1[0] + q[1] * f2[0],
                    q[0] * f1[1] + q[1] * f2[1],
                ];
                let (a, b) = (rms(&e[part]), rms(&hand));
                assert!(
                    (a - b).abs() <= 0.005 * b.max(1.0),
                    "part {part} at y {}: {a} V/m against {b} V/m by hand",
                    pt[1]
                );
            }
            // The worst over the angle: exact for two parts — no sampled
            // angle above it, the best sampled within a thousandth.
            let w = worst(e);
            let mut top: f64 = 0.0;
            for i in 0..3600 {
                let t = f64::from(i) * PI / 1800.0;
                let rot = C::from_polar(1.0, t);
                let sum: [C; 3] = std::array::from_fn(|c| e[0][c] + rot * e[1][c]);
                top = top.max(rms(&sum));
            }
            assert!(top <= w * (1.0 + 1e-12), "{top} above {w}");
            assert!(top >= w * (1.0 - 1e-3), "{top} below {w}");
            assert!(w <= rms(&e[0]) + rms(&e[1]) + 1e-9);
        }
    }

    /// An earthed wire beside a phase screens it: the phase's field below
    /// with the earthed wire solved together is lower than alone (which is
    /// why fields solved apart and added are not a proven bound — the
    /// screen's own field, negative here, is not a separate source).
    #[test]
    fn an_earthed_wire_screens() {
        let long = |y: f64, h: f64| -> Vec<[f64; 3]> {
            (0..=200)
                .map(|i| [-1000.0 + 10.0 * f64::from(i), y, h])
                .collect()
        };
        let phase = JointWire {
            points: long(0.0, 10.0),
            radius: 0.015,
            part: Some(0),
            volts: C::new(230e3 / 3f64.sqrt(), 0.0),
        };
        let earth = JointWire {
            points: long(2.0, 8.0),
            radius: 0.01,
            part: None,
            volts: C::new(0.0, 0.0),
        };
        let s = JointSettings::default();
        let at = [[0.0, 2.0, 1.0]];
        let alone = solve(
            std::slice::from_ref(&phase),
            1,
            [0.0, 0.0],
            &at,
            &|_, _| 0.0,
            &s,
        )
        .expect("alone");
        let both = solve(&[phase, earth], 1, [0.0, 0.0], &at, &|_, _| 0.0, &s).expect("both");
        assert!(rms(&both.parts[0][0]) < 0.9 * rms(&alone.parts[0][0]));
    }

    /// Three parts: the bound is above every sampled pair of angles and
    /// below the parts added by size.
    #[test]
    fn three_parts_bounded() {
        let e = [
            [C::new(1.0, 0.2), C::new(0.0, 0.5), C::new(0.3, 0.0)],
            [C::new(0.2, -0.4), C::new(0.6, 0.1), C::new(0.0, 0.2)],
            [C::new(-0.3, 0.3), C::new(0.1, 0.1), C::new(0.5, -0.2)],
        ];
        let w = worst(&e);
        let by_size: f64 = e.iter().map(rms).sum();
        assert!(w <= by_size + 1e-12);
        for i in 0..90 {
            for j in 0..90 {
                let (a, b) = (
                    C::from_polar(1.0, f64::from(i) * PI / 45.0),
                    C::from_polar(1.0, f64::from(j) * PI / 45.0),
                );
                let sum: [C; 3] = std::array::from_fn(|c| e[0][c] + a * e[1][c] + b * e[2][c]);
                assert!(rms(&sum) <= w + 1e-12);
            }
        }
    }
}
