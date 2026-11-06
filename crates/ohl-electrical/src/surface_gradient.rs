// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The conductor surface gradient in 2D cross-sections (roadmap O18 item
//! 7; built 2026-11-04, 2D only, in active development, not approved
//! yet; owner, 2026-11-04: "only for the 2D cuts for the noise do not
//! apply it in 3D as these are different usage").
//!
//! **Method: a charge simulation** (Singer, Steinbigler and Weiss, 1974).
//! Every subconductor and earth wire of the cross-section is a smooth
//! cylinder of its wire's radius \\( r \\), infinitely long and parallel
//! to level, perfectly conducting ground. Inside each, \\( n \\) line
//! charges ([`CHARGES`]) on a circle of radius \\( 0.3\,r \\)
//! ([`PLACEMENT`]; not \\( r/k \\), \\( k = 1 + 2\sin(\pi/n) \\),
//! [`charge_radius`]: measured, its field ripples by 0.3 % between the
//! collocation points), at the angles \\( 2\pi j/n \\); on its surface
//! \\( n \\) collocation points at the same angles; every charge's image
//! in the ground. The potentials at the collocation points equal the
//! wire's voltage to earth (a phase: its system's voltage between phases
//! over \\( \sqrt 3 \\) at its phase angle, as the electric field takes it;
//! an earth wire: zero) — one real linear system, solved once per
//! cross-section and frequency for the complex (phasor) right-hand side.
//! The field normal to the surface is then evaluated at [`POINTS`] points
//! round every subconductor; the rms of its phasor (frequencies added as
//! \\( \sqrt{\sum_f E_f^2} \\)) is the surface gradient there.
//!
//! **Reported per subconductor:** the largest surface gradient and its
//! angle (degrees from the horizontal to the right looking ahead,
//! counter-clockwise: 90° the top, −90° the bottom), and its mean
//! \\( |Q|/(2\pi\varepsilon_0 r) \\) (Gauss's law); **per bundle:** the
//! *average maximum* — the mean over its subconductors of each one's
//! largest gradient, the value corona noise formulas take — and the
//! true maximum; beside them **Markt–Mengele**'s average maximum from the
//! bundle's charge of the equivalent-wire potential coefficients,
//! \\( \frac{|Q|}{n\,2\pi\varepsilon_0 r}\left(1 + (n-1)\frac{r}{R}\right) \\),
//! as a cross-check. All in kV/cm **rms**; the peak is \\( \sqrt 2 \\)
//! times. A self-check per cross-section: the largest potential error
//! between the collocation points, as a share of the largest voltage.
//!
//! **Not modelled** (each said in the theory, *Surface gradient*): the
//! structures' steel, corona rings, spacers and fittings (2D: infinite
//! parallel wires at the cut's heights); the stranding (a smooth
//! cylinder — the noise formulas' surface factor stands for it); the
//! ground level and perfectly conducting under the cut; DC systems (a
//! static field with space charge: another calculation).

use std::collections::BTreeMap;
use std::f64::consts::PI;

use nalgebra::{DMatrix, DVector};
use ohl_core::text::csv_field;
use ohl_line::Bundle;
use serde::{Deserialize, Serialize};

use crate::cross_section::{self, Axis, CrossSection, LineSpan, Sources, StationAt};
use crate::earth::EPS0;
use crate::electric::{C, CircuitVoltage, cross_section_charges, phase_voltage};
use crate::magnetic::{self, StationSpec};

/// \\( 1/(2\pi\varepsilon_0) \\), m/F.
const K2: f64 = 1.0 / (2.0 * PI * EPS0);

/// The points round every subconductor at which the surface field is
/// evaluated (one a degree).
pub const POINTS: usize = 360;

/// The line charges inside every subconductor by default: the smallest
/// count within 0.05 % of 64 on the benchmark cross-sections and the
/// closed forms (the convergence study in the theory page, *Surface
/// gradient*: 8 at [`PLACEMENT`], 0.01 % on the cross-sections).
pub const CHARGES: usize = 8;

/// The radius of the line charges' circle as a share of the
/// subconductor's radius (measured, the theory page's table: with
/// \\( r/k \\), [`charge_radius`], the field between the collocation points
/// ripples by some 0.3 % however many charges — \\( (1/k)^n \\) tends to
/// \\( e^{-2\pi} \\); at 0.3 r the ripple falls as \\( 0.3^n \\), fastest of
/// those measured on line geometries, and the solution stays exact to
/// 1e-11 up to 64 charges).
pub const PLACEMENT: f64 = 0.3;

/// The radius of the circle the `n` line charges of a cylinder of radius
/// `r` stand on: \\( r/k \\), \\( k = 1 + 2\sin(\pi/n) \\) (the charges
/// nearer the surface the more there are; one charge: at the centre).
pub fn charge_radius(r: f64, n: usize) -> f64 {
    if n <= 1 {
        0.0
    } else {
        r / (1.0 + 2.0 * (PI / n as f64).sin())
    }
}

// ---------------------------------------------------------------------------
// The charge simulation.

/// A cylinder of the cross-section: its centre (across, height above the
/// ground), radius and voltage to earth (rms phasor), m and V.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cylinder {
    /// Across, m.
    pub y: f64,
    /// Height of the centre above the ground, m.
    pub z: f64,
    /// Radius, m.
    pub radius: f64,
    /// Voltage to earth, V (rms phasor).
    pub volts: C,
}

/// A solved charge simulation at one frequency.
#[derive(Debug, Clone)]
pub struct Solution {
    /// The cylinders.
    pub cylinders: Vec<Cylinder>,
    /// Line charges per cylinder.
    pub n: usize,
    /// With images in a ground at z = 0 (else free space).
    pub ground: bool,
    /// The charges' places, cylinder by cylinder, `n` each.
    pub places: Vec<[f64; 2]>,
    /// The charges, C/m (rms phasors), in the order of `places`.
    pub charges: Vec<C>,
}

/// The potential at `p` of 1 C/m at `c` (and of −1 C/m at its image in
/// the ground at z = 0 when `ground`), V.
fn potential_of(c: [f64; 2], p: [f64; 2], ground: bool) -> f64 {
    let d = (p[0] - c[0]).hypot(p[1] - c[1]).max(1e-15);
    if ground {
        K2 * ((p[0] - c[0]).hypot(p[1] + c[1]) / d).ln()
    } else {
        -K2 * d.ln()
    }
}

/// The field at `p` of 1 C/m at `c` (and its image), V/m.
fn field_of(c: [f64; 2], p: [f64; 2], ground: bool) -> [f64; 2] {
    let (dy, dz) = (p[0] - c[0], p[1] - c[1]);
    let r2 = (dy * dy + dz * dz).max(1e-30);
    let mut e = [K2 * dy / r2, K2 * dz / r2];
    if ground {
        let (iy, iz) = (p[0] - c[0], p[1] + c[1]);
        let i2 = (iy * iy + iz * iz).max(1e-30);
        e[0] -= K2 * iy / i2;
        e[1] -= K2 * iz / i2;
    }
    e
}

/// The point at angle `a` (radians) on the circle of radius `rho` round
/// cylinder `c`.
fn round(c: &Cylinder, rho: f64, a: f64) -> [f64; 2] {
    [c.y + rho * a.cos(), c.z + rho * a.sin()]
}

/// Solves the charge simulation of `cylinders` with `n` line charges in
/// each (on a circle of `placement` times the radius, [`PLACEMENT`] by
/// default; 0: [`charge_radius`]) and `n` collocation points on each surface, with images in a
/// perfectly conducting ground at z = 0 when `ground` (else in free
/// space). `None`: the system is singular (cylinders overlapping, a
/// cylinder reaching the ground).
pub fn solve(cylinders: &[Cylinder], n: usize, ground: bool, placement: f64) -> Option<Solution> {
    let n = n.max(1);
    let total = cylinders.len() * n;
    if total == 0 {
        return None;
    }
    let mut places = Vec::with_capacity(total);
    let mut colloc = Vec::with_capacity(total);
    let mut volts = Vec::with_capacity(total);
    for c in cylinders {
        if ground && c.z <= c.radius {
            return None;
        }
        let a = if placement > 0.0 {
            placement * c.radius
        } else {
            charge_radius(c.radius, n)
        };
        for j in 0..n {
            let t = 2.0 * PI * j as f64 / n as f64;
            places.push(round(c, a, t));
            colloc.push(round(c, c.radius, t));
            volts.push(c.volts);
        }
    }
    let p = DMatrix::from_fn(total, total, |i, j| {
        potential_of(places[j], colloc[i], ground)
    });
    let lu = p.lu();
    let re = lu.solve(&DVector::from_iterator(total, volts.iter().map(|v| v.re)))?;
    let im = lu.solve(&DVector::from_iterator(total, volts.iter().map(|v| v.im)))?;
    let charges: Vec<C> = re
        .iter()
        .zip(im.iter())
        .map(|(&a, &b)| C::new(a, b))
        .collect();
    if charges
        .iter()
        .any(|q| !(q.re.is_finite() && q.im.is_finite()))
    {
        return None;
    }
    Some(Solution {
        cylinders: cylinders.to_vec(),
        n,
        ground,
        places,
        charges,
    })
}

impl Solution {
    /// The potential at `p`, V (phasor).
    pub fn potential(&self, p: [f64; 2]) -> C {
        self.places
            .iter()
            .zip(&self.charges)
            .map(|(c, q)| q * potential_of(*c, p, self.ground))
            .sum()
    }

    /// The field at `p`, V/m (phasors of its two components).
    pub fn field(&self, p: [f64; 2]) -> [C; 2] {
        let mut e = [C::new(0.0, 0.0); 2];
        for (c, q) in self.places.iter().zip(&self.charges) {
            let f = field_of(*c, p, self.ground);
            e[0] += q * f[0];
            e[1] += q * f[1];
        }
        e
    }

    /// The field normal to cylinder `k`'s surface (outward) at angle `a`
    /// (radians), V/m (phasor), and the tangential one.
    pub fn surface_field(&self, k: usize, a: f64) -> (C, C) {
        let c = &self.cylinders[k];
        let e = self.field(round(c, c.radius, a));
        let (cs, sn) = (a.cos(), a.sin());
        (e[0] * cs + e[1] * sn, -e[0] * sn + e[1] * cs)
    }

    /// The total charge of cylinder `k`, C/m (phasor).
    pub fn charge(&self, k: usize) -> C {
        self.charges[k * self.n..(k + 1) * self.n].iter().sum()
    }

    /// The largest potential error at the points midway between the
    /// collocation points of every cylinder, V.
    pub fn potential_error(&self) -> f64 {
        let mut worst: f64 = 0.0;
        for c in &self.cylinders {
            for j in 0..self.n {
                let a = 2.0 * PI * (j as f64 + 0.5) / self.n as f64;
                worst = worst.max((self.potential(round(c, c.radius, a)) - c.volts).norm());
            }
        }
        worst
    }
}

/// The surface gradient of cylinder `k` over solutions of several
/// frequencies (the same cylinders in each): at `points` angles round it,
/// the rms of the normal field \\( \sqrt{\sum_f |E_{n,f}|^2} \\). Returns
/// (its largest value, V/m; that angle, degrees from the horizontal,
/// counter-clockwise, −180…180; the mean \\( \sqrt{\sum_f |Q_f|^2} /
/// (2\pi\varepsilon_0 r) \\), V/m).
pub fn surface_gradient(solutions: &[Solution], k: usize, points: usize) -> (f64, f64, f64) {
    let points = points.max(4);
    let mut best = (f64::NEG_INFINITY, 0.0);
    for i in 0..points {
        let a = 2.0 * PI * i as f64 / points as f64;
        let e = solutions
            .iter()
            .map(|s| s.surface_field(k, a).0.norm_sqr())
            .sum::<f64>()
            .sqrt();
        if e > best.0 {
            best = (e, a);
        }
    }
    let r = solutions.first().map_or(1.0, |s| s.cylinders[k].radius);
    let mean = solutions
        .iter()
        .map(|s| s.charge(k).norm_sqr())
        .sum::<f64>()
        .sqrt()
        * K2
        / r;
    let mut deg = best.1.to_degrees();
    if deg > 180.0 {
        deg -= 360.0;
    }
    (best.0, deg, mean)
}

/// Markt–Mengele's average maximum surface gradient of a bundle of `n`
/// subconductors of radius `r` on a circle of radius `big_r` carrying
/// `q` C/m in all (rms), V/m: \\( \frac{q}{n\,2\pi\varepsilon_0 r}(1 +
/// (n-1) r/R) \\) (one wire: \\( q/(2\pi\varepsilon_0 r) \\)).
pub fn markt_mengele(q: f64, n: u32, r: f64, big_r: f64) -> f64 {
    let n = n.max(1);
    let mean = q * K2 / (f64::from(n) * r);
    if n == 1 || big_r <= 0.0 {
        mean
    } else {
        mean * (1.0 + f64::from(n - 1) * r / big_r)
    }
}

// ---------------------------------------------------------------------------
// The line's cross-sections.

/// What to compute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The circuits' voltages (as the electric field's; a circuit not
    /// given: its system's nominal, said — an ordinance or a noise
    /// assessment may ask the highest operating voltage).
    pub voltages: Vec<CircuitVoltage>,
    /// The spans' cross-sections asked; empty: every span.
    pub stations: Vec<StationSpec>,
    /// Each span also at both ends (at its structures: the first
    /// cross-section every wire of the span reaches).
    pub ends: bool,
    /// Line charges per subconductor ([`CHARGES`] by default).
    pub charges: usize,
    /// Evaluation points round every subconductor ([`POINTS`]).
    pub points: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            voltages: Vec::new(),
            stations: Vec::new(),
            ends: true,
            charges: CHARGES,
            points: POINTS,
        }
    }
}

/// One subconductor's surface gradient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubconductorGradient {
    /// Across the line from the axis to the right looking ahead, m.
    pub offset: f64,
    /// Height of its centre above the ground at the cross-section, m.
    pub height: f64,
    /// The largest surface gradient round it, kV/cm (rms).
    pub max_kv_cm: f64,
    /// Where, degrees from the horizontal to the right, counter-clockwise
    /// (90: the top, −90: the bottom).
    pub angle_deg: f64,
    /// Its mean round it, \\( |Q|/(2\pi\varepsilon_0 r) \\), kV/cm (rms).
    pub mean_kv_cm: f64,
}

/// A bundle's (or a single wire's) surface gradient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleGradient {
    /// Its section.
    pub section: String,
    /// Its circuit (its name, else its system's id); empty: an earth wire.
    pub circuit: String,
    /// Its phase's label; empty: an earth wire.
    pub phase: String,
    /// Its voltage to earth, kV (rms; 0: an earth wire).
    pub kv: f64,
    /// Subconductors.
    pub count: u32,
    /// A subconductor's diameter, m.
    pub diameter: f64,
    /// The subconductors.
    pub subconductors: Vec<SubconductorGradient>,
    /// The average maximum: the mean of the subconductors' largest
    /// gradients, kV/cm (rms) — what the corona noise formulas take.
    pub average_max_kv_cm: f64,
    /// The largest of them, kV/cm (rms).
    pub max_kv_cm: f64,
    /// Markt–Mengele's average maximum from the bundle's charge (the
    /// equivalent wire's potential coefficients), kV/cm (rms).
    pub markt_mengele_kv_cm: f64,
}

/// One cross-section's surface gradients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientCut {
    /// The span's first structure.
    pub from: String,
    /// Its second.
    pub to: String,
    /// The station, m from the first structure.
    pub at: f64,
    /// Which: `start`, `lowest`, `given` or `end`.
    pub place: String,
    /// The bundles and earth wires, in the order of the line's sections.
    pub bundles: Vec<BundleGradient>,
    /// The largest potential error between collocation points, as a share
    /// of the largest voltage to earth (the simulation's self-check).
    pub potential_error: f64,
}

/// The surface gradients of a line's cross-sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGradients {
    /// Line charges per subconductor.
    pub charges: usize,
    /// Evaluation points round each.
    pub points: usize,
    /// The cross-sections.
    pub cuts: Vec<GradientCut>,
    /// What was assumed or left out.
    pub notes: Vec<String>,
}

/// The stretch of `span` along its axis that every wire reaches (from the
/// last start to the first end of their solved points), m.
fn reached(src: &Sources<'_>, span: &LineSpan, axis: &Axis) -> Option<(f64, f64)> {
    let line = src.line;
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for &(k, j) in &span.wires {
        let section = &line.sections[k];
        let Some(sp) = src
            .shapes
            .iter()
            .find(|s| s.section == section.id)
            .and_then(|s| s.spans.get(j))
            .filter(|s| s.points.len() >= 2)
        else {
            continue;
        };
        let ts: Vec<f64> = sp.points.iter().map(|p| axis.local(p).0).collect();
        let (a, b) = ts
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &t| {
                (a.min(t), b.max(t))
            });
        lo = lo.max(a);
        hi = hi.min(b);
    }
    (lo.is_finite() && hi.is_finite() && lo <= hi).then_some((lo, hi))
}

/// The surface gradients of `src`'s line at the cross-sections asked:
/// per span its lowest point (or the station given) and, with
/// [`Settings::ends`], both its ends.
pub fn run(src: &Sources<'_>, settings: &Settings) -> Result<SurfaceGradients, String> {
    let line = src.line;
    let n = settings.charges.max(1);
    let points = settings.points.max(4);
    let spans = cross_section::line_spans(line);
    if spans.is_empty() {
        return Err("the line has no spans".into());
    }
    let mut notes = Vec::new();
    // (span, station along it, place).
    let mut asked: Vec<(usize, f64, &'static str)> = Vec::new();
    let add_ends = |i: usize, axis: &Axis, asked: &mut Vec<(usize, f64, &'static str)>| {
        if !settings.ends {
            return;
        }
        let (a, b) = reached(src, &spans[i], axis).unwrap_or((0.0, axis.length));
        asked.push((i, a.clamp(0.0, axis.length), "start"));
        asked.push((i, b.clamp(0.0, axis.length), "end"));
    };
    if settings.stations.is_empty() {
        for (i, s) in spans.iter().enumerate() {
            let Ok(axis) = Axis::of(line, s) else {
                continue;
            };
            asked.push((i, magnetic::lowest_station(src, s, &axis), "lowest"));
            add_ends(i, &axis, &mut asked);
        }
    } else {
        for st in &settings.stations {
            let i = spans
                .iter()
                .position(|s| {
                    (s.from == st.from && s.to == st.to) || (s.from == st.to && s.to == st.from)
                })
                .ok_or_else(|| format!("no span between structures {} and {}", st.from, st.to))?;
            let axis = Axis::of(line, &spans[i])?;
            match st.fraction {
                Some(f) if (0.0..=1.0).contains(&f) => asked.push((i, f * axis.length, "given")),
                Some(f) => return Err(format!("a place in the span from 0 to 1, not {f}")),
                None => asked.push((i, magnetic::lowest_station(src, &spans[i], &axis), "lowest")),
            }
            add_ends(i, &axis, &mut asked);
        }
    }
    asked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    asked.dedup_by(|a, b| a.0 == b.0 && (a.1 - b.1).abs() < 1e-6);
    let mut defaulted = BTreeMap::new();
    let mut cuts = Vec::new();
    for (i, t, place) in asked {
        let span = &spans[i];
        let axis = Axis::of(line, span)?;
        let cs =
            cross_section::cross_section_reached(src, span, StationAt::Fraction(t / axis.length))?;
        for nt in &cs.notes {
            if !notes.contains(nt) {
                notes.push(nt.clone());
            }
        }
        match cut_gradients(&cs, &settings.voltages, n, points, &mut defaulted) {
            Ok(bundles) => cuts.push(GradientCut {
                from: span.from.clone(),
                to: span.to.clone(),
                at: t,
                place: place.to_owned(),
                bundles: bundles.0,
                potential_error: bundles.1,
            }),
            Err(e) => notes.push(format!("span {}–{} at {t:.1} m: {e}", span.from, span.to)),
        }
    }
    for (c, kv) in &defaulted {
        notes.push(format!(
            "{c}: no voltage given, its system's nominal {kv} kV taken (a noise assessment usually asks the highest operating voltage: give it)"
        ));
    }
    if line
        .systems
        .iter()
        .any(|s| s.kind == ohl_line::electrical::SystemKind::Dc)
    {
        notes.push("DC systems left out (a static field with space charge; not computed)".into());
    }
    notes.push(format!(
        "2D: every wire an infinite smooth cylinder parallel to level, perfectly conducting ground at the cross-section; {n} line charges in each subconductor, the field at {points} points round it; the structures' steel, corona rings, spacers and the stranding not modelled; kV/cm rms (peak: √2 times)"
    ));
    Ok(SurfaceGradients {
        charges: n,
        points,
        cuts,
        notes,
    })
}

/// The bundles' surface gradients of one cross-section, and the largest
/// potential error (a share of the largest voltage).
fn cut_gradients(
    cs: &CrossSection,
    given: &[CircuitVoltage],
    n: usize,
    points: usize,
    defaulted: &mut BTreeMap<String, f64>,
) -> Result<(Vec<BundleGradient>, f64), String> {
    // Every subconductor a cylinder; per wire its range in the list.
    let mut cyl: Vec<(Cylinder, Option<(f64, C)>)> = Vec::new();
    let mut ranges = Vec::new();
    for w in &cs.wires {
        let r = 0.5 * w.wire.diameter;
        let v = w
            .phase
            .as_ref()
            .and_then(|p| phase_voltage(p, given, defaulted));
        let bundle = Bundle {
            count: w.bundle.count.max(1),
            spacing: w.bundle.spacing,
            rotation_deg: Some(w.bundle.rotation_deg),
        };
        let start = cyl.len();
        for o in bundle.offsets() {
            let z = w.elevation - cs.ground.elevation + o[1];
            if z <= r {
                return Err(format!("section {}: at or below the ground", w.section));
            }
            cyl.push((
                Cylinder {
                    y: w.offset + o[0],
                    z,
                    radius: r,
                    volts: C::new(0.0, 0.0),
                },
                v,
            ));
        }
        ranges.push(start..cyl.len());
    }
    if cyl.is_empty() {
        return Err("no wire".into());
    }
    let mut freqs: Vec<f64> = cyl.iter().filter_map(|c| c.1.map(|v| v.0)).collect();
    freqs.sort_by(f64::total_cmp);
    freqs.dedup();
    if freqs.is_empty() {
        return Err("no phase at an AC voltage".into());
    }
    let vmax = cyl
        .iter()
        .filter_map(|c| c.1.map(|v| v.1.norm()))
        .fold(0.0, f64::max);
    let mut solutions = Vec::new();
    let mut error: f64 = 0.0;
    for &f in &freqs {
        let list: Vec<Cylinder> = cyl
            .iter()
            .map(|(c, v)| Cylinder {
                volts: match v {
                    Some((g, v)) if *g == f => *v,
                    _ => C::new(0.0, 0.0),
                },
                ..*c
            })
            .collect();
        let s = solve(&list, n, true, PLACEMENT)
            .ok_or("the charge simulation is singular (wires overlapping?)")?;
        if vmax > 0.0 {
            error = error.max(s.potential_error() / vmax);
        }
        solutions.push(s);
    }
    // Markt–Mengele: the bundles' charges of the equivalent wires.
    let eq = cross_section_charges(cs, given, &mut BTreeMap::new(), &mut Vec::new());
    let mut out = Vec::new();
    for (k, (w, range)) in cs.wires.iter().zip(ranges).enumerate() {
        let r = 0.5 * w.wire.diameter;
        let subs: Vec<SubconductorGradient> = range
            .clone()
            .map(|j| {
                let (max, angle, mean) = surface_gradient(&solutions, j, points);
                SubconductorGradient {
                    offset: cyl[j].0.y,
                    height: cyl[j].0.z,
                    max_kv_cm: max / 1e5,
                    angle_deg: angle,
                    mean_kv_cm: mean / 1e5,
                }
            })
            .collect();
        let average = subs.iter().map(|s| s.max_kv_cm).sum::<f64>() / subs.len() as f64;
        let max = subs.iter().map(|s| s.max_kv_cm).fold(0.0, f64::max);
        let count = w.bundle.count.max(1);
        let big_r = match w.bundle.spacing {
            Some(s) if count >= 2 && s > 0.0 => s / (2.0 * (PI / f64::from(count)).sin()),
            _ => 0.0,
        };
        let q = eq
            .iter()
            .map(|(_, q)| q.get(k).map_or(0.0, |x| x.norm_sqr()))
            .sum::<f64>()
            .sqrt();
        let (circuit, phase, kv) = match &w.phase {
            Some(p) => (
                crate::parameters::CircuitKey {
                    circuit: p.circuit.clone(),
                    system: p.system.clone(),
                }
                .label(),
                p.label.clone(),
                phase_voltage(p, given, &mut BTreeMap::new()).map_or(0.0, |v| v.1.norm() / 1e3),
            ),
            None => (String::new(), String::new(), 0.0),
        };
        out.push(BundleGradient {
            section: w.section.clone(),
            circuit,
            phase,
            kv,
            count,
            diameter: w.wire.diameter,
            subconductors: subs,
            average_max_kv_cm: average,
            max_kv_cm: max,
            markt_mengele_kv_cm: markt_mengele(q, count, r, big_r) / 1e5,
        });
    }
    Ok((out, error))
}

/// The surface gradients as CSV: one row per subconductor, with its
/// bundle's average maximum, maximum and Markt–Mengele value.
pub fn csv(g: &SurfaceGradients) -> String {
    let mut out = String::from(
        "from,to,station_m,place,section,circuit,phase,kv_to_earth,subconductor,offset_m,height_m,max_kv_cm_rms,angle_deg,mean_kv_cm_rms,bundle_average_max_kv_cm_rms,bundle_max_kv_cm_rms,markt_mengele_kv_cm_rms\n",
    );
    for c in &g.cuts {
        for b in &c.bundles {
            for (k, s) in b.subconductors.iter().enumerate() {
                out.push_str(&format!(
                    "{},{},{:.2},{},{},{},{},{:.3},{},{:.3},{:.3},{:.4},{:.1},{:.4},{:.4},{:.4},{:.4}\n",
                    csv_field(&c.from),
                    csv_field(&c.to),
                    c.at,
                    c.place,
                    csv_field(&b.section),
                    csv_field(&b.circuit),
                    csv_field(&b.phase),
                    b.kv,
                    k + 1,
                    s.offset,
                    s.height,
                    s.max_kv_cm,
                    s.angle_deg,
                    s.mean_kv_cm,
                    b.average_max_kv_cm,
                    b.max_kv_cm,
                    b.markt_mengele_kv_cm,
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact surface field of a cylinder (radius `r`, centre at `h`)
    /// at `v` volts over a conducting plane, at angle `a`: its charge as a
    /// line charge at \\( d = \sqrt{h^2 - r^2} \\) and the image.
    fn exact(h: f64, r: f64, v: f64, a: f64) -> f64 {
        let d = (h * h - r * r).sqrt();
        let q = v / (K2 * ((h + d) / r).ln());
        let p = [r * a.cos(), h + r * a.sin()];
        let ra = p[0].hypot(p[1] - d);
        let rb = p[0].hypot(p[1] + d);
        q * K2 * 2.0 * d / (ra * rb)
    }

    #[test]
    fn a_cylinder_over_the_ground_as_the_closed_form() {
        for &(h, r) in &[(20.0, 0.0159), (2.0, 0.5), (1.2, 1.0)] {
            let v = 100e3;
            let s = solve(
                &[Cylinder {
                    y: 0.0,
                    z: h,
                    radius: r,
                    volts: C::new(v, 0.0),
                }],
                64,
                true,
                PLACEMENT,
            )
            .unwrap();
            for i in 0..72 {
                let a = 2.0 * PI * i as f64 / 72.0;
                let (en, et) = s.surface_field(0, a);
                let e = exact(h, r, v, a);
                assert!(
                    (en.norm() / e - 1.0).abs() < 1e-3,
                    "h {h} r {r} a {a}: {} {e}",
                    en.norm()
                );
                assert!(et.norm() / e < 2e-3);
            }
            let (max, angle, _) = surface_gradient(&[s], 0, 360);
            let e = exact(h, r, v, -PI / 2.0);
            // The closed form's largest value at the bottom.
            let d = (h * h - r * r).sqrt();
            let closed = v * d / (r * (h - r) * ((h + d) / r).ln());
            assert!((e / closed - 1.0).abs() < 1e-12);
            assert!((max / closed - 1.0).abs() < 1e-3, "{max} {closed}");
            assert!((angle + 90.0).abs() < 1e-9);
        }
    }

    #[test]
    fn two_cylinders_at_opposite_voltages_as_the_closed_form() {
        // Free space: the plane midway is at zero, so each cylinder is the
        // one over a plane at half the distance (bipolar coordinates).
        let (s, r, v) = (0.3, 0.1, 1000.0);
        let sol = solve(
            &[
                Cylinder {
                    y: 0.0,
                    z: s,
                    radius: r,
                    volts: C::new(v, 0.0),
                },
                Cylinder {
                    y: 0.0,
                    z: -s,
                    radius: r,
                    volts: C::new(-v, 0.0),
                },
            ],
            64,
            false,
            PLACEMENT,
        )
        .unwrap();
        for i in 0..36 {
            let a = 2.0 * PI * i as f64 / 36.0;
            let e = exact(s, r, v, a);
            let got = sol.surface_field(0, a).0.norm();
            assert!((got / e - 1.0).abs() < 1e-3, "{got} {e}");
        }
        assert!((sol.charge(0) + sol.charge(1)).norm() < 1e-9 * sol.charge(0).norm());
    }

    #[test]
    fn converges_with_the_charges() {
        let c = [Cylinder {
            y: 0.0,
            z: 1.5,
            radius: 1.0,
            volts: C::new(1.0, 0.0),
        }];
        let closed = exact(1.5, 1.0, 1.0, -PI / 2.0);
        let mut last = f64::INFINITY;
        for n in [4, 8, 16, 32, 64] {
            let (m, _, _) = surface_gradient(&[solve(&c, n, true, PLACEMENT).unwrap()], 0, 360);
            let err = (m / closed - 1.0).abs();
            assert!(err <= last * 1.0001, "n {n}: {err} after {last}");
            last = err;
        }
        assert!(last < 1e-6);
    }

    #[test]
    fn earth_wire_at_zero_and_symmetric_bundle() {
        // A 2-bundle under an earth wire, symmetric about the vertical.
        let r = 0.0159;
        let v = C::new(220e3, 0.0);
        let cyl = [
            Cylinder {
                y: -0.2,
                z: 20.0,
                radius: r,
                volts: v,
            },
            Cylinder {
                y: 0.2,
                z: 20.0,
                radius: r,
                volts: v,
            },
            Cylinder {
                y: 0.0,
                z: 30.0,
                radius: 0.008,
                volts: C::new(0.0, 0.0),
            },
        ];
        let s = solve(&cyl, 16, true, PLACEMENT).unwrap();
        // The earth wire's surface at zero potential.
        for i in 0..16 {
            let a = 2.0 * PI * (i as f64 + 0.5) / 16.0;
            let p = round(&cyl[2], 0.008, a);
            assert!(s.potential(p).norm() < 1e-4 * v.norm());
        }
        // Its charge opposes the phases'.
        assert!(s.charge(2).re < 0.0);
        // Mirror symmetry: the two subconductors alike.
        let (m0, a0, _) = surface_gradient(std::slice::from_ref(&s), 0, 360);
        let (m1, a1, _) = surface_gradient(std::slice::from_ref(&s), 1, 360);
        assert!((m0 / m1 - 1.0).abs() < 1e-9);
        // Their largest gradients away from each other: the left one's
        // to the left, the right one's to the right (mirror angles).
        assert!(a0.abs() > 90.0 && a1.abs() < 90.0, "{a0} {a1}");
        assert!((a0.abs() + a1.abs() - 180.0).abs() < 1.0 + 1e-9);
        // The average maximum above the mean (the bundle effect) and near
        // Markt–Mengele's.
        let (_, _, mean) = surface_gradient(std::slice::from_ref(&s), 0, 360);
        assert!(m0 > mean);
        let q = (s.charge(0) + s.charge(1)).norm();
        let mm = markt_mengele(q, 2, r, 0.2);
        assert!((m0 / mm - 1.0).abs() < 0.01, "{m0} {mm}");
    }

    #[test]
    fn proportional_to_the_voltage() {
        let mk = |v: f64| {
            let c = [Cylinder {
                y: 0.0,
                z: 10.0,
                radius: 0.02,
                volts: C::new(v, 0.0),
            }];
            surface_gradient(&[solve(&c, 16, true, PLACEMENT).unwrap()], 0, 360).0
        };
        assert!((mk(2.0) / mk(1.0) - 2.0).abs() < 1e-12);
    }

    /// The convergence study of the theory page: the benchmark
    /// cross-sections' (and a 4-bundle's) average maximum gradients
    /// against 64 charges, and the closed forms, per placement.
    /// `cargo test -p ohl-electrical --release convergence_table -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn convergence_table() {
        let three = |kv: f64, places: &[(f64, f64)], n_b: usize, a: f64, d: f64| {
            let mut out = Vec::new();
            let rb = if n_b > 1 {
                a / 2.0 / (PI / n_b as f64).sin()
            } else {
                0.0
            };
            // As hvlbuzz places them: the first at the bundle angle (0).
            for (k, &(x, y)) in places.iter().enumerate() {
                let ang = -2.0 * PI * (k % 3) as f64 / 3.0;
                for i in 0..n_b {
                    let b =
                        2.0 * PI * i as f64 / n_b as f64 + if n_b == 4 { PI / 4.0 } else { 0.0 };
                    out.push(Cylinder {
                        y: x + rb * b.cos(),
                        z: y + rb * b.sin(),
                        radius: d / 2.0,
                        volts: C::from_polar(kv * 1e3 / 3f64.sqrt(), ang),
                    });
                }
            }
            out
        };
        let a = three(
            380.0,
            &[(-9.0, 20.0), (0.0, 20.0), (9.0, 20.0)],
            2,
            0.4,
            0.0318,
        );
        let b = three(
            110.0,
            &[
                (5.0, 22.0),
                (5.0, 27.0),
                (5.0, 32.0),
                (-5.0, 22.0),
                (-5.0, 27.0),
                (-5.0, 32.0),
            ],
            1,
            0.0,
            0.0281,
        );
        let ew = |y: f64, z: f64| Cylinder {
            y,
            z,
            radius: 0.008,
            volts: C::new(0.0, 0.0),
        };
        let mut c = b.clone();
        c.push(ew(0.0, 38.0));
        let mut d = three(
            420.0,
            &[(-12.0, 18.0), (0.0, 18.0), (12.0, 18.0)],
            4,
            0.45,
            0.0318,
        );
        d.push(ew(-8.0, 30.0));
        d.push(ew(8.0, 30.0));
        let ns = [1usize, 2, 4, 6, 8, 12, 16, 24, 32, 48, 64];
        for placement in [0.0, 0.3, 0.5] {
            println!("\nplacement {placement} r (0: r/k, k = 1 + 2 sin(pi/n))");
            let mut rows: Vec<String> = ns.iter().map(|n| format!("| {n} |")).collect();
            for (h, r) in [(20.0f64, 0.0159f64), (2.0, 0.5), (0.3, 0.1), (1.2, 1.0)] {
                let dd = (h * h - r * r).sqrt();
                let closed = dd / (r * (h - r) * ((h + dd) / r).ln());
                for (row, &n) in rows.iter_mut().zip(&ns) {
                    let c = [Cylinder {
                        y: 0.0,
                        z: h,
                        radius: r,
                        volts: C::new(1.0, 0.0),
                    }];
                    let s = solve(&c, n, true, placement).unwrap();
                    let (m, _, _) = surface_gradient(std::slice::from_ref(&s), 0, 360);
                    row.push_str(&format!(" {:.1e} |", m / closed - 1.0));
                }
            }
            for (cyl, nb) in [(&a, 2usize), (&b, 1), (&c, 1), (&d, 4)] {
                let phases =
                    (cyl.len() - cyl.iter().filter(|c| c.volts.norm() == 0.0).count()) / nb;
                let avg = |n: usize| -> (Vec<f64>, f64, f64) {
                    let t = std::time::Instant::now();
                    let s = solve(cyl, n, true, placement).unwrap();
                    let v = (0..phases)
                        .map(|k| {
                            (0..nb)
                                .map(|j| {
                                    surface_gradient(std::slice::from_ref(&s), k * nb + j, 360).0
                                })
                                .sum::<f64>()
                                / nb as f64
                        })
                        .collect();
                    let vmax = cyl.iter().map(|c| c.volts.norm()).fold(0.0, f64::max);
                    (v, s.potential_error() / vmax, t.elapsed().as_secs_f64())
                };
                let reference = avg(64).0;
                for (row, &n) in rows.iter_mut().zip(&ns) {
                    let (v, err, dt) = avg(n);
                    let worst = v
                        .iter()
                        .zip(&reference)
                        .map(|(a, b)| (a / b - 1.0).abs())
                        .fold(0.0, f64::max);
                    row.push_str(&format!(
                        " {:.4} % ({:.0e}; {:.3} s) |",
                        worst * 100.0,
                        err,
                        dt
                    ));
                }
                println!("first phase at 64: {:.5} kV/cm", reference[0] / 1e5);
            }
            for r in rows {
                println!("{r}");
            }
        }
    }
}
