// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Magnetic field…** (roadmap O18 step 4, owner
//! 2026-10-27; built, in active development, not approved yet): the
//! magnetic flux density of the line's currents ([`ohl_electrical::magnetic`])
//! in a weather case — the currents per circuit and their load-flow
//! directions, earth wire currents, the profiles' height and width, the
//! stations, an optional limit, 3D or 2D, points from a CSV — and on Run
//! the profiles plotted (B against the distance from the axis), a table
//! per profile and per point with the limit's margin, the notes; Export
//! CSV. Out of date like the other checks when the line or the ground
//! changes. The model's pieces and length, the levels drawn and the views
//! (isolines from above, the cross-section panels, volumes) in
//! [`views`] (owner, 2026-10-29). Not yet: the thermal rating as the
//! currents (in `ohl electrical magnetic --thermal`).
//!
//! The window's inputs are kept in the project, per project line
//! ([`SETTING`]; owner, 2026-10-27): the weather case, the currents and
//! directions, the earth wires, the profiles, the stations, the limit, 2D,
//! and the points (the CSV's text, so they travel with the project).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use ohl_electrical::magnetic::{
    self, CircuitCurrent, Heights2d, MagneticField, Model as FieldModel, Progress, Settings,
    StationSpec,
};

#[path = "magnetic_views.rs"]
pub mod views;

use crate::check_help as help;
use crate::check_ui;
use crate::field_common::{
    self, FieldPage, FieldShared, FieldStart, KeepStep, Spans, Stations, StoredCommon,
    beside_stations,
};
use crate::field_results::{self, ResultRow, RowAction};
use crate::job::save_csv;
use crate::model::Model;
use ohl_session::reports::CheckReport;
use views::Above;
#[cfg(test)]
use views::VolumeSetup;

/// The answer: the field and the weather case.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The field.
    pub field: MagneticField,
    /// The weather case's name.
    pub case: String,
    /// The 3D model's wires (for the cross-section panels' cuts).
    pub wires: Option<Arc<magnetic::Wires3d>>,
    /// The volumes' surfaces (level, mesh), in their order.
    pub meshes: Vec<(f64, Arc<ohl_electrical::contour::Mesh>)>,
    /// The points computed and the pieces, for the speed.
    pub work: f64,
}

/// Every circuit's end points: its label and the structures.
type CircuitEnds = Vec<(String, Vec<String>)>;

/// A phase's typed current: its label, A (rms) and its angle, degrees.
type PhaseRow = (String, f64, f64);

/// How a circuit's current is given.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Given {
    /// One balanced current for the circuit.
    Circuit,
    /// At its end points.
    Ends,
    /// Per phase: magnitude and angle.
    Phases,
}

/// The choice "per phase" of a circuit's current.
pub const PER_PHASE_LABEL: &str = "per phase";

/// What the choice does, on hover.
pub const PER_PHASE_HELP: &str = "The circuit: one current everywhere, balanced at the system's \
     phase angles, along the line or against it. At its end points (owner, 2026-10-29): the \
     current in or out at each end (one end may take the rest); every section's current \
     follows by Kirchhoff's current law, branches included (a meshed circuit needs the load \
     flow: refused). Per phase (owner, 2026-11-11): each phase's current and angle typed — an \
     unbalanced circuit. Its phases' sum is a net current, whose field falls off as 1/d with \
     the distance, a balanced circuit's as 1/d²: far from the line the net current governs. \
     Balanced input has none and says so in its results (not on the safe side far from the \
     line). No effect on the time.";

/// The tick of an earth wire's typed current.
pub const EARTH_TYPED_HELP: &str = "Ticked: this earth wire carries the current typed here \
     (rms, along the line from its first structure to its last, in every span) instead of a \
     computed one — also with the earth wire currents off. With them on, the other earth \
     wires are induced by the phases and by this one. Not together with \"through the \
     structures' earthing\" (that network computes every earth wire's current): the run is \
     refused. The field changes by this current's own field; no effect on the time.";

/// The Magnetic field window.
pub struct MagneticFieldWindow {
    /// What both field windows have (reached through `Deref` from outside).
    pub common: FieldShared<Answer>,

    /// Current (A) and reverse per circuit, by its label.
    currents: BTreeMap<String, (f64, bool)>,
    /// The circuits given by their end points (owner, 2026-10-29): per
    /// circuit, each end's structure and current into the line (A,
    /// negative: out; none: the rest).
    ends: BTreeMap<String, Vec<(String, Option<f64>)>>,
    /// The circuits given per phase (owner, 2026-11-11): per circuit,
    /// each phase's label, current (A) and angle (degrees).
    phases: BTreeMap<String, Vec<PhaseRow>>,
    /// The earth wires given a current instead of a computed one: per
    /// earth wire section, its current (A) and angle (degrees).
    earth_typed: BTreeMap<String, (f64, f64)>,
    /// The circuits' end points, by the line's version.
    ends_seen: Option<(u64, CircuitEnds)>,
    earth_wires: bool,
    /// The earth wires as a network through the structures' earthing,
    /// the structures carrying current.
    towers: bool,
    mean_heights: bool,
    /// Cuts and volumes take the wires within this distance only, m.
    modelled_within: Option<f64>,
    /// The isolines from above' step (their spans: ticked in the table).
    above: Option<Above>,
    /// The levels the last run was asked (their order in its reports).
    run_levels: Vec<f64>,
    /// The last run's wires (3D).
    wires: Option<Arc<magnetic::Wires3d>>,
}

impl std::ops::Deref for MagneticFieldWindow {
    type Target = FieldShared<Answer>;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

impl std::ops::DerefMut for MagneticFieldWindow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.common
    }
}

impl Default for MagneticFieldWindow {
    fn default() -> Self {
        Self {
            common: FieldShared::new(FieldStart {
                jumpers: true,
                height: 1.0,
                width: magnetic::WIDTH,
                step: 1.0,
                levels: crate::field_overlay::default_levels(),
                volume_width: magnetic::WIDTH,
                cut_width: magnetic::WIDTH,
            }),
            currents: BTreeMap::new(),
            ends: BTreeMap::new(),
            phases: BTreeMap::new(),
            earth_typed: BTreeMap::new(),
            ends_seen: None,
            earth_wires: false,
            towers: true,
            mean_heights: false,
            // 2 km (owner, 2026-11-01, the defaults reviewed: measured on
            // a 28 km line 0.03 % off the whole line, ten times quicker;
            // each run says what was left out).
            modelled_within: Some(CUTOFF),
            above: Some(Above { step: 2.0 }),
            run_levels: Vec::new(),
            wires: None,
        }
    }
}

/// The project setting holding the window's inputs (per project line:
/// `app.magnetic_field.<line>`).
pub const SETTING: &str = "app.magnetic_field";

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "magnetic-field-window";

/// The window's own page of the user guide (owner, 2026-11-12: the
/// field windows reviewed as the vertical clearances were); its headings'
/// **user guide ↗** open its sections.
pub const GUIDE: &str = "user-guide/magnetic-field";

/// The wires modelled within this distance of a cut or volume by default,
/// m (owner, 2026-11-01).
pub const CUTOFF: f64 = 2000.0;

/// The window's inputs as kept in the project (JSON).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Stored {
    /// What both field windows keep.
    #[serde(flatten)]
    common: StoredCommon,

    currents: BTreeMap<String, (f64, bool)>,
    ends: BTreeMap<String, Vec<(String, Option<f64>)>>,
    /// Since 2026-11-11 (absent: none): the circuits given per phase.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    phases: BTreeMap<String, Vec<PhaseRow>>,
    /// Since 2026-11-11 (absent: none): the earth wires' typed currents.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    earth_typed: BTreeMap<String, (f64, f64)>,
    earth_wires: bool,
    towers: Option<bool>,
    mean_heights: bool,
    modelled_within: Option<f64>,
    /// The cut-off as kept since 2026-11-01: 0 the whole line, else the
    /// distance, m (absent: `modelled_within` as before).
    modelled: Option<f64>,
    /// Isolines from above: (formerly one span; none: the spans ticked)
    /// and the step.
    above: Option<(Option<(String, String)>, f64)>,
    above_spans: Vec<(String, String)>,
}

/// A circuit's phases at the balanced current `amps` (against the line:
/// turned by 180°), at its system's phase angles: where "per phase"
/// starts from.
fn balanced_rows(
    model: &Model,
    c: &ohl_electrical::parameters::CircuitKey,
    amps: f64,
    reverse: bool,
) -> Vec<PhaseRow> {
    let Some(system) = model.line.systems.iter().find(|s| s.id == c.system) else {
        return Vec::new();
    };
    let n = system.phases.len().max(1) as f64;
    system
        .phases
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let angle = p.angle_deg.unwrap_or(-(k as f64) * 360.0 / n);
            let angle = if reverse { angle + 180.0 } else { angle };
            // Within −180° to 180°.
            let angle = (angle + 180.0).rem_euclid(360.0) - 180.0;
            (p.label.clone(), amps, angle)
        })
        .collect()
}

/// The net current of typed phases: their phasors' sum, A and degrees.
pub fn net_current(rows: &[PhaseRow]) -> (f64, f64) {
    let (re, im) = rows.iter().fold((0.0, 0.0), |(re, im), (_, a, deg)| {
        let r = deg.to_radians();
        (re + a * r.cos(), im + a * r.sin())
    });
    (re.hypot(im), im.atan2(re).to_degrees())
}

/// A circuit's phases, each with its current and angle, and their sum.
fn phases_ui(ui: &mut egui::Ui, rows: &mut [PhaseRow]) {
    ui.vertical(|ui| {
        for (label, amps, angle) in rows.iter_mut() {
            ui.horizontal(|ui| {
                ui.label(label.as_str());
                crate::style::field(
                    ui,
                    egui::DragValue::new(amps)
                        .range(0.0..=100_000.0)
                        .speed(5.0)
                        .suffix(" A"),
                )
                .on_hover_text(
                    "rms, this phase, along the line (from its first structure to its last). \
                     The field of this phase grows in proportion",
                );
                crate::style::field(
                    ui,
                    egui::DragValue::new(angle)
                        .range(-360.0..=360.0)
                        .speed(1.0)
                        .suffix("°"),
                )
                .on_hover_text(
                    "This current's phase angle, in the frame of the systems' phase angles \
                     (a balanced circuit: L1 0°, L2 −120°, L3 +120° unless its system gives \
                     others; against the line: 180° more)",
                );
            });
        }
        let (net, at) = net_current(rows);
        ui.label(
            egui::RichText::new(format!("net current {net:.1} A at {at:.1}°"))
                .small()
                .weak(),
        )
        .on_hover_text(
            "The sum of the phases' currents. Its field falls off as 1/d with the distance, \
             the balanced part's as 1/d²: far from the line it governs",
        );
    });
}

impl MagneticFieldWindow {
    /// The earth wires' typed currents (owner, 2026-11-11): per earth wire
    /// section a tick, its current and its angle; nothing shown on a line
    /// without earth wires.
    fn earth_typed_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let earthed: Vec<&str> = model
            .line
            .sections
            .iter()
            .filter(|s| s.system.is_none())
            .map(|s| s.id.as_str())
            .collect();
        // A section no longer an earth wire: not kept.
        self.earth_typed
            .retain(|id, _| earthed.contains(&id.as_str()));
        if earthed.is_empty() {
            return;
        }
        ui.add_space(4.0);
        egui::CollapsingHeader::new(format!(
            "Earth wire currents typed ({} of {})",
            self.earth_typed.len(),
            earthed.len()
        ))
        .id_salt("magnetic-field-earth-typed")
        .show(ui, |ui| {
            help::list(
                ui,
                "magnetic-field-earth-typed-list",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("magnetic-field-earth-typed-rows"),
                |ui| {
                    for id in &earthed {
                        ui.horizontal(|ui| {
                            let mut typed = self.earth_typed.contains_key(*id);
                            if ui
                                .checkbox(&mut typed, *id)
                                .on_hover_text(EARTH_TYPED_HELP)
                                .changed()
                            {
                                if typed {
                                    self.earth_typed.insert((*id).to_owned(), (0.0, 0.0));
                                } else {
                                    self.earth_typed.remove(*id);
                                }
                            }
                            if let Some((amps, angle)) = self.earth_typed.get_mut(*id) {
                                crate::style::field(
                                    ui,
                                    egui::DragValue::new(amps)
                                        .range(0.0..=100_000.0)
                                        .speed(1.0)
                                        .suffix(" A"),
                                )
                                .on_hover_text("rms, along the line, in every span");
                                crate::style::field(
                                    ui,
                                    egui::DragValue::new(angle)
                                        .range(-360.0..=360.0)
                                        .speed(1.0)
                                        .suffix("°"),
                                )
                                .on_hover_text(
                                    "Its phase angle, in the frame of the systems' phase angles",
                                );
                            }
                        });
                    }
                },
            );
        });
    }

    /// The inputs as kept in the project.
    fn stored(&mut self, model: &Model) -> Stored {
        Stored {
            common: self.common.stored_common(model),
            currents: self.currents.clone(),
            ends: self.ends.clone(),
            phases: self.phases.clone(),
            earth_typed: self.earth_typed.clone(),
            earth_wires: self.earth_wires,
            towers: Some(self.towers),
            mean_heights: self.mean_heights,
            modelled_within: self.modelled_within,
            modelled: Some(self.modelled_within.unwrap_or(0.0)),
            above: self.above.as_ref().map(|a| (None, a.step)),
            above_spans: self.common.span_pick.above.iter().cloned().collect(),
        }
    }

    /// Reads the inputs from the project when not read yet for the line
    /// worked on, and writes them into it when they changed — once a run
    /// starts or the window closes, not while a value is dragged or typed.
    /// A project this user cannot change keeps them for the session.
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        match self.common.keep_step(model, ctx, SETTING) {
            KeepStep::Nothing => {}
            KeepStep::Read => self.load(model),
            KeepStep::Write(key) => {
                let stored = self.stored(model);
                self.common
                    .write_inputs(model, key, &stored, "magnetic field inputs");
            }
        }
    }

    /// Takes the inputs kept in the project for the line worked on (after
    /// opening it, switching lines, or another user changing them); what
    /// is no longer there (a case, a span) falls back to the default.
    fn load(&mut self, model: &Model) {
        let Some(session) = model.session() else {
            return;
        };
        let key = format!("{SETTING}.{}", session.current_line());
        let text = session.setting(&key).ok().flatten();
        // Nothing kept yet: the defaults (the cut-off at 2 km).
        let st: Stored = text
            .as_deref()
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_else(|| Stored {
                modelled: Some(CUTOFF),
                ..Stored::default()
            });
        self.take(model, st);
        self.common.saved = serde_json::to_string(&self.stored(model))
            .ok()
            .map(|json| (key, json));
    }

    /// Takes inputs kept (in the project or with a saved run).
    fn take(&mut self, model: &Model, st: Stored) {
        self.common.take_common(model, st.common);
        let d = Self::default();
        // A current of 0 A along the line was written for every circuit
        // shown before the systems had an operating current (2026-11-04):
        // left out, so that the circuit takes its system's (none: 0 A).
        self.currents = st
            .currents
            .into_iter()
            .filter(|(_, v)| *v != (0.0, false))
            .collect();
        self.ends = st.ends;
        self.phases = st.phases;
        self.earth_typed = st.earth_typed;
        self.earth_wires = st.earth_wires;
        self.towers = st.towers.unwrap_or(d.towers);
        // Kept since 2026-11-01 (0: the whole line); before, as it was.
        // The whole line is no longer offered (owner, 2026-11-12): kept
        // as it, the default distance.
        self.modelled_within = match st.modelled {
            Some(d) if d > 0.0 => Some(d),
            Some(_) => Some(CUTOFF),
            None => st.modelled_within.or(Some(CUTOFF)),
        };
        self.above = Some(Above {
            step: st.above.map_or(2.0, |(_, step)| step),
        });
        self.common.span_pick.above = st.above_spans.into_iter().collect();
        self.mean_heights = st.mean_heights;
    }

    /// Gives a circuit per phase: each phase's label, A and angle (tests).
    #[cfg(test)]
    pub fn set_phases(&mut self, circuit: &str, phases: Vec<PhaseRow>) {
        self.ends.remove(circuit);
        self.phases.insert(circuit.to_owned(), phases);
    }

    /// The circuits given per phase (tests).
    #[cfg(test)]
    pub fn phases(&self, circuit: &str) -> Option<&Vec<PhaseRow>> {
        self.phases.get(circuit)
    }

    /// Takes a circuit's typed phases out: balanced again (tests).
    #[cfg(test)]
    pub fn clear_phases(&mut self, circuit: &str) {
        self.phases.remove(circuit);
    }

    /// An earth wire's typed current, A and degrees (tests).
    #[cfg(test)]
    pub fn earth_typed(&self, section: &str) -> Option<(f64, f64)> {
        self.earth_typed.get(section).copied()
    }

    /// Types an earth wire's current, A and degrees (tests).
    #[cfg(test)]
    pub fn set_earth_typed(&mut self, section: &str, amps: f64, angle_deg: f64) {
        self.earth_typed
            .insert(section.to_owned(), (amps, angle_deg));
    }

    /// Sets a circuit's current, A (tests).
    #[cfg(test)]
    pub fn set_current(&mut self, circuit: &str, amps: f64) {
        self.currents.insert(circuit.to_owned(), (amps, false));
    }

    /// A circuit's current, A (tests).
    #[cfg(test)]
    pub fn current_of(&self, circuit: &str) -> Option<f64> {
        self.currents.get(circuit).map(|c| c.0)
    }

    /// A circuit's current as the window shows it, A: its own, else its
    /// system's operating current, else 0 (tests).
    #[cfg(test)]
    pub fn shown_current(&self, model: &Model, circuit: &str) -> f64 {
        self.current_of(circuit).unwrap_or_else(|| {
            magnetic::circuits(&model.line)
                .iter()
                .find(|c| c.label() == circuit)
                .and_then(|c| model.line.system_current(&c.system))
                .unwrap_or(0.0)
        })
    }

    /// Ticks span `k` only (tests).
    #[cfg(test)]
    pub fn tick_only(&mut self, model: &Model, k: usize) {
        let spans = self.common.spans(model);
        self.common.span_pick.only(model, &spans, k);
        // Its volume and isolines from above ticked too (the tests' views
        // over one span).
        self.common.span_pick.volume.insert(spans[k].clone());
        self.common.span_pick.above.insert(spans[k].clone());
    }

    /// Sets the views (tests): isolines from above every `above` m, the
    /// cross-section panels every `cuts` m, a volume over the first span
    /// every `volume` m.
    #[cfg(test)]
    pub fn set_views(&mut self, above: Option<f64>, cuts: Option<f64>, volume: Option<f64>) {
        self.above = above.map(|step| Above { step });
        self.common.in_cuts = cuts;
        self.common.volume = volume.map(|step| VolumeSetup {
            step,
            width: self.common.width,
        });
        // A volume asked: the 3D volumes page (2026-11-12: a run makes
        // the page shown).
        if volume.is_some() {
            self.common.page = FieldPage::Volumes;
        } else if self.common.page == FieldPage::Volumes {
            self.common.page = FieldPage::Profiles;
        }
    }

    /// The views (tests): from above, the panels, the volume's step.
    #[cfg(test)]
    pub fn views(&self) -> (Option<f64>, Option<f64>, Option<f64>) {
        (
            self.above.as_ref().map(|a| a.step),
            self.common.in_cuts,
            self.common.volume.as_ref().map(|v| v.step),
        )
    }

    /// The earth wires carrying their induced currents (tests).
    #[cfg(test)]
    pub fn set_earth_wires(&mut self, on: bool) {
        self.earth_wires = on;
    }

    /// Gives circuit `label` by its end points (tests): each end's
    /// structure and current in (none: the rest).
    #[cfg(test)]
    pub fn set_ends(&mut self, label: &str, ends: Vec<(String, Option<f64>)>) {
        self.ends.insert(label.to_owned(), ends);
    }

    /// The settings as the engine takes them.
    fn settings(&mut self, model: &Model) -> Settings {
        let spans = self.common.spans(model);
        let stations = match &self.common.stations {
            Stations::EverySpan => match self.common.span_pick.chosen(model, &spans) {
                (_, true) => Vec::new(),
                (chosen, false) => chosen
                    .into_iter()
                    .map(|k| StationSpec {
                        from: spans[k].0.clone(),
                        to: spans[k].1.clone(),
                        fraction: None,
                    })
                    .collect(),
            },
            Stations::One { span, fraction } => spans
                .get(*span)
                .map(|(from, to)| StationSpec {
                    from: from.clone(),
                    to: to.clone(),
                    fraction: *fraction,
                })
                .into_iter()
                .collect(),
            Stations::Beside { distance } => {
                let (mut chosen, _) = self.common.span_pick.chosen(model, &spans);
                chosen.sort_unstable();
                beside_stations(model, &spans, &chosen, *distance)
            }
        };
        Settings {
            model: if self.common.two_d {
                FieldModel::TwoD
            } else {
                FieldModel::ThreeD
            },
            heights_2d: if self.mean_heights {
                Heights2d::Mean
            } else {
                Heights2d::Station
            },
            currents: self
                .currents
                .iter()
                .filter(|(c, _)| !self.ends.contains_key(*c) && !self.phases.contains_key(*c))
                .map(|(c, &(amps, reverse))| CircuitCurrent {
                    circuit: c.clone(),
                    amps,
                    reverse,
                })
                .collect(),
            ends: self
                .ends
                .iter()
                .flat_map(|(c, list)| {
                    list.iter().map(move |(s, a)| magnetic::EndCurrent {
                        circuit: c.clone(),
                        structure: s.clone(),
                        amps: *a,
                    })
                })
                .collect(),
            phase_currents: self
                .phases
                .iter()
                .flat_map(|(c, list)| {
                    list.iter()
                        .map(move |(phase, amps, angle)| magnetic::PhaseCurrent {
                            circuit: c.clone(),
                            phase: phase.clone(),
                            amps: *amps,
                            angle_deg: *angle,
                        })
                })
                .collect(),
            earth_currents: self
                .earth_typed
                .iter()
                .map(|(section, &(amps, angle_deg))| magnetic::EarthCurrent {
                    section: section.clone(),
                    amps,
                    angle_deg,
                    frequency_hz: None,
                })
                .collect(),
            earth_wires: self.earth_wires,
            towers: self.towers,
            jumpers: self.common.jumpers,
            height: self.common.height,
            width: self.common.width,
            step: self.common.step,
            stations,
            profiles: self.common.page == FieldPage::Profiles,
            // No isolines from above (owner, 2026-11-01: dropped).
            grids: Vec::new(),
            grid_step: None,
            piece_length: self.common.piece_length,
            modelled_within: self.modelled_within,
            // The cuts asked in the table (owner, 2026-10-31: "for the 2D
            // cuts it should be asked at which stations we want one").
            cuts: if self.common.page != FieldPage::Cuts {
                Vec::new()
            } else {
                self.common
                    .span_pick
                    .cut_places(model, &spans)
                    .0
                    .into_iter()
                    .map(|at| magnetic::CutSpec {
                        at,
                        width: self.common.cut_width,
                        above: self.common.cut_above,
                        step: self.common.cut_step,
                        label: None,
                    })
                    .chain(self.common.buildings.iter().flat_map(|b| b.cut_specs()))
                    .collect()
            },
            // A volume: one along each run of spans following each other
            // with Volume ticked (continuous across the line angles).
            volumes: match (&self.common.volume, self.common.page) {
                (Some(v), FieldPage::Volumes) => self
                    .common
                    .span_pick
                    .volume_runs(model, &spans)
                    .into_iter()
                    .map(|((from, to), _)| magnetic::VolumeSpec {
                        from,
                        to,
                        width: v.width,
                        above: views::ABOVE,
                        step: v.step,
                    })
                    .collect(),
                _ => Vec::new(),
            },
            levels: crate::field_preset::with_levels(
                crate::field_overlay::shown(&self.common.levels)
                    .iter()
                    .map(|l| l.value)
                    .collect(),
                self.common
                    .buildings
                    .as_ref()
                    .map(|b| b.levels_ut.as_slice()),
            ),
            // The CSV's points with the profiles; the buildings' places
            // with the cuts (2026-11-12: their page).
            points: {
                let mut v = if self.common.page == FieldPage::Profiles {
                    self.common
                        .points
                        .as_ref()
                        .map(|p| p.1.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                if self.common.page == FieldPage::Cuts
                    && let Some(b) = &self.common.buildings
                {
                    v.extend(b.points());
                }
                v
            },
            limit_ut: self.common.limit,
        }
    }

    /// Starts a run on the case's solved sections.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        // The inputs run with: kept in the project (the next frame).
        self.common.commit = true;
        let Some(solved) = model.cases.get(self.common.case) else {
            self.common.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        // By the window's calculation: the case solved for the run.
        let line =
            ohl_clearance::calculation::line_for(&model.line, self.common.calculation).into_owned();
        let library = model.library.clone();
        let mut shapes = solved.shapes.clone();
        let mut jumpers = solved.jumpers.clone();
        let case = solved.case.clone();
        // A case the app has not solved yet (it solves a case when it is
        // shown): solved for the run, in the background (2026-10-31: the
        // run took no wire and gave 0 kV/m).
        let unsolved = !solved.solved_for(model.wind) || self.common.calculation.is_some();
        // The systems in cases or calculations of their own (2026-11-01).
        let plan = crate::field_systems::plan(
            model,
            &self.common.per_system,
            &solved.case,
            self.common.calculation,
        );
        let wind = model.wind;
        // The ground chosen (2026-11-01), read in the background.
        let ground_source = crate::field_ground::source(model, &self.common.ground_from);
        let settings = self.settings(model);
        // Each kind of output its own step, one after another (owner,
        // 2026-11-01: "not all at once and clearly separated"): the cuts
        // and the volumes from the wires in 3D, the profiles and points by
        // their model.
        let steps = steps_of(&settings);
        self.run_levels = settings.levels.clone();
        let progress = Arc::new(Progress::default());
        self.common.progress = Some(progress.clone());
        // A new run's outputs all drawn.
        self.common.hidden.clear();
        self.common.saving.stale = None;
        self.common.job.start_at(ctx, model.revision(), move || {
            progress.plan_steps(field_common::steps_planned(
                &steps
                    .iter()
                    .map(|(_, s)| (s.cuts.len(), s.volumes.len()))
                    .collect::<Vec<_>>(),
                unsolved,
                0,
            ));
            if unsolved {
                progress.next_step(format!("solving the weather case {}", case.name));
                let s = crate::model::Solved::with_wind(&line, &library, case.clone(), wind);
                shapes = s.shapes;
                jumpers = s.jumpers;
            }
            let (ground, ground_note) = ground_source.read()?;
            let (shapes, per_notes) =
                crate::field_systems::compose(&line, &library, wind, shapes, &plan);
            let height = |x: f64, y: f64| ground.as_ref().and_then(|t| t.height(x, y));
            let g: Option<&(dyn Fn(f64, f64) -> Option<f64> + Sync)> = ground
                .as_ref()
                .map(|_| &height as &(dyn Fn(f64, f64) -> Option<f64> + Sync));
            let src = ohl_electrical::Sources {
                line: &line,
                library: &library,
                shapes: &shapes,
                temperature: case.temperature,
                ground: g,
                jumpers: &jumpers,
            };
            let mut field: Option<MagneticField> = None;
            let mut wires = None;
            for (what, s) in &steps {
                // The engine steps through cuts and volumes itself.
                if s.cuts.is_empty() && s.volumes.is_empty() {
                    progress.next_step(what.clone());
                }
                let (f, w) = magnetic::run_keeping(&src, s, Some(&progress))?;
                if w.is_some() {
                    wires = w;
                }
                field = Some(match field {
                    None => f,
                    Some(mut all) => {
                        all.profiles.extend(f.profiles);
                        all.points.extend(f.points);
                        all.cuts.extend(f.cuts);
                        all.volumes.extend(f.volumes);
                        if s.profiles {
                            all.model = f.model;
                        }
                        for n in f.notes {
                            if !all.notes.contains(&n) {
                                all.notes.push(n);
                            }
                        }
                        all
                    }
                });
            }
            progress.set_stage(String::new());
            let mut field = field.ok_or("nothing asked: tick a span, a cut or a volume")?;
            let work = progress.total.load(Ordering::Relaxed) as f64
                * wires.as_ref().map_or(0, |w| w.count()) as f64;
            field_common::add_run_notes(
                &mut field.notes,
                &line,
                &shapes,
                per_notes,
                ground_note,
                g.is_some(),
            );
            let meshes = views::take_meshes(&mut field.volumes);
            Ok(Answer {
                field,
                case: case.name.clone(),
                wires: wires.map(Arc::new),
                meshes,
                work,
            })
        });
    }

    /// The saved run `r` shown: its inputs set, its field and surfaces,
    /// with what changed on the line since it was saved.
    pub fn load_saved(&mut self, r: &CheckReport, model: &Model) -> Result<(), String> {
        let (case, mut field, meshes, with) =
            crate::field_saved::read::<MagneticField>(&r.results)?;
        if let Some(st) = crate::field_saved::inputs::<Stored>(&r.parameters) {
            self.take(model, st);
        }
        if !with && !field.volumes.is_empty() {
            field.notes.push(
                "Saved without its volumes' surfaces (too large): run it again to see them.".into(),
            );
        }
        self.common.job.stop();
        self.common
            .job
            .stamp
            .set(crate::revision::saved_revision(&r.parameters, model));
        self.common.job.result = Some(Ok(Answer {
            field,
            case,
            wires: None,
            meshes,
            work: 0.0,
        }));
        self.common.job.generation += 1;
        self.common.hidden.clear();
        self.wires = None;
        self.common.saving.stale = crate::field_saved::changed_since(r, model);
        self.common.saving.name = r.id.clone();
        self.common.open = true;
        Ok(())
    }

    /// The run shown as report `name` of the performed checks.
    fn check_report(&mut self, model: &Model, name: &str) -> Result<CheckReport, String> {
        let inputs = self.stored(model);
        let Some(Ok(a)) = &self.common.job.result else {
            return Err("run it first".into());
        };
        let (results, triangles) = crate::field_saved::results(&a.case, &a.field, &a.meshes)?;
        let largest = a
            .field
            .profiles
            .iter()
            .map(|p| p.max_ut)
            .fold(0.0, f64::max);
        let summary = format!(
            "\"{}\": {} profile(s), largest {largest:.2} µT{}; {} cut(s), {} volume(s){}, {} \
             from above",
            a.case,
            a.field.profiles.len(),
            a.field
                .limit_ut
                .map_or(String::new(), |l| format!(" (limit {l} µT)")),
            a.field.cuts.len(),
            a.field.volumes.len(),
            if triangles > crate::field_saved::MAX_SAVED_TRIANGLES {
                " (surfaces not saved)"
            } else {
                ""
            },
            a.field.grids.len(),
        );
        crate::field_saved::report(
            model,
            (crate::field_saved::MAGNETIC_CHECK_ID, "Magnetic field"),
            name,
            summary,
            crate::field_saved::parameters(&inputs, self.common.job.stamp.loads_per(model)),
            results,
        )
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) {
        let generation = self.common.job.generation;
        self.common.job.poll();
        if !self.common.job.running() {
            self.common.progress = None;
        }
        if self.common.job.generation != generation {
            // A new answer: its wires for the panels, its speed and its
            // volumes' share for the estimates.
            self.wires = None;
            if let Some(Ok(a)) = &self.common.job.result {
                self.wires = a.wires.clone();
                if let Some(t) = self.common.job.took
                    && a.work > 0.0
                {
                    self.common.rate = Some(t.as_secs_f64() / a.work);
                }
                let (c, f) = a.field.volumes.iter().fold((0usize, 0usize), |s, v| {
                    (s.0 + v.values.computed, s.1 + v.values.full)
                });
                if f > 0 {
                    self.common.volume_share = Some(c as f64 / f as f64);
                }
            }
        }
        if !self.common.open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let stale = self.out_of_date(model);
        help::window(
            ctx,
            crate::dockable::Panel::MagneticField,
            "Magnetic field",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
                let why = if names.is_empty() {
                    Some("no weather case in the model".to_owned())
                } else if !self
                    .currents
                    .iter()
                    .any(|(c, v)| v.0 > 0.0 && !self.ends.contains_key(c))
                    && !self
                        .ends
                        .values()
                        .flatten()
                        .any(|e| e.1.is_some_and(|a| a != 0.0))
                    && !self.phases.values().flatten().any(|p| p.1 > 0.0)
                    && !self.earth_typed.values().any(|e| e.0 > 0.0)
                {
                    Some("give a circuit a current (2 Scope)".to_owned())
                } else {
                    self.common.page_blocker(model)
                };
                // Two tabs (owner, 2026-10-27): the setup scrolls, the
                // results fill theirs; Run in the footer, always visible.
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    None,
                    self.common.job.generation,
                    why.is_some() && !self.common.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        // Three pages (owner, 2026-11-12), each with only
                        // what its output needs; Run makes the page shown.
                        field_common::page_ui(ui, &mut self.common.page);
                        ui.separator();
                        let page = self.common.page;
                        if let Some(a) =
                            check_ui::section(ui, 1, "What it computes", Some(page.guide()))
                        {
                            self.common.guide = Some(a);
                        }
                        let (short, long) = what_it_computes(page);
                        help::explain(ui, short, long);
                        ui.horizontal(|ui| match page {
                            FieldPage::Profiles => {
                                crate::field_help::profile(
                                    ui,
                                    crate::field_help::Field::Magnetic,
                                    self.common.width,
                                    self.common.step,
                                    self.common.height,
                                );
                                crate::field_help::sources(
                                    ui,
                                    crate::field_help::Field::Magnetic,
                                    false,
                                );
                            }
                            FieldPage::Cuts => {
                                crate::field_help::cut(ui, self.common.cut_step);
                            }
                            FieldPage::Volumes => {
                                crate::field_help::sources(
                                    ui,
                                    crate::field_help::Field::Magnetic,
                                    false,
                                );
                            }
                        });
                        ui.separator();
                        if let Some(a) = check_ui::section(ui, 2, "Scope", Some(page.guide())) {
                            self.common.guide = Some(a);
                        }
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .route_of(model)
                            .cases(check_ui::Cases::One(&names, &mut self.common.case))
                            .ui(ui);
                        // How the wires are solved (owner, 2026-11-01).
                        ui.horizontal(|ui| {
                            ui.label("Wires by");
                            crate::rule_table::calculation_combo(
                                ui,
                                "field-calculation",
                                &mut self.common.calculation,
                            );
                            // Its explanation behind the (?) (owner,
                            // 2026-11-12: no grey lines between fields).
                            help::question(ui, field_common::WIRES_BY_HELP);
                        });
                        crate::field_systems::ui(ui, WINDOW, model, &mut self.common.per_system);
                        crate::field_ground::ui(ui, WINDOW, model, &mut self.common.ground_from);
                        self.inputs_ui(ui, model);
                        ui.separator();
                        if let Some(a) =
                            check_ui::section(ui, 3, "What and where", Some(page.guide()))
                        {
                            self.common.guide = Some(a);
                        }
                        match page {
                            FieldPage::Profiles => {
                                field_common::profiles_method_ui(
                                    ui,
                                    &mut self.common.two_d,
                                    &PROFILES_3D,
                                    &PROFILES_2D,
                                );
                                if self.common.two_d {
                                    ui.checkbox(
                                        &mut self.mean_heights,
                                        "wires at the span's mean height",
                                    )
                                    .on_hover_text(
                                        "Off: the wires where they cross the profile (at the \
                                         lowest point by default, the larger field). On: the \
                                         span's mean height, as the line parameters take it — \
                                         less than under the lowest point (not on the safe side)",
                                    );
                                }
                            }
                            FieldPage::Cuts => field_common::cuts_method_ui(ui, &CUTS_3D),
                            FieldPage::Volumes => {}
                        }
                        self.spans_ui(ui, model);
                        self.views_ui(ui, model);
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        egui::ScrollArea::both()
                            .id_salt("magnetic-field-results")
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.results_ui(ui, model, &stale, &mut run));
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    let running = self.common.progress.as_ref().map(|p| {
                        field_common::run_progress(p, self.common.job.elapsed().unwrap_or_default())
                    });
                    let f = check_ui::RunFooter {
                        running,
                        why_not: why.clone(),
                        hover: "Compute the field from the wires as solved in the weather case \
                                chosen",
                        scope: None,
                        info: Some(match self.common.job.took {
                            Some(t) => format!("{} · last run {t:.1?}", self.summary(model)),
                            None => self.summary(model),
                        }),
                    };
                    match check_ui::run_footer(ui, f, |_| {}) {
                        check_ui::Action::Run => run = true,
                        check_ui::Action::Stop => {
                            if let Some(p) = &self.common.progress {
                                p.cancel.store(true, Ordering::Relaxed);
                            }
                        }
                        check_ui::Action::None => {}
                    }
                });
            },
        );
        // Closed: its inputs kept in the project (the next frame).
        if self.common.open && !open {
            self.common.commit = true;
        }
        self.common.open = open;
        if run {
            self.start(model, ctx);
        }
    }

    fn inputs_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let circuits = magnetic::circuits(&model.line);
        if circuits.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No section carries a system's phase: assign the systems (Line › Electrical \
                     systems) to get circuits.",
                )
                .color(ui.visuals().warn_fg_color),
            );
        }
        egui::Grid::new("magnetic-field-currents")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                for h in ["circuit", "system", "given", "current", "load flow"] {
                    ui.strong(h);
                }
                ui.end_row();
                let version = model.line_version;
                if self.ends_seen.as_ref().is_none_or(|(v, _)| *v != version) {
                    let list = magnetic::circuit_ends(&model.line)
                        .into_iter()
                        .map(|(k, e)| (k.label(), e))
                        .collect();
                    self.ends_seen = Some((version, list));
                }
                let seen = self
                    .ends_seen
                    .as_ref()
                    .map(|s| s.1.clone())
                    .unwrap_or_default();
                for c in &circuits {
                    let label = c.label();
                    ui.label(&label);
                    ui.label(&c.system);
                    let points: Vec<String> = seen
                        .iter()
                        .find(|(l, _)| *l == label)
                        .map(|(_, e)| e.clone())
                        .unwrap_or_default();
                    let was = if self.phases.contains_key(&label) {
                        Given::Phases
                    } else if self.ends.contains_key(&label) {
                        Given::Ends
                    } else {
                        Given::Circuit
                    };
                    let mut given_as = was;
                    egui::ComboBox::from_id_salt(("magnetic-field-given", &label))
                        .selected_text(match given_as {
                            Given::Circuit => "the circuit",
                            Given::Ends => "at its end points",
                            Given::Phases => PER_PHASE_LABEL,
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut given_as, Given::Circuit, "the circuit");
                            ui.add_enabled_ui(points.len() >= 2, |ui| {
                                ui.selectable_value(
                                    &mut given_as,
                                    Given::Ends,
                                    "at its end points",
                                );
                            });
                            ui.selectable_value(&mut given_as, Given::Phases, PER_PHASE_LABEL);
                        })
                        .response
                        .on_hover_text(PER_PHASE_HELP);
                    let by_ends = given_as == Given::Ends;
                    if given_as == Given::Phases && was != Given::Phases {
                        // From the balanced current it had: each phase at
                        // it, at its system's angle (against the line:
                        // turned by 180°).
                        let (amps, reverse) = self.currents.get(&label).copied().unwrap_or((
                            model.line.system_current(&c.system).unwrap_or(0.0),
                            false,
                        ));
                        self.ends.remove(&label);
                        self.phases
                            .insert(label.clone(), balanced_rows(model, c, amps, reverse));
                    } else if given_as != Given::Phases {
                        self.phases.remove(&label);
                    }
                    if let Some(rows) = self.phases.get_mut(&label) {
                        phases_ui(ui, rows);
                        ui.label("");
                        ui.end_row();
                        continue;
                    }
                    if by_ends && !self.ends.contains_key(&label) {
                        // In at the first end, the rest out at the others.
                        let amps = self.currents.get(&label).map_or_else(
                            || model.line.system_current(&c.system).unwrap_or(0.0),
                            |c| c.0,
                        );
                        let mut list: Vec<(String, Option<f64>)> =
                            points.iter().map(|p| (p.clone(), Some(0.0))).collect();
                        if let Some(first) = list.first_mut() {
                            first.1 = Some(amps);
                        }
                        if let Some(last) = list.last_mut() {
                            last.1 = None;
                        }
                        self.ends.insert(label.clone(), list);
                    } else if !by_ends {
                        self.ends.remove(&label);
                    }
                    if let Some(list) = self.ends.get_mut(&label) {
                        // The ends as the line has them now.
                        list.retain(|(s, _)| points.contains(s));
                        for p in &points {
                            if !list.iter().any(|(s, _)| s == p) {
                                list.push((p.clone(), Some(0.0)));
                            }
                        }
                        ui.vertical(|ui| {
                            let mut rest_at = list.iter().position(|e| e.1.is_none());
                            for (k, (s, a)) in list.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("at {s}"));
                                    let mut rest = rest_at == Some(k);
                                    if ui
                                        .checkbox(&mut rest, "the rest")
                                        .on_hover_text("The others' sum, out (one end at most)")
                                        .changed()
                                    {
                                        rest_at = rest.then_some(k);
                                    }
                                    if rest_at == Some(k) {
                                        *a = None;
                                    } else {
                                        let v = a.get_or_insert(0.0);
                                        crate::style::field(
                                            ui,
                                            egui::DragValue::new(v)
                                                .range(-100_000.0..=100_000.0)
                                                .speed(5.0)
                                                .suffix(" A"),
                                        )
                                        .on_hover_text(
                                            "rms, each phase, into the line here (negative: out \
                                             of it)",
                                        );
                                    }
                                });
                            }
                            // Only one "rest".
                            for (k, e) in list.iter_mut().enumerate() {
                                if e.1.is_none() && rest_at != Some(k) {
                                    e.1 = Some(0.0);
                                }
                            }
                        });
                        ui.label("");
                        ui.end_row();
                        continue;
                    }
                    // Its system's operating current (Line › Electrical
                    // systems, 2026-11-04) is the default; a value given
                    // here overrides it (kept only once changed).
                    let line_amps = model.line.system_current(&c.system);
                    let given = self.currents.get(&label).copied();
                    let default = (line_amps.unwrap_or(0.0), false);
                    let mut value = given.unwrap_or(default);
                    let mut back = false;
                    ui.horizontal(|ui| {
                        crate::style::field(
                            ui,
                            egui::DragValue::new(&mut value.0)
                                .range(0.0..=100_000.0)
                                .speed(5.0)
                                .suffix(" A"),
                        )
                        .on_hover_text(
                            "rms, each phase; balanced phases at the system's phase angles \
                             (L1 0°, L2 −120°, L3 +120° unless the system gives others). The \
                             field grows in proportion. By default the system's operating \
                             current (Line › Electrical systems, column A); a value typed here \
                             overrides it for this window",
                        );
                        match (given, line_amps) {
                            (None, Some(_)) => {
                                ui.label(egui::RichText::new("the line's").small().weak())
                                    .on_hover_text(
                                        "The system's operating current from the line \
                                         (Line › Electrical systems, column A)",
                                    );
                            }
                            (Some(g), Some(i)) if g.0 != i => {
                                back = ui
                                    .small_button("↺")
                                    .on_hover_text(format!(
                                        "Back to the system's operating current, {i} A"
                                    ))
                                    .clicked();
                            }
                            _ => {}
                        }
                    });
                    let mut along = !value.1;
                    egui::ComboBox::from_id_salt(("magnetic-field-dir", &label))
                        .selected_text(if along {
                            "along the line"
                        } else {
                            "against the line"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut along, true, "along the line");
                            ui.selectable_value(&mut along, false, "against the line");
                        })
                        .response
                        .on_hover_text(
                            "The load flow's direction: along the line (from its first structure \
                             to its last) or against it — every phase turned by 180°. Which \
                             combination governs is the ordinance's question",
                        );
                    value.1 = !along;
                    if back {
                        self.currents.remove(&label);
                    } else if Some(value) != given && (given.is_some() || value != default) {
                        self.currents.insert(label.clone(), value);
                    }
                    ui.end_row();
                }
            });
        self.earth_typed_ui(ui, model);
        ui.add_space(4.0);
        egui::Grid::new("magnetic-field-settings")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Limit [µT]");
                ui.horizontal(|ui| {
                    let mut given = self.common.limit.is_some();
                    ui.checkbox(&mut given, "given").on_hover_text(
                        "Your limit, µT: the margin at every point and the limit line's \
                             extent per profile. None is built in (an ordinance's values come \
                             from its parameter file)",
                    );
                    if !given {
                        self.common.limit = None;
                    } else {
                        let l = self.common.limit.get_or_insert(1.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(l)
                                .range(0.001..=10_000.0)
                                .speed(0.01)
                                .suffix(" µT"),
                        );
                    }
                });
                ui.end_row();
                if self.common.page == FieldPage::Profiles {
                    ui.label("Points");
                    ui.horizontal(|ui| {
                        if ui
                            .button("Points from CSV…")
                            .on_hover_text(
                                "A CSV file with a header naming x, y and z (elevation) or height \
                             (above the ground), optionally name: e.g. places of sensitive use",
                            )
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("CSV", &["csv", "txt"])
                                .pick_file()
                        {
                            match std::fs::read_to_string(&path)
                                .map_err(|e| e.to_string())
                                .and_then(|t| magnetic::parse_points(&t))
                            {
                                Ok(p) => {
                                    let name = path.file_name().map_or(String::new(), |n| {
                                        n.to_string_lossy().into_owned()
                                    });
                                    self.common.points = Some((name, p));
                                    self.common.points_text = std::fs::read_to_string(&path).ok();
                                    self.common.points_error = None;
                                }
                                Err(e) => {
                                    self.common.points_error =
                                        Some(format!("{}: {e}", path.display()))
                                }
                            }
                        }
                        if let Some((name, p)) = &self.common.points {
                            ui.label(format!("{} points from {name}", p.len()));
                            if ui.small_button("Clear").clicked() {
                                self.common.points = None;
                                self.common.points_text = None;
                            }
                        }
                    });
                    ui.end_row();
                }
                if self.common.page == FieldPage::Cuts
                    && crate::field_preset::row(ui, &mut self.common.buildings, true)
                {
                    self.common.commit = true;
                }
            });
        if let Some(e) = &self.common.points_error {
            crate::menu::error_label(ui, e.as_str());
        }
    }

    /// The page computes by the 2D-only method (the profiles, 2D only).
    pub(super) fn two_d_page(&self) -> bool {
        self.common.page == FieldPage::Profiles && self.common.two_d
    }

    /// The modelling settings of the inputs (owner, 2026-11-12: in 4 Model,
    /// folded, with the wire pieces and the wires modelled): the earth
    /// wires, the jumpers (3D).
    pub(super) fn model_inputs_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("magnetic-field-model-inputs")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Earth wires");
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut self.earth_wires,
                        "carry the currents the phases induce",
                    )
                    .on_hover_text(
                        "On: the earth wires carry the currents the phases induce in them; this may \
                         raise the field as well as lower it (run both when it matters). A network \
                         solve over the line. Off: no current in the earth wires",
                    );
                    if self.earth_wires {
                        ui.checkbox(&mut self.towers, "through the structures' earthing")
                            .on_hover_text(
                                "On: the earth wires as a network over the line, each structure \
                                 leading current into the ground through its earthing resistance \
                                 (Structures table, earthing Ω; else the line's, else 10 Ω \
                                 assumed), the structures' currents in the field (3D). Off: an \
                                 infinitely long earth wire earthed everywhere, I_e = −inv(Z_ee) · \
                                 Z_ep · I_p per span",
                            );
                    }
                });
                ui.end_row();
                // The jumpers: in the 3D model only.
                if !self.two_d_page() {
                    ui.label("Jumpers");
                    ui.checkbox(&mut self.common.jumpers, "carry their phase's current")
                        .on_hover_text(
                            "A jumper between two section ends of one phase carries that phase's \
                             current, as solved in the weather case (3D). Off: left out (not \
                             conservative near the structures). Jumpers on taps or between \
                             different phases are left out, said",
                        );
                    ui.end_row();
                }
            });
    }

    fn spans_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let spans = self.common.spans(model);
        self.common.spans_ui(ui, "magnetic-field", model, &spans);
        // The cuts' or the volumes' sizes (the profiles': in the table's
        // setup).
        if self.common.page != FieldPage::Profiles {
            self.sizes_ui(ui, model, &spans);
        }
    }

    /// The Results list (owner, 2026-10-31, validated): every profile,
    /// cut, volume and grid of `a` with its largest value; Plot, Open cut,
    /// Show in 3D.
    fn output_rows(&self, model: &Model, a: &Answer) -> Vec<ResultRow> {
        let mut rows = Vec::new();
        for (k, p) in a.field.profiles.iter().enumerate() {
            rows.push(ResultRow {
                kind: "Profile",
                place: format!("{}–{} at {:.0} m", p.from, p.to, p.at),
                max: format!("{:.3} µT", p.max_ut),
                note: format!("at {:.0} m from the axis", p.max_offset),
                button: "Show plot",
                action: RowAction::Plot(k),
                drawn: None,
            });
        }
        for (k, c) in a.field.cuts.iter().enumerate() {
            let largest = c.values.values.iter().copied().fold(0.0, f64::max);
            rows.push(ResultRow {
                kind: "Cut",
                place: format!(
                    "{} at {:.0} m{} (span {}–{})",
                    c.alignment,
                    c.station,
                    if c.angle_point { ", angle point" } else { "" },
                    c.from,
                    c.to
                ),
                max: format!("{largest:.1} µT"),
                note: "at the wires; isolines at the levels".into(),
                button: "Open cut",
                action: field_results::open_cut(c, self.common.cut_width),
                drawn: Some(k),
            });
        }
        let per = a
            .field
            .volumes
            .first()
            .map_or(1, |v| v.values.surfaces.len().max(1));
        for (k, v) in a.field.volumes.iter().enumerate() {
            let meshes = a
                .meshes
                .iter()
                .skip(k * per)
                .take(per)
                .map(|m| m.1.as_ref());
            let levels: Vec<String> = v
                .values
                .surfaces
                .iter()
                .map(|s| format!("{}", s.0))
                .collect();
            let left: usize = v.values.left_out.iter().sum();
            rows.push(ResultRow {
                kind: "Volume",
                place: format!("{}–{}", v.from, v.to),
                max: format!("{} µT", levels.join(", ")),
                note: if left > 0 {
                    format!("{left} thin piece(s) left out")
                } else {
                    String::new()
                },
                button: "Show in 3D",
                action: match field_results::centre(meshes) {
                    Some(c) => {
                        field_results::look(model, &v.from, &v.to, c, v.values.bounds.path.length())
                    }
                    None => RowAction::None,
                },
                drawn: Some(k),
            });
        }
        for (k, g) in a.field.grids.iter().enumerate() {
            let largest = g.points.iter().map(|p| p.b_ut).fold(0.0, f64::max);
            let at = |id: &str| model.line.structure(id).map(|s| s.position);
            let action = match (at(&g.from), at(&g.to)) {
                (Some(p), Some(q)) => {
                    let mid = std::array::from_fn(|k| 0.5 * (p[k] + q[k]));
                    let len = (q[0] - p[0]).hypot(q[1] - p[1]);
                    field_results::look(model, &g.from, &g.to, mid, len)
                }
                _ => RowAction::None,
            };
            rows.push(ResultRow {
                kind: "From above",
                place: format!("{}–{}", g.from, g.to),
                max: format!("{largest:.2} µT"),
                note: format!("{} × {} points", g.rows, g.columns),
                button: "Show in 3D",
                action,
                drawn: Some(k),
            });
        }
        rows
    }

    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        stale: &Option<String>,
        run: &mut bool,
    ) {
        // The outputs listed first (owner, 2026-10-31): a click shows one.
        if let Some(Ok(a)) = &self.common.job.result {
            let rows = self.output_rows(model, a);
            match field_results::list(ui, "magnetic-field", &rows, &mut self.common.hidden) {
                Some(RowAction::Plot(k)) => {
                    self.common.shown = k;
                    self.common.reveal = true;
                }
                Some(RowAction::Ask(r)) => self.common.requests.push(r),
                _ => {}
            }
            ui.separator();
        }
        // Save in Performed checks (owner, 2026-11-01).
        if matches!(self.common.job.result, Some(Ok(_)))
            && let Some(name) = self.common.saving.ui(ui, model)
        {
            let r = self.check_report(model, &name).map(|report| {
                self.common.apply =
                    Some((format!("saved the magnetic field {name}"), report.put()));
            });
            self.common.saving.outcome(&name, &r);
        }
        let answer = match &self.common.job.result {
            None => {
                ui.label("Run to compute the field.");
                return;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e.as_str());
                return;
            }
            Some(Ok(a)) => a,
        };
        if let Some(what) = stale {
            *run |= crate::revision::banner(ui, what, !self.common.job.running());
        }
        let m = &answer.field;
        ui.horizontal(|ui| {
            ui.label(format!(
                "In \"{}\"; {}; {} m above the ground{}",
                answer.case,
                m.model.describe(),
                m.height,
                m.limit_ut
                    .map_or(String::new(), |l| format!("; limit {l} µT"))
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("magnetic-field.csv", &magnetic::csv(m));
            }
        });
        // The 2D-only method's assumptions said with its results.
        if m.model == FieldModel::TwoD && !m.profiles.is_empty() {
            ui.colored_label(ui.visuals().warn_fg_color, field_common::TWO_D_ASSUMES);
        }
        for c in &m.currents {
            ui.label(
                egui::RichText::new(if c.source == magnetic::PER_PHASE_SOURCE {
                    // Typed per phase: the phases and their net current are
                    // in the notes.
                    format!(
                        "circuit {}: per phase, the largest {:.1} A, {} Hz (each phase and \
                         the net current: the notes)",
                        c.circuit, c.amps, c.frequency_hz
                    )
                } else {
                    format!(
                        "circuit {}: {:.1} A, {} Hz, {}",
                        c.circuit,
                        c.amps,
                        c.frequency_hz,
                        if c.reverse {
                            "against the line"
                        } else {
                            "along the line"
                        }
                    )
                })
                .small(),
            );
        }
        for e in &m.earth_wires {
            ui.label(
                egui::RichText::new(format!(
                    "earth wire {} {}–{}: {:.1} A at {:.1}°",
                    e.section, e.from, e.to, e.amps, e.angle_deg
                ))
                .small(),
            );
        }
        for t in &m.towers {
            ui.label(
                egui::RichText::new(format!(
                    "structure {}: {:.1} A at {:.1}° into the ground through {} Ω{}",
                    t.structure,
                    t.amps,
                    t.angle_deg,
                    t.resistance_ohm,
                    if t.assumed { " (assumed)" } else { "" }
                ))
                .small(),
            );
        }
        for n in &m.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        self.views_results_ui(ui, m, &answer.meshes);
        if !m.profiles.is_empty() {
            ui.separator();
            self.common.shown = self.common.shown.min(m.profiles.len() - 1);
            let header = ui
                .horizontal(|ui| {
                    ui.strong("Profile");
                    let name =
                        |p: &magnetic::Profile| format!("{}–{} at {:.1} m", p.from, p.to, p.at);
                    egui::ComboBox::from_id_salt("magnetic-field-profile")
                        .selected_text(name(&m.profiles[self.common.shown]))
                        .show_ui(ui, |ui| {
                            for (k, p) in m.profiles.iter().enumerate() {
                                ui.selectable_value(&mut self.common.shown, k, name(p));
                            }
                        });
                })
                .response
                .rect;
            let drawn = {
                let p = &m.profiles[self.common.shown];
                let values: Vec<(f64, f64)> = p.points.iter().map(|q| (q.offset, q.b_ut)).collect();
                let major: Vec<(f64, f64)> =
                    p.points.iter().map(|q| (q.offset, q.major_ut)).collect();
                field_common::plot(
                    ui,
                    &field_common::ProfilePlot {
                        values: &values,
                        faint: Some(&major),
                        axis: "B µT",
                        unit: "µT",
                        hover: &|k| {
                            let q = &p.points[k];
                            format!(
                                "{:.1} m: {:.4} µT (largest over the cycle {:.4})",
                                q.offset, q.b_ut, q.major_ut
                            )
                        },
                    },
                    m.limit_ut,
                )
            };
            // The list's Show plot (2026-11-01: the plot sat below the
            // notes and views, out of sight): the results scroll to it.
            if std::mem::take(&mut self.common.reveal) {
                ui.scroll_to_rect(header.union(drawn), Some(egui::Align::TOP));
            }
            help::list(
                ui,
                "magnetic-field-profiles",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("magnetic-field-profiles-scroll"),
                |ui| {
                    egui::Grid::new("magnetic-field-profiles")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in [
                                "span",
                                "station m",
                                "along the line m",
                                "max µT",
                                "at m",
                                "limit line m",
                                "",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (k, p) in m.profiles.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        k == self.common.shown,
                                        format!("{}–{}", p.from, p.to),
                                    )
                                    .clicked()
                                {
                                    self.common.shown = k;
                                }
                                ui.label(format!("{:.1}", p.at));
                                ui.label(format!("{:.1}", p.along_line));
                                ui.label(format!("{:.3}", p.max_ut));
                                ui.label(format!("{:.1}", p.max_offset));
                                ui.label(match p.limit {
                                    Some(l) => format!(
                                        "{:.1} … {:.1}{}",
                                        l.left,
                                        l.right,
                                        if l.beyond { " (beyond)" } else { "" }
                                    ),
                                    None if m.limit_ut.is_some() => "below the limit".into(),
                                    None => String::new(),
                                });
                                if p.warnings.is_empty() {
                                    ui.label("");
                                } else {
                                    ui.label(
                                        egui::RichText::new(format!("⚠ {}", p.warnings.len()))
                                            .color(ui.visuals().warn_fg_color),
                                    )
                                    .on_hover_text(p.warnings.join("\n"));
                                }
                                ui.end_row();
                            }
                        });
                },
            );
            for w in &m.profiles[self.common.shown].warnings {
                ui.label(
                    egui::RichText::new(w)
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            }
        }
        if !m.points.is_empty() {
            ui.separator();
            ui.strong("Points");
            help::list(
                ui,
                "magnetic-field-points",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("magnetic-field-points-scroll"),
                |ui| {
                    egui::Grid::new("magnetic-field-points")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in [
                                "name",
                                "x m",
                                "y m",
                                "z m",
                                "height m",
                                "B µT",
                                "largest µT",
                                "margin µT",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for p in &m.points {
                                ui.label(&p.name);
                                ui.label(format!("{:.2}", p.x));
                                ui.label(format!("{:.2}", p.y));
                                ui.label(format!("{:.2}", p.z));
                                ui.label(format!("{:.2}", p.height));
                                ui.label(format!("{:.4}", p.b_ut));
                                ui.label(format!("{:.4}", p.major_ut));
                                match p.margin_ut {
                                    Some(x) if x < 0.0 => {
                                        ui.label(
                                            egui::RichText::new(format!("{x:.4}"))
                                                .color(ui.visuals().error_fg_color),
                                        );
                                    }
                                    Some(x) => {
                                        ui.label(format!("{x:.4}"));
                                    }
                                    None => {
                                        ui.label("—");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                },
            );
        }
    }
}

/// A run's steps (owner, 2026-11-01): 2D — the cuts (the wires in 3D),
/// then the profiles and points (by their model); 3D — the volumes. Each
/// with what it computes in words; none empty.
fn steps_of(all: &Settings) -> Vec<(String, Settings)> {
    let none = Settings {
        profiles: false,
        stations: Vec::new(),
        points: Vec::new(),
        cuts: Vec::new(),
        volumes: Vec::new(),
        grids: Vec::new(),
        ..all.clone()
    };
    let mut out = Vec::new();
    if !all.cuts.is_empty() {
        out.push((
            format!("{} cut(s)", all.cuts.len()),
            Settings {
                model: FieldModel::ThreeD,
                cuts: all.cuts.clone(),
                // The buildings' places with the cuts (no profile asked).
                points: if all.profiles {
                    Vec::new()
                } else {
                    all.points.clone()
                },
                ..none.clone()
            },
        ));
    }
    if all.profiles {
        out.push((
            "the profiles".to_owned(),
            Settings {
                profiles: true,
                stations: all.stations.clone(),
                points: all.points.clone(),
                ..none.clone()
            },
        ));
    }
    if !all.volumes.is_empty() {
        out.push((
            format!("{} volume(s)", all.volumes.len()),
            Settings {
                model: FieldModel::ThreeD,
                volumes: all.volumes.clone(),
                ..none
            },
        ));
    }
    out
}

/// What a page computes: its line and what is behind its (?).
fn what_it_computes(page: FieldPage) -> (&'static str, &'static str) {
    const ALL: &str = "The magnetic flux density (µT, the rms of the vector) of the line's \
        currents, from the wires where they hang in a weather case. A limit is yours to give; \
        none is built in. Built, in active development, not approved yet.";
    match page {
        FieldPage::Profiles => (
            "B (µT) across each span ticked, at a height above the ground, and at points from \
             CSV: plotted and listed in Results.",
            ALL,
        ),
        FieldPage::Cuts => (
            "Isolines of B (µT) in vertical planes across the line at the stations asked: \
             Results › Open cut shows each on the wires.",
            ALL,
        ),
        FieldPage::Volumes => (
            "Surfaces of B (µT) at the levels around the wires over the spans with Volume \
             ticked: Results › Show in 3D.",
            ALL,
        ),
    }
}

/// The profiles from the 3D model.
const PROFILES_3D: field_common::MethodText = field_common::MethodText {
    short: "The wires as solved (sag, line angles, the line's length, jumpers); for any span; \
            the time below (4 Model).",
    long: "Biot–Savart for every straight piece of the wires as solved in the weather case \
           (bundles subconductor by subconductor), the jumpers and, with the earth wires on \
           through the structures, their currents. It sees what the span does along it: the \
           sag, the line angles, the line's ends. The time grows with the pieces (4 Model) and \
           the profile's points (its width over its step). Profiles take the whole line.",
};

/// The profiles by the 2D-only method.
const PROFILES_2D: field_common::MethodText = field_common::MethodText {
    short: "Infinite straight parallel conductors at the section's heights; enough on long \
            straight spans away from structures and angles; in a moment.",
    long: "Each wire an infinitely long straight conductor at the height where it crosses the \
           profile (at the lowest point by default; or at the span's mean height), the \
           classic cross-section formula. No sag along the span, no line angle, no jumpers, no \
           structures. Under the lowest point it is conservative against the 3D model on \
           sagging spans (theory); near a line angle over 5°, a crossing or a span between \
           differently arranged structures it is not valid (warned per profile). The width and \
           step change only the points shown; a profile takes a moment.",
};

/// The cuts from the 3D model (the only method the engine has for cuts).
const CUTS_3D: field_common::MethodText = field_common::MethodText {
    short: "The wires as solved within the distance modelled (4 Model); the time beside the \
            cuts' sizes below.",
    long: "Biot–Savart for every straight piece of the wires as solved within the distance \
           modelled of the cut (4 Model; each run says what the rest change), at every point \
           of the cut's lattice; the isolines drawn straight between the points. Finer steps \
           and wider cuts take longer (said beside them).",
};
