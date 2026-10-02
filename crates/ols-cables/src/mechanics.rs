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
//! Coefficients are data supplied with the cable (see `docs/theory`); OLS CAD
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
            return self.linear_stiffness * strain;
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
        load.max(-c.max_compression)
    }
}

impl Mechanics {
    /// Builds the model from a cable. Components without stress-strain data
    /// are linear with their modulus.
    pub fn from_cable(cable: &Cable) -> Result<Self, CableError> {
        let total_area = cable.area().si();
        let mut reference: Option<Temperature> = None;
        let mut components = Vec::new();
        for (label, component) in [("outer", Some(&cable.outer)), ("core", cable.core.as_ref())] {
            let Some(component) = component else { continue };
            let curves = match &component.stress_strain {
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
            components.push(ComponentMechanics {
                thermal_expansion: component.thermal_expansion,
                linear_stiffness: component.axial_stiffness().si(),
                curves,
            });
        }
        Ok(Self {
            components,
            reference_temperature: reference.unwrap_or_else(default_reference_temperature),
        })
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
    /// strain at zero tension at the reference temperature).
    pub fn longer(&self, a: MaterialState, b: MaterialState) -> Result<MaterialState, CableError> {
        let t = self.reference_temperature;
        let (sa, sb) = (
            self.strain(Force::ZERO, t, &a)?,
            self.strain(Force::ZERO, t, &b)?,
        );
        Ok(if sb > sa { b } else { a })
    }
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
    let max_strain = data.max_strain.unwrap_or(1.0) / strain_scale;
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
        initial = [0.0, 300.0, -150.0]
        creep = [0.0, 200.0, -100.0]
        final_modulus = 550.0
        max_compression = 0.0
        max_strain = 0.9

        [cable.core]
        area = "65 mm²"
        modulus = "190 GPa"
        thermal_expansion = 11.5e-6
        [cable.core.stress_strain]
        stress_unit = "MPa"
        initial = [0.0, 1800.0, -100.0]
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
        // At 0.1 % strain, 20 °C: outer 300·0.1 − 150·0.01 = 28.5 MPa on 400 mm²,
        // core 1800·0.1 − 100·0.01 = 179 MPa on 65 mm².
        let expected = 28.5e6 * 400e-6 + 179e6 * 65e-6;
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
        let longer = m.longer(after_creep.clone(), after_load.clone()).unwrap();
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
    fn rejects_falling_curve() {
        let text = SYNTHETIC.replace(
            "initial = [0.0, 300.0, -150.0]",
            "initial = [0.0, 300.0, -400.0]",
        );
        let mut library = Library::default();
        let error = library.add_toml(Path::new("t.toml"), &text).unwrap_err();
        assert!(error.to_string().contains("stops rising"), "{error}");
    }
}
