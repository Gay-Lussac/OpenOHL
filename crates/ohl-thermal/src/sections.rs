// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The conductor temperature of a line's sections for the current their
//! circuits carry (2026-11-04): per section, its phase current — given,
//! or its system's operating current from the line file
//! ([`ohl_line::electrical::System::current_a`]) — shared by the
//! subconductors of its bundle, and the steady temperature of one
//! subconductor at that share ([`crate::temperature`]) in each ambient
//! case. Earthed sections (earth wires, OPGW) carry no operating current
//! and get none. The formulas are those of [`crate::temperature`]; only
//! the current per subconductor is found here.

use std::collections::BTreeSet;

use ohl_cables::Library;
use ohl_line::Line;

use crate::{Ambient, Conductor, HeatBalance, Method};

/// Where a section's current comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Current {
    /// Each phase section's system's operating current (line file); a
    /// system without one: none.
    Line,
    /// This current, A rms per phase, in every phase section.
    Given(f64),
}

/// One section's steady temperature at its current.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionHeat {
    /// Section id.
    pub section: String,
    /// Its wire type.
    pub cable: String,
    /// Subconductors.
    pub bundle: u32,
    /// The phase current, A rms, or why there is none (earthed, its
    /// system gives none).
    pub current: Result<f64, String>,
    /// Per ambient case: the heat balance of one subconductor at its
    /// steady temperature, or why there is none.
    pub cases: Vec<Result<HeatBalance, String>>,
}

impl SectionHeat {
    /// The current per subconductor, A.
    pub fn per_wire(&self) -> Option<f64> {
        self.current
            .as_ref()
            .ok()
            .map(|i| i / f64::from(self.bundle.max(1)))
    }

    /// The hottest ambient case: its index and the temperature, °C.
    pub fn hottest(&self) -> Option<(usize, f64)> {
        self.cases
            .iter()
            .enumerate()
            .filter_map(|(k, b)| b.as_ref().ok().map(|b| (k, b.temperature)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The temperature in ambient case `k`, °C.
    pub fn in_case(&self, k: usize) -> Option<f64> {
        self.cases.get(k)?.as_ref().ok().map(|b| b.temperature)
    }
}

/// The conductor of wire type `cable` for the thermal rating (the line's
/// default values where the wire type gives none), or why there is none.
pub fn conductor_of(line: &Line, library: &Library, cable: &str) -> Result<Conductor, String> {
    let c = library
        .get(cable)
        .ok_or_else(|| format!("wire type {cable} not in the library"))?;
    Conductor::from_cable(c)
        .map(|c| c.with_defaults(&line.defaults))
        .map_err(|e| e.to_string())
}

/// The steady temperature of each section of `line` (those in `only`;
/// `None`: every one) carrying `current` in each of the `ambient` cases,
/// by `method`. Microseconds per section and case.
pub fn section_temperatures(
    line: &Line,
    library: &Library,
    method: Method,
    ambient: &[Ambient],
    current: Current,
    only: Option<&BTreeSet<String>>,
) -> Vec<SectionHeat> {
    let phases = line.section_phases().unwrap_or_default();
    line.sections
        .iter()
        .filter(|s| only.is_none_or(|o| o.contains(&s.id)))
        .map(|s| {
            let system = phases.get(&s.id).copied().flatten().map(|(k, _)| k);
            let current = match (system, current) {
                (None, _) => Err("earthed: no operating current".to_owned()),
                (Some(_), Current::Given(i)) => Ok(i),
                (Some(k), Current::Line) => line.systems[k].current_a.ok_or_else(|| {
                    format!(
                        "system {} gives no operating current (Line › Electrical systems, \"A\")",
                        line.systems[k].id
                    )
                }),
            };
            let bundle = s.bundle.max(1);
            let conductor = conductor_of(line, library, &s.cable);
            let cases = ambient
                .iter()
                .map(|a| {
                    let i = current.as_ref().map_err(Clone::clone)?;
                    let c = conductor.as_ref().map_err(Clone::clone)?;
                    crate::temperature(
                        method,
                        c,
                        a,
                        i / f64::from(bundle),
                        &crate::default_steady_budget(),
                    )
                    .map_err(|e| e.to_string())
                })
                .collect();
            SectionHeat {
                section: s.id.clone(),
                cable: s.cable.clone(),
                bundle,
                current,
                cases,
            }
        })
        .collect()
}

/// What the thermal results of `line` leave out, not on the safe side
/// (2026-11-08): for the sections carrying current, one uniform
/// temperature in the conductor and along the section; for those of
/// steel-cored conductors (ACSR, ACSS), the core's magnetic heating
/// beyond what the AC resistance holds. Empty: no section carries
/// current.
pub fn not_safe_notes(line: &Line, library: &Library) -> Vec<String> {
    let potentials = line.potentials().unwrap_or_default();
    let live: Vec<&ohl_line::Section> = line
        .sections
        .iter()
        .filter(|s| potentials.get(&s.id).is_some_and(Option::is_some))
        .collect();
    if live.is_empty() {
        return Vec::new();
    }
    let mut notes = vec![
        "NOT ON THE SAFE SIDE: one uniform temperature in the conductor — at a high current \
         its core runs hotter than its surface and the sag is larger than at the temperature \
         given"
            .to_owned(),
        "NOT ON THE SAFE SIDE: the same temperature along each section — a span sheltered from \
         the wind runs hotter"
            .to_owned(),
    ];
    let steel: BTreeSet<&str> = live
        .iter()
        .filter(|s| {
            library.get(&s.cable).is_some_and(|c| {
                matches!(
                    c.kind,
                    ohl_cables::CableKind::Acsr | ohl_cables::CableKind::Acss
                )
            })
        })
        .map(|s| s.id.as_str())
        .collect();
    if !steel.is_empty() {
        let mut first: Vec<&str> = steel.iter().copied().take(10).collect();
        if steel.len() > 10 {
            first.push("…");
        }
        notes.push(format!(
            "NOT ON THE SAFE SIDE: the steel core's magnetic heating beyond what the AC \
             resistance holds not counted ({} section(s) of steel-cored conductors: {})",
            steel.len(),
            first.join(", ")
        ));
    }
    notes
}

/// [`not_safe_notes`] for one wire type rated alone: its uniform
/// temperature, and the steel core's magnetic heating when it is
/// steel-cored (ACSR, ACSS).
pub fn cable_notes(cable: &ohl_cables::Cable) -> Vec<String> {
    let mut notes = vec![
        "NOT ON THE SAFE SIDE: one uniform temperature in the conductor — at a high current \
         its core runs hotter than its surface and the sag is larger than at the temperature \
         given"
            .to_owned(),
    ];
    if matches!(
        cable.kind,
        ohl_cables::CableKind::Acsr | ohl_cables::CableKind::Acss
    ) {
        notes.push(
            "NOT ON THE SAFE SIDE: the steel core's magnetic heating beyond what the AC \
             resistance holds not counted"
                .to_owned(),
        );
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn the_line_current_or_a_given_one() {
        let ex = root().join("examples/synthetic-line");
        let mut line = Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
        let library = Library::load(&root().join("libraries/cables")).unwrap();
        let ambient = crate::AmbientCases::load(&ex.join("thermal-ambient.ohlta"))
            .unwrap()
            .cases;
        // No current in the line: none, said why.
        let none = section_temperatures(
            &line,
            &library,
            Method::Ieee738,
            &ambient,
            Current::Line,
            None,
        );
        assert_eq!(none.len(), line.sections.len());
        assert!(
            none.iter()
                .all(|s| s.current.is_err() && s.hottest().is_none())
        );
        // The system's current: every phase section, every case.
        line.systems[0].current_a = Some(900.0);
        let from_line = section_temperatures(
            &line,
            &library,
            Method::Ieee738,
            &ambient,
            Current::Line,
            None,
        );
        let given = section_temperatures(
            &line,
            &library,
            Method::Ieee738,
            &ambient,
            Current::Given(900.0),
            None,
        );
        assert_eq!(from_line, given);
        let s1 = &from_line[0];
        assert_eq!(s1.current, Ok(900.0));
        let (k, t) = s1.hottest().unwrap();
        // The same as the conductor alone at 900 A per subconductor.
        let c = conductor_of(&line, &library, &s1.cable).unwrap();
        let b = crate::temperature(
            Method::Ieee738,
            &c,
            &ambient[k],
            900.0 / f64::from(s1.bundle),
            &crate::default_steady_budget(),
        )
        .unwrap();
        assert!((b.temperature - t).abs() < 1e-9);
        // The hottest is the hottest.
        assert!(s1.cases.iter().flatten().all(|b| b.temperature <= t));
        // A bundle of two shares it: cooler.
        line.sections[0].bundle = 2;
        let two = section_temperatures(
            &line,
            &library,
            Method::Ieee738,
            &ambient,
            Current::Given(900.0),
            None,
        );
        assert_eq!(two[0].per_wire(), Some(450.0));
        assert!(two[0].hottest().unwrap().1 < t);
        // Only some sections.
        let only: BTreeSet<String> = ["S2".to_owned()].into();
        let one = section_temperatures(
            &line,
            &library,
            Method::Ieee738,
            &ambient,
            Current::Line,
            Some(&only),
        );
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].section, "S2");
    }
}
