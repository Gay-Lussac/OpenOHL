// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The electrical model of a line, kept apart from the physical wires.
//!
//! - A [`System`] is a group of phases (AC) or poles (DC) with its nominal
//!   voltage: a three-phase system L1/L2/L3, a two-phase railway system
//!   (16.7 Hz, phases at 0° and 180°), a DC bipole (+/−). Systems are
//!   listed once in the line file (`systems`).
//! - A **conductor path** is a set of sections joined electrically: by
//!   jumpers, or by sharing an attachment point ([`conductor_paths`]).
//! - A path is assigned to one phase of one system by giving `system` on
//!   any of its sections — the section's `phase_label` names the phase.
//!   The other sections of the path follow; contradicting assignments are
//!   errors. Paths without a system are earthed (earth wires, OPGW).
//!
//! Each section then has a [`Potential`]: the system's nominal voltage and
//! its phase's voltage to earth as a phasor (AC) or a signed value (DC),
//! from which the voltage between any two conductors follows
//! ([`Potential::between`]).
//!
//! Voltages:
//!
//! - AC: `voltage_kv` is the nominal voltage between phases (rms). With
//!   `n` phases evenly spread, the voltage to earth is
//!   `U / (2·sin(π/n))` — `U/√3` for three phases, `U/2` for two; a single
//!   phase (to earth) has `U`. Angles default to `0, −360/n, −2·360/n, …`
//!   (two phases: 0°, 180°) and can be set per phase.
//! - DC: `voltage_kv` is the pole voltage to earth; poles have a polarity
//!   (+1, −1, 0 for a metallic return), by default +1, −1, 0.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Line, LineError};

/// AC or DC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SystemKind {
    /// Alternating current.
    #[default]
    Ac,
    /// Direct current.
    Dc,
}

/// A phase (AC) or pole (DC) of a system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPhase {
    /// Label, matched with the sections' `phase_label` (e.g. `L1`, `UU`,
    /// `+`).
    pub label: String,
    /// AC: phase angle, degrees (default: evenly spread from 0°).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_deg: Option<f64>,
    /// DC: polarity +1, −1, or 0 for a metallic return (default: +1, −1,
    /// 0 in order).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polarity: Option<i8>,
}

/// An electrical system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct System {
    /// Unique id, referenced by sections.
    pub id: String,
    /// Name, e.g. `380 kV Alpha–Beta 1`.
    #[serde(default)]
    pub name: String,
    /// AC (default) or DC.
    #[serde(default)]
    pub kind: SystemKind,
    /// Nominal voltage, kV: AC between phases (rms), DC pole to earth.
    pub voltage_kv: f64,
    /// AC frequency, Hz (e.g. 50, 16.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_hz: Option<f64>,
    /// Phases (AC) or poles (DC).
    pub phases: Vec<SystemPhase>,
}

impl System {
    /// Voltage of phase `k` to earth: (re, im) kV rms for AC, (signed
    /// value, 0) kV for DC.
    pub fn to_earth(&self, k: usize) -> [f64; 2] {
        let n = self.phases.len().max(1);
        let phase = &self.phases[k];
        match self.kind {
            SystemKind::Ac => {
                let e = if n == 1 {
                    self.voltage_kv
                } else {
                    self.voltage_kv / (2.0 * (std::f64::consts::PI / n as f64).sin())
                };
                let angle = phase
                    .angle_deg
                    .unwrap_or(-(k as f64) * 360.0 / n as f64)
                    .to_radians();
                [e * angle.cos(), e * angle.sin()]
            }
            SystemKind::Dc => {
                let p = phase.polarity.unwrap_or(match k {
                    0 => 1,
                    1 => -1,
                    _ => 0,
                });
                [f64::from(p) * self.voltage_kv, 0.0]
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        let id = &self.id;
        if !(self.voltage_kv.is_finite() && self.voltage_kv > 0.0) {
            return Err(format!("system {id}: voltage must be a number of kV > 0"));
        }
        if self.phases.is_empty() {
            return Err(format!("system {id}: needs at least one phase or pole"));
        }
        let mut seen = BTreeMap::new();
        for p in &self.phases {
            if seen.insert(p.label.as_str(), ()).is_some() {
                return Err(format!("system {id}: duplicate phase {}", p.label));
            }
            match self.kind {
                SystemKind::Ac if p.polarity.is_some() => {
                    return Err(format!(
                        "system {id}: phase {}: polarity is for DC poles (AC phases have angle_deg)",
                        p.label
                    ));
                }
                SystemKind::Dc if p.angle_deg.is_some() => {
                    return Err(format!(
                        "system {id}: pole {}: angle_deg is for AC phases (DC poles have polarity)",
                        p.label
                    ));
                }
                _ => {}
            }
            if p.polarity.is_some_and(|x| !(-1..=1).contains(&x)) {
                return Err(format!(
                    "system {id}: pole {}: polarity is +1, -1 or 0",
                    p.label
                ));
            }
            if p.angle_deg.is_some_and(|a| !a.is_finite()) {
                return Err(format!(
                    "system {id}: phase {}: angle must be a number",
                    p.label
                ));
            }
        }
        match (self.kind, self.frequency_hz) {
            (SystemKind::Dc, Some(_)) => Err(format!("system {id}: DC has no frequency")),
            (_, Some(f)) if !(f.is_finite() && f > 0.0) => {
                Err(format!("system {id}: frequency must be > 0 Hz"))
            }
            _ => Ok(()),
        }
    }
}

/// The electrical state of a conductor: what clearance rules use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Potential {
    /// The system's nominal voltage, kV.
    pub nominal_kv: f64,
    /// AC or DC.
    pub kind: SystemKind,
    /// AC frequency, Hz, if given.
    pub frequency_hz: Option<f64>,
    /// Voltage to earth: phasor (re, im) kV rms for AC; (signed kV, 0) for
    /// DC.
    pub to_earth: [f64; 2],
}

impl Potential {
    /// Voltage to earth, kV (magnitude).
    pub fn to_earth_kv(&self) -> f64 {
        self.to_earth[0].hypot(self.to_earth[1])
    }

    /// Voltage between two conductors, kV (earthed: `None`).
    ///
    /// - AC of the same frequency (or frequencies not given): magnitude of
    ///   the phasor difference — `U` between two phases of a system, 0
    ///   between the same phase of two parallel systems.
    /// - DC: difference of the pole voltages.
    /// - AC of different frequencies, or AC with DC: the sum of the
    ///   voltages to earth (their peaks can meet).
    pub fn between(a: Option<&Self>, b: Option<&Self>) -> f64 {
        match (a, b) {
            (None, None) => 0.0,
            (Some(p), None) | (None, Some(p)) => p.to_earth_kv(),
            (Some(a), Some(b)) => {
                let synchronous = a.kind == b.kind
                    && match (a.frequency_hz, b.frequency_hz) {
                        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
                        _ => true,
                    };
                if synchronous {
                    (a.to_earth[0] - b.to_earth[0]).hypot(a.to_earth[1] - b.to_earth[1])
                } else {
                    a.to_earth_kv() + b.to_earth_kv()
                }
            }
        }
    }
}

/// The conductor paths of a line: sections joined by jumpers or by sharing
/// an attachment point, as lists of section ids (in line order).
pub fn conductor_paths(line: &Line) -> Vec<Vec<String>> {
    let n = line.sections.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut union = |a: usize, b: usize| {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };
    // Sections at each attachment point.
    let mut at: BTreeMap<(&str, u32, u32), Vec<usize>> = BTreeMap::new();
    for (k, s) in line.sections.iter().enumerate() {
        for p in &s.points {
            at.entry((p.structure.as_str(), p.set, p.phase))
                .or_default()
                .push(k);
        }
    }
    for list in at.values() {
        for w in list.windows(2) {
            union(w[0], w[1]);
        }
    }
    for j in &line.jumpers {
        let key = |p: &crate::SectionPoint| (p.structure.clone(), p.set, p.phase);
        let (f, t) = (key(&j.from), key(&j.to));
        let first = |k: &(String, u32, u32)| {
            at.get(&(k.0.as_str(), k.1, k.2))
                .and_then(|v| v.first().copied())
        };
        if let (Some(a), Some(b)) = (first(&f), first(&t)) {
            union(a, b);
        }
    }
    let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for k in 0..n {
        let r = find(&mut parent, k);
        groups
            .entry(r)
            .or_default()
            .push(line.sections[k].id.clone());
    }
    groups.into_values().collect()
}

impl Line {
    /// The system and phase of every section (by id): given on the section
    /// or on another section of its conductor path; `None` for earthed
    /// sections. Errors on unknown systems or phases and on contradicting
    /// assignments within a path.
    pub fn section_phases(&self) -> Result<BTreeMap<String, Option<(usize, usize)>>, LineError> {
        let invalid = |m: String| LineError::Invalid(m);
        let mut ids = BTreeMap::new();
        for s in &self.systems {
            if ids.insert(s.id.as_str(), ()).is_some() {
                return Err(invalid(format!("duplicate system id {}", s.id)));
            }
            s.validate().map_err(invalid)?;
        }
        let by_id: BTreeMap<&str, &crate::Section> =
            self.sections.iter().map(|s| (s.id.as_str(), s)).collect();
        let mut out = BTreeMap::new();
        for path in conductor_paths(self) {
            let mut assigned: Option<(usize, usize, String)> = None;
            for id in &path {
                let s = by_id[id.as_str()];
                let Some(sys) = &s.system else { continue };
                let k = self
                    .systems
                    .iter()
                    .position(|x| &x.id == sys)
                    .ok_or_else(|| invalid(format!("section {id}: unknown system {sys}")))?;
                let p = self.systems[k]
                    .phases
                    .iter()
                    .position(|x| x.label == s.phase_label)
                    .ok_or_else(|| {
                        invalid(format!(
                            "section {id}: system {sys} has no phase \"{}\" (phase_label); it has {}",
                            s.phase_label,
                            self.systems[k]
                                .phases
                                .iter()
                                .map(|x| x.label.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    })?;
                match &assigned {
                    None => assigned = Some((k, p, id.clone())),
                    Some((k0, p0, first)) if (*k0, *p0) != (k, p) => {
                        return Err(invalid(format!(
                            "sections {first} and {id} are joined (jumpers or a shared attachment) but assigned to {} {} and {} {}",
                            self.systems[*k0].id,
                            self.systems[*k0].phases[*p0].label,
                            self.systems[k].id,
                            self.systems[k].phases[p].label
                        )));
                    }
                    Some(_) => {}
                }
            }
            let a = assigned.map(|(k, p, _)| (k, p));
            for id in path {
                out.insert(id, a);
            }
        }
        Ok(out)
    }

    /// The potential of every section (by id): `None` for earthed
    /// sections. See [`Line::section_phases`].
    pub fn potentials(&self) -> Result<BTreeMap<String, Option<Potential>>, LineError> {
        Ok(self
            .section_phases()?
            .into_iter()
            .map(|(id, a)| {
                let p = a.map(|(k, p)| {
                    let s = &self.systems[k];
                    Potential {
                        nominal_kv: s.voltage_kv,
                        kind: s.kind,
                        frequency_hz: s.frequency_hz,
                        to_earth: s.to_earth(p),
                    }
                });
                (id, p)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(kind: SystemKind, kv: f64, labels: &[&str]) -> System {
        System {
            id: "s".into(),
            name: String::new(),
            kind,
            voltage_kv: kv,
            frequency_hz: None,
            phases: labels
                .iter()
                .map(|l| SystemPhase {
                    label: (*l).into(),
                    angle_deg: None,
                    polarity: None,
                })
                .collect(),
        }
    }

    fn potential(s: &System, k: usize) -> Potential {
        Potential {
            nominal_kv: s.voltage_kv,
            kind: s.kind,
            frequency_hz: s.frequency_hz,
            to_earth: s.to_earth(k),
        }
    }

    #[test]
    fn voltages_between_conductors() {
        // Three-phase 380 kV: 219.4 kV to earth, 380 kV between phases.
        let three = system(SystemKind::Ac, 380.0, &["L1", "L2", "L3"]);
        let (l1, l2, l3) = (
            potential(&three, 0),
            potential(&three, 1),
            potential(&three, 2),
        );
        assert!((l1.to_earth_kv() - 380.0 / 3f64.sqrt()).abs() < 1e-9);
        for (a, b) in [(&l1, &l2), (&l2, &l3), (&l3, &l1)] {
            assert!((Potential::between(Some(a), Some(b)) - 380.0).abs() < 1e-9);
        }
        // The same phase of a parallel system: 0; to an earth wire: U/√3.
        assert!(Potential::between(Some(&l1), Some(&l1)).abs() < 1e-9);
        assert!((Potential::between(Some(&l2), None) - 219.39).abs() < 0.01);
        // Two-phase railway 132 kV (0°, 180°): 66 kV to earth, 132 between.
        let mut rail = system(SystemKind::Ac, 132.0, &["UU", "VV"]);
        rail.frequency_hz = Some(16.7);
        let (u, v) = (potential(&rail, 0), potential(&rail, 1));
        assert!((u.to_earth_kv() - 66.0).abs() < 1e-9);
        assert!((Potential::between(Some(&u), Some(&v)) - 132.0).abs() < 1e-9);
        // 16.7 Hz against 50 Hz: not synchronous, voltages to earth add.
        let mut l1_50 = l1;
        l1_50.frequency_hz = Some(50.0);
        let d = Potential::between(Some(&u), Some(&l1_50));
        assert!((d - (66.0 + 380.0 / 3f64.sqrt())).abs() < 1e-9);
        // A set angle: L2 at +120° instead of −120° gives the same 380 kV.
        let mut turned = three.clone();
        turned.phases[1].angle_deg = Some(120.0);
        let t2 = potential(&turned, 1);
        assert!((Potential::between(Some(&l1), Some(&t2)) - 380.0).abs() < 1e-9);
        // DC bipole ±500 kV: 1000 kV between the poles; metallic return 0.
        let dc = system(SystemKind::Dc, 500.0, &["+", "-", "MR"]);
        let (p, n, mr) = (potential(&dc, 0), potential(&dc, 1), potential(&dc, 2));
        assert_eq!(Potential::between(Some(&p), Some(&n)), 1000.0);
        assert_eq!(mr.to_earth_kv(), 0.0);
    }

    fn example() -> Line {
        Line::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/synthetic-line/synthetic-line.ols.json"),
        )
        .unwrap()
    }

    #[test]
    fn conductor_paths_follow_jumpers() {
        let mut line = example();
        // The branch S3 is joined to S1 by the jumper at T3.
        let mut paths = conductor_paths(&line);
        paths.sort();
        assert_eq!(
            paths,
            [
                vec!["S1".to_owned(), "S3".to_owned()],
                vec!["S2".to_owned()]
            ]
        );
        // S3 names no system: it carries S1's phase L1 of ex220.
        let phases = line.section_phases().unwrap();
        assert_eq!(phases["S3"], phases["S1"]);
        assert_eq!(phases["S1"], Some((0, 0)));
        assert_eq!(phases["S2"], Some((0, 1)));
        let pot = line.potentials().unwrap();
        let between = Potential::between(pot["S1"].as_ref(), pot["S2"].as_ref());
        assert!((between - 220.0).abs() < 1e-9);
        // A contradiction on the path is an error naming both sections.
        line.sections[2].system = Some("ex220".into());
        line.sections[2].phase_label = "L2".into();
        let e = line.validate().unwrap_err().to_string();
        assert!(
            e.contains("S1") && e.contains("S3") && e.contains("joined"),
            "{e}"
        );
        // Unknown system or phase.
        line.sections[2].system = Some("nope".into());
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("unknown system")
        );
        line.sections[2].system = None;
        line.sections[0].phase_label = "L9".into();
        assert!(
            line.validate()
                .unwrap_err()
                .to_string()
                .contains("no phase \"L9\"")
        );
        // Without the jumper, S3 is on its own path and earthed.
        let mut line = example();
        line.jumpers.clear();
        assert_eq!(line.section_phases().unwrap()["S3"], None);
    }

    #[test]
    fn systems_are_checked() {
        let mut s = system(SystemKind::Ac, 220.0, &["L1", "L1"]);
        assert!(s.validate().unwrap_err().contains("duplicate"));
        s = system(SystemKind::Dc, 500.0, &["+"]);
        s.frequency_hz = Some(50.0);
        assert!(s.validate().is_err());
        s = system(SystemKind::Ac, 220.0, &["L1"]);
        s.phases[0].polarity = Some(1);
        assert!(s.validate().is_err());
        s = system(SystemKind::Dc, 500.0, &["+"]);
        s.phases[0].polarity = Some(2);
        assert!(s.validate().is_err());
        assert!(system(SystemKind::Ac, 0.0, &["L1"]).validate().is_err());
    }
}
