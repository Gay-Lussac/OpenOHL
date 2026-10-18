// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
//!   permanent set, and back onto the initial curve if loaded beyond.
//!
//! Components without curves are linear (\\( P = EA\,\varepsilon \\)) in every state.
//! Coefficients are data supplied with the cable (see `docs/theory`); OpenOHL
//! ships no manufacturer curves.

use ols_core::units::{Force, Quantity, Stress, Temperature};
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
    /// Upper end of the curves' validity, in strain units (default 1 %).
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

#[derive(Debug, Clone, PartialEq)]
struct ComponentMechanics {
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
                    poly(&c.initial, strain)
                } else {
                    elastic_from(0.0)
                }
            }
            MaterialState::Creep => {
                let curve = c.creep.as_ref().unwrap_or(&c.initial);
                if strain >= 0.0 {
                    poly(curve, strain)
                } else {
                    elastic_from(0.0)
                }
            }
            MaterialState::Final { permanent } => {
                let p = permanent[index];
                let line = elastic_from(p);
                if strain >= 0.0 {
                    line.min(poly(&c.initial, strain))
                } else {
                    line
                }
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
    /// Tension increases with strain (the core is never limited in
    /// compression), so the strain is found by bracketing and bisection;
    /// 200 halvings is a fixed exit clause (far more than the ~55 needed).
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
        while f(lo) > 0.0 || f(hi) < 0.0 {
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
            if f(hi) < 0.0 {
                hi *= 2.0;
            }
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if f(mid) < 0.0 {
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
                Some(curves) => (e - c.load(e, curve, k) / curves.final_stiffness).max(0.0),
            })
            .collect();
        Ok(MaterialState::Final { permanent })
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
    let curves = Curves {
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
    // inverted to strain.
    // Default validity: 1 % strain, whatever unit the coefficients use (the
    // default used to be applied before the unit conversion, i.e. 100 % strain
    // for `strain = "unit"`).
    let max_strain = data.max_strain.map_or(0.01, |m| m / strain_scale);
    for (name, c) in [
        ("initial", Some(&curves.initial)),
        ("creep", curves.creep.as_ref()),
    ] {
        let Some(c) = c else { continue };
        if let Some(k) =
            (0..=100).find(|&k| poly_derivative(c, max_strain * k as f64 / 100.0) <= 0.0)
        {
            return Err(error(format!(
                "{name} curve stops rising at {:.4} % strain (inside its validity range)",
                max_strain * k as f64
            )));
        }
    }
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
        let mut library = Library::default();
        let error = library.add_toml(Path::new("t.toml"), &text).unwrap_err();
        assert!(error.to_string().contains("stops rising"), "{error}");
    }
}
