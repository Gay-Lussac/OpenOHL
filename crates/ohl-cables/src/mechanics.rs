// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Non-linear stress-strain and creep behaviour of composite cables.
//!
//! Each component (outer / core) may have experimental curves, as published
//! for the polynomial method (Aluminum Association practice, IEEE / CIGRE
//! sag-tension methods):
//!
//! - **initial** curve: stress (or load) as a polynomial of strain, followed
//!   when the cable is loaded beyond anything it has seen before;
//! - **creep** curve (typically 10-year creep): the strain reached at a given
//!   stress after the creep period;
//! - **final modulus**: the slope of unloading and reloading after permanent
//!   stretch.
//!
//! Components share the total strain \\( \varepsilon \\); component *k* has
//! mechanical strain \\( \varepsilon - \alpha_k (t - t_0) \\) and carries load
//! \\( P_k \\); the cable tension is \\( \sum_k P_k \\). The outer layers can be
//! limited in compression (aluminium unloads onto the core at high
//! temperature — the "knee point").
//!
//! States ([`MaterialState`]):
//! - `Initial`: \\( P_k = \mathrm{initial}_k(\varepsilon_k) \\) (final-modulus
//!   line below zero strain);
//! - `Creep`: the creep curve, used to find the permanent set after creep;
//! - `Final { permanent }`: \\( P_k = \min(EA_{f,k}(\varepsilon_k - \varepsilon_{p,k}),
//!   \mathrm{initial}_k(\varepsilon_k)) \\) — elastic along the final modulus from the
//!   permanent set, and back onto the initial curve if loaded beyond. The
//!   set is signed: a component whose initial curve rises more steeply
//!   than its final modulus (some steel cores) has a slightly negative
//!   one, so that the line passes through the point reached (below zero
//!   strain the initial state is the final-modulus line from zero).
//!
//! Beyond the curves' validity (`max_strain`, when the cable gives one)
//! the polynomial is still evaluated; [`Mechanics::beyond_range`] says
//! when a state gets there, for the warnings of the results. Without a
//! `max_strain` there is no range and no warning (owner, 2026-10-22).
//!
//! Components without curves are linear (\\( P = EA\,\varepsilon \\)) in every state.
//! Coefficients are data supplied with the cable (see `docs/theory`); OpenOHL
//! ships no manufacturer curves.

use ohl_core::units::{Force, Quantity, Stress, Temperature};
use serde::Deserialize;

use crate::{Cable, CableError, Component};

/// Unit of strain used by the coefficients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrainUnit {
    /// Strain in percent (common in published curves).
    #[default]
    Percent,
    /// Dimensionless strain.
    Unit,
}

/// Area the published stress refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StressBasis {
    /// Stress on the component's own area.
    #[default]
    Component,
    /// Stress on the total cable area (as some sources tabulate both curves).
    Total,
}

fn default_reference_temperature() -> Temperature {
    Temperature::from_celsius(20.0)
}

/// Stress-strain data of one component, as published.
///
/// Stress \\( = \sum_k a_k \varepsilon^k \\) in `stress_unit`, with
/// \\( \varepsilon \\) in `strain` units, on the area given by `basis`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StressStrainData {
    /// Strain unit of the coefficients.
    #[serde(default)]
    pub strain: StrainUnit,
    /// Stress unit symbol, e.g. `"psi"`, `"MPa"`.
    pub stress_unit: String,
    /// Area the stress refers to.
    #[serde(default)]
    pub basis: StressBasis,
    /// Initial curve coefficients \\( a_0, a_1, \dots \\).
    pub initial: Vec<f64>,
    /// Creep curve coefficients (none = no creep).
    #[serde(default)]
    pub creep: Option<Vec<f64>>,
    /// Final (unloading) modulus, in `stress_unit` per strain unit.
    pub final_modulus: f64,
    /// Largest compressive stress the component carries (positive number,
    /// in `stress_unit`); none = unlimited.
    #[serde(default)]
    pub max_compression: Option<f64>,
    /// Upper end of the curves' validity, in strain units; none: no range
    /// given, no range warning (the curves must still rise up to 1 %).
    #[serde(default)]
    pub max_strain: Option<f64>,
    /// Temperature at which the curves were measured.
    #[serde(default = "default_reference_temperature")]
    pub reference_temperature: Temperature,
}

/// Curves converted to SI: load (N) as a polynomial of unit strain.
#[derive(Debug, Clone, PartialEq)]
struct Curves {
    initial: Vec<f64>,
    creep: Option<Vec<f64>>,
    final_stiffness: f64,
    max_compression: f64,
    /// Upper end of the curves' validity, unit strain (mechanical); none:
    /// not given (no range warning).
    max_strain: Option<f64>,
    /// Without a range given: where the initial / creep curve stops
    /// rising before 1 % strain, unit strain — the curve held flat beyond
    /// (owner, 2026-10-27; conservative: the layer takes no more load,
    /// the wire stretches more).
    flat: [Option<f64>; 2],
}

impl Curves {
    /// The initial curve's load at `strain` (held flat beyond its turn).
    fn initial_at(&self, strain: f64) -> f64 {
        poly(
            &self.initial,
            self.flat[0].map_or(strain, |f| strain.min(f)),
        )
    }

    /// The creep curve's load (the initial curve's without one).
    fn creep_at(&self, strain: f64) -> f64 {
        match &self.creep {
            Some(c) => poly(c, self.flat[1].map_or(strain, |f| strain.min(f))),
            None => self.initial_at(strain),
        }
    }

    /// Where the first of its curves is held flat, unit strain.
    fn held_from(&self) -> Option<f64> {
        match self.flat {
            [Some(a), Some(b)] => Some(a.min(b)),
            [a, b] => a.or(b),
        }
    }
}

fn poly(c: &[f64], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &a| acc * x + a)
}

fn poly_derivative(c: &[f64], x: f64) -> f64 {
    c.iter()
        .enumerate()
        .skip(1)
        .rev()
        .fold(0.0, |acc, (k, &a)| acc * x + k as f64 * a)
}

/// A component strained beyond its curves' validity
/// ([`Mechanics::beyond_range`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeExceeded {
    /// `"outer"` or `"core"`.
    pub component: &'static str,
    /// Its mechanical strain, unit strain.
    pub strain: f64,
    /// The curves' validity limit, unit strain.
    pub max_strain: f64,
    /// The limit is where a curve stops rising (no range given): held
    /// flat beyond it, not extrapolated.
    pub held: bool,
}

impl std::fmt::Display for RangeExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.held {
            write!(
                f,
                "{} strain {:.4} % beyond where its curve stops rising ({:.4} %): held flat there \
                 (conservative: the wire stretches more)",
                self.component,
                self.strain * 100.0,
                self.max_strain * 100.0
            )
        } else {
            write!(
                f,
                "{} strain {:.4} % beyond its curves' range ({:.4} %): the polynomial is extrapolated",
                self.component,
                self.strain * 100.0,
                self.max_strain * 100.0
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ComponentMechanics {
    label: &'static str,
    thermal_expansion: f64,
    linear_stiffness: f64,
    curves: Option<Curves>,
    /// Largest compressive load, N (the curves' and the bimetallic
    /// model's).
    max_compression: f64,
}

/// Which curve a state follows.
#[derive(Debug, Clone, PartialEq)]
pub enum MaterialState {
    /// Initial loading curve.
    Initial,
    /// Creep curve (strain after the creep period).
    Creep,
    /// Elastic along the final modulus from a permanent set (one value of
    /// mechanical strain per component).
    Final {
        /// Permanent strain per component (outer, core).
        permanent: Vec<f64>,
    },
}

/// Composite mechanical model of a cable.
#[derive(Debug, Clone, PartialEq)]
pub struct Mechanics {
    components: Vec<ComponentMechanics>,
    reference_temperature: Temperature,
}

impl ComponentMechanics {
    fn load(&self, strain: f64, state: &MaterialState, index: usize) -> f64 {
        let Some(c) = &self.curves else {
            return (self.linear_stiffness * strain).max(-self.max_compression);
        };
        let elastic_from = |permanent: f64| c.final_stiffness * (strain - permanent);
        let load = match state {
            MaterialState::Initial => {
                if strain >= 0.0 {
                    c.initial_at(strain)
                } else {
                    elastic_from(0.0)
                }
            }
            MaterialState::Creep => {
                if strain >= 0.0 {
                    c.creep_at(strain)
                } else {
                    elastic_from(0.0)
                }
            }
            MaterialState::Final { permanent } => {
                // The final-modulus line from the (signed) set, never
                // above the initial state (the initial curve, the final
                // modulus from zero below zero strain): a set below zero
                // (an initial curve steeper than the final modulus) leaves
                // the low end on the initial state, continuous at zero.
                let line = elastic_from(permanent[index]);
                let envelope = if strain >= 0.0 {
                    c.initial_at(strain)
                } else {
                    elastic_from(0.0)
                };
                line.min(envelope)
            }
        };
        load.max(-self.max_compression)
    }
}

impl Mechanics {
    /// Builds the model from a cable. Components without stress-strain data
    /// are linear with their modulus; the outer strands of a bimetallic
    /// cable are limited in compression as the cable says.
    pub fn from_cable(cable: &Cable) -> Result<Self, CableError> {
        let outer_limit = cable.outer_compression_limit();
        let total_area = cable.area().si();
        let mut reference: Option<Temperature> = None;
        let mut components = Vec::new();
        for (label, component) in [("outer", Some(&cable.outer)), ("core", cable.core.as_ref())] {
            let Some(component) = component else { continue };
            // The linear models leave the curves aside (kept in the file).
            let data = component
                .stress_strain
                .as_ref()
                .filter(|_| cable.model == crate::CableModel::Nonlinear);
            let curves = match data {
                None => None,
                Some(data) => {
                    if reference.is_some_and(|r| r != data.reference_temperature) {
                        return Err(invalid(
                            cable,
                            "components have different curve reference temperatures",
                        ));
                    }
                    reference = Some(data.reference_temperature);
                    Some(convert(cable, label, component, data, total_area)?)
                }
            };
            let mut max_compression = curves.as_ref().map_or(f64::INFINITY, |c| c.max_compression);
            if label == "outer" {
                max_compression = max_compression.min(outer_limit);
            }
            components.push(ComponentMechanics {
                label,
                thermal_expansion: component.thermal_expansion,
                linear_stiffness: component.axial_stiffness().si(),
                curves,
                max_compression,
            });
        }
        Ok(Self {
            components,
            reference_temperature: reference.unwrap_or_else(default_reference_temperature),
        })
    }

    /// The largest compressive load of the outer strands, N: the cable's
    /// bimetallic behaviour and its curves' own limit, whichever is lower.
    pub fn outer_compression_limit(&self) -> f64 {
        self.components
            .first()
            .map_or(f64::INFINITY, |c| c.max_compression)
    }

    /// Temperature at which the total strain is measured.
    pub fn reference_temperature(&self) -> Temperature {
        self.reference_temperature
    }

    /// The own weight per metre of unstressed cable at `at` over the data
    /// sheet's at `reference`, in the additive strain form of the curves:
    /// \( (1 + \varepsilon(0, t_{ref})) / (1 + \varepsilon(0, t)) \), the
    /// zero-tension lengths of the initial state (owner decision
    /// 2026-10-21; [`crate::Cable::weight_factor_at`] is the
    /// multiplicative form's).
    pub fn weight_factor(
        &self,
        reference: Temperature,
        at: Temperature,
    ) -> Result<f64, CableError> {
        let zero = |t| self.strain(Force::ZERO, t, &MaterialState::Initial);
        Ok((1.0 + zero(reference)?) / (1.0 + zero(at)?))
    }

    /// Whether any component has non-linear curves.
    pub fn is_nonlinear(&self) -> bool {
        self.components.iter().any(|c| c.curves.is_some())
    }

    fn mechanical_strains(
        &self,
        strain: f64,
        temperature: Temperature,
    ) -> impl Iterator<Item = f64> + '_ {
        let dt = (temperature - self.reference_temperature).kelvin();
        self.components
            .iter()
            .map(move |c| strain - c.thermal_expansion * dt)
    }

    /// Cable tension at total strain `strain` and `temperature`.
    pub fn tension(&self, strain: f64, temperature: Temperature, state: &MaterialState) -> Force {
        Force::from_si(
            self.mechanical_strains(strain, temperature)
                .zip(&self.components)
                .enumerate()
                .map(|(k, (e, c))| c.load(e, state, k))
                .sum(),
        )
    }

    /// Total strain at which the cable carries `tension` at `temperature`.
    ///
    /// Tension increases with strain, so the strain is found by bracketing
    /// and bisection; 200 halvings is a fixed exit clause (far more than
    /// the ~55 needed). Where the tension stays at the target over a range
    /// of strain — zero tension when every component is limited in
    /// compression and all are slack — the strain is the range's upper
    /// end, where the cable starts to carry tension (2026-10-22: before,
    /// the bracket's lower end, an arbitrary −0.1 % for a zero tension).
    pub fn strain(
        &self,
        tension: Force,
        temperature: Temperature,
        state: &MaterialState,
    ) -> Result<f64, CableError> {
        let target = tension.si();
        let f = |e: f64| self.tension(e, temperature, state).si() - target;
        let (mut lo, mut hi) = (-1e-3, 1e-3);
        let mut guard = 0;
        while f(lo) > 0.0 || f(hi) <= 0.0 {
            guard += 1;
            if guard > 60 {
                return Err(CableError::Invalid {
                    id: String::new(),
                    message: format!(
                        "no strain gives a tension of {target:.3} N (curves not monotone?)"
                    ),
                });
            }
            if f(lo) > 0.0 {
                lo *= 2.0;
            }
            if f(hi) <= 0.0 {
                hi *= 2.0;
            }
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if f(mid) <= 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
            if hi - lo <= 1e-15 {
                break;
            }
        }
        Ok(0.5 * (lo + hi))
    }

    /// Permanent set left after the cable carried `tension` at `temperature`
    /// on the initial curve (`MaterialState::Initial`, "after load") or on the
    /// creep curve (`MaterialState::Creep`, "after creep").
    pub fn permanent_set(
        &self,
        tension: Force,
        temperature: Temperature,
        curve: &MaterialState,
    ) -> Result<MaterialState, CableError> {
        let strain = self.strain(tension, temperature, curve)?;
        let permanent = self
            .mechanical_strains(strain, temperature)
            .zip(&self.components)
            .enumerate()
            .map(|(k, (e, c))| match &c.curves {
                None => 0.0,
                // Signed: the final-modulus line through the point
                // reached (2026-10-22; clamped at zero before, which put
                // the line under the initial curve at that very point when
                // the initial curve rises more steeply than the final
                // modulus).
                Some(curves) => e - c.load(e, curve, k) / curves.final_stiffness,
            })
            .collect();
        Ok(MaterialState::Final { permanent })
    }

    /// The components whose mechanical strain at total strain `strain` and
    /// `temperature` lies beyond their curves' validity (`max_strain`,
    /// only where the cable gives one): label (`"outer"`, `"core"`), the strain and the
    /// limit (unit strain). Empty within the range, and for components
    /// without curves. The polynomial is evaluated beyond it all the same
    /// (no extrapolation is invented); the results warn.
    pub fn beyond_range(&self, strain: f64, temperature: Temperature) -> Vec<RangeExceeded> {
        self.mechanical_strains(strain, temperature)
            .zip(&self.components)
            .filter_map(|(e, c)| {
                let curves = c.curves.as_ref()?;
                let (max_strain, held) = match (curves.max_strain, curves.held_from()) {
                    (Some(m), _) => (m, false),
                    (None, Some(h)) => (h, true),
                    (None, None) => return None,
                };
                (e > max_strain).then_some(RangeExceeded {
                    component: c.label,
                    strain: e,
                    max_strain,
                    held,
                })
            })
            .collect()
    }

    /// Of two final states, the one that leaves the cable longer (larger
    /// strain at the working tension `at`, reference temperature; not at
    /// zero tension, where outer strands that take no compression are
    /// slack and only the core's set counts).
    pub fn longer(
        &self,
        a: MaterialState,
        b: MaterialState,
        at: Force,
    ) -> Result<MaterialState, CableError> {
        let t = self.reference_temperature;
        let (sa, sb) = (self.strain(at, t, &a)?, self.strain(at, t, &b)?);
        Ok(if sb > sa { b } else { a })
    }
}

/// The largest load, N, at which a cable's curves start (their constant
/// term): published polynomials often do not pass through zero, so the
/// tension jumps by that much near zero strain. `None`: they start at zero
/// (or the cable has no curves, or a linear model).
pub fn curve_offset(cable: &Cable) -> Option<f64> {
    if cable.model != crate::CableModel::Nonlinear {
        return None;
    }
    let total_area = cable.area().si();
    let mut worst: f64 = 0.0;
    for (label, component) in [("outer", Some(&cable.outer)), ("core", cable.core.as_ref())] {
        let Some(component) = component else { continue };
        let Some(data) = &component.stress_strain else {
            continue;
        };
        let Ok(c) = convert(cable, label, component, data, total_area) else {
            continue;
        };
        for curve in std::iter::once(&c.initial).chain(c.creep.as_ref()) {
            worst = worst.max(curve.first().map_or(0.0, |a| a.abs()));
        }
    }
    (worst > 1.0).then_some(worst)
}

/// One line on the cables whose curves stop rising before 1 % strain and
/// are held flat from there (no range given; owner, 2026-10-27), or
/// `None`.
pub fn curve_turn_warning(library: &crate::Library) -> Option<String> {
    let mut found = Vec::new();
    for cable in library.iter() {
        let Ok(m) = Mechanics::from_cable(cable) else {
            continue;
        };
        for c in &m.components {
            if let Some(h) = c.curves.as_ref().and_then(Curves::held_from) {
                found.push(format!("{} {} at {:.2} %", cable.id, c.label, h * 100.0));
            }
        }
    }
    (!found.is_empty()).then(|| {
        format!(
            "stress-strain curves that stop rising before 1 % strain, held flat from there \
             (conservative: the wire stretches more; a warning where a state goes beyond): {}",
            found.join(", ")
        )
    })
}

/// Everything worth saying about a loaded wire library, one line each:
/// the cables left out ([`crate::Library::skipped`]), the curves held
/// flat ([`curve_turn_warning`]), the curves not starting at zero
/// ([`curve_offset_warning`]).
pub fn library_warnings(library: &crate::Library) -> Vec<String> {
    let mut out: Vec<String> = library
        .skipped()
        .iter()
        .map(|s| format!("left out (the sections on it find no wire): {s}"))
        .collect();
    out.extend(curve_turn_warning(library));
    out.extend(curve_offset_warning(library));
    out
}

/// One line on the cables whose curves do not start at zero
/// ([`curve_offset`]), or `None`.
pub fn curve_offset_warning(library: &crate::Library) -> Option<String> {
    let found: Vec<(String, f64)> = library
        .iter()
        .filter_map(|c| curve_offset(c).map(|o| (c.id.clone(), o)))
        .collect();
    let worst = found.iter().map(|f| f.1).fold(0.0, f64::max);
    let ids: Vec<&str> = found.iter().map(|f| f.0.as_str()).collect();
    (!found.is_empty()).then(|| {
        format!(
            "stress-strain curves not starting at zero ({}): the tension jumps by up to {:.1} kN \
             near zero strain (hot cases); check the data",
            ids.join(", "),
            worst / 1000.0
        )
    })
}

fn invalid(cable: &Cable, message: &str) -> CableError {
    CableError::Invalid {
        id: cable.id.clone(),
        message: message.to_owned(),
    }
}

fn convert(
    cable: &Cable,
    label: &str,
    component: &Component,
    data: &StressStrainData,
    total_area: f64,
) -> Result<Curves, CableError> {
    let error = |m: String| CableError::Invalid {
        id: cable.id.clone(),
        message: format!("{label} stress-strain: {m}"),
    };
    let stress = <Stress as Quantity>::from_value(1.0, &data.stress_unit)
        .ok_or_else(|| error(format!("unknown stress unit \"{}\"", data.stress_unit)))?
        .si();
    let strain_scale: f64 = match data.strain {
        StrainUnit::Percent => 100.0,
        StrainUnit::Unit => 1.0,
    };
    let area = match data.basis {
        StressBasis::Component => component.area.si(),
        StressBasis::Total => total_area,
    };
    let to_load = |c: &[f64]| -> Vec<f64> {
        c.iter()
            .enumerate()
            .map(|(k, a)| stress * area * a * strain_scale.powi(k as i32))
            .collect()
    };
    if data.initial.len() < 2 {
        return Err(error(
            "the initial curve needs at least two coefficients".into(),
        ));
    }
    // The range given (none: no range warning, owner 2026-10-22). The
    // curves must rise over it, or up to 1 % strain when none is given,
    // whatever unit the coefficients use, or tension cannot be inverted.
    let given = data.max_strain.map(|m| m / strain_scale);
    let max_strain = given.unwrap_or(0.01);
    let mut curves = Curves {
        max_strain: given,
        flat: [None, None],
        initial: to_load(&data.initial),
        creep: data.creep.as_deref().map(to_load),
        final_stiffness: stress * area * data.final_modulus * strain_scale,
        max_compression: data
            .max_compression
            .map_or(f64::INFINITY, |m| stress * area * m.abs()),
    };
    if !(curves.final_stiffness.is_finite() && curves.final_stiffness > 0.0) {
        return Err(error("final modulus must be positive".into()));
    }
    // Curves must rise over their validity range, or tension cannot be
    // inverted to strain. Without a range given (every PLS-CADD export):
    // a curve that stops rising before 1 % is held flat from there (owner,
    // 2026-10-27) — near the top of a strong core's range, an aluminium
    // layer's fit may turn — and said ([`curve_turn_warning`]); from
    // zero it is refused.
    let mut flat = [None, None];
    for (i, (name, c)) in [
        ("initial", Some(&curves.initial)),
        ("creep", curves.creep.as_ref()),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(c) = c else { continue };
        let Some(k) = (0..=100).find(|&k| poly_derivative(c, max_strain * k as f64 / 100.0) <= 0.0)
        else {
            continue;
        };
        if given.is_some() || k == 0 {
            return Err(error(format!(
                "{name} curve stops rising at {:.4} % strain (inside its validity range)",
                max_strain * k as f64
            )));
        }
        // The turn between the last rising step and this one (bisection,
        // 60 halvings: far below any data's precision).
        let (mut lo, mut hi) = (
            max_strain * (k - 1) as f64 / 100.0,
            max_strain * k as f64 / 100.0,
        );
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if poly_derivative(c, mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        flat[i] = Some(lo);
    }
    curves.flat = flat;
    Ok(curves)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Library;
    use std::path::Path;

    /// Synthetic curves for testing (NOT real conductor data): an outer layer
    /// that stiffens with strain and creeps, and a linear-ish core.
    const SYNTHETIC: &str = r#"
        [[cable]]
        id = "synthetic"
        name = "Synthetic ACSR-like test cable"
        kind = "acsr"
        source = "Synthetic test data — not a real conductor"
        diameter = "28 mm"
        mass_per_length = "1.6 kg/m"
        rated_tensile_strength = "140 kN"

        [cable.outer]
        area = "400 mm²"
        modulus = "55 GPa"
        thermal_expansion = 23.0e-6
        [cable.outer.stress_strain]
        stress_unit = "MPa"
        initial = [0.0, 250.0, -110.0, 40.0, -8.0]
        creep = [0.0, 160.0, -60.0, 15.0, -2.0]
        final_modulus = 550.0
        max_compression = 0.0
        max_strain = 0.9

        [cable.core]
        area = "65 mm²"
        modulus = "190 GPa"
        thermal_expansion = 11.5e-6
        [cable.core.stress_strain]
        stress_unit = "MPa"
        initial = [0.0, 1800.0, -120.0, 30.0, -5.0]
        final_modulus = 1900.0
    "#;

    fn cable() -> Cable {
        let mut library = Library::default();
        library.add_toml(Path::new("t.toml"), SYNTHETIC).unwrap();
        library.get("synthetic").unwrap().clone()
    }

    fn t(c: f64) -> Temperature {
        Temperature::from_celsius(c)
    }

    #[test]
    fn conversion_from_percent_and_mpa() {
        let m = Mechanics::from_cable(&cable()).unwrap();
        // At 0.1 % strain, 20 °C (degree-4 curves):
        // outer 250·0.1 − 110·0.1² + 40·0.1³ − 8·0.1⁴ = 23.9392 MPa on 400 mm²,
        // core 1800·0.1 − 120·0.1² + 30·0.1³ − 5·0.1⁴ = 178.8295 MPa on 65 mm².
        let expected = 23.9392e6 * 400e-6 + 178.8295e6 * 65e-6;
        let got = m.tension(0.001, t(20.0), &MaterialState::Initial).newtons();
        assert!((got - expected).abs() < 1e-6, "{got} vs {expected}");
    }

    #[test]
    fn strain_inverts_tension() {
        let m = Mechanics::from_cable(&cable()).unwrap();
        for state in [MaterialState::Initial, MaterialState::Creep] {
            for kn in [5.0, 25.0, 60.0] {
                let e = m
                    .strain(Force::from_kilonewtons(kn), t(35.0), &state)
                    .unwrap();
                let back = m.tension(e, t(35.0), &state).kilonewtons();
                assert!((back - kn).abs() < 1e-9, "{state:?} {kn}: {back}");
            }
        }
    }

    #[test]
    fn creep_and_load_leave_permanent_stretch() {
        let m = Mechanics::from_cable(&cable()).unwrap();
        let initial_strain = m
            .strain(
                Force::from_kilonewtons(25.0),
                t(20.0),
                &MaterialState::Initial,
            )
            .unwrap();
        let after_creep = m
            .permanent_set(
                Force::from_kilonewtons(25.0),
                t(20.0),
                &MaterialState::Creep,
            )
            .unwrap();
        let after_load = m
            .permanent_set(
                Force::from_kilonewtons(60.0),
                t(-10.0),
                &MaterialState::Initial,
            )
            .unwrap();
        for state in [&after_creep, &after_load] {
            let final_strain = m
                .strain(Force::from_kilonewtons(25.0), t(20.0), state)
                .unwrap();
            assert!(
                final_strain > initial_strain,
                "{state:?}: {final_strain} <= {initial_strain}"
            );
        }
        // Reloading to the max load on the final curve returns to the same strain.
        let e_max = m
            .strain(
                Force::from_kilonewtons(60.0),
                t(-10.0),
                &MaterialState::Initial,
            )
            .unwrap();
        let e_reload = m
            .strain(Force::from_kilonewtons(60.0), t(-10.0), &after_load)
            .unwrap();
        assert!((e_max - e_reload).abs() < 1e-12, "{e_max} vs {e_reload}");
        let longer = m
            .longer(
                after_creep.clone(),
                after_load.clone(),
                Force::from_kilonewtons(25.0),
            )
            .unwrap();
        assert!(longer == after_creep || longer == after_load);
    }

    #[test]
    fn a_core_steeper_than_its_final_modulus_gets_a_negative_set() {
        // Core initial slope 2000 MPa/% > final 1900: P(e) > EA_f e near
        // zero, so the set e - P/EA_f is negative (2026-10-22: it was
        // clamped at zero, and the final line passed under the point
        // reached).
        let text = SYNTHETIC.replace(
            "initial = [0.0, 1800.0, -120.0, 30.0, -5.0]",
            "initial = [0.0, 2000.0, -120.0, 30.0, -5.0]",
        );
        let mut library = Library::default();
        library.add_toml(Path::new("t.ohlw"), &text).unwrap();
        let m = Mechanics::from_cable(library.get("synthetic").unwrap()).unwrap();
        let (load, cold) = (Force::from_kilonewtons(60.0), t(-10.0));
        let state = m
            .permanent_set(load, cold, &MaterialState::Initial)
            .unwrap();
        let MaterialState::Final { permanent } = &state else {
            panic!("{state:?}")
        };
        assert!(permanent[1] < 0.0, "{permanent:?}");
        // Reloaded to the load that left the set: the same strain (so the
        // final tension equals the initial at the heaviest case).
        let e_max = m.strain(load, cold, &MaterialState::Initial).unwrap();
        let e_reload = m.strain(load, cold, &state).unwrap();
        assert!((e_max - e_reload).abs() < 1e-12, "{e_max} vs {e_reload}");
        // Never above the initial state, and continuous at zero strain.
        for k in -20..=40 {
            let e = k as f64 * 1e-4;
            let fin = m.tension(e, t(20.0), &state).newtons();
            let ini = m.tension(e, t(20.0), &MaterialState::Initial).newtons();
            assert!(fin <= ini + 1e-9, "{e}: {fin} > {ini}");
        }
        let at = |e: f64| m.tension(e, t(20.0), &state).newtons();
        assert!((at(1e-12) - at(-1e-12)).abs() < 1e-3);
    }

    #[test]
    fn zero_tension_strain_of_a_slack_cable_is_where_it_starts_to_carry() {
        // Both components without compression: zero tension over a range
        // of strain; the strain is its upper end (the outer strands, whose
        // curve starts at zero, start to carry at zero mechanical strain).
        let text = SYNTHETIC.replace(
            "final_modulus = 1900.0",
            "final_modulus = 1900.0\n        max_compression = 0.0",
        );
        let mut library = Library::default();
        library.add_toml(Path::new("t.ohlw"), &text).unwrap();
        let m = Mechanics::from_cable(library.get("synthetic").unwrap()).unwrap();
        let hot = t(60.0);
        let e0 = m.strain(Force::ZERO, hot, &MaterialState::Initial).unwrap();
        // The core (11.5e-6 /K) starts first: at 40 K, 0.046 %.
        assert!((e0 - 11.5e-6 * 40.0).abs() < 1e-12, "{e0}");
    }

    #[test]
    fn strains_beyond_the_curves_range_are_reported() {
        let m = Mechanics::from_cable(&cable()).unwrap();
        // Outer valid to 0.9 %, the core without a range: never warned
        // (owner, 2026-10-22: no 1 % default).
        assert!(m.beyond_range(0.005, t(20.0)).is_empty());
        let beyond = m.beyond_range(0.0095, t(20.0));
        assert_eq!(beyond.len(), 1);
        assert_eq!(beyond[0].component, "outer");
        assert!((beyond[0].max_strain - 0.009).abs() < 1e-15);
        assert!(beyond[0].to_string().contains("beyond its curves' range"));
        // Cold: the mechanical strain is larger than the total; the core
        // (no range) at 2.4 % still not warned.
        assert_eq!(m.beyond_range(0.0095, t(-30.0)).len(), 1);
        assert_eq!(m.beyond_range(0.024, t(20.0)).len(), 1);
    }

    #[test]
    fn outer_unloads_at_high_temperature() {
        // With no aluminium compression, at high temperature and low tension
        // the core carries everything: the slope equals the core stiffness.
        let m = Mechanics::from_cable(&cable()).unwrap();
        let state = m
            .permanent_set(
                Force::from_kilonewtons(60.0),
                t(-10.0),
                &MaterialState::Initial,
            )
            .unwrap();
        let hot = t(150.0);
        let e = m.strain(Force::from_kilonewtons(5.0), hot, &state).unwrap();
        let slope = (m.tension(e + 1e-6, hot, &state) - m.tension(e, hot, &state)).newtons() / 1e-6;
        // Core final stiffness: 1900 MPa per % = 190 GPa, times 65 mm².
        assert!(
            (slope - 190e9 * 65e-6).abs() / slope < 1e-6,
            "slope {slope}"
        );
    }

    #[test]
    fn bimetallic_behavior_limits_the_outer_strands() {
        // Linear components: hot and slack, the outer strands would be in
        // compression. Compression (default): both carry (the composite
        // slope); bird cage: the core alone (its slope).
        let mut c = cable();
        c.outer.stress_strain = None;
        c.core.as_mut().unwrap().stress_strain = None;
        let hot = t(150.0);
        let slope = |c: &Cable| {
            let m = Mechanics::from_cable(c).unwrap();
            let state = MaterialState::Initial;
            let e = m.strain(Force::from_kilonewtons(2.0), hot, &state).unwrap();
            (m.tension(e + 1e-7, hot, &state) - m.tension(e, hot, &state)).newtons() / 1e-7
        };
        let core = c.core.as_ref().unwrap().axial_stiffness().newtons();
        let all = c.axial_stiffness().newtons();
        assert!((slope(&c) - all).abs() / all < 1e-6);
        c.bimetallic_behavior = Some(crate::BimetallicBehavior::BirdCage);
        assert!((slope(&c) - core).abs() / core < 1e-6);
        // A small limit: past it, the core alone again.
        c.bimetallic_behavior = Some(crate::BimetallicBehavior::Compression);
        c.max_virtual_compression = Some(Stress::from_megapascals(0.1));
        assert!((slope(&c) - core).abs() / core < 1e-6);
        // No core: never limited.
        c.core = None;
        assert_eq!(c.outer_compression_limit(), f64::INFINITY);
    }

    #[test]
    fn linear_cable_without_curves() {
        let mut c = cable();
        c.outer.stress_strain = None;
        c.core.as_mut().unwrap().stress_strain = None;
        let m = Mechanics::from_cable(&c).unwrap();
        assert!(!m.is_nonlinear());
        let e = m
            .strain(
                Force::from_kilonewtons(30.0),
                t(20.0),
                &MaterialState::Initial,
            )
            .unwrap();
        assert!((e - 30e3 / c.axial_stiffness().newtons()).abs() < 1e-15);
    }

    #[test]
    fn any_polynomial_degree_is_accepted() {
        // No degree cap: a degree-6 curve (a5 = a6 = 0 here, so same values).
        let text = SYNTHETIC.replace(
            "initial = [0.0, 1800.0, -120.0, 30.0, -5.0]",
            "initial = [0.0, 1800.0, -120.0, 30.0, -5.0, 0.0, 0.0]",
        );
        let mut library = Library::default();
        library.add_toml(Path::new("t.ohlw"), &text).unwrap();
        let six = Mechanics::from_cable(library.get("synthetic").unwrap()).unwrap();
        let four = Mechanics::from_cable(&cable()).unwrap();
        let state = MaterialState::Initial;
        assert_eq!(
            six.tension(0.004, t(20.0), &state),
            four.tension(0.004, t(20.0), &state)
        );
    }

    #[test]
    fn default_validity_is_one_percent_in_any_strain_unit() {
        // Same outer curve in unit strain: 250·100 e − 110·100² e² … MPa.
        // It turns over near 2.2 % — beyond the default 1 % range, so it is
        // accepted; with the old default (100 % in unit strain) it was rejected.
        let text = SYNTHETIC.replace(
            "initial = [0.0, 250.0, -110.0, 40.0, -8.0]",
            "strain = \"unit\"\n        initial = [0.0, 25000.0, -1100000.0, 40000000.0, -800000000.0]",
        )
        .replace("creep = [0.0, 160.0, -60.0, 15.0, -2.0]", "")
        .replace("final_modulus = 550.0", "final_modulus = 55000.0")
        .replace("max_strain = 0.9", "");
        let mut library = Library::default();
        library.add_toml(Path::new("t.ohlw"), &text).unwrap();
    }

    #[test]
    fn rejects_falling_curve() {
        let text = SYNTHETIC.replace(
            "initial = [0.0, 250.0, -110.0, 40.0, -8.0]",
            "initial = [0.0, 300.0, -400.0]",
        );
        // Inside a range the cable gives: left out, said.
        let mut library = Library::default();
        library.add_toml(Path::new("t.toml"), &text).unwrap();
        assert!(library.get("synthetic").is_none());
        assert!(
            library.skipped()[0].contains("stops rising"),
            "{:?}",
            library.skipped()
        );
    }

    /// Without a range given (as PLS-CADD exports): a curve that turns
    /// before 1 % is held flat from there (owner, 2026-10-27), said, and
    /// a state beyond it warned about.
    #[test]
    fn a_curve_turning_without_a_range_is_held_flat() {
        let text = SYNTHETIC
            .replace(
                "initial = [0.0, 250.0, -110.0, 40.0, -8.0]",
                "initial = [0.0, 300.0, -400.0]",
            )
            .replace("max_strain = 0.9\n", "");
        let mut library = Library::default();
        library.add_toml(Path::new("t.toml"), &text).unwrap();
        assert!(library.skipped().is_empty(), "{:?}", library.skipped());
        let w = curve_turn_warning(&library).unwrap();
        // 300 − 800 ε = 0 at ε = 0.375 %.
        assert!(
            w.contains("synthetic outer at 0.37 %") || w.contains("synthetic outer at 0.38 %"),
            "{w}"
        );
        let cable = library.get("synthetic").unwrap();
        let m = Mechanics::from_cable(cable).unwrap();
        let t = m.reference_temperature;
        // Beyond the turn the outer layer's load stays at its peak: the
        // load grows by the core alone.
        let peak = m.components[0].load(0.00375, &MaterialState::Initial, 0);
        let past = m.components[0].load(0.006, &MaterialState::Initial, 0);
        assert!(
            (peak - past).abs() < 1e-6 * peak.abs().max(1.0),
            "{peak} {past}"
        );
        let beyond = m.beyond_range(0.006, t);
        assert!(
            beyond.iter().any(|b| b.held && b.component == "outer"),
            "{beyond:?}"
        );
        assert!(beyond[0].to_string().contains("held flat"));
    }
}
