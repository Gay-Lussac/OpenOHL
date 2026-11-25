// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The electric field on a working set with the lines near it (ROADMAP
//! O17, 2026-11-24; built, in active development, not approved yet).
//!
//! The lines are taken as the magnetic field takes them
//! ([`crate::fields::near_sections`]: every section, of this line or
//! another, whose spans can come within the field distance of the working
//! set's wires) and **solved together** ([`ohl_electrical::joint`]): one
//! system of charges over every line's wires and earth wires — a
//! neighbour's conductors change this line's charges and its earth wires
//! screen, so fields computed apart and added are not a proven bound. The
//! other lines' phase angles to this line's are not known: the worst over
//! them is taken, exactly for one other line (two parts), as an upper
//! bound for more (said with every result, [`ohl_electrical::joint::worst`]).
//!
//! At every profile point the result is **the larger** of this line alone
//! in its own model (`ohl_electrical::electric`, the ground plane under
//! each point) and the lines solved together: the two models differ (the
//! joint one holds every wire, jumpers among them (2026-11-25), at its
//! voltage and takes every charge from the
//! system), and neither is let to lower the other.
//!
//! **NOT ON THE SAFE SIDE**: the wires farther than the field distance
//! from a profile's middle are left out of its system — their field is not
//! in the result, so the real field can be higher; a larger field distance
//! takes them in. Said in every result, with the distance ([`beyond_note`],
//! the same words as the magnetic field's).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ohl_electrical::Sources;
pub use ohl_electrical::electric::CircuitVoltage;
use ohl_electrical::electric::{self, GroundModel};
use ohl_electrical::joint::{self, JointSettings, JointWire};
use ohl_weather::WeatherCase;
use serde::{Deserialize, Serialize};

use crate::fields::{near_sections, solved, stations_of};
use crate::{Neighbour, NeighbourKind, Neighbourhood, PulledIn, Reach, WorkingSet};

/// What an electric field check computes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectricSpec {
    /// Lines within this distance of the checked wires are pulled in, and
    /// a profile's system takes the wires within it of its middle, m.
    pub distance: f64,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// To either side of the axis, m.
    pub width: f64,
    /// Between the profile's points, m.
    pub step: f64,
    /// A limit, kV/m (none: none checked).
    pub limit_kv_m: Option<f64>,
}

impl Default for ElectricSpec {
    fn default() -> Self {
        Self {
            distance: crate::reach::FIELD_DISTANCE,
            height: 1.0,
            width: electric::WIDTH,
            step: 5.0,
            limit_kv_m: None,
        }
    }
}

/// The field at one point of a profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Across the span's axis, m (right of the line, looking ahead).
    pub offset: f64,
    /// Where, m.
    pub at: [f64; 3],
    /// This line alone in its own model, kV/m.
    pub own_kv_m: f64,
    /// The lines solved together, the worst over their phase angles (or
    /// its bound), kV/m; none: no other line near.
    pub joint_kv_m: Option<f64>,
    /// The larger of both, kV/m.
    pub e_kv_m: f64,
}

/// A profile across a span of the working set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The span's structures.
    pub from: String,
    /// Its second structure.
    pub to: String,
    /// Its points.
    pub points: Vec<Point>,
    /// The largest field on it, kV/m.
    pub max_kv_m: f64,
    /// The pieces of its joint system (0: none solved).
    pub pieces: usize,
}

/// A scoped electric field check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectricResult {
    /// The reach it pulled in by.
    pub reach: Reach,
    /// Per span of the working set.
    pub profiles: Vec<Profile>,
    /// The largest field, kV/m.
    pub max_kv_m: f64,
    /// The limit, kV/m (none: none checked).
    pub limit_kv_m: Option<f64>,
    /// Points at or above the limit.
    pub above_limit: usize,
    /// What the joint system took in (this line's sections, each other
    /// line's).
    pub taken_in: Vec<String>,
    /// How the unknown phase angles were taken: exact or a bound
    /// ([`joint::method_words`]).
    pub method: String,
    /// What it pulled in.
    pub pulled: PulledIn,
    /// What it could not do: each makes it *not all checked*.
    pub not_all_checked: Vec<String>,
    /// What it assumed.
    pub notes: Vec<String>,
}

/// Every line near a working set as one system of wires
/// ([`joint_lines`]).
#[derive(Clone)]
pub struct JointLines {
    /// The wires, each with its part.
    pub wires: Vec<JointWire>,
    /// The parts: one line's phases of one frequency each (the first:
    /// this line's).
    pub parts: Vec<String>,
    /// This line's sections in the system (the working set's and those
    /// near).
    pub own_sections: Vec<String>,
    /// Every other line in it, with its sections.
    pub lines: Vec<(String, Vec<String>)>,
    /// The ground where no terrain gives it: straight between the
    /// structures' feet of every line in, by the nearest span.
    pub ground: Arc<dyn Fn(f64, f64) -> f64 + Send + Sync>,
    /// The other lines' sections pulled in (and this line's beyond the
    /// working set).
    pub pulled: PulledIn,
    /// What could not be done (each makes it *not all checked*).
    pub not_all_checked: Vec<String>,
    /// What was assumed (voltages, left out).
    pub notes: Vec<String>,
}

impl std::fmt::Debug for JointLines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JointLines")
            .field("wires", &self.wires.len())
            .field("parts", &self.parts)
            .field("lines", &self.lines)
            .finish_non_exhaustive()
    }
}

impl JointLines {
    /// How the unknown phase angles are taken: exact or a bound
    /// ([`joint::method_words`]).
    pub fn method(&self) -> String {
        joint::method_words(self.parts.len())
    }

    /// What the system took in, in words.
    pub fn taken_in(&self, model: &str) -> Vec<String> {
        let mut out = vec![format!(
            "line {model}: {} section(s) (the working set's and this line's near)",
            self.own_sections.len()
        )];
        out.extend(
            self.lines
                .iter()
                .map(|(m, s)| format!("line {m}: {} section(s) near", s.len())),
        );
        out
    }
}

/// The lines near working set `ws` in weather case `case` as one system:
/// this line's sections (the working set's and its own near) and every
/// other line's within `distance` of the working set's wires (the
/// network's selection, [`near_sections`]), each solved in `case` with its
/// own settings and wires, its phases at their systems' voltages (the
/// highest operating voltage where the line gives one, else the nominal:
/// said NOT ON THE SAFE SIDE, as the highest gives a higher field — give
/// it with the system), its earth wires earthed. `given`: voltages for
/// this line's circuits (OHL-Design's window; the network's check: none).
pub fn joint_lines(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    case: &WeatherCase,
    distance: f64,
    given: &[CircuitVoltage],
) -> Result<JointLines, String> {
    let near = near_sections(net, ws, case, distance)?;
    let mut not_all: Vec<String> = near.wires_failed.clone();
    let mut notes = Vec::new();
    let mut pulled = PulledIn::default();
    let ws_sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
    let mut own: BTreeSet<String> = near.own_near.clone();
    for s in own.iter().filter(|s| !ws_sections.contains(*s)) {
        pulled
            .read
            .insert(Neighbour::new(&ws.model, NeighbourKind::Section, s));
        pulled
            .used
            .insert(Neighbour::new(&ws.model, NeighbourKind::Section, s));
    }
    own.extend(ws_sections.iter().cloned());
    let own_list: Vec<String> = own.into_iter().collect();
    let mut wires = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    let mut lines_in: Vec<(String, Vec<String>)> = Vec::new();
    let mut grounds: Vec<ohl_line::Line> = Vec::new();
    // Each line's wires, a part per (line, frequency).
    let add = |model: &str,
               given: &[CircuitVoltage],
               part: &ohl_line::Line,
               lib: &ohl_cables::Library,
               wires: &mut Vec<JointWire>,
               parts: &mut Vec<String>,
               not_all: &mut Vec<String>,
               notes: &mut Vec<String>| {
        let (shapes, jumpers, failed) = solved(part, lib, case);
        for f in failed {
            not_all.push(format!("line {model}: {f}: its field left out"));
        }
        // A jumper that did not solve: its field missing (2026-11-25).
        for j in part
            .jumpers
            .iter()
            .filter(|j| !jumpers.iter().any(|s| s.jumper == j.id))
        {
            not_all.push(format!(
                "line {model}: jumper {}: not solved: its field left out",
                j.id
            ));
        }
        let mut by_freq: BTreeMap<u64, usize> = BTreeMap::new();
        for (f, mut w) in joint::line_wires(
            part,
            lib,
            &shapes,
            &jumpers,
            given,
            &format!("line {model}"),
            notes,
        ) {
            if let Some(f) = f {
                let k = *by_freq.entry(f.to_bits()).or_insert_with(|| {
                    parts.push(format!("line {model} ({f} Hz)"));
                    parts.len() - 1
                });
                w.part = Some(k);
            }
            wires.push(w);
        }
    };
    let library = net.library(&ws.model)?;
    let own_part = net.part(&ws.model, &[], &own_list)?;
    add(
        &ws.model,
        given,
        &own_part,
        &library,
        &mut wires,
        &mut parts,
        &mut not_all,
        &mut notes,
    );
    grounds.push(own_part);
    for (m, secs) in near.others {
        let list: Vec<String> = secs.iter().cloned().collect();
        let got = net
            .part(&m, &[], &list)
            .and_then(|p| net.library(&m).map(|l| (p, l)));
        let (p, lib) = match got {
            Ok(x) => x,
            Err(e) => {
                not_all.push(format!(
                    "line {m}: {} section(s) near not read ({e}): left out of the system",
                    list.len()
                ));
                continue;
            }
        };
        add(
            &m,
            &[],
            &p,
            &lib,
            &mut wires,
            &mut parts,
            &mut not_all,
            &mut notes,
        );
        for s in &secs {
            pulled
                .read
                .insert(Neighbour::new(&m, NeighbourKind::Section, s));
            pulled
                .used
                .insert(Neighbour::new(&m, NeighbourKind::Section, s));
        }
        lines_in.push((m, list));
        grounds.push(p);
    }
    let refs: Vec<&ohl_line::Line> = grounds.iter().collect();
    let ground: Arc<dyn Fn(f64, f64) -> f64 + Send + Sync> = Arc::new(joint::feet_ground_of(&refs));
    Ok(JointLines {
        wires,
        parts,
        own_sections: own_list,
        lines: lines_in,
        ground,
        pulled,
        not_all_checked: not_all,
        notes,
    })
}

/// Where a profile is computed with the lines together: its middle (in
/// plan) and its points.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileAt {
    /// Its middle (the axis), m.
    pub centre: [f64; 2],
    /// Its points, m.
    pub points: Vec<[f64; 3]>,
}

/// A profile computed with the lines together.
#[derive(Debug, Clone, PartialEq)]
pub struct JointProfile {
    /// At each point, the worst over the parts' phase angles (or its
    /// bound), kV/m.
    pub worst_kv_m: Vec<f64>,
    /// The pieces solved.
    pub pieces: usize,
}

/// The settings of a profile's system: the field distance as its reach.
pub fn joint_settings(distance: f64) -> JointSettings {
    JointSettings {
        reach: distance,
        ..JointSettings::default()
    }
}

/// `lines` solved together at each of `profiles` (its system: the wires
/// within `distance` of its middle), the worst over the parts' phase
/// angles at every point — the network's electric field check and
/// OHL-Design's Electric field window on an offline working set's file
/// (2026-11-24) on the same code.
pub fn joint_at(
    lines: &JointLines,
    profiles: &[ProfileAt],
    distance: f64,
) -> Vec<Result<JointProfile, String>> {
    let s = joint_settings(distance);
    let ground = lines.ground.clone();
    let g = move |x: f64, y: f64| ground(x, y);
    profiles
        .iter()
        .map(|p| {
            let f = joint::solve(&lines.wires, lines.parts.len(), p.centre, &p.points, &g, &s)?;
            Ok(JointProfile {
                worst_kv_m: f.parts.iter().map(|e| joint::worst(e) * 1e-3).collect(),
                pieces: f.pieces,
            })
        })
        .collect()
}

/// The length of a stretch of a run with a system of its own
/// ([`JointAlong`]), m: twice the distance at which the pieces start to
/// grow (shortest piece over grading) — every point of a stretch lies
/// within half of it of its system's middle (along the run), where the
/// wires there are cut into the shortest pieces, as at a profile's
/// middle. Infinite: pieces that never grow (one system).
pub fn stretch(s: &JointSettings) -> f64 {
    if s.grading > 0.0 {
        2.0 * s.min_piece / s.grading
    } else {
        f64::INFINITY
    }
}

/// The lines solved together along a run (a volume's path; 2026-11-25):
/// the run cut into stretches of [`stretch`], each with its own system
/// centred on the run there ([`joint::System`], every wire within the
/// field distance of that middle), solved when a point of it is first
/// asked; a point takes its stretch's system — as a profile's points take
/// their profile's (those to the side of the run included), the worst over
/// the parts' phase angles (exact for two parts, a bound for more).
pub struct JointAlong<'a> {
    lines: &'a JointLines,
    station: &'a (dyn Fn([f64; 2]) -> f64 + Sync),
    place: &'a (dyn Fn(f64) -> [f64; 2] + Sync),
    length: f64,
    settings: JointSettings,
    systems: std::sync::Mutex<BTreeMap<usize, Result<Arc<joint::System>, String>>>,
}

impl<'a> JointAlong<'a> {
    /// Along a run of `length` m: `station` gives a plan point's station on
    /// it, `place` the plan point of a station on it; the systems reach
    /// `distance` from their middles.
    pub fn new(
        lines: &'a JointLines,
        station: &'a (dyn Fn([f64; 2]) -> f64 + Sync),
        place: &'a (dyn Fn(f64) -> [f64; 2] + Sync),
        length: f64,
        distance: f64,
    ) -> Self {
        Self {
            lines,
            station,
            place,
            length,
            settings: joint_settings(distance),
            systems: std::sync::Mutex::new(BTreeMap::new()),
        }
    }

    /// The stretches of the run.
    fn stretches(&self) -> usize {
        let s = stretch(&self.settings);
        if s.is_finite() {
            ((self.length / s).ceil() as usize).max(1)
        } else {
            1
        }
    }

    /// The system of stretch `k` (solved once).
    fn system(&self, k: usize) -> Result<Arc<joint::System>, String> {
        let mut systems = self
            .systems
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(s) = systems.get(&k) {
            return s.clone();
        }
        let n = self.stretches();
        let t = self.length * (k as f64 + 0.5) / n as f64;
        let centre = (self.place)(t);
        let ground = self.lines.ground.clone();
        let g = move |x: f64, y: f64| ground(x, y);
        let s = joint::System::solve(
            &self.lines.wires,
            self.lines.parts.len(),
            centre,
            &g,
            &self.settings,
        )
        .map(Arc::new);
        systems.insert(k, s.clone());
        s
    }

    /// At each of `points` the worst over the parts' phase angles, kV/m —
    /// 0 where its stretch's system did not solve ([`JointAlong::failed`]
    /// says where: NOT ALL CHECKED there).
    pub fn worst_kv_m(&self, points: &[[f64; 3]]) -> Vec<f64> {
        let n = self.stretches();
        let mut by: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, p) in points.iter().enumerate() {
            let t = (self.station)([p[0], p[1]]);
            let k = ((t / self.length.max(1e-9) * n as f64).floor().max(0.0) as usize).min(n - 1);
            by.entry(k).or_default().push(i);
        }
        let ground = self.lines.ground.clone();
        let g = move |x: f64, y: f64| ground(x, y);
        let mut out = vec![0.0; points.len()];
        for (k, idx) in by {
            let Ok(system) = self.system(k) else { continue };
            // On this thread: the volume's lattice spreads its blocks of
            // points over the cores.
            for &i in &idx {
                out[i] = joint::worst(&system.field_at(points[i], &g)) * 1e-3;
            }
        }
        out
    }

    /// The stretches whose system did not solve, with why.
    pub fn failed(&self) -> Vec<String> {
        let systems = self
            .systems
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let n = self.stretches();
        systems
            .iter()
            .filter_map(|(k, s)| {
                s.as_ref().err().map(|e| {
                    format!(
                        "the stretch {:.0}–{:.0} m along it: {e}",
                        self.length * *k as f64 / n as f64,
                        self.length * (*k + 1) as f64 / n as f64
                    )
                })
            })
            .collect()
    }

    /// The systems solved, and the most pieces in one.
    pub fn solved(&self) -> (usize, usize) {
        let systems = self
            .systems
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ok: Vec<usize> = systems
            .values()
            .filter_map(|s| s.as_ref().ok().map(|s| s.pieces()))
            .collect();
        (ok.len(), ok.iter().copied().max().unwrap_or(0))
    }

    /// Every wire's straight pieces as solved (each span's polyline), m:
    /// where a volume's lattice is refined.
    pub fn wire_pieces(&self) -> Vec<[[f64; 3]; 2]> {
        self.lines
            .wires
            .iter()
            .flat_map(|w| w.points.windows(2).map(|p| [p[0], p[1]]))
            .collect()
    }
}

/// The note said whenever lines are solved together.
pub const SOLVED_TOGETHER: &str = "the lines solved together (charge simulation: every line's \
                                   wires and earth wires one system of potential coefficients \
                                   over the ground plane under each point, the jumpers each at \
                                   its section's voltage); \
                                   at every point the larger of this line alone in its own model \
                                   and the lines together";

/// The note said with every scoped electric field: the wires beyond the
/// field distance left out (NOT ON THE SAFE SIDE: their field is missing,
/// so the real field can be higher) — the magnetic field's words
/// ([`crate::fields::beyond_words`]), with each profile's own system.
pub fn beyond_note(distance: f64) -> String {
    crate::fields::beyond_words(
        &format!(
            "the wires farther than {distance:.0} m from the working set's wires, and in each \
             profile's system those farther than {distance:.0} m from its middle,"
        ),
        "a larger field distance takes them in",
    )
}

/// The middle of a profile: its point nearest the axis.
pub fn centre_of(points: &[electric::FieldPoint]) -> Option<[f64; 2]> {
    points
        .iter()
        .min_by(|a, b| a.offset.abs().total_cmp(&b.offset.abs()))
        .map(|p| [p.x, p.y])
}

/// The electric field of working set `ws` in weather case `case`, the
/// lines within `spec.distance` pulled in from `net` and solved together
/// (see the module's notes).
///
/// Expected cost: this line near the working set solved and its own field
/// at the profiles; with other lines near, per profile a system of some
/// 50 pieces per wire and kilometre (its LU on all cores).
pub fn electric_field(
    net: &mut dyn Neighbourhood,
    ws: &WorkingSet,
    case: &WeatherCase,
    spec: &ElectricSpec,
) -> Result<ElectricResult, String> {
    if !(spec.distance.is_finite() && spec.distance > 0.0) {
        return Err(format!("a field distance of {} m", spec.distance));
    }
    let lines = joint_lines(net, ws, case, spec.distance, &[])?;
    let mut not_all = lines.not_all_checked.clone();
    let mut notes = vec![beyond_note(spec.distance)];
    notes.extend(lines.notes.iter().cloned());
    let ws_sections: BTreeSet<String> = ws.sections.iter().cloned().collect();
    let library = net.library(&ws.model)?;
    let own_part = net.part(&ws.model, &[], &lines.own_sections)?;
    let (shapes, jumpers, _) = solved(&own_part, &library, case);
    let src = Sources {
        line: &own_part,
        library: &library,
        shapes: &shapes,
        temperature: case.temperature,
        ground: None,
        jumpers: &jumpers,
    };
    let settings = electric::Settings {
        height: spec.height,
        width: spec.width,
        step: spec.step,
        stations: stations_of(&own_part, &ws_sections),
        profiles: true,
        limit_kv_m: spec.limit_kv_m,
        ground_model: GroundModel::Plane,
        ..electric::Settings::default()
    };
    let own = electric::run(&src, &settings, None)?;
    notes.extend(own.notes.iter().cloned());
    let with_others = !lines.lines.is_empty();
    let at: Vec<ProfileAt> = own
        .profiles
        .iter()
        .map(|p| ProfileAt {
            centre: centre_of(&p.points).unwrap_or([0.0, 0.0]),
            points: p.points.iter().map(|q| [q.x, q.y, q.z]).collect(),
        })
        .collect();
    let joint = if with_others {
        notes.push(SOLVED_TOGETHER.into());
        joint_at(&lines, &at, spec.distance)
    } else {
        notes.push(format!(
            "no other line within {:.0} m: this line alone",
            spec.distance
        ));
        Vec::new()
    };
    let mut profiles = Vec::new();
    let (mut max, mut above) = (0.0_f64, 0);
    for (k, p) in own.profiles.iter().enumerate() {
        let j = match joint.get(k) {
            Some(Ok(j)) => Some(j),
            Some(Err(e)) => {
                not_all.push(format!(
                    "span {}–{}: the lines not solved together ({e}): this line alone there",
                    p.from, p.to
                ));
                None
            }
            None => None,
        };
        let points: Vec<Point> = p
            .points
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let joint_kv_m = j.and_then(|j| j.worst_kv_m.get(i).copied());
                Point {
                    offset: q.offset,
                    at: [q.x, q.y, q.z],
                    own_kv_m: q.e_kv_m,
                    joint_kv_m,
                    e_kv_m: joint_kv_m.map_or(q.e_kv_m, |x| x.max(q.e_kv_m)),
                }
            })
            .collect();
        let m = points.iter().map(|q| q.e_kv_m).fold(0.0, f64::max);
        max = max.max(m);
        if let Some(l) = spec.limit_kv_m {
            above += points.iter().filter(|q| q.e_kv_m >= l).count();
        }
        profiles.push(Profile {
            from: p.from.clone(),
            to: p.to.clone(),
            points,
            max_kv_m: m,
            pieces: j.map_or(0, |j| j.pieces),
        });
    }
    notes.sort();
    notes.dedup();
    Ok(ElectricResult {
        reach: Reach::Lines {
            metres: spec.distance,
        },
        profiles,
        max_kv_m: max,
        limit_kv_m: spec.limit_kv_m,
        above_limit: above,
        taken_in: lines.taken_in(&ws.model),
        method: if with_others {
            joint::method_words(lines.parts.len())
        } else {
            "this line alone (no other line near)".into()
        },
        pulled: lines.pulled,
        not_all_checked: not_all,
        notes,
    })
}
