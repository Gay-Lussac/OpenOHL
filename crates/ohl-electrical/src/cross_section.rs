// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Cross-sections of the line: every conductor and earth wire of the line
//! in the plane across it, at a station or as a span's mean — the one
//! record the electrical calculations share (line parameters now; the
//! load flow, the fields and the noise, and the plugins later).
//!
//! A cross-section belongs to a **span of the line**: two structures
//! next to each other along the line, with every wire spanning between
//! them. Its axis runs in plan from the first structure (along the line)
//! to the second; across the line is measured to the right looking ahead
//! (the program's convention for offsets), heights above the ground under
//! each wire.
//!
//! Where the wires are: from the sections solved in a weather case
//! (`SectionShape`s of [`ohl_line::solve`]) —
//!
//! - **at a station** ([`cross_section_at`]): each wire where its solved
//!   polyline crosses the plane square to the axis at that place;
//! - **as the span's mean** ([`span_cross_sections`], [`MeanHeight`]):
//!   [`MeanHeight::Solved`] the exact average of the solved wire's
//!   offset and height along the axis (trapezoids over the solved
//!   points); [`MeanHeight::TwoThirdsSag`] the textbook approximation,
//!   the mean of its two attachment heights minus two thirds of its sag
//!   (exact for a parabola over level ground), its offset the mean of its
//!   attachments'.
//!
//! The ground: a terrain when given, else straight between the two
//! structures' feet (their positions) and level across the line — a
//! **simplifying assumption** where no terrain is loaded.

use ohl_core::units::Temperature;
use ohl_line::electrical::{MeanHeight, SystemKind};
use ohl_line::solve::SectionShape;
use ohl_line::{Line, Point};
use serde::Serialize;

/// A cross-section of the line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossSection {
    /// Where it is taken.
    pub station: Station,
    /// The ground there.
    pub ground: Ground,
    /// The line's conductors and earth wires there, in the order of the
    /// line's sections.
    pub wires: Vec<CrossWire>,
    /// Other lines' wires in the same corridor — **an extension point,
    /// empty today** (roadmap O18: parallel lines, crossings); the
    /// calculations take every wire listed here the same way as the
    /// line's own.
    pub other_lines: Vec<OtherLineWire>,
    /// What was assumed or is missing (said with the results).
    pub notes: Vec<String>,
}

/// Where a cross-section is taken.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Station {
    /// The structure the span starts from (the first along the line).
    pub from: String,
    /// The structure it ends at.
    pub to: String,
    /// The span's horizontal length between the two structures, m.
    pub span_length: f64,
    /// At a place in the span, or its mean.
    pub at: StationAt,
}

/// The place of a cross-section in its span.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StationAt {
    /// The span's mean, by a [`MeanHeight`] rule.
    Mean(MeanHeight),
    /// At this fraction of the span from `from` (0–1), m along the axis
    /// being `fraction × span_length`.
    Fraction(f64),
}

/// The ground under a cross-section.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Ground {
    /// Its elevation on the axis, m (at the station; the span's middle for
    /// a mean).
    pub elevation: f64,
    /// Its slope across the line, rise per metre to the right (0: level,
    /// or no terrain).
    pub slope_across: f64,
    /// Soil resistivity, Ω·m.
    pub soil_resistivity_ohm_m: f64,
    /// The resistivity is the assumed default
    /// ([`ohl_line::electrical::DEFAULT_SOIL_RESISTIVITY_OHM_M`]): the
    /// line gives none.
    pub soil_resistivity_assumed: bool,
}

/// A wire (a bundle: its centre) in a cross-section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrossWire {
    /// Its section.
    pub section: String,
    /// The span in its section (0 = the section's first).
    pub span: usize,
    /// Across the line from the axis, m, to the right looking ahead.
    pub offset: f64,
    /// Height above the ground under it, m.
    pub height: f64,
    /// Elevation, m.
    pub elevation: f64,
    /// Its bundle.
    pub bundle: BundleData,
    /// Its wire type's data.
    pub wire: WireData,
    /// Its system and phase; `None`: earthed (earth wire, OPGW).
    pub phase: Option<PhaseData>,
}

/// A bundle in a cross-section.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BundleData {
    /// Subconductors (1: a single wire).
    pub count: u32,
    /// Distance between neighbouring subconductors, m.
    pub spacing: Option<f64>,
    /// Rotation of the arrangement, degrees (see `ohl_line::Bundle`).
    pub rotation_deg: f64,
}

/// A subconductor's wire data in a cross-section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WireData {
    /// Wire type id.
    pub cable: String,
    /// Diameter, m.
    pub diameter: f64,
    /// Geometric mean radius, m; `None`: not in the wire library.
    pub gmr: Option<f64>,
    /// AC resistance at `temperature`, Ω/m; `None`: not in the wire
    /// library (nor a DC resistance, taken instead when it is the only one).
    pub resistance: Option<f64>,
    /// The resistance is the DC one (no AC resistance given).
    pub resistance_is_dc: bool,
    /// The conductor temperature of the resistance (the weather case's), °C.
    pub temperature_c: f64,
}

/// A phase conductor's system in a cross-section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhaseData {
    /// System id.
    pub system: String,
    /// The phase's place in its system (0 = its first).
    pub index: usize,
    /// Phase (pole) label.
    pub label: String,
    /// The circuit (its name; empty: none).
    pub circuit: String,
    /// AC or DC.
    pub kind: SystemKind,
    /// The system's nominal voltage, kV (AC between phases, DC to earth).
    pub voltage_kv: f64,
    /// AC frequency, Hz, when the system gives one.
    pub frequency_hz: Option<f64>,
    /// AC phase angle, degrees (DC: 0).
    pub angle_deg: f64,
}

/// A wire of another line in the corridor (extension point; none today).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OtherLineWire {
    /// The other line's name.
    pub line: String,
    /// The wire, placed in this cross-section's axes.
    pub wire: CrossWire,
}

/// What a cross-section is made from.
pub struct Sources<'a> {
    /// The line (structure types resolved).
    pub line: &'a Line,
    /// Its wire library.
    pub library: &'a ohl_cables::Library,
    /// Its sections solved in one weather case (any order; a section
    /// missing is left out, said).
    pub shapes: &'a [SectionShape],
    /// The conductor temperature for the resistances: the weather case's.
    pub temperature: Temperature,
    /// The ground's elevation at a plan point, when a terrain is given.
    pub ground: Option<&'a dyn Fn(f64, f64) -> Option<f64>>,
    /// Its jumpers solved in the same weather case (the magnetic field's
    /// current paths; none: left out).
    pub jumpers: &'a [ohl_line::jumper::JumperShape],
}

/// A span of the line: two structures and the wires between them.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSpan {
    /// The first structure along the line.
    pub from: String,
    /// The second.
    pub to: String,
    /// Every wire spanning between them: (section index, span index).
    pub wires: Vec<(usize, usize)>,
}

/// The spans of the line, in order along it (the first structure's place
/// along the line, then its id).
pub fn line_spans(line: &Line) -> Vec<LineSpan> {
    let place = |id: &str| {
        line.structure(id)
            .and_then(|s| line.place_along([s.position[0], s.position[1]]))
            .unwrap_or((usize::MAX, f64::INFINITY))
    };
    // Off the alignments (both structures unplaced): the way most of the
    // sections between the two structures are drawn (2026-10-29: a section
    // drawn backwards no longer makes a span of its own), ties by the
    // first drawn.
    let mut drawn: std::collections::BTreeMap<(String, String), i64> = Default::default();
    for s in &line.sections {
        for w in s.points.windows(2) {
            let (a, b) = (&w[0].structure, &w[1].structure);
            if a != b {
                let (lo, hi, d) = if a < b { (a, b, 1) } else { (b, a, -1) };
                *drawn.entry((lo.clone(), hi.clone())).or_insert(0) += d;
            }
        }
    }
    let first: std::collections::BTreeMap<(String, String), bool> = {
        let mut m = std::collections::BTreeMap::new();
        for s in &line.sections {
            for w in s.points.windows(2) {
                let (a, b) = (&w[0].structure, &w[1].structure);
                if a != b {
                    let key = if a < b {
                        (a.clone(), b.clone())
                    } else {
                        (b.clone(), a.clone())
                    };
                    m.entry(key).or_insert(a < b);
                }
            }
        }
        m
    };
    let mut spans: Vec<LineSpan> = Vec::new();
    let mut index: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for (k, s) in line.sections.iter().enumerate() {
        for (j, w) in s.points.windows(2).enumerate() {
            let (a, b) = (&w[0].structure, &w[1].structure);
            if a == b {
                continue;
            }
            let (pa, pb) = (place(a), place(b));
            let ahead = if pa.0 == usize::MAX && pb.0 == usize::MAX {
                // Low to high id when most are drawn so.
                let key = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                let n = drawn.get(&key).copied().unwrap_or(0);
                let low_first = n > 0 || (n == 0 && first.get(&key).copied().unwrap_or(true));
                (a < b) == low_first
            } else {
                pa.0 < pb.0 || (pa.0 == pb.0 && pa.1 <= pb.1)
            };
            let (from, to) = if ahead { (a, b) } else { (b, a) };
            let key = (from.clone(), to.clone());
            let at = *index.entry(key).or_insert_with(|| {
                spans.push(LineSpan {
                    from: from.clone(),
                    to: to.clone(),
                    wires: Vec::new(),
                });
                spans.len() - 1
            });
            spans[at].wires.push((k, j));
        }
    }
    spans.sort_by(|x, y| {
        let (a, b) = (place(&x.from), place(&y.from));
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(x.from.cmp(&y.from))
            .then(x.to.cmp(&y.to))
    });
    spans
}

/// The axis of a span: start, unit direction, unit right, length (plan).
pub(crate) struct Axis {
    pub(crate) a: [f64; 3],
    pub(crate) b: [f64; 3],
    pub(crate) u: [f64; 2],
    pub(crate) right: [f64; 2],
    pub(crate) length: f64,
}

impl Axis {
    pub(crate) fn of(line: &Line, span: &LineSpan) -> Result<Self, String> {
        let get = |id: &str| {
            line.structure(id)
                .map(|s| s.position)
                .ok_or_else(|| format!("structure {id} not in the line"))
        };
        let (a, b) = (get(&span.from)?, get(&span.to)?);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length < 1e-6 {
            return Err(format!(
                "span {}–{}: the structures stand at one place",
                span.from, span.to
            ));
        }
        let u = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
        Ok(Self {
            a,
            b,
            u,
            right: [u[1], -u[0]],
            length,
        })
    }

    /// Along the axis and to its right, m.
    pub(crate) fn local(&self, p: &Point) -> (f64, f64) {
        let d = [p[0] - self.a[0], p[1] - self.a[1]];
        (
            d[0] * self.u[0] + d[1] * self.u[1],
            d[0] * self.right[0] + d[1] * self.right[1],
        )
    }

    /// The plan point `t` along the axis and `s` to its right.
    pub(crate) fn plan(&self, t: f64, s: f64) -> [f64; 2] {
        [
            self.a[0] + t * self.u[0] + s * self.right[0],
            self.a[1] + t * self.u[1] + s * self.right[1],
        ]
    }

    /// Ground elevation at `t` along the axis and `s` to its right: the
    /// terrain's, else straight between the feet and level across.
    pub(crate) fn ground(
        &self,
        ground: Option<&dyn Fn(f64, f64) -> Option<f64>>,
        t: f64,
        s: f64,
    ) -> f64 {
        let p = self.plan(t, s);
        ground.and_then(|g| g(p[0], p[1])).unwrap_or_else(|| {
            let f = (t / self.length).clamp(0.0, 1.0);
            self.a[2] + (self.b[2] - self.a[2]) * f
        })
    }
}

/// Every span of the line as its mean cross-section by `rule`, in order
/// along the line; spans that cannot be made are left out with a note.
pub fn span_cross_sections(
    src: &Sources<'_>,
    rule: MeanHeight,
) -> (Vec<(LineSpan, CrossSection)>, Vec<String>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for span in line_spans(src.line) {
        match cross_section(src, &span, StationAt::Mean(rule)) {
            Ok(c) => out.push((span, c)),
            Err(e) => notes.push(e),
        }
    }
    (out, notes)
}

/// The cross-section of the span between structures `from` and `to` (in
/// either order) at `fraction` of it from the first along the line.
pub fn cross_section_at(
    src: &Sources<'_>,
    from: &str,
    to: &str,
    fraction: f64,
) -> Result<CrossSection, String> {
    if !(0.0..=1.0).contains(&fraction) {
        return Err(format!("a place in the span from 0 to 1, not {fraction}"));
    }
    let span = line_spans(src.line)
        .into_iter()
        .find(|s| (s.from == from && s.to == to) || (s.from == to && s.to == from))
        .ok_or_else(|| format!("no span between structures {from} and {to}"))?;
    cross_section(src, &span, StationAt::Fraction(fraction))
}

/// The cross-section of `span` at `at`.
pub fn cross_section(
    src: &Sources<'_>,
    span: &LineSpan,
    at: StationAt,
) -> Result<CrossSection, String> {
    let line = src.line;
    let axis = Axis::of(line, span)?;
    let phases = line.section_phases().map_err(|e| e.to_string())?;
    let (rho, assumed) = line.span_soil_resistivity(&span.from, &span.to);
    let mut notes = Vec::new();
    let t_station = match at {
        StationAt::Fraction(f) => f * axis.length,
        StationAt::Mean(_) => 0.5 * axis.length,
    };
    let g0 = axis.ground(src.ground, t_station, 0.0);
    let slope_across = match src.ground {
        Some(_) => {
            let (l, r) = (
                axis.ground(src.ground, t_station, -5.0),
                axis.ground(src.ground, t_station, 5.0),
            );
            (r - l) / 10.0
        }
        None => 0.0,
    };
    let mut wires = Vec::new();
    for &(k, j) in &span.wires {
        let section = &line.sections[k];
        let Some(shape) = src.shapes.iter().find(|s| s.section == section.id) else {
            notes.push(format!("section {}: not solved, left out", section.id));
            continue;
        };
        let Some(sp) = shape.spans.get(j).filter(|s| s.points.len() >= 2) else {
            notes.push(format!(
                "section {} span {}: no wire (broken), left out",
                section.id,
                j + 1
            ));
            continue;
        };
        let Some(cable) = src.library.get(&section.cable) else {
            notes.push(format!(
                "section {}: wire type {} not in the wire library, left out",
                section.id, section.cable
            ));
            continue;
        };
        // Along, offset, height above the ground under it, elevation.
        let samples: Vec<(f64, f64, f64, f64)> = sp
            .points
            .iter()
            .map(|p| {
                let (t, s) = axis.local(p);
                (t, s, p[2] - axis.ground(src.ground, t, s), p[2])
            })
            .collect();
        let (offset, height, elevation) = match at {
            StationAt::Mean(MeanHeight::Solved) => mean_along(&samples),
            StationAt::Mean(MeanHeight::TwoThirdsSag) => {
                let (a, b) = (samples[0], samples[samples.len() - 1]);
                let drop = 2.0 / 3.0 * sp.sag;
                (
                    0.5 * (a.1 + b.1),
                    0.5 * (a.2 + b.2) - drop,
                    0.5 * (a.3 + b.3) - drop,
                )
            }
            StationAt::Fraction(_) => at_along(&samples, t_station).ok_or_else(|| {
                format!(
                    "section {} span {}: the wire does not reach the station",
                    section.id,
                    j + 1
                )
            })?,
        };
        let electrical = cable.electrical.as_ref();
        let temperature_c = src.temperature.celsius();
        let ac = electrical.and_then(|e| e.ac_resistance_at(src.temperature, cable.kind));
        let dc = electrical.and_then(|e| e.dc_resistance.as_ref());
        let (resistance, resistance_is_dc) = match (ac, dc) {
            (Some(r), _) => (Some(r.si()), false),
            (None, Some(d)) => (Some(d.resistance.si()), true),
            (None, None) => (None, false),
        };
        let phase = phases
            .get(&section.id)
            .copied()
            .flatten()
            .map(|at| phase_data(line, section, at));
        wires.push(CrossWire {
            section: section.id.clone(),
            span: j,
            offset,
            height,
            elevation,
            bundle: BundleData {
                count: section.bundle.max(1),
                spacing: section.bundle_spacing,
                rotation_deg: section.bundle_rotation_deg.unwrap_or(0.0),
            },
            wire: WireData {
                cable: cable.id.clone(),
                diameter: cable.diameter.si(),
                gmr: electrical
                    .and_then(|e| e.geometric_mean_radius)
                    .map(|g| g.si()),
                resistance,
                resistance_is_dc,
                temperature_c,
            },
            phase,
        });
    }
    if src.ground.is_none() {
        notes.push(
            "ground: straight between the structures' feet and level across the line (no terrain)"
                .into(),
        );
    }
    Ok(CrossSection {
        station: Station {
            from: span.from.clone(),
            to: span.to.clone(),
            span_length: axis.length,
            at,
        },
        ground: Ground {
            elevation: g0,
            slope_across,
            soil_resistivity_ohm_m: rho,
            soil_resistivity_assumed: assumed,
        },
        wires,
        other_lines: Vec::new(),
        notes,
    })
}

/// The system data of a phase conductor of `section`: (system index,
/// phase index) in `line` as [`Line::section_phases`] gives it. AC phase
/// angles not given are spread evenly from 0° in the phases' order
/// (L1 0°, L2 −120°, L3 +120° for three).
pub(crate) fn phase_data(
    line: &Line,
    section: &ohl_line::Section,
    (ks, kp): (usize, usize),
) -> PhaseData {
    let sys = &line.systems[ks];
    let n = sys.phases.len().max(1) as f64;
    PhaseData {
        system: sys.id.clone(),
        index: kp,
        label: sys.phases[kp].label.clone(),
        circuit: section.circuit_name.clone(),
        kind: sys.kind,
        voltage_kv: sys.voltage_kv,
        frequency_hz: sys.frequency_hz,
        angle_deg: match sys.kind {
            SystemKind::Ac => sys.phases[kp].angle_deg.unwrap_or(-(kp as f64) * 360.0 / n),
            SystemKind::Dc => 0.0,
        },
    }
}

/// The mean offset, height and elevation of samples (along, offset,
/// height, elevation) over their extent along the axis (trapezoids); the
/// plain mean when they do not extend along it.
fn mean_along(samples: &[(f64, f64, f64, f64)]) -> (f64, f64, f64) {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let extent = sorted[sorted.len() - 1].0 - sorted[0].0;
    if extent < 1e-9 {
        let n = sorted.len() as f64;
        let sum = sorted
            .iter()
            .fold((0.0, 0.0, 0.0), |s, p| (s.0 + p.1, s.1 + p.2, s.2 + p.3));
        return (sum.0 / n, sum.1 / n, sum.2 / n);
    }
    let mut acc = (0.0, 0.0, 0.0);
    for w in sorted.windows(2) {
        let dt = w[1].0 - w[0].0;
        acc.0 += 0.5 * (w[0].1 + w[1].1) * dt;
        acc.1 += 0.5 * (w[0].2 + w[1].2) * dt;
        acc.2 += 0.5 * (w[0].3 + w[1].3) * dt;
    }
    (acc.0 / extent, acc.1 / extent, acc.2 / extent)
}

/// Offset, height and elevation where the samples pass `t` along the
/// axis (linear between the solved points).
fn at_along(samples: &[(f64, f64, f64, f64)], t: f64) -> Option<(f64, f64, f64)> {
    samples.windows(2).find_map(|w| {
        let (a, b) = (w[0], w[1]);
        let (lo, hi) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
        if t < lo - 1e-9 || t > hi + 1e-9 {
            return None;
        }
        let f = if (b.0 - a.0).abs() < 1e-12 {
            0.0
        } else {
            (t - a.0) / (b.0 - a.0)
        };
        Some((
            a.1 + (b.1 - a.1) * f,
            a.2 + (b.2 - a.2) * f,
            a.3 + (b.3 - a.3) * f,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A parabola \\( h(t) = H - 4f\,t(L-t)/L^2 \\) over level ground has
    /// the mean height \\( H - \tfrac23 f \\) (its integral by hand); the
    /// trapezoids over 201 points come within \\( f/(3\cdot 200^2) \\).
    #[test]
    fn mean_of_a_parabola_is_two_thirds_of_its_sag_down() {
        let (big_h, f, l) = (30.0, 12.0, 400.0);
        let samples: Vec<_> = (0..=200)
            .map(|k| {
                let t = l * f64::from(k) / 200.0;
                let h = big_h - 4.0 * f * t * (l - t) / (l * l);
                (t, 1.5, h, h + 500.0)
            })
            .collect();
        let (o, h, e) = mean_along(&samples);
        assert!((o - 1.5).abs() < 1e-12);
        assert!((h - (big_h - 2.0 / 3.0 * f)).abs() < 1e-3, "{h}");
        assert!((e - h - 500.0).abs() < 1e-9);
        let mid = at_along(&samples, 200.0).unwrap();
        assert!((mid.1 - (big_h - f)).abs() < 1e-9);
    }
}
