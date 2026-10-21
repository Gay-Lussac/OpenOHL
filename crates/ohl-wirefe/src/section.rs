// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tension sections as finite-element models.
//!
//! A section runs from a dead-end to a dead-end through suspension points
//! (I-strings or V-strings). It is built the way lines are built:
//!
//! 1. **Sagging**: with every suspension string plumb, the cable is strung at
//!    the stringing horizontal tension and temperature; this fixes the
//!    unstressed length of every span (and of every span piece between
//!    point-load sites).
//! 2. **Clamping**: from then on each span keeps its unstressed length; any
//!    other state (temperature, ice, wind on cable and insulators, point
//!    loads, a broken span) is solved by the Newton solver, with insulator
//!    swing, unequal span tensions and 3D blowout included.
//!
//! Coordinates: x along the line, y transverse (wind positive), z up.

use std::fmt;

use nalgebra::Vector3;
use ohl_core::budget::Budget;

use crate::catenary_element::{CableProperties, default_element_budget};
use crate::model::{CableElement, FeError, Model, Node, Solution, TrussElement};

/// Insulator assembly at a support.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Assembly {
    /// Strain (dead-end) attachment: the cable is clamped at the attachment
    /// point. (Strain insulator length is not modelled yet.)
    DeadEnd,
    /// Suspension I-string hanging from the attachment point.
    Suspension {
        /// String length, m.
        length: f64,
        /// String weight, N.
        weight: f64,
        /// String axial stiffness, N.
        axial_stiffness: f64,
    },
    /// V-string: two legs from attachment points at ±`half_width` (transverse)
    /// to one clamp; legs carry tension only (a leg goes slack in compression).
    VString {
        /// Transverse half distance between the two leg attachments, m.
        half_width: f64,
        /// Leg length, m.
        leg_length: f64,
        /// Weight of each leg, N.
        leg_weight: f64,
        /// Leg axial stiffness, N.
        axial_stiffness: f64,
    },
}

/// A support of the section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Support {
    /// Attachment point on the structure (centre for V-strings), m.
    pub attachment: Vector3<f64>,
    /// Insulator assembly.
    pub assembly: Assembly,
}

/// Cable data for the section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CableSpec {
    /// Axial stiffness EA, N.
    pub axial_stiffness: f64,
    /// Thermal expansion coefficient, 1/K.
    pub thermal_expansion: f64,
    /// Bare weight per unit (unstressed) length, N/m.
    pub weight: f64,
}

/// A place in a span where point loads may act (marker, spacer, cart…).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadSite {
    /// Span index (0 = first span).
    pub span: usize,
    /// Horizontal fraction of the span from its back support, 0–1.
    pub fraction: f64,
}

/// Section definition.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionSpec {
    /// Supports in line order; first and last must be dead-ends.
    pub supports: Vec<Support>,
    /// Cable.
    pub cable: CableSpec,
    /// Stringing horizontal tension, N (all spans, insulators plumb).
    pub stringing_tension: f64,
    /// Stringing temperature, °C.
    pub stringing_temperature: f64,
    /// Point-load sites.
    pub load_sites: Vec<LoadSite>,
}

/// One analysis state.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// Cable temperature, °C.
    pub temperature: f64,
    /// Load on the cable per unit length (weight + ice downward as −z, wind
    /// as +y), N/m. It includes the cable's own weight
    /// [`CableSpec::weight`], which is per metre of the cable as strung:
    /// the solve spreads it over the span's unstressed length at the
    /// state's temperature (\\( w e^{-\alpha\Delta t} \\) per metre of
    /// it), so a span weighs the same hot or cold; ice and wind act per
    /// metre of the real cable, longer or shorter with the temperature and
    /// longer by its elastic strain ([`crate::CableElement::stretched_load`]).
    pub cable_load: Vector3<f64>,
    /// Transverse wind force on each insulator string (per leg for V), N.
    pub insulator_wind: f64,
    /// Force at each load site (same order as `SectionSpec::load_sites`), N.
    pub site_loads: Vec<Vector3<f64>>,
    /// Spans removed (broken conductor).
    pub broken_spans: Vec<usize>,
    /// Spans in a state of their own (temperature and load per metre),
    /// by span index; the others in `temperature` and `cable_load`. E.g.
    /// one span iced while its neighbours are not.
    pub span_states: Vec<(usize, SpanState)>,
}

/// The state of one span when it differs from the section's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanState {
    /// Cable temperature, °C.
    pub temperature: f64,
    /// Load on the cable per unit length, N/m (as [`State::cable_load`]).
    pub cable_load: Vector3<f64>,
}

impl State {
    /// The temperature and load per metre of span `span`.
    pub fn of_span(&self, span: usize) -> (f64, Vector3<f64>) {
        self.span_states
            .iter()
            .rev()
            .find(|(s, _)| *s == span)
            .map_or((self.temperature, self.cable_load), |(_, x)| {
                (x.temperature, x.cable_load)
            })
    }
}

impl State {
    /// Bare cable, still air, no point loads, at `temperature`.
    pub fn bare(spec: &SectionSpec, temperature: f64) -> Self {
        Self {
            temperature,
            cable_load: Vector3::new(0.0, 0.0, -spec.cable.weight),
            insulator_wind: 0.0,
            site_loads: vec![Vector3::zeros(); spec.load_sites.len()],
            broken_spans: vec![],
            span_states: vec![],
        }
    }
}

/// Errors building or solving a section.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionError {
    /// Invalid section definition.
    Invalid(String),
    /// Solver error.
    Fe(FeError),
}

impl fmt::Display for SectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) => write!(f, "invalid section: {m}"),
            Self::Fe(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SectionError {}

impl From<FeError> for SectionError {
    fn from(e: FeError) -> Self {
        Self::Fe(e)
    }
}

/// A cable piece between two consecutive nodes of a span.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    span: usize,
    i: usize,
    j: usize,
    unstressed_length: f64,
}

/// A built (sagged) section.
#[derive(Debug, Clone, PartialEq)]
pub struct FeSection {
    spec: SectionSpec,
    /// Model with nodes, insulators and fixed supports (cables added per state).
    base: Model,
    /// Clamp node of each support.
    clamps: Vec<usize>,
    /// Node of each load site.
    site_nodes: Vec<usize>,
    pieces: Vec<Piece>,
    /// Insulator elements of each support (in `base.trusses`).
    insulators: Vec<Vec<usize>>,
}

/// Result per span.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanResult {
    /// Horizontal (in the load plane, perpendicular to the load) tension of
    /// the first piece, N.
    pub horizontal_tension: f64,
    /// Tension at the back clamp, N.
    pub tension_back: f64,
    /// Tension at the ahead clamp, N.
    pub tension_ahead: f64,
    /// Maximum vertical distance between the clamp-to-clamp chord and the
    /// cable, m.
    pub sag: f64,
    /// Largest transverse displacement of the cable from the clamp chord, m.
    pub blowout: f64,
}

/// Result per support.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportResult {
    /// Clamp position, m.
    pub clamp: Vector3<f64>,
    /// Swing of the (I-)string from vertical, degrees (0 for dead-ends and V).
    pub swing_deg: f64,
    /// Longitudinal swing component, degrees (positive towards +x).
    pub swing_longitudinal_deg: f64,
    /// Axial force of each insulator element (leg), N.
    pub insulator_forces: Vec<f64>,
    /// Total force the support carries, N.
    pub load: Vector3<f64>,
}

/// Result of a section state.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionResult {
    /// Per span (broken spans omitted: `None`).
    pub spans: Vec<Option<SpanResult>>,
    /// Per support.
    pub supports: Vec<SupportResult>,
    /// Newton iterations.
    pub iterations: u64,
}

fn clamp_position(support: &Support) -> Result<Vector3<f64>, SectionError> {
    Ok(match support.assembly {
        Assembly::DeadEnd => support.attachment,
        Assembly::Suspension { length, .. } => support.attachment - Vector3::new(0.0, 0.0, length),
        Assembly::VString {
            half_width,
            leg_length,
            ..
        } => {
            if leg_length <= half_width {
                return Err(SectionError::Invalid(
                    "two strings to one wire point shorter than the half width".into(),
                ));
            }
            support.attachment
                - Vector3::new(
                    0.0,
                    0.0,
                    (leg_length * leg_length - half_width * half_width).sqrt(),
                )
        }
    })
}

impl FeSection {
    /// Builds the section: nodes, insulators, and the unstressed length of
    /// every cable piece from sagging with plumb insulators.
    pub fn build(spec: SectionSpec, budget: &Budget) -> Result<Self, SectionError> {
        let n = spec.supports.len();
        if n < 2 {
            return Err(SectionError::Invalid(
                "a section needs at least two supports".into(),
            ));
        }
        if spec.supports[0].assembly != Assembly::DeadEnd
            || spec.supports[n - 1].assembly != Assembly::DeadEnd
        {
            return Err(SectionError::Invalid(
                "first and last supports must be dead-ends".into(),
            ));
        }
        if let Some(site) = spec
            .load_sites
            .iter()
            .find(|s| s.span >= n - 1 || !(0.0 < s.fraction && s.fraction < 1.0))
        {
            return Err(SectionError::Invalid(format!(
                "load site {site:?} is not inside a span"
            )));
        }
        let mut base = Model::default();
        let mut clamps = Vec::with_capacity(n);
        let mut insulators = Vec::with_capacity(n);
        for support in &spec.supports {
            let clamp = clamp_position(support)?;
            match support.assembly {
                Assembly::DeadEnd => {
                    clamps.push(base.nodes.len());
                    base.nodes.push(Node::fixed(clamp));
                    insulators.push(vec![]);
                }
                Assembly::Suspension {
                    length,
                    weight,
                    axial_stiffness,
                } => {
                    let attach = base.nodes.len();
                    base.nodes.push(Node::fixed(support.attachment));
                    clamps.push(base.nodes.len());
                    base.nodes.push(Node::free(clamp));
                    insulators.push(vec![base.trusses.len()]);
                    base.trusses.push(TrussElement {
                        i: attach,
                        j: attach + 1,
                        unstressed_length: length,
                        axial_stiffness,
                        tension_only: true,
                        weight,
                    });
                }
                Assembly::VString {
                    half_width,
                    leg_length,
                    leg_weight,
                    axial_stiffness,
                } => {
                    let left = base.nodes.len();
                    base.nodes.push(Node::fixed(
                        support.attachment - Vector3::new(0.0, half_width, 0.0),
                    ));
                    base.nodes.push(Node::fixed(
                        support.attachment + Vector3::new(0.0, half_width, 0.0),
                    ));
                    let clamp_node = base.nodes.len();
                    clamps.push(clamp_node);
                    base.nodes.push(Node::free(clamp));
                    insulators.push(vec![base.trusses.len(), base.trusses.len() + 1]);
                    for leg in [left, left + 1] {
                        base.trusses.push(TrussElement {
                            i: leg,
                            j: clamp_node,
                            unstressed_length: leg_length,
                            axial_stiffness,
                            tension_only: true,
                            weight: leg_weight,
                        });
                    }
                }
            }
        }

        let gravity = Vector3::new(0.0, 0.0, -spec.cable.weight);
        let element_budget = default_element_budget();

        // Plumb strings stretch under the clamp load (~0.3 mm for 5 kN on a
        // 3 m composite string). Sagging is done on the loaded strings, so
        // place each clamp at its stretched plumb position. Without this the
        // stringing state was off by 1.2e-5 in tension (found in testing).
        // Fixed 3 passes: the stretch changes the loads only at second order.
        for _ in 0..3 {
            let mut vertical = vec![0.0; n];
            for span in 0..n - 1 {
                let (pa, pb) = (
                    base.nodes[clamps[span]].position,
                    base.nodes[clamps[span + 1]].position,
                );
                let d = pb - pa;
                let props = CableProperties {
                    unstressed_length: CableProperties::unstressed_length_for_tension(
                        d,
                        spec.stringing_tension,
                        spec.cable.axial_stiffness,
                        gravity,
                        budget,
                    )
                    .map_err(|error| FeError::Element {
                        element: span,
                        error,
                    })?,
                    axial_stiffness: spec.cable.axial_stiffness,
                    load: gravity,
                };
                let state =
                    props
                        .solve(d, None, &element_budget)
                        .map_err(|error| FeError::Element {
                            element: span,
                            error,
                        })?;
                vertical[span] -= state.force_i.z;
                vertical[span + 1] -= state.force_j.z;
            }
            for (k, support) in spec.supports.iter().enumerate() {
                let node = clamps[k];
                match support.assembly {
                    Assembly::DeadEnd => {}
                    Assembly::Suspension {
                        length,
                        weight,
                        axial_stiffness,
                    } => {
                        let force = vertical[k] + weight / 2.0;
                        let stretched = length * (1.0 + force / axial_stiffness);
                        base.nodes[node].position =
                            support.attachment - Vector3::new(0.0, 0.0, stretched);
                    }
                    Assembly::VString {
                        half_width,
                        leg_length,
                        leg_weight,
                        axial_stiffness,
                    } => {
                        let depth = base.nodes[node].position.z - support.attachment.z;
                        let cos_beta = depth.abs() / leg_length;
                        let leg_force = (vertical[k] + leg_weight) / 2.0 / cos_beta;
                        let stretched = leg_length * (1.0 + leg_force / axial_stiffness);
                        let new_depth = (stretched * stretched - half_width * half_width).sqrt();
                        base.nodes[node].position =
                            support.attachment - Vector3::new(0.0, 0.0, new_depth);
                    }
                }
            }
        }

        // Sagging: each span at the stringing tension with plumb insulators,
        // then split at the load sites along the sagged shape.
        let mut pieces = Vec::new();
        let mut site_nodes = vec![0; spec.load_sites.len()];
        for span in 0..n - 1 {
            let (a, b) = (clamps[span], clamps[span + 1]);
            let (pa, pb) = (base.nodes[a].position, base.nodes[b].position);
            let d = pb - pa;
            let l0 = CableProperties::unstressed_length_for_tension(
                d,
                spec.stringing_tension,
                spec.cable.axial_stiffness,
                gravity,
                budget,
            )
            .map_err(|error| FeError::Element {
                element: span,
                error,
            })?;
            let props = CableProperties {
                unstressed_length: l0,
                axial_stiffness: spec.cable.axial_stiffness,
                load: gravity,
            };
            let sagged =
                props
                    .solve(d, None, &element_budget)
                    .map_err(|error| FeError::Element {
                        element: span,
                        error,
                    })?;
            let mut sites: Vec<(f64, usize)> = spec
                .load_sites
                .iter()
                .enumerate()
                .filter(|(_, s)| s.span == span)
                .map(|(k, s)| (props.arc_length_at_fraction(d, &sagged, s.fraction), k))
                .collect();
            sites.sort_by(|x, y| x.0.total_cmp(&y.0));
            let (mut prev_node, mut prev_s) = (a, 0.0);
            for (s0, k) in sites {
                let node = base.nodes.len();
                base.nodes
                    .push(Node::free(pa + props.point_at(d, &sagged, s0)));
                site_nodes[k] = node;
                pieces.push(Piece {
                    span,
                    i: prev_node,
                    j: node,
                    unstressed_length: s0 - prev_s,
                });
                (prev_node, prev_s) = (node, s0);
            }
            pieces.push(Piece {
                span,
                i: prev_node,
                j: b,
                unstressed_length: l0 - prev_s,
            });
        }
        Ok(Self {
            spec,
            base,
            clamps,
            site_nodes,
            pieces,
            insulators,
        })
    }

    /// The section definition.
    pub fn spec(&self) -> &SectionSpec {
        &self.spec
    }

    /// Solves one state.
    ///
    /// Expected cost: a few Newton iterations of a system with 3 × (clamps +
    /// load sites) unknowns — well under 10 ms for a 20-span section.
    pub fn solve(&self, state: &State, budget: &Budget) -> Result<SectionResult, SectionError> {
        if state.site_loads.len() != self.site_nodes.len() {
            return Err(SectionError::Invalid(format!(
                "{} site loads given for {} load sites",
                state.site_loads.len(),
                self.site_nodes.len()
            )));
        }
        let mut model = self.base.clone();
        let mut piece_elements = Vec::with_capacity(self.pieces.len());
        for piece in &self.pieces {
            if state.broken_spans.contains(&piece.span) {
                piece_elements.push(None);
                continue;
            }
            piece_elements.push(Some(model.cables.len()));
            // The span's own state, or the section's.
            let (temperature, load) = state.of_span(piece.span);
            let thermal = (self.spec.cable.thermal_expansion
                * (temperature - self.spec.stringing_temperature))
                .exp();
            let weight = self.spec.cable.weight;
            model.cables.push(CableElement {
                i: piece.i,
                j: piece.j,
                properties: CableProperties {
                    unstressed_length: piece.unstressed_length * thermal,
                    axial_stiffness: self.spec.cable.axial_stiffness,
                    // The own weight conserved: w / e^{αΔt} per metre of
                    // the thermally longer unstressed length.
                    load: Vector3::new(0.0, 0.0, -weight / thermal),
                },
                // Ice and wind per metre of the stretched cable.
                stretched_load: load + Vector3::new(0.0, 0.0, weight),
            });
        }
        // Load sites of broken spans are dropped; their nodes stay free but
        // unconnected, so fix them to keep the system regular.
        for (k, &node) in self.site_nodes.iter().enumerate() {
            let site = self.spec.load_sites[k];
            if state.broken_spans.contains(&site.span) {
                model.nodes[node].fixed = true;
            } else {
                model.nodes[node].load += state.site_loads[k];
            }
        }
        // Insulator wind: half of each string's wind load at its clamp.
        for (support, elements) in self.insulators.iter().enumerate() {
            for _ in elements {
                model.nodes[self.clamps[support]].load +=
                    Vector3::new(0.0, state.insulator_wind / 2.0, 0.0);
            }
        }
        let solution = model.solve(budget)?;
        Ok(self.summarise(&model, &solution, &piece_elements))
    }

    fn summarise(
        &self,
        model: &Model,
        solution: &Solution,
        piece_elements: &[Option<usize>],
    ) -> SectionResult {
        let p = &solution.positions;
        let n_spans = self.clamps.len() - 1;
        let mut spans = vec![None; n_spans];
        for (span, slot) in spans.iter_mut().enumerate() {
            let pieces: Vec<(usize, usize)> = self
                .pieces
                .iter()
                .zip(piece_elements)
                .filter(|(piece, _)| piece.span == span)
                .filter_map(|(_, e)| e.map(|e| (e, model.cables[e].i)))
                .collect();
            if pieces.is_empty() {
                continue;
            }
            let (a, b) = (p[self.clamps[span]], p[self.clamps[span + 1]]);
            let chord = b - a;
            let horizontal_dir = Vector3::new(chord.x, chord.y, 0.0).normalize();
            let lateral_dir = Vector3::new(-horizontal_dir.y, horizontal_dir.x, 0.0);
            let (mut sag, mut blowout) = (0.0_f64, 0.0_f64);
            for &(e, i) in &pieces {
                let element = &model.cables[e];
                let d = p[element.j] - p[i];
                let props = solution.cable_properties(model, e);
                for point in props.shape(d, &solution.cables[e], 64) {
                    let r = p[i] + point - a;
                    // Vertical distance below the chord at the same horizontal position.
                    let t = r.dot(&horizontal_dir) / Vector3::new(chord.x, chord.y, 0.0).norm();
                    sag = sag.max(a.z + t * chord.z - (a.z + r.z));
                    blowout = blowout.max(r.dot(&lateral_dir).abs());
                }
            }
            let first = &solution.cables[pieces[0].0];
            let last = &solution.cables[pieces[pieces.len() - 1].0];
            *slot = Some(SpanResult {
                horizontal_tension: first.horizontal,
                tension_back: first.tension_i,
                tension_ahead: last.tension_j,
                sag,
                blowout,
            });
        }
        let supports = self
            .spec
            .supports
            .iter()
            .enumerate()
            .map(|(k, support)| {
                let clamp = p[self.clamps[k]];
                let (swing, swing_long) = match support.assembly {
                    Assembly::Suspension { .. } => {
                        let s = clamp - support.attachment;
                        (
                            s.x.hypot(s.y).atan2(-s.z).to_degrees(),
                            s.x.atan2(-s.z).to_degrees(),
                        )
                    }
                    _ => (0.0, 0.0),
                };
                let forces: Vec<f64> = self.insulators[k]
                    .iter()
                    .map(|&t| solution.truss_forces[t])
                    .collect();
                let load = match support.assembly {
                    Assembly::DeadEnd => solution.reactions[self.clamps[k]],
                    _ => self.insulators[k]
                        .iter()
                        .map(|&t| solution.reactions[model.trusses[t].i])
                        .sum(),
                };
                SupportResult {
                    clamp,
                    swing_deg: swing,
                    swing_longitudinal_deg: swing_long,
                    insulator_forces: forces,
                    load,
                }
            })
            .collect();
        SectionResult {
            spans,
            supports,
            iterations: solution.iterations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::default_fe_budget;

    const W: f64 = 15.97;

    fn suspension() -> Assembly {
        Assembly::Suspension {
            length: 3.0,
            weight: 800.0,
            axial_stiffness: 5e7,
        }
    }

    fn spec(spans: &[(f64, f64)], middle: Assembly) -> SectionSpec {
        let mut x = 0.0;
        let mut z = 30.0;
        let mut supports = vec![Support {
            attachment: Vector3::new(0.0, 0.0, z - 3.0),
            assembly: Assembly::DeadEnd,
        }];
        for (k, &(span, rise)) in spans.iter().enumerate() {
            x += span;
            z += rise;
            let last = k == spans.len() - 1;
            supports.push(Support {
                attachment: Vector3::new(x, 0.0, if last { z - 3.0 } else { z }),
                assembly: if last { Assembly::DeadEnd } else { middle },
            });
        }
        SectionSpec {
            supports,
            cable: CableSpec {
                axial_stiffness: 34.64e6,
                thermal_expansion: 18.9e-6,
                weight: W,
            },
            stringing_tension: 28_000.0,
            stringing_temperature: 15.0,
            load_sites: vec![],
        }
    }

    fn budget() -> Budget {
        Budget::new().max_iterations(200)
    }

    #[test]
    fn stringing_state_is_equilibrium_with_plumb_strings() {
        let section = FeSection::build(
            spec(&[(300.0, 0.0), (350.0, 20.0), (280.0, -15.0)], suspension()),
            &budget(),
        )
        .unwrap();
        let r = section
            .solve(&State::bare(section.spec(), 15.0), &default_fe_budget())
            .unwrap();
        for span in r.spans.iter().flatten() {
            assert!(
                (span.horizontal_tension - 28_000.0).abs() < 1e-3,
                "{}",
                span.horizontal_tension
            );
        }
        for s in &r.supports {
            assert!(s.swing_deg.abs() < 1e-6, "{}", s.swing_deg);
        }
    }

    /// A span weighs the same hot or cold: the supports carry its own
    /// weight per metre as strung times its unstressed length as cut
    /// (before 2026-10-21: 0.28 % more at 150 °C, by exp(alpha dt)).
    #[test]
    fn total_weight_is_conserved_hot_and_cold() {
        let mut s = spec(&[(300.0, 0.0)], suspension());
        for support in &mut s.supports {
            support.assembly = Assembly::DeadEnd;
        }
        let section = FeSection::build(s, &budget()).unwrap();
        let cut: f64 = section.pieces.iter().map(|p| p.unstressed_length).sum();
        for t in [-20.0, 15.0, 70.0, 150.0] {
            let r = section
                .solve(&State::bare(section.spec(), t), &default_fe_budget())
                .unwrap();
            let carried: f64 = r.supports.iter().map(|s| s.load.z).sum();
            assert!(
                (carried.abs() / (W * cut) - 1.0).abs() < 1e-9,
                "{t} °C: {carried} N vs {} N",
                W * cut
            );
        }
    }

    #[test]
    fn hot_conductor_unequal_spans_swing_strings() {
        let section = FeSection::build(
            spec(&[(200.0, 0.0), (450.0, 0.0), (250.0, 0.0)], suspension()),
            &budget(),
        )
        .unwrap();
        let r = section
            .solve(&State::bare(section.spec(), 75.0), &default_fe_budget())
            .unwrap();
        // Tension drops with heat; the long span pulls the strings towards it.
        let h: Vec<f64> = r
            .spans
            .iter()
            .flatten()
            .map(|s| s.horizontal_tension)
            .collect();
        assert!(h.iter().all(|&t| t < 28_000.0));
        assert!(
            r.supports[1].swing_longitudinal_deg > 0.0,
            "{}",
            r.supports[1].swing_longitudinal_deg
        );
        assert!(
            r.supports[2].swing_longitudinal_deg < 0.0,
            "{}",
            r.supports[2].swing_longitudinal_deg
        );
    }

    #[test]
    fn wind_blows_out_cable_and_strings() {
        let section =
            FeSection::build(spec(&[(300.0, 0.0), (300.0, 0.0)], suspension()), &budget()).unwrap();
        let mut state = State::bare(section.spec(), 15.0);
        state.cable_load = Vector3::new(0.0, 8.0, -W);
        state.insulator_wind = 150.0;
        let r = section.solve(&state, &default_fe_budget()).unwrap();
        assert!(r.supports[1].swing_deg > 15.0);
        assert!(r.spans[0].as_ref().unwrap().blowout > 1.0);
    }

    #[test]
    fn broken_span_and_v_string() {
        let v = Assembly::VString {
            half_width: 2.0,
            leg_length: 4.0,
            leg_weight: 400.0,
            axial_stiffness: 5e7,
        };
        let section = FeSection::build(
            spec(&[(300.0, 0.0), (300.0, 0.0), (300.0, 0.0)], v),
            &budget(),
        )
        .unwrap();
        let mut state = State::bare(section.spec(), 15.0);
        state.cable_load = Vector3::new(0.0, 10.0, -W);
        let windy = section.solve(&state, &default_fe_budget()).unwrap();
        // Wind towards +y: the +y leg goes slack, the −y leg carries the load.
        let legs = &windy.supports[1].insulator_forces;
        assert!(legs[0] > 1000.0 && legs[1] < legs[0], "{legs:?}");

        state.broken_spans = vec![2];
        let broken = section.solve(&state, &default_fe_budget()).unwrap();
        assert!(broken.spans[2].is_none());
        assert!(
            broken.spans[1].as_ref().unwrap().horizontal_tension
                < windy.spans[1].as_ref().unwrap().horizontal_tension
        );
    }

    #[test]
    fn point_load_site_adds_sag() {
        let mut s = spec(&[(300.0, 0.0), (300.0, 0.0)], suspension());
        s.load_sites = vec![LoadSite {
            span: 0,
            fraction: 0.5,
        }];
        let section = FeSection::build(s, &budget()).unwrap();
        let bare = section
            .solve(&State::bare(section.spec(), 15.0), &default_fe_budget())
            .unwrap();
        let mut loaded_state = State::bare(section.spec(), 15.0);
        loaded_state.site_loads[0] = Vector3::new(0.0, 0.0, -2_000.0);
        let loaded = section.solve(&loaded_state, &default_fe_budget()).unwrap();
        let (b, l) = (
            bare.spans[0].as_ref().unwrap(),
            loaded.spans[0].as_ref().unwrap(),
        );
        assert!(l.sag > b.sag + 1.0, "bare {} loaded {}", b.sag, l.sag);
        // The loaded span pulls the shared string towards it.
        assert!(loaded.supports[1].swing_longitudinal_deg < 0.0);
    }

    /// A span in a state of its own: two level 300 m spans at −5 °C,
    /// span 0 with 10 N/m of ice, then span 0 at 60 °C and span 1 at
    /// 15 °C. Reference: benchmarks/wirefe/reference_span_cases.py (the
    /// closed-form elastic catenary and the clamp's balance, its own
    /// implementation; 15.97 N/m). With every span in the section's state,
    /// the same as without span states, bit for bit.
    #[test]
    fn a_span_in_its_own_state() {
        let section =
            FeSection::build(spec(&[(300.0, 0.0), (300.0, 0.0)], suspension()), &budget()).unwrap();
        let close = |a: f64, b: f64, tol: f64| assert!((a - b).abs() < tol, "{a} {b}");
        let mut cold = State::bare(section.spec(), -5.0);
        let plain = section.solve(&cold, &default_fe_budget()).unwrap();
        cold.span_states = vec![(
            1,
            SpanState {
                temperature: -5.0,
                cable_load: cold.cable_load,
            },
        )];
        assert_eq!(section.solve(&cold, &default_fe_budget()).unwrap(), plain);
        let check = |state: &State, h: [f64; 2], x: f64, swing: f64| {
            let r = section.solve(state, &default_fe_budget()).unwrap();
            for (s, h) in h.iter().enumerate() {
                close(
                    r.spans[s].as_ref().unwrap().horizontal_tension,
                    *h,
                    1e-7 * h,
                );
            }
            close(r.supports[1].clamp.x, x, 1e-6);
            close(r.supports[1].swing_longitudinal_deg, swing, 1e-6);
        };
        cold.span_states = vec![(
            0,
            SpanState {
                temperature: -5.0,
                cable_load: Vector3::new(0.0, 0.0, -(W + 10.0)),
            },
        )];
        // The ice per stretched metre (2026-10-21; before: 38955.83 N, 38617.56 N).
        check(&cold, [38964.13886, 38625.44942], 299.8484214, -2.895781995);
        let mut hot = State::bare(section.spec(), 15.0);
        hot.span_states = vec![(
            0,
            SpanState {
                temperature: 60.0,
                cable_load: hot.cable_load,
            },
        )];
        check(&hot, [24695.66197, 24908.196], 300.1226835, 2.343492256);
    }
}
