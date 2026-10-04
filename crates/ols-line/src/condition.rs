// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cable conditions and calculation methods of a section.
//!
//! **Conditions** — the cable as strung (*initial*), after its permanent
//! stretch due to *creep* (years at an everyday weather case), after its
//! permanent stretch due to the heaviest *load* (a short exposure to the
//! worst of the load weather cases), or whichever of the two sags more in
//! the weather case solved (*max sag*). The weather cases come from the
//! line's [`CreepStretch`] criteria.
//!
//! **Methods** — *FE*: the section's finite-element model (suspension
//! strings swing, each span its own tension); *RS*: the ruling-span
//! assumption (one horizontal tension for the whole section, every span a
//! catenary between its clamps held where they hang in still air, blown
//! out of the vertical plane by the wind).

use serde::{Deserialize, Serialize};

/// A cable condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Condition {
    /// As strung.
    Initial,
    /// After the permanent stretch due to creep.
    Creep,
    /// After the permanent stretch due to the heaviest load.
    Load,
    /// After creep or after load, whichever sags more in the case solved.
    MaxSag,
}

/// A calculation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// Ruling span: one horizontal tension, clamps held.
    Rs,
    /// Finite elements: strings swing, tensions per span.
    Fe,
}

/// How a section is solved: a condition and a method (8 choices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Calculation {
    /// Initial, ruling span.
    InitialRs,
    /// After creep, ruling span.
    CreepRs,
    /// After load, ruling span.
    LoadRs,
    /// Max sag of creep and load, ruling span.
    MaxSagRs,
    /// Initial, finite elements (the default).
    #[default]
    InitialFe,
    /// After creep, finite elements.
    CreepFe,
    /// After load, finite elements.
    LoadFe,
    /// Max sag of creep and load, finite elements.
    MaxSagFe,
}

impl Calculation {
    /// Every calculation, in the order of the lists.
    pub const ALL: [Calculation; 8] = [
        Calculation::InitialRs,
        Calculation::CreepRs,
        Calculation::LoadRs,
        Calculation::MaxSagRs,
        Calculation::InitialFe,
        Calculation::CreepFe,
        Calculation::LoadFe,
        Calculation::MaxSagFe,
    ];

    /// Whether it is the default (Initial FE).
    pub fn is_default(&self) -> bool {
        *self == Calculation::default()
    }

    /// Its condition.
    pub fn condition(self) -> Condition {
        match self {
            Calculation::InitialRs | Calculation::InitialFe => Condition::Initial,
            Calculation::CreepRs | Calculation::CreepFe => Condition::Creep,
            Calculation::LoadRs | Calculation::LoadFe => Condition::Load,
            Calculation::MaxSagRs | Calculation::MaxSagFe => Condition::MaxSag,
        }
    }

    /// Its method.
    pub fn method(self) -> Method {
        match self {
            Calculation::InitialRs
            | Calculation::CreepRs
            | Calculation::LoadRs
            | Calculation::MaxSagRs => Method::Rs,
            _ => Method::Fe,
        }
    }

    /// Its name in lists, e.g. `Creep RS`.
    pub fn label(self) -> &'static str {
        match self {
            Calculation::InitialRs => "Initial RS",
            Calculation::CreepRs => "Creep RS",
            Calculation::LoadRs => "Load RS",
            Calculation::MaxSagRs => "Max Sag RS",
            Calculation::InitialFe => "Initial FE",
            Calculation::CreepFe => "Creep FE",
            Calculation::LoadFe => "Load FE",
            Calculation::MaxSagFe => "Max Sag FE",
        }
    }

    /// From its name (case and spaces ignored).
    pub fn parse(text: &str) -> Option<Self> {
        let key = |t: &str| t.to_lowercase().replace([' ', '-', '_'], "");
        let k = key(text);
        Self::ALL.into_iter().find(|c| key(c.label()) == k)
    }
}

/// Weather cases for the permanent stretch due to creep and load, by name
/// (Criteria › Creep-Stretch).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreepStretch {
    /// Case for the final after creep: normally an everyday case the wires
    /// see over long periods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creep_case: Option<String>,
    /// Cases for the final after load: the one giving the largest load
    /// (hence the most stretch) is used, for each wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_cases: Vec<String>,
}

impl CreepStretch {
    /// Whether nothing is chosen.
    pub fn is_empty(&self) -> bool {
        self.creep_case.is_none() && self.load_cases.is_empty()
    }
}

/// The weather cases of the [`CreepStretch`] criteria, as given by the
/// program solving the line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConditionCases {
    /// For the final after creep.
    pub creep: Option<ols_weather::WeatherCase>,
    /// For the final after load.
    pub loads: Vec<ols_weather::WeatherCase>,
}

impl crate::Line {
    /// Gives the line the weather cases its creep-stretch criteria name
    /// (from `cases`); names not found are left out (said by
    /// [`crate::Line::missing_condition_cases`]).
    pub fn set_weather_cases(&mut self, cases: &[ols_weather::WeatherCase]) {
        let find = |name: &str| cases.iter().find(|c| c.name == name).cloned();
        self.condition_cases = ConditionCases {
            creep: self.creep_stretch.creep_case.as_deref().and_then(find),
            loads: self
                .creep_stretch
                .load_cases
                .iter()
                .filter_map(|n| find(n))
                .collect(),
        };
    }

    /// Names of the criteria not among the cases given.
    pub fn missing_condition_cases(&self) -> Vec<String> {
        let c = &self.condition_cases;
        let mut out = Vec::new();
        if let Some(n) = &self.creep_stretch.creep_case
            && c.creep.as_ref().is_none_or(|w| &w.name != n)
        {
            out.push(n.clone());
        }
        for n in &self.creep_stretch.load_cases {
            if !c.loads.iter().any(|w| &w.name == n) {
                out.push(n.clone());
            }
        }
        out
    }
}
