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

impl Condition {
    /// Every condition, in the order of the lists.
    pub const ALL: [Condition; 4] = [
        Condition::Initial,
        Condition::Creep,
        Condition::Load,
        Condition::MaxSag,
    ];

    /// Its name in files (`max-sag`) and in lists (`Max Sag`).
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Condition::Initial => ("initial", "Initial"),
            Condition::Creep => ("creep", "Creep"),
            Condition::Load => ("load", "Load"),
            Condition::MaxSag => ("max-sag", "Max Sag"),
        }
    }
}

impl Method {
    /// Every method, in the order of the lists.
    pub const ALL: [Method; 2] = [Method::Rs, Method::Fe];

    /// Its name in files (`fe`) and in lists (`FE`).
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Method::Rs => ("rs", "RS"),
            Method::Fe => ("fe", "FE"),
        }
    }
}

/// How a section is solved: a condition and a method. Written by its name,
/// condition and method joined (`creep-fe`, `max-sag-rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Calculation {
    /// The cable's condition.
    pub condition: Condition,
    /// The method.
    pub method: Method,
}

impl Default for Calculation {
    /// Initial, finite elements.
    fn default() -> Self {
        Self::INITIAL_FE
    }
}

impl Calculation {
    /// Initial, ruling span.
    pub const INITIAL_RS: Self = Self::of(Condition::Initial, Method::Rs);
    /// After creep, ruling span.
    pub const CREEP_RS: Self = Self::of(Condition::Creep, Method::Rs);
    /// After load, ruling span.
    pub const LOAD_RS: Self = Self::of(Condition::Load, Method::Rs);
    /// Max sag of creep and load, ruling span.
    pub const MAX_SAG_RS: Self = Self::of(Condition::MaxSag, Method::Rs);
    /// Initial, finite elements (the default).
    pub const INITIAL_FE: Self = Self::of(Condition::Initial, Method::Fe);
    /// After creep, finite elements.
    pub const CREEP_FE: Self = Self::of(Condition::Creep, Method::Fe);
    /// After load, finite elements.
    pub const LOAD_FE: Self = Self::of(Condition::Load, Method::Fe);
    /// Max sag of creep and load, finite elements.
    pub const MAX_SAG_FE: Self = Self::of(Condition::MaxSag, Method::Fe);

    /// Every calculation, in the order of the lists (by method, then
    /// condition).
    pub const ALL: [Calculation; 8] = [
        Self::INITIAL_RS,
        Self::CREEP_RS,
        Self::LOAD_RS,
        Self::MAX_SAG_RS,
        Self::INITIAL_FE,
        Self::CREEP_FE,
        Self::LOAD_FE,
        Self::MAX_SAG_FE,
    ];

    /// `condition` by `method`.
    pub const fn of(condition: Condition, method: Method) -> Self {
        Self { condition, method }
    }

    /// Whether it is the default (Initial FE).
    pub fn is_default(&self) -> bool {
        *self == Calculation::default()
    }

    /// Its condition.
    pub fn condition(self) -> Condition {
        self.condition
    }

    /// Its method.
    pub fn method(self) -> Method {
        self.method
    }

    /// Its name in lists, e.g. `Creep RS`.
    pub fn label(self) -> String {
        format!("{} {}", self.condition.names().1, self.method.names().1)
    }

    /// Its name in files, e.g. `creep-rs`.
    pub fn name(self) -> String {
        format!("{}-{}", self.condition.names().0, self.method.names().0)
    }

    /// From its name or label (case, spaces, `-` and `_` ignored).
    pub fn parse(text: &str) -> Option<Self> {
        let key = |t: &str| t.to_lowercase().replace([' ', '-', '_'], "");
        let k = key(text);
        Self::ALL.into_iter().find(|c| key(&c.label()) == k)
    }
}

impl TryFrom<String> for Calculation {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        Self::parse(&text).ok_or_else(|| format!("calculation \"{text}\" unknown"))
    }
}

impl From<Calculation> for String {
    fn from(c: Calculation) -> String {
        c.name()
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
