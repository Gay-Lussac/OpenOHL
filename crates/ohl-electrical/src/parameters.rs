// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line parameters from cross-sections: per span the phase matrices per
//! unit length (series impedance with earth return, shunt capacitance,
//! earth wires reduced out), and along the line per circuit — its
//! stretches of the same sections, its total, its sequence values — and
//! the coupling between circuits sharing spans.
//!
//! Along the line the phase matrices are summed span by span, each times
//! its span's horizontal length, indexed by the phases (not by the
//! wires' places): a transposition along the line is in the totals as
//! built. Series impedances add (the spans are in series), and so do the
//! capacitances (in parallel along the line).

use std::collections::BTreeMap;
use std::f64::consts::PI;

use nalgebra::{Complex, DMatrix};
use ohl_line::electrical::{EarthReturn, SystemKind};

use crate::cross_section::{CrossSection, CrossWire, LineSpan};
use crate::matrices::{self, CMatrix, Wire};

/// The frequency taken for a system that gives none, Hz (said with the
/// results).
pub const DEFAULT_FREQUENCY_HZ: f64 = 50.0;

/// A phase conductor's place in the results: circuit (name, or empty),
/// system id, the phase's index in its system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhaseKey {
    /// The circuit's name (empty: none).
    pub circuit: String,
    /// The system's id.
    pub system: String,
    /// The phase's place in its system.
    pub index: usize,
}

/// A circuit as the results group it: the phases of one system in one
/// named circuit (or in none).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CircuitKey {
    /// The circuit's name (empty: none).
    pub circuit: String,
    /// The system's id.
    pub system: String,
}

impl CircuitKey {
    /// For tables: the circuit's name, else the system's id.
    pub fn label(&self) -> String {
        if self.circuit.is_empty() {
            format!("system {}", self.system)
        } else {
            self.circuit.clone()
        }
    }
}

/// The phase matrices of one span per unit length at one frequency.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanMatrices {
    /// The frequency, Hz.
    pub frequency_hz: f64,
    /// The phases, in the matrices' order.
    pub phases: Vec<PhaseKey>,
    /// Their sections (in the same order).
    pub sections: Vec<String>,
    /// Series impedance, Ω/m, the earth wires reduced out.
    pub z: CMatrix,
    /// Shunt capacitance (Maxwell's coefficients), F/m, the earth wires
    /// reduced out.
    pub c: DMatrix<f64>,
}

/// The equivalent wire of a cross-section's wire, or why there is none.
fn wire_of(w: &CrossWire, notes: &mut Vec<String>) -> Option<Wire> {
    let radius = 0.5 * w.wire.diameter;
    let gmr = match w.wire.gmr {
        Some(g) => g,
        None => {
            notes.push(format!(
                "wire type {}: no geometric mean radius in the wire library; e^(-1/4) × radius taken (a solid round conductor — an assumption, give it)",
                w.wire.cable
            ));
            (-0.25f64).exp() * radius
        }
    };
    let Some(r) = w.wire.resistance else {
        notes.push(format!(
            "wire type {}: no resistance in the wire library; section {} left out",
            w.wire.cable, w.section
        ));
        return None;
    };
    if w.wire.resistance_is_dc {
        notes.push(format!(
            "wire type {}: no AC resistance; the DC resistance taken",
            w.wire.cable
        ));
    }
    if w.height <= radius {
        notes.push(format!(
            "section {} span {}: at or below the ground ({:.2} m), left out",
            w.section,
            w.span + 1,
            w.height
        ));
        return None;
    }
    let n = w.bundle.count.max(1);
    let (gmr, radius) = matrices::bundle(n, w.bundle.spacing, gmr, radius);
    Some(Wire {
        across: w.offset,
        height: w.height,
        gmr,
        radius,
        resistance: r / f64::from(n),
    })
}

/// The phase matrices of a cross-section, one per AC frequency of its
/// systems (a system without a frequency: [`DEFAULT_FREQUENCY_HZ`]):
/// every wire in the primitive matrices, the earthed ones reduced out
/// (Kron), the phases of that frequency kept. DC poles are left out
/// (said). Notes collect what was assumed or left out.
pub fn span_matrices(
    cs: &CrossSection,
    method: EarthReturn,
    notes: &mut Vec<String>,
) -> Vec<SpanMatrices> {
    let rho = cs.ground.soil_resistivity_ohm_m;
    let mut wires = Vec::new();
    let mut keys: Vec<Option<(PhaseKey, f64)>> = Vec::new();
    let mut sections = Vec::new();
    for w in cs
        .wires
        .iter()
        .chain(cs.other_lines.iter().map(|o| &o.wire))
    {
        if let Some(p) = &w.phase
            && p.kind == SystemKind::Dc
        {
            notes.push(format!(
                "system {}: DC, no AC line parameters (section {} left out)",
                p.system, w.section
            ));
            continue;
        }
        let Some(wire) = wire_of(w, notes) else {
            continue;
        };
        let key = w.phase.as_ref().map(|p| {
            let f = p.frequency_hz.unwrap_or_else(|| {
                notes.push(format!(
                    "system {}: no frequency given; {DEFAULT_FREQUENCY_HZ} Hz taken",
                    p.system
                ));
                DEFAULT_FREQUENCY_HZ
            });
            (
                PhaseKey {
                    circuit: p.circuit.clone(),
                    system: p.system.clone(),
                    index: p.index,
                },
                f,
            )
        });
        if let Some((k, _)) = &key
            && keys.iter().flatten().any(|(x, _)| x == k)
        {
            notes.push(format!(
                "span {}–{}: two wires of {} phase {} (section {} left out: parallel wires of one phase are not modelled)",
                cs.station.from,
                cs.station.to,
                k.system,
                k.index + 1,
                w.section
            ));
            continue;
        }
        wires.push(wire);
        keys.push(key);
        sections.push(w.section.clone());
    }
    let mut frequencies: Vec<f64> = keys.iter().flatten().map(|(_, f)| *f).collect();
    frequencies.sort_by(f64::total_cmp);
    frequencies.dedup();
    let p_all = matrices::complex(&matrices::potential(&wires));
    let mut out = Vec::new();
    for f in frequencies {
        let keep: Vec<usize> = (0..wires.len())
            .filter(|&i| keys[i].as_ref().is_some_and(|(_, g)| *g == f))
            .collect();
        // The other frequencies' phases stay in the matrices (carrying
        // their own currents); earthed wires are reduced out.
        let phases_all: Vec<usize> = (0..wires.len()).filter(|&i| keys[i].is_some()).collect();
        let z_prim = matrices::impedance(&wires, method, f, rho);
        let (Some(z), Some(p)) = (
            matrices::kron(&z_prim, &phases_all),
            matrices::kron(&p_all, &phases_all),
        ) else {
            notes.push(format!(
                "span {}–{}: the earth wires' matrix is singular; left out",
                cs.station.from, cs.station.to
            ));
            continue;
        };
        let Some(c_all) = p.map(|x| x.re).try_inverse() else {
            notes.push(format!(
                "span {}–{}: the potential coefficients are singular; left out",
                cs.station.from, cs.station.to
            ));
            continue;
        };
        let pos: Vec<usize> = keep
            .iter()
            .map(|i| phases_all.iter().position(|j| j == i).expect("a phase"))
            .collect();
        let n = pos.len();
        out.push(SpanMatrices {
            frequency_hz: f,
            phases: keep
                .iter()
                .map(|&i| keys[i].as_ref().expect("a phase").0.clone())
                .collect(),
            sections: keep.iter().map(|&i| sections[i].clone()).collect(),
            z: CMatrix::from_fn(n, n, |i, j| z[(pos[i], pos[j])]),
            c: DMatrix::from_fn(n, n, |i, j| c_all[(pos[i], pos[j])]),
        });
    }
    out
}

/// Sums of a circuit's matrices over a length of line.
#[derive(Debug, Clone, PartialEq)]
pub struct Totals {
    /// Horizontal length of line, m.
    pub length: f64,
    /// Series impedance, Ω (the phase matrix times the length, summed).
    pub z: CMatrix,
    /// Capacitance, F.
    pub c: DMatrix<f64>,
}

impl Totals {
    fn zero(n: usize) -> Self {
        Self {
            length: 0.0,
            z: CMatrix::zeros(n, n),
            c: DMatrix::zeros(n, n),
        }
    }

    fn add(&mut self, other: &Totals) {
        self.length += other.length;
        self.z += &other.z;
        self.c += &other.c;
    }

    /// Per kilometre: (Ω/km, F/km).
    pub fn per_km(&self) -> (CMatrix, DMatrix<f64>) {
        let k = if self.length > 0.0 {
            1000.0 / self.length
        } else {
            0.0
        };
        (self.z.map(|x| x * k), self.c.map(|x| x * k))
    }

    /// The sequence values of a three-phase circuit (`None` otherwise),
    /// its phases taken in the order `abc` (indices into the matrices:
    /// a, then b lagging it by 120°, then c; [`CircuitParameters::abc`]).
    pub fn sequence(&self, frequency_hz: f64, abc: &[usize]) -> Option<Sequence> {
        (self.z.nrows() == 3 && abc.len() == 3).then(|| {
            let z = CMatrix::from_fn(3, 3, |i, j| self.z[(abc[i], abc[j])]);
            let c = DMatrix::from_fn(3, 3, |i, j| self.c[(abc[i], abc[j])]);
            let z012 = matrices::sequence(&z);
            let c012 = matrices::sequence(&matrices::complex(&c));
            let (z1t, z0t) = matrices::transposed(&z);
            let (c1t, c0t) = matrices::transposed(&matrices::complex(&c));
            let off = [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)]
                .iter()
                .map(|&ij| z012[ij].norm())
                .fold(0.0, f64::max);
            let omega = 2.0 * PI * frequency_hz;
            Sequence {
                z1: z012[(1, 1)],
                z2: z012[(2, 2)],
                z0: z012[(0, 0)],
                c1: c012[(1, 1)].re,
                c0: c012[(0, 0)].re,
                z1_transposed: z1t,
                z0_transposed: z0t,
                c1_transposed: c1t.re,
                c0_transposed: c0t.re,
                b1_transposed: omega * c1t.re,
                b0_transposed: omega * c0t.re,
                unbalance: if z012[(1, 1)].norm() > 0.0 {
                    off / z012[(1, 1)].norm()
                } else {
                    0.0
                },
            }
        })
    }
}

/// The sequence values of a three-phase circuit over its length (totals:
/// Ω, F, S; divide by the length for per-unit-length values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sequence {
    /// Positive sequence impedance as built (the sequence matrix's
    /// diagonal; the phases a, b, c in the order of their angles).
    pub z1: Complex<f64>,
    /// Negative sequence impedance as built.
    pub z2: Complex<f64>,
    /// Zero sequence impedance as built.
    pub z0: Complex<f64>,
    /// Positive sequence capacitance as built.
    pub c1: f64,
    /// Zero sequence capacitance as built.
    pub c0: f64,
    /// Positive sequence impedance, ideally transposed.
    pub z1_transposed: Complex<f64>,
    /// Zero sequence impedance, ideally transposed.
    pub z0_transposed: Complex<f64>,
    /// Positive sequence capacitance, ideally transposed.
    pub c1_transposed: f64,
    /// Zero sequence capacitance, ideally transposed.
    pub c0_transposed: f64,
    /// Positive sequence susceptance, ideally transposed, S.
    pub b1_transposed: f64,
    /// Zero sequence susceptance, ideally transposed, S.
    pub b0_transposed: f64,
    /// The largest coupling between the sequences as built, relative to
    /// the positive sequence impedance (0 for an ideally transposed line).
    pub unbalance: f64,
}

/// A stretch of a circuit: consecutive spans with the same sections.
#[derive(Debug, Clone, PartialEq)]
pub struct Stretch {
    /// Its sections, in the circuit's phase order.
    pub sections: Vec<String>,
    /// The structure it starts at, and the one it ends at.
    pub from: String,
    /// The structure it ends at.
    pub to: String,
    /// Its spans.
    pub spans: usize,
    /// Its sums.
    pub totals: Totals,
}

impl CircuitParameters {
    /// The order of its phases as a, b, c for the symmetrical components:
    /// its first phase as a, then the others by their angle behind it
    /// (b lagging a by about 120°). `None` unless three phases.
    pub fn abc(&self) -> Option<Vec<usize>> {
        if self.angles.len() != 3 {
            return None;
        }
        let behind = |k: usize| (self.angles[0] - self.angles[k]).rem_euclid(360.0);
        let mut order = [1, 2];
        order.sort_by(|&x, &y| {
            (behind(x) - 120.0)
                .abs()
                .total_cmp(&(behind(y) - 120.0).abs())
        });
        Some(vec![0, order[0], order[1]])
    }

    /// Its sequence values over its whole length (three phases only).
    pub fn sequence(&self) -> Option<Sequence> {
        self.total.sequence(self.frequency_hz, &self.abc()?)
    }
}

/// A circuit's parameters along the line.
#[derive(Debug, Clone, PartialEq)]
pub struct CircuitParameters {
    /// The circuit.
    pub key: CircuitKey,
    /// Its frequency, Hz.
    pub frequency_hz: f64,
    /// Its phases' labels, in the matrices' order (the system's order).
    pub phases: Vec<String>,
    /// Their angles, degrees.
    pub angles: Vec<f64>,
    /// Its stretches, along the line.
    pub stretches: Vec<Stretch>,
    /// Its sums over the whole line.
    pub total: Totals,
}

/// The coupling of two circuits over the spans they share.
#[derive(Debug, Clone, PartialEq)]
pub struct Coupling {
    /// The first circuit.
    pub a: CircuitKey,
    /// The second circuit.
    pub b: CircuitKey,
    /// The length they share, m.
    pub length: f64,
    /// The mutual phase matrix (rows a's phases, columns b's), Ω.
    pub z: CMatrix,
    /// The mutual capacitances (Maxwell's, negative), F.
    pub c: DMatrix<f64>,
}

impl Coupling {
    /// The zero sequence mutual impedance, Ω: the mean of the matrix
    /// times three — \\( Z_{0M} = \tfrac13\sum_{ij} Z_{ij} \\) (the voltage
    /// on one circuit's zero sequence per zero sequence current in the
    /// other).
    pub fn z0m(&self) -> Complex<f64> {
        self.z.iter().sum::<Complex<f64>>() / Complex::new(self.a_phases() as f64, 0.0)
    }

    fn a_phases(&self) -> usize {
        self.z.nrows().max(1)
    }
}

/// A circuit's phases by their place in the system: label, angle (°).
type PhaseLabels = BTreeMap<usize, (String, f64)>;

/// The line parameters of a line from its spans' cross-sections.
#[derive(Debug, Clone, PartialEq)]
pub struct LineParameters {
    /// The earth return method.
    pub method: EarthReturn,
    /// Each span's phase matrices per unit length (one per frequency).
    pub spans: Vec<(LineSpan, CrossSection, Vec<SpanMatrices>)>,
    /// Per circuit.
    pub circuits: Vec<CircuitParameters>,
    /// Between circuits sharing spans (same frequency).
    pub couplings: Vec<Coupling>,
    /// What was assumed or left out (each once).
    pub notes: Vec<String>,
}

/// The line parameters from the spans' cross-sections (in order along the
/// line, [`crate::cross_section::span_cross_sections`]).
pub fn line_parameters(
    spans: Vec<(LineSpan, CrossSection)>,
    method: EarthReturn,
    mut notes: Vec<String>,
) -> LineParameters {
    let mut with = Vec::new();
    for (span, cs) in spans {
        notes.extend(cs.notes.iter().cloned());
        let m = span_matrices(&cs, method, &mut notes);
        with.push((span, cs, m));
    }
    // Every circuit's phases (by system order) and frequency.
    let mut circuits: BTreeMap<CircuitKey, (f64, PhaseLabels)> = BTreeMap::new();
    for (_, cs, ms) in &with {
        for m in ms {
            for p in &m.phases {
                let label = cs
                    .wires
                    .iter()
                    .filter_map(|w| w.phase.as_ref())
                    .find(|x| x.system == p.system && x.index == p.index && x.circuit == p.circuit)
                    .map(|x| (x.label.clone(), x.angle_deg))
                    .unwrap_or_default();
                circuits
                    .entry(CircuitKey {
                        circuit: p.circuit.clone(),
                        system: p.system.clone(),
                    })
                    .or_insert((m.frequency_hz, BTreeMap::new()))
                    .1
                    .insert(p.index, label);
            }
        }
    }
    let mut out_circuits = Vec::new();
    for (key, (f, phase_map)) in &circuits {
        let order: Vec<usize> = phase_map.keys().copied().collect();
        let n = order.len();
        let mut stretches: Vec<Stretch> = Vec::new();
        let mut total = Totals::zero(n);
        let mut incomplete = 0usize;
        for (span, cs, ms) in &with {
            // Its wires in the span (some may have been left out).
            let present = cs.wires.iter().any(|w| {
                w.phase
                    .as_ref()
                    .is_some_and(|p| p.system == key.system && p.circuit == key.circuit)
            });
            let Some(m) = ms.iter().find(|m| m.frequency_hz == *f) else {
                incomplete += usize::from(present);
                continue;
            };
            let at: Vec<Option<usize>> = order
                .iter()
                .map(|&i| {
                    m.phases.iter().position(|p| {
                        p.index == i && p.system == key.system && p.circuit == key.circuit
                    })
                })
                .collect();
            if at.iter().all(Option::is_none) {
                incomplete += usize::from(present);
                continue;
            }
            let Some(at) = at.into_iter().collect::<Option<Vec<usize>>>() else {
                incomplete += 1;
                continue;
            };
            let l = cs.station.span_length;
            let piece = Totals {
                length: l,
                z: CMatrix::from_fn(n, n, |i, j| m.z[(at[i], at[j])] * l),
                c: DMatrix::from_fn(n, n, |i, j| m.c[(at[i], at[j])] * l),
            };
            total.add(&piece);
            let sections: Vec<String> = at.iter().map(|&i| m.sections[i].clone()).collect();
            match stretches.last_mut() {
                Some(s) if s.sections == sections && s.to == span.from => {
                    s.totals.add(&piece);
                    s.to = span.to.clone();
                    s.spans += 1;
                }
                _ => stretches.push(Stretch {
                    sections,
                    from: span.from.clone(),
                    to: span.to.clone(),
                    spans: 1,
                    totals: piece,
                }),
            }
        }
        if incomplete > 0 {
            notes.push(format!(
                "{}: {incomplete} span(s) without all its phases left out (see the notes on its wires); its totals are short of them",
                key.label()
            ));
        }
        out_circuits.push(CircuitParameters {
            key: key.clone(),
            frequency_hz: *f,
            phases: phase_map.values().map(|v| v.0.clone()).collect(),
            angles: phase_map.values().map(|v| v.1).collect(),
            stretches,
            total,
        });
    }
    // Couplings: pairs of circuits of one frequency in the same spans.
    let mut couplings = Vec::new();
    for (ia, a) in out_circuits.iter().enumerate() {
        for b in &out_circuits[ia + 1..] {
            if a.frequency_hz != b.frequency_hz {
                continue;
            }
            let (na, nb) = (a.phases.len(), b.phases.len());
            let mut z = CMatrix::zeros(na, nb);
            let mut c = DMatrix::zeros(na, nb);
            let mut length = 0.0;
            for (_, cs, ms) in &with {
                let Some(m) = ms.iter().find(|m| m.frequency_hz == a.frequency_hz) else {
                    continue;
                };
                let place = |k: &CircuitKey, n: usize| -> Option<Vec<usize>> {
                    let sys_order = circuits[k].1.keys().copied().collect::<Vec<_>>();
                    let v: Option<Vec<usize>> = sys_order
                        .iter()
                        .map(|&i| {
                            m.phases.iter().position(|p| {
                                p.index == i && p.system == k.system && p.circuit == k.circuit
                            })
                        })
                        .collect();
                    v.filter(|v| v.len() == n)
                };
                let (Some(pa), Some(pb)) = (place(&a.key, na), place(&b.key, nb)) else {
                    continue;
                };
                let l = cs.station.span_length;
                length += l;
                for i in 0..na {
                    for j in 0..nb {
                        z[(i, j)] += m.z[(pa[i], pb[j])] * l;
                        c[(i, j)] += m.c[(pa[i], pb[j])] * l;
                    }
                }
            }
            if length > 0.0 {
                couplings.push(Coupling {
                    a: a.key.clone(),
                    b: b.key.clone(),
                    length,
                    z,
                    c,
                });
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    notes.retain(|n| seen.insert(n.clone()));
    LineParameters {
        method,
        spans: with,
        circuits: out_circuits,
        couplings,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero sequence mutual of two circuits by hand: a block of equal
    /// entries m gives \\( Z_{0M} = \tfrac13 \cdot 9m = 3m \\).
    #[test]
    fn zero_sequence_mutual_by_hand() {
        let m = Complex::new(0.05, 0.2);
        let k = |s: &str| CircuitKey {
            circuit: s.into(),
            system: "s".into(),
        };
        let c = Coupling {
            a: k("A"),
            b: k("B"),
            length: 1.0,
            z: CMatrix::from_element(3, 3, m),
            c: DMatrix::zeros(3, 3),
        };
        assert!((c.z0m() - m * 3.0).norm() < 1e-15);
    }
}
