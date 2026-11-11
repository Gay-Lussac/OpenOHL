// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Creep at elevated temperature (roadmap O22 item 6): the **mechanism
//! only**. A wire type may give a law of the extra permanent strain its
//! strands take when held hot — a product of powers of the stress, of the
//! time and of a temperature factor, with coefficients that are **data of
//! the wire file** and its `source`.
//!
//! **No coefficient is built in, and none is shipped.** The published
//! predictor equations and their coefficients per conductor family are not
//! among the project's sources yet; until they are, every number of a law
//! is the user's, and every result computed with one says so and names the
//! `source` the user gave. The family of forms offered here is the one of
//! the design note of 2026-11-10; it is to be confirmed against the
//! sources before the format is frozen (unreleased until then). The tests
//! use invented coefficients, named as such.
//!
//! The law ([`HotCreep::strain`]), in microstrain:
//!
//! \\[ \varepsilon = k\\, s^{a}\\, t^{b}\\, g(T) \\]
//!
//! with \\( s \\) the stress (tension over the whole area, MPa, or the
//! tension in percent of the rated tensile strength), \\( t \\) the time in
//! hours, \\( 0 < b < 1 \\) (creep decelerates), and \\( g \\) the
//! temperature factor: \\( e^{c (T - T_{ref})} \\), \\( (T / T_{ref})^{c} \\)
//! (both in °C), or 1.
//!
//! Several stages (a time at one temperature, then at another) are joined
//! by **strain hardening** ([`HotCreep::after_stage`]): the creep rate
//! depends on the strain reached, so a stage starts at the *equivalent
//! time* at which its own stress and temperature would have given the
//! strain so far ([`HotCreep::equivalent_hours`]),
//! \\( t_{eq} = (\varepsilon / (k s^a g))^{1/b} \\), and runs on from
//! there. The result never exceeds the plain sum of the stages taken from
//! fresh.

use ohl_core::units::Temperature;
use serde::Deserialize;

/// What the stress of a law is measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StressMeasure {
    /// The tension over the cable's whole area, MPa.
    #[default]
    Mpa,
    /// The tension in percent of the rated tensile strength.
    PercentRts,
}

/// The temperature factor of a law.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureFactor {
    /// No dependence on temperature (1).
    #[default]
    None,
    /// \\( e^{c (T - T_{ref})} \\).
    Exp,
    /// \\( (T / T_{ref})^{c} \\), both in °C (above 0 °C).
    Power,
}

/// The form of a law (one so far).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotCreepForm {
    /// \\( \varepsilon = k s^a t^b g(T) \\).
    #[default]
    Power,
}

fn default_reference() -> Temperature {
    Temperature::from_celsius(20.0)
}

/// A wire type's law of creep at elevated temperature (`[cable.hot_creep]`
/// of a wire file, version 3): see the module's text. Every coefficient is
/// the user's, from `source`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotCreep {
    /// Where the coefficients come from (required, not empty): named with
    /// every result computed with them.
    pub source: String,
    /// The form of the law.
    #[serde(default)]
    pub form: HotCreepForm,
    /// What the stress is measured in.
    #[serde(default)]
    pub stress: StressMeasure,
    /// \\( k \\): microstrain at unit stress, one hour, \\( g = 1 \\).
    pub k: f64,
    /// \\( a \\): the stress exponent (≥ 0).
    pub stress_exponent: f64,
    /// \\( b \\): the time exponent, between 0 and 1.
    pub time_exponent: f64,
    /// The temperature factor \\( g \\).
    #[serde(default)]
    pub temperature: TemperatureFactor,
    /// \\( c \\): the temperature factor's coefficient (per K for `exp`,
    /// an exponent for `power`).
    #[serde(default)]
    pub temperature_coefficient: f64,
    /// \\( T_{ref} \\) of the temperature factor (default 20 °C).
    #[serde(default = "default_reference")]
    pub reference_temperature: Temperature,
    /// The law applies at or above this temperature; a stage below it
    /// adds nothing (and is said).
    pub above: Temperature,
}

impl HotCreep {
    /// What is wrong with the law (none: nothing).
    pub fn problem(&self) -> Option<String> {
        let finite = [
            self.k,
            self.stress_exponent,
            self.time_exponent,
            self.temperature_coefficient,
            self.reference_temperature.celsius(),
            self.above.celsius(),
        ]
        .iter()
        .all(|v| v.is_finite());
        if self.source.trim().is_empty() {
            return Some("hot_creep needs its source (where the coefficients come from)".into());
        }
        if !finite {
            return Some("hot_creep: every value must be a number".into());
        }
        if self.k < 0.0 || self.stress_exponent < 0.0 {
            return Some("hot_creep: k and stress_exponent must be ≥ 0".into());
        }
        if !(self.time_exponent > 0.0 && self.time_exponent < 1.0) {
            return Some("hot_creep: time_exponent must be between 0 and 1".into());
        }
        if self.temperature == TemperatureFactor::Power
            && !(self.reference_temperature.celsius() > 0.0 && self.above.celsius() > 0.0)
        {
            return Some(
                "hot_creep: temperature = \"power\" needs reference_temperature and above \
                 over 0 °C"
                    .into(),
            );
        }
        None
    }

    /// The stress of the law for a `tension` (N) on a cable of total
    /// `area` (m²) and rated tensile strength `rts` (N): MPa, or percent
    /// of the RTS (0 without an RTS).
    pub fn stress_of(&self, tension: f64, area: f64, rts: f64) -> f64 {
        match self.stress {
            StressMeasure::Mpa => tension / area / 1e6,
            StressMeasure::PercentRts if rts > 0.0 => 100.0 * tension / rts,
            StressMeasure::PercentRts => 0.0,
        }
    }

    /// Whether the law applies at `temperature` (at or above `above`).
    pub fn applies_at(&self, temperature: Temperature) -> bool {
        temperature.celsius() >= self.above.celsius()
    }

    /// \\( k\\, s^a\\, g(T) \\): the strain after one hour from fresh,
    /// microstrain (0 below `above`, at no stress or with \\( k = 0 \\)).
    fn rate(&self, stress: f64, temperature: Temperature) -> f64 {
        if !self.applies_at(temperature) || stress <= 0.0 {
            return 0.0;
        }
        let (t, t_ref) = (temperature.celsius(), self.reference_temperature.celsius());
        let g = match self.temperature {
            TemperatureFactor::None => 1.0,
            TemperatureFactor::Exp => (self.temperature_coefficient * (t - t_ref)).exp(),
            TemperatureFactor::Power => (t / t_ref).powf(self.temperature_coefficient),
        };
        self.k * stress.powf(self.stress_exponent) * g
    }

    /// The creep strain (unit strain) after `hours` at `stress` (the law's
    /// measure, [`HotCreep::stress_of`]) and `temperature`, from fresh.
    pub fn strain(&self, stress: f64, temperature: Temperature, hours: f64) -> f64 {
        1e-6 * self.rate(stress, temperature) * hours.max(0.0).powf(self.time_exponent)
    }

    /// The time (hours) at `stress` and `temperature` that gives `strain`
    /// (unit strain) from fresh: \\( (\varepsilon / (k s^a g))^{1/b} \\);
    /// 0 for no strain, infinite where the law gives none.
    pub fn equivalent_hours(&self, stress: f64, temperature: Temperature, strain: f64) -> f64 {
        if strain <= 0.0 {
            return 0.0;
        }
        let rate = self.rate(stress, temperature);
        if rate <= 0.0 {
            return f64::INFINITY;
        }
        (strain * 1e6 / rate).powf(1.0 / self.time_exponent)
    }

    /// The creep strain (unit strain) after a further stage of `hours` at
    /// `stress` and `temperature`, `so_far` being reached before: strain
    /// hardening by the equivalent time. Never below `so_far`; a stage the
    /// law gives nothing for (below `above`, no stress) leaves it.
    pub fn after_stage(
        &self,
        so_far: f64,
        stress: f64,
        temperature: Temperature,
        hours: f64,
    ) -> f64 {
        let start = self.equivalent_hours(stress, temperature, so_far);
        if !start.is_finite() {
            return so_far;
        }
        self.strain(stress, temperature, start + hours.max(0.0))
            .max(so_far)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVENTED coefficients, for the tests of the mechanism only — not a
    /// conductor's, not from any publication: k = 1, a = 1.3, b = 0.16,
    /// g = exp(0.02 (T − 20 °C)), from 90 °C.
    fn invented() -> HotCreep {
        toml::from_str(
            "source = \"INVENTED for the tests — not a conductor's law\"\n\
             k = 1.0\nstress_exponent = 1.3\ntime_exponent = 0.16\n\
             temperature = \"exp\"\ntemperature_coefficient = 0.02\n\
             above = \"90 °C\"\n",
        )
        .unwrap()
    }

    fn c(t: f64) -> Temperature {
        Temperature::from_celsius(t)
    }

    /// By hand, at 30 MPa held: 500 h at 100 °C give
    /// 1 × 30^1.3 × 500^0.16 × e^1.6 = 1114.2 µε; then 100 h at 150 °C
    /// start at the equivalent time 0.9652 h and reach f(150 °C, 100.9652
    /// h) = 2344.7 µε — under the plain sum of the two stages from fresh,
    /// 3455.3 µε.
    #[test]
    fn two_stages_by_hand() {
        let law = invented();
        assert_eq!(law.problem(), None);
        let first = law.strain(30.0, c(100.0), 500.0);
        let by_hand = 30f64.powf(1.3) * 500f64.powf(0.16) * 1.6f64.exp();
        assert!((first * 1e6 - by_hand).abs() < 1e-9);
        assert!((first * 1e6 - 1114.2).abs() < 0.05, "{}", first * 1e6);
        let t_eq = law.equivalent_hours(30.0, c(150.0), first);
        assert!((t_eq - 0.9652).abs() < 5e-5, "{t_eq}");
        let both = law.after_stage(first, 30.0, c(150.0), 100.0);
        assert!((both * 1e6 - 2344.7).abs() < 0.05, "{}", both * 1e6);
        let fresh_sum = first + law.strain(30.0, c(150.0), 100.0);
        assert!(
            (fresh_sum * 1e6 - 3455.3).abs() < 0.05,
            "{}",
            fresh_sum * 1e6
        );
        assert!(both < fresh_sum && both > first);
        // The equivalent time gives the strain back.
        assert!((law.strain(30.0, c(150.0), t_eq) - first).abs() < 1e-15);
        // One stage in two halves is the stage.
        let half = law.after_stage(0.0, 30.0, c(100.0), 250.0);
        let whole = law.after_stage(half, 30.0, c(100.0), 250.0);
        assert!((whole - first).abs() < 1e-15);
    }

    /// Below `above`, without stress or without time the law adds nothing
    /// and keeps what was reached; the stress in MPa or in percent of the
    /// RTS; the power form of the temperature factor.
    #[test]
    fn where_the_law_adds_nothing() {
        let law = invented();
        assert_eq!(law.strain(30.0, c(80.0), 500.0), 0.0);
        assert_eq!(law.strain(0.0, c(100.0), 500.0), 0.0);
        assert_eq!(law.after_stage(1e-3, 30.0, c(80.0), 500.0), 1e-3);
        assert_eq!(law.after_stage(1e-3, 30.0, c(100.0), 0.0), 1e-3);
        assert!(law.applies_at(c(90.0)) && !law.applies_at(c(89.9)));
        assert!((law.stress_of(15_000.0, 500e-6, 120e3) - 30.0).abs() < 1e-12);
        let mut percent = invented();
        percent.stress = StressMeasure::PercentRts;
        assert!((percent.stress_of(15_000.0, 500e-6, 120e3) - 12.5).abs() < 1e-12);
        assert_eq!(percent.stress_of(15_000.0, 500e-6, 0.0), 0.0);
        let mut power = invented();
        power.temperature = TemperatureFactor::Power;
        power.temperature_coefficient = 2.0;
        // (100 / 20)^2 = 25 times the rate at the reference temperature.
        let none = HotCreep {
            temperature: TemperatureFactor::None,
            ..invented()
        };
        let ratio = power.strain(30.0, c(100.0), 10.0) / none.strain(30.0, c(100.0), 10.0);
        assert!((ratio - 25.0).abs() < 1e-12, "{ratio}");
    }

    /// A law must name its source and keep its exponents in range.
    #[test]
    fn a_law_is_checked() {
        let mut law = invented();
        law.source = "  ".into();
        assert!(law.problem().unwrap().contains("source"));
        for b in [0.0, 1.0, -0.2, f64::NAN] {
            let law = HotCreep {
                time_exponent: b,
                ..invented()
            };
            assert!(law.problem().is_some(), "{b}");
        }
        let law = HotCreep {
            k: -1.0,
            ..invented()
        };
        assert!(law.problem().unwrap().contains("≥ 0"));
        let law = HotCreep {
            temperature: TemperatureFactor::Power,
            reference_temperature: c(0.0),
            ..invented()
        };
        assert!(law.problem().unwrap().contains("over 0 °C"));
        // An unknown key is refused; the source and `above` are required.
        assert!(toml::from_str::<HotCreep>("source = \"x\"\nk = 1.0\n").is_err());
        let e = toml::from_str::<HotCreep>(
            "source = \"x\"\nk = 1.0\nstress_exponent = 1.0\ntime_exponent = 0.2\n\
             above = \"90 °C\"\nrate = 3\n",
        )
        .unwrap_err();
        assert!(e.to_string().contains("unknown field"), "{e}");
    }
}
