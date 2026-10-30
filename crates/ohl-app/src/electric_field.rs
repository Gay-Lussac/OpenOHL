// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Electrical › **Electric field…** (roadmap O18 step 5, owner
//! 2026-10-29; built, in active development, not approved yet): the
//! electric field of the line's voltages ([`ohl_electrical::electric`]) in
//! a weather case — a voltage per circuit (the system's nominal unless
//! given; 0: voltage-free), 3D or 2D, the jumpers, the ground plane's
//! radius, the profiles' height and width, the stations, an optional
//! limit, points from a CSV — and on Run the charges per wire, the
//! profiles plotted (E against the distance from the axis), a table per
//! profile and per point with the limit's margin, the notes; Export CSV.
//! The model's pieces, the levels drawn and the views (the cross-section
//! panels, volumes) in [`views`], as for the magnetic field. Out of date
//! like the other checks when the line or the ground changes. Not yet:
//! isolines from above.
//!
//! The window's inputs are kept in the project, per project line
//! ([`SETTING`]), written when a run starts or the window closes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use ohl_electrical::electric::{self, CircuitVoltage, ElectricField, Settings};
use ohl_electrical::magnetic::{self, GivenPoint, Model as FieldModel, Progress, StationSpec};
use ohl_line::electrical::SystemKind;

#[path = "electric_views.rs"]
pub mod views;

use crate::check_help as help;
use crate::check_ui;
use crate::field_overlay::Level;
use crate::job::{Job, save_csv};
use crate::magnetic_field::views::{PanelCuts, PieceStats, VolumeSetup};
use crate::magnetic_field::{Spans, Stations};
use crate::model::Model;
use crate::span_pick::SpanPick;

/// A span by its first and last structure.
type SpanIds = (String, String);

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE_GROUND;

/// The answer: the field and the weather case.
#[derive(Debug, Clone)]
pub struct Answer {
    /// The field.
    pub field: ElectricField,
    /// The weather case's name.
    pub case: String,
    /// The 3D model's charges (for the cross-section panels' cuts).
    pub charges: Option<Arc<electric::Charges3d>>,
    /// The ground plane's radius the run took, m.
    pub radius: f64,
    /// The ground model the run took.
    pub ground_model: electric::GroundModel,
    /// The volumes' surfaces (level, mesh), in their order.
    pub meshes: Vec<(f64, Arc<ohl_electrical::contour::Mesh>)>,
    /// The points computed and the pieces, for the speed.
    pub work: f64,
}

/// A duration in words (the magnetic window's).
fn magnetic_duration(s: f64) -> String {
    crate::magnetic_field::views::duration(s)
}

/// The default levels, kV/m: display only (no limit).
pub fn default_levels() -> Vec<Level> {
    vec![
        Level {
            value: 1.0,
            rgb: [70, 150, 255],
            on: true,
        },
        Level {
            value: 5.0,
            rgb: [255, 170, 40],
            on: true,
        },
        Level {
            value: 10.0,
            rgb: [230, 50, 50],
            on: false,
        },
    ]
}

/// The Electric field window.
pub struct ElectricFieldWindow {
    /// Shown.
    pub open: bool,
    /// The weather case (index into the model's cases).
    pub case: usize,
    /// The voltage between phases given per circuit (by its label), kV;
    /// a circuit not here: its system's nominal.
    voltages: BTreeMap<String, f64>,
    /// The jumpers carry their phase's charge.
    jumpers: bool,
    height: f64,
    width: f64,
    step: f64,
    stations: Stations,
    /// The spans ticked for the profiles (every span's).
    span_pick: SpanPick,
    limit: Option<f64>,
    two_d: bool,
    /// The ground plane under a point is fitted over this distance, m.
    ground_radius: f64,
    /// The ground a tiled conducting floor (the default; owner,
    /// 2026-10-29), else a plane under each point.
    tiled: bool,
    /// The floor's fine tiles, m.
    tile: f64,
    /// The tiles under the cuts across the line, m (coarser by default;
    /// owner, 2026-10-29).
    cut_tile: f64,
    /// The tiles under the volumes, m.
    volume_tile: f64,
    /// Points read from a CSV file, and its name.
    points: Option<(String, Vec<GivenPoint>)>,
    /// The points file's text (kept in the project).
    points_text: Option<String>,
    /// The project setting's key and the inputs as last read or written
    /// (none: not read yet).
    saved: Option<(String, String)>,
    /// The weather case came from the project (opening keeps it).
    case_kept: bool,
    /// Why the last points file could not be read.
    points_error: Option<String>,
    /// The profile shown in the plot.
    shown: usize,
    /// The run's progress and Stop.
    progress: Option<Arc<Progress>>,
    /// The run.
    pub(crate) job: Job<Answer>,
    /// The line's spans (first and last structure), by the line's version.
    spans: Option<(u64, Spans)>,
    /// The inputs are written into the project at the next frame (a run
    /// started, the window closed).
    commit: bool,
    /// The longest piece of a wire, m (none: as solved).
    piece_length: Option<f64>,
    /// The levels drawn, kV/m.
    levels: Vec<Level>,
    /// The cuts coloured by band.
    pub(crate) fill: bool,
    /// The field in the cross-section panels: the step, m.
    in_cuts: Option<f64>,
    /// A volume.
    volume: Option<VolumeSetup>,
    /// The panels' cuts.
    panel: PanelCuts,
    /// Seconds per piece and point of the last run here.
    rate: Option<f64>,
    /// The share of a volume's lattice the last run computed.
    volume_share: Option<f64>,
    /// The floors' solves of the last run against their expected time.
    floor_speed: Option<f64>,
    /// The wires' pieces, by line version and case.
    pieces_seen: Option<((u64, usize), PieceStats)>,
    /// The overlay as last handed out.
    drawn: Option<(bool, u64, u64, String, bool, bool)>,
    /// The last run's charges (3D) and its ground plane's radius.
    charges: Option<(Arc<electric::Charges3d>, f64, electric::GroundModel, f64)>,
}

impl Default for ElectricFieldWindow {
    fn default() -> Self {
        let d = Settings::default();
        Self {
            open: false,
            case: 0,
            voltages: BTreeMap::new(),
            jumpers: d.jumpers,
            height: d.height,
            width: d.width,
            step: d.step,
            stations: Stations::EverySpan,
            span_pick: SpanPick::default(),
            limit: None,
            two_d: false,
            ground_radius: d.ground_radius,
            tiled: true,
            tile: 2.0,
            cut_tile: 8.0,
            volume_tile: 8.0,
            points: None,
            points_text: None,
            saved: None,
            case_kept: false,
            points_error: None,
            shown: 0,
            progress: None,
            job: Job::default(),
            spans: None,
            commit: false,
            piece_length: None,
            levels: default_levels(),
            fill: true,
            in_cuts: None,
            volume: None,
            panel: PanelCuts::default(),
            rate: None,
            volume_share: None,
            floor_speed: None,
            pieces_seen: None,
            drawn: None,
            charges: None,
        }
    }
}

/// The project setting holding the window's inputs (per project line:
/// `app.electric_field.<line>`).
pub const SETTING: &str = "app.electric_field";

/// The id of the window's body, tabs and footer.
const WINDOW: &str = "electric-field-window";

/// The window's inputs as kept in the project (JSON).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Stored {
    case: Option<String>,
    /// kV between phases per circuit label (absent: the nominal).
    voltages: BTreeMap<String, f64>,
    jumpers: Option<bool>,
    height: Option<f64>,
    width: Option<f64>,
    step: Option<f64>,
    /// One span's stations: its structures and the place in it (none:
    /// every span at its lowest point).
    span: Option<(String, String, Option<f64>)>,
    /// The spans on an alignment unticked and those on none ticked.
    spans_off: Vec<(String, String)>,
    spans_on: Vec<(String, String)>,
    limit: Option<f64>,
    two_d: bool,
    ground_radius: Option<f64>,
    tiled: Option<bool>,
    tile: Option<f64>,
    cut_tile: Option<f64>,
    volume_tile: Option<f64>,
    /// The points file's name and text.
    points: Option<(String, String)>,
    piece_length: Option<f64>,
    levels: Option<Vec<Level>>,
    fill: Option<bool>,
    in_cuts: Option<f64>,
    /// A volume: its first and last spans and the step.
    volume: Option<(SpanIds, SpanIds, f64)>,
    /// A volume over the spans ticked: its step (2026-10-30; `volume`
    /// before: its spans now the ticked ones, its step read).
    volume_step: Option<f64>,
}

impl ElectricFieldWindow {
    /// The line's spans by their first and last structure, listed once per
    /// version of the line.
    fn spans(&mut self, model: &Model) -> Spans {
        if let Some((v, list)) = &self.spans
            && *v == model.line_version
        {
            return list.clone();
        }
        let list: Spans = Arc::new(
            ohl_electrical::cross_section::line_spans(&model.line)
                .into_iter()
                .map(|s| (s.from, s.to))
                .collect(),
        );
        self.spans = Some((model.line_version, list.clone()));
        list
    }

    /// The inputs as kept in the project.
    fn stored(&mut self, model: &Model) -> Stored {
        let spans = self.spans(model);
        let span = match &self.stations {
            Stations::EverySpan => None,
            Stations::One { span, fraction } => spans
                .get(*span)
                .map(|(from, to)| (from.clone(), to.clone(), *fraction)),
        };
        Stored {
            case: model.cases.get(self.case).map(|c| c.case.name.clone()),
            voltages: self.voltages.clone(),
            jumpers: Some(self.jumpers),
            height: Some(self.height),
            width: Some(self.width),
            step: Some(self.step),
            span,
            spans_off: self.span_pick.off.iter().cloned().collect(),
            spans_on: self.span_pick.on.iter().cloned().collect(),
            limit: self.limit,
            two_d: self.two_d,
            ground_radius: Some(self.ground_radius),
            tiled: Some(self.tiled),
            tile: Some(self.tile),
            cut_tile: Some(self.cut_tile),
            volume_tile: Some(self.volume_tile),
            points: match (&self.points, &self.points_text) {
                (Some((name, _)), Some(text)) => Some((name.clone(), text.clone())),
                _ => None,
            },
            piece_length: self.piece_length,
            levels: Some(self.levels.clone()),
            fill: Some(self.fill),
            in_cuts: self.in_cuts,
            volume: None,
            volume_step: self.volume.as_ref().map(|v| v.step),
        }
    }

    /// Takes the inputs kept in the project for the line worked on; what
    /// is no longer there (a case, a span) falls back to the default.
    fn load(&mut self, model: &Model) {
        let Some(session) = model.session() else {
            return;
        };
        let key = format!("{SETTING}.{}", session.current_line());
        let text = session.setting(&key).ok().flatten();
        let st: Stored = text
            .as_deref()
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default();
        let d = Self::default();
        self.case_kept = false;
        if let Some(k) = st
            .case
            .as_ref()
            .and_then(|n| model.cases.iter().position(|c| &c.case.name == n))
        {
            self.case = k;
            self.case_kept = true;
        }
        self.voltages = st.voltages;
        self.jumpers = st.jumpers.unwrap_or(d.jumpers);
        self.height = st.height.unwrap_or(d.height);
        self.width = st.width.unwrap_or(d.width);
        self.step = st.step.unwrap_or(d.step);
        let spans = self.spans(model);
        self.span_pick.off = st.spans_off.into_iter().collect();
        self.span_pick.on = st.spans_on.into_iter().collect();
        self.stations = st
            .span
            .and_then(|(from, to, fraction)| {
                spans
                    .iter()
                    .position(|(a, b)| *a == from && *b == to)
                    .map(|span| Stations::One { span, fraction })
            })
            .unwrap_or_default();
        self.limit = st.limit;
        self.two_d = st.two_d;
        self.ground_radius = st.ground_radius.unwrap_or(d.ground_radius);
        self.tiled = st.tiled.unwrap_or(d.tiled);
        self.tile = st.tile.unwrap_or(d.tile);
        self.cut_tile = st.cut_tile.unwrap_or(d.cut_tile);
        self.volume_tile = st.volume_tile.unwrap_or(d.volume_tile);
        self.piece_length = st.piece_length;
        self.levels = st.levels.unwrap_or(d.levels);
        self.fill = st.fill.unwrap_or(d.fill);
        self.in_cuts = st.in_cuts;
        self.volume = st
            .volume_step
            .or(st.volume.map(|v| v.2))
            .map(|step| VolumeSetup { step });
        (self.points, self.points_text, self.points_error) = match st.points {
            Some((name, text)) => match magnetic::parse_points(&text) {
                Ok(p) => (Some((name, p)), Some(text), None),
                Err(e) => (
                    None,
                    None,
                    Some(format!("{name} (kept in the project): {e}")),
                ),
            },
            None => (None, None, None),
        };
        self.saved = serde_json::to_string(&self.stored(model))
            .ok()
            .map(|json| (key, json));
    }

    /// Reads the inputs from the project when not read yet for the line
    /// worked on, and writes them into it when they changed — once a run
    /// starts or the window closes, not while a value is dragged or typed.
    /// A project this user cannot change keeps them for the session.
    pub fn keep(&mut self, model: &mut Model, ctx: &egui::Context) {
        let Some(line) = model.session().map(|s| s.current_line().to_owned()) else {
            return;
        };
        let key = format!("{SETTING}.{line}");
        if self.saved.as_ref().is_none_or(|(k, _)| *k != key) {
            self.load(model);
            return;
        }
        if !self.commit
            || ctx.input(|i| i.pointer.any_down())
            || ctx.memory(|m| m.focused().is_some())
        {
            return;
        }
        self.commit = false;
        let Ok(json) = serde_json::to_string(&self.stored(model)) else {
            return;
        };
        if self.saved.as_ref().is_some_and(|(_, j)| *j == json) {
            return;
        }
        let op = ohl_server::Op::SetSetting {
            key: key.clone(),
            value: Some(json.clone()),
        };
        // (Read-only for this user: kept for the session.)
        let _ = model.project_op("electric field inputs", op);
        self.saved = Some((key, json));
    }

    /// Opens the window on the weather case shown.
    pub fn show(&mut self, case: usize) {
        if !self.open && !self.case_kept {
            self.case = case;
        }
        self.open = true;
    }

    /// About how long a floor of `tiles` takes here, s: as measured on
    /// the floors of the last run (as expected before one).
    fn floor_seconds(&self, tiles: usize) -> f64 {
        electric::TiledFloor::seconds(tiles) * self.floor_speed.unwrap_or(1.0)
    }

    /// The ground a tiled floor or a plane under each point (tests).
    #[cfg(test)]
    pub fn set_tiled(&mut self, on: bool) {
        self.tiled = on;
    }

    /// Gives a circuit's voltage between phases, kV (tests).
    #[cfg(test)]
    pub fn set_voltage(&mut self, circuit: &str, kv: f64) {
        self.voltages.insert(circuit.to_owned(), kv);
    }

    /// A circuit's voltage given, kV (tests).
    #[cfg(test)]
    pub fn voltage_of(&self, circuit: &str) -> Option<f64> {
        self.voltages.get(circuit).copied()
    }

    /// Ticks span `k` only (tests).
    #[cfg(test)]
    pub fn tick_only(&mut self, model: &Model, k: usize) {
        let spans = self.spans(model);
        self.span_pick.only(model, &spans, k);
    }

    /// Sets the views (tests): the cross-section panels every `cuts` m, a
    /// volume over the first span every `volume` m.
    #[cfg(test)]
    pub fn set_views(&mut self, cuts: Option<f64>, volume: Option<f64>) {
        self.in_cuts = cuts;
        self.volume = volume.map(|step| VolumeSetup { step });
    }

    /// The views (tests): the panels' and the volume's step.
    #[cfg(test)]
    pub fn views(&self) -> (Option<f64>, Option<f64>) {
        (self.in_cuts, self.volume.as_ref().map(|v| v.step))
    }

    /// The panels' cuts computed (tests).
    #[cfg(test)]
    pub fn panel_cuts_done(&self) -> usize {
        self.panel.done.len()
    }

    /// The panels' cuts' values, kV/m (tests).
    #[cfg(test)]
    pub fn panel_values(&self) -> Vec<Arc<ohl_electrical::field_views::CutValues>> {
        self.panel.done.iter().map(|c| c.values.clone()).collect()
    }

    /// The limit, kV/m (tests).
    #[cfg(test)]
    pub fn limit(&self) -> Option<f64> {
        self.limit
    }

    /// Sets the limit, kV/m (tests).
    #[cfg(test)]
    pub fn set_limit(&mut self, limit: Option<f64>) {
        self.limit = limit;
    }

    /// What the last run was computed from that changed since, in words
    /// (`None`: nothing, or no answer).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.job.result, Some(Ok(_)))
            .then(|| self.job.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// A circuit's voltage between phases: given, else its system's
    /// nominal (none: DC, or no such system).
    fn voltage(&self, model: &Model, c: &ohl_electrical::parameters::CircuitKey) -> Option<f64> {
        let system = model.line.systems.iter().find(|s| s.id == c.system)?;
        if system.kind == SystemKind::Dc {
            return None;
        }
        Some(
            self.voltages
                .get(&c.label())
                .copied()
                .unwrap_or(system.voltage_kv),
        )
    }

    /// The settings as the engine takes them.
    fn settings(&mut self, model: &Model) -> Settings {
        let spans = self.spans(model);
        let stations = match &self.stations {
            Stations::EverySpan => match self.span_pick.chosen(model, &spans) {
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
        };
        Settings {
            model: if self.two_d {
                FieldModel::TwoD
            } else {
                FieldModel::ThreeD
            },
            voltages: self
                .voltages
                .iter()
                .map(|(c, &kv)| CircuitVoltage {
                    circuit: c.clone(),
                    kv,
                })
                .collect(),
            height: self.height,
            width: self.width,
            step: self.step,
            stations,
            points: self
                .points
                .as_ref()
                .map(|p| p.1.clone())
                .unwrap_or_default(),
            limit_kv_m: self.limit,
            ground_radius: self.ground_radius,
            ground_model: if self.tiled {
                electric::GroundModel::Tiled(electric::TiledFloor {
                    fine: self.tile,
                    ..Default::default()
                })
            } else {
                electric::GroundModel::Plane
            },
            cut_tile: self.cut_tile,
            volume_tile: self.volume_tile,
            jumpers: self.jumpers,
            piece_length: self.piece_length,
            cuts: Vec::new(),
            // A volume: one along each run of ticked spans following each
            // other (continuous across the line angles).
            volumes: match (&self.volume, self.two_d) {
                (Some(v), false) => self
                    .span_pick
                    .runs(model, &spans)
                    .into_iter()
                    .map(|(from, to)| magnetic::VolumeSpec {
                        from,
                        to,
                        width: self.width,
                        above: crate::magnetic_field::views::ABOVE,
                        step: v.step,
                    })
                    .collect(),
                _ => Vec::new(),
            },
            levels: crate::field_overlay::shown(&self.levels)
                .iter()
                .map(|l| l.value)
                .collect(),
        }
    }

    /// Starts a run on the case's solved sections.
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        // The inputs run with: kept in the project (the next frame).
        self.commit = true;
        let Some(solved) = model.cases.get(self.case) else {
            self.job.result = Some(Err("no weather case in the model".into()));
            return;
        };
        let line = model.line.clone();
        let library = model.library.clone();
        let shapes = solved.shapes.clone();
        let jumpers = solved.jumpers.clone();
        let case = solved.case.clone();
        let ground = model.ground_for_checks();
        let settings = self.settings(model);
        let progress = Arc::new(Progress::default());
        self.progress = Some(progress.clone());
        self.job.start_at(ctx, model.revision(), move || {
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
            let (mut field, charges) = electric::run_keeping(&src, &settings, Some(&progress))?;
            // Each point sums every piece twice: the charges and their
            // images.
            let work = progress.total.load(Ordering::Relaxed) as f64
                * charges.as_ref().map_or(0, |c| c.count()) as f64;
            for id in line
                .sections
                .iter()
                .map(|s| &s.id)
                .filter(|id| !shapes.iter().any(|s| &s.section == *id))
            {
                field
                    .notes
                    .push(format!("section {id}: not solved in this case, left out"));
            }
            if g.is_none() {
                field.notes.push(
                    "ground: straight between the structures' feet and level across (no terrain surface)"
                        .into(),
                );
            }
            let meshes = crate::magnetic_field::views::take_meshes(&mut field.volumes);
            Ok(Answer {
                field,
                case: case.name.clone(),
                charges: charges.map(Arc::new),
                radius: settings.ground_radius,
                ground_model: settings.ground_model,
                meshes,
                work,
            })
        });
    }

    /// The window.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model) {
        let generation = self.job.generation;
        self.job.poll();
        if !self.job.running() {
            // The floors' speed here for the estimates (the time a floor
            // takes on this computer against the one measured).
            if let Some(f) = self.progress.take().and_then(|p| p.floor_speed()) {
                self.floor_speed = Some(f);
            }
        }
        if self.job.generation != generation {
            // A new answer: its charges for the panels, its speed and its
            // volumes' share for the estimates.
            self.charges = None;
            if let Some(Ok(a)) = &self.job.result {
                self.charges = a
                    .charges
                    .clone()
                    .map(|c| (c, a.radius, a.ground_model, self.cut_tile));
                if let Some(t) = self.job.took
                    && a.work > 0.0
                {
                    self.rate = Some(t.as_secs_f64() / a.work);
                }
                let (c, f) = a.field.volumes.iter().fold((0usize, 0usize), |s, v| {
                    (s.0 + v.values.computed, s.1 + v.values.full)
                });
                if f > 0 {
                    self.volume_share = Some(c as f64 / f as f64);
                }
            }
        }
        if !self.open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let stale = self.out_of_date(model);
        help::window(
            ctx,
            crate::dockable::Panel::ElectricField,
            "Electric field",
            [860.0, 760.0],
            &mut open,
            |ui| {
                let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
                let circuits = magnetic::circuits(&model.line);
                let why = if names.is_empty() {
                    Some("no weather case in the model".to_owned())
                } else if circuits.is_empty() {
                    Some("no circuit: assign the systems (Line › Electrical systems)".to_owned())
                } else if !circuits
                    .iter()
                    .any(|c| self.voltage(model, c).is_some_and(|v| v > 0.0))
                {
                    Some("every circuit voltage-free: give one a voltage (2 Scope)".to_owned())
                } else if self.stations == Stations::EverySpan && {
                    let spans = self.spans(model);
                    self.span_pick.chosen(model, &spans).0.is_empty()
                } {
                    Some("no span ticked (2 Scope, Spans)".to_owned())
                } else {
                    None
                };
                let tab = check_ui::tabs(
                    ui,
                    WINDOW,
                    None,
                    self.job.generation,
                    why.is_some() && !self.job.running(),
                );
                match tab {
                    check_ui::Tab::Setup => help::body(ui, WINDOW, |ui| {
                        check_ui::section(ui, 1, "What it checks", None);
                        help::explain(
                            ui,
                            "The electric field strength (kV/m, the rms of the vector) of the \
                             line's voltages, from the wires where they hang in a weather case.",
                            "The charge per metre of every wire from its span's mean cross-section \
                             (q = inv(P) · V, the line parameters' potential coefficients); 3D from the \
                             solved wires and jumpers as pieces of uniform charge with their \
                             images in the ground under each point, or 2D (infinite line charges \
                             over level ground). The structures, trees and buildings are not \
                             modelled. A limit is yours to give; none is built in. Built, in \
                             active development, not approved yet.",
                        );
                        ui.horizontal(|ui| {
                            crate::field_help::profile(
                                ui,
                                crate::field_help::Field::Electric,
                                self.width,
                                self.step,
                                self.height,
                            );
                            crate::field_help::sources(
                                ui,
                                crate::field_help::Field::Electric,
                                self.tiled,
                            );
                        });
                        ui.separator();
                        check_ui::section(ui, 2, "Scope", None);
                        check_ui::ScopePicker::new(WINDOW, &model.line)
                            .cases(check_ui::Cases::One(&names, &mut self.case))
                            .ui(ui);
                        self.inputs_ui(ui, model);
                        if !self.two_d {
                            let spans = self.spans(model);
                            self.views_ui(ui, model, &spans);
                        }
                    }),
                    check_ui::Tab::Results => check_ui::results_area(ui, WINDOW, |ui| {
                        egui::ScrollArea::both()
                            .id_salt("electric-field-results")
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.results_ui(ui, &stale, &mut run));
                    }),
                }
                help::footer(ui, WINDOW, |ui| {
                    let running = self.progress.as_ref().map(|p| {
                        let total = p.total.load(Ordering::Relaxed).max(1);
                        let done = p.done.load(Ordering::Relaxed);
                        let elapsed = self.job.elapsed().unwrap_or_default();
                        let mut pr =
                            check_ui::Progress::with_left(elapsed, p.seconds_left(elapsed));
                        let stage = p.stage();
                        pr.text = if stage.is_empty() {
                            format!("{done} of {total} points; {}", pr.text)
                        } else {
                            format!("{stage}; {}", pr.text)
                        };
                        pr
                    });
                    let f = check_ui::RunFooter {
                        running,
                        why_not: why.clone(),
                        hover: "Compute the field from the wires as solved in the weather case \
                                chosen",
                        scope: None,
                        info: self.job.took.map(|t| format!("computed in {t:.1?}")),
                    };
                    match check_ui::run_footer(ui, f, |_| {}) {
                        check_ui::Action::Run => run = true,
                        check_ui::Action::Stop => {
                            if let Some(p) = &self.progress {
                                p.cancel.store(true, Ordering::Relaxed);
                            }
                        }
                        check_ui::Action::None => {}
                    }
                });
            },
        );
        // Closed: its inputs kept in the project (the next frame).
        if self.open && !open {
            self.commit = true;
        }
        self.open = open;
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
        egui::Grid::new("electric-field-voltages")
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                for h in ["circuit", "system", "voltage between phases", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for c in &circuits {
                    let label = c.label();
                    ui.label(&label);
                    ui.label(&c.system);
                    let Some(mut kv) = self.voltage(model, c) else {
                        ui.label(egui::RichText::new("DC: left out").weak())
                            .on_hover_text("A static field with space charge: not computed");
                        ui.label("");
                        ui.end_row();
                        continue;
                    };
                    let nominal = model
                        .line
                        .systems
                        .iter()
                        .find(|s| s.id == c.system)
                        .map_or(0.0, |s| s.voltage_kv);
                    let r = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut kv)
                            .range(0.0..=2000.0)
                            .speed(1.0)
                            .suffix(" kV"),
                    )
                    .on_hover_text(
                        "rms, between phases; each phase at it over √3 to earth at its \
                         system's phase angle (L1 0°, L2 −120°, L3 +120° unless the system gives \
                         others). 0: voltage-free (at earth potential, like the earth wires)",
                    );
                    if r.changed() {
                        self.voltages.insert(label.clone(), kv);
                    }
                    ui.horizontal(|ui| {
                        let given = self.voltages.contains_key(&label);
                        let what = if kv == 0.0 {
                            "voltage-free"
                        } else if given {
                            "given"
                        } else {
                            "the system's nominal"
                        };
                        ui.label(egui::RichText::new(what).weak());
                        if given
                            && ui
                                .small_button("nominal")
                                .on_hover_text(format!(
                                    "Back to the system's nominal voltage, {nominal} kV"
                                ))
                                .clicked()
                        {
                            self.voltages.remove(&label);
                        }
                    });
                    ui.end_row();
                }
            });
        help::explain(
            ui,
            "Each circuit at its system's nominal voltage unless given; 0 kV: voltage-free.",
            "The NISV guide (BAFU 2007, 8.7.3) asks the highest operating voltage, the largest \
             sag (the weather case) and, on a double circuit built to compensate, one circuit \
             voltage-free. Which governs is yours to choose; none is built in. A higher \
             voltage raises the field in proportion; the time is the same.",
        );
        ui.add_space(4.0);
        egui::Grid::new("electric-field-settings")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Model");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.two_d, false, "3D")
                        .on_hover_text(
                            "The wires as solved (sag, line angles, the line's length) and the \
                         jumpers as pieces of uniform charge, the ground under each point a \
                         plane with the charges' images in it",
                        );
                    ui.selectable_value(&mut self.two_d, true, "2D")
                        .on_hover_text(
                            "Infinite line charges where the wires cross the profile, over level \
                         ground at the cross-section: not valid near a line angle, a structure \
                         or on sloping ground",
                        );
                });
                ui.end_row();
                ui.label("");
                help::explain(
                    ui,
                    if self.two_d {
                        "2D: a profile's points in a moment; the sag along the span, the line \
                         angles and the jumpers not seen (under the lowest point close to 3D on \
                         a long level span)."
                    } else {
                        "3D: every point sums every piece of charge twice (the charges and their \
                         images): the time below (3 Model)."
                    },
                    "Both take the same charges per wire (the span's mean cross-section). 3D \
                     sees what the span does along it; 2D assumes every span infinitely long \
                     and straight, its ground level.",
                );
                ui.end_row();
                if !self.two_d {
                    ui.label("Jumpers");
                    ui.checkbox(&mut self.jumpers, "carry their phase's charge")
                        .on_hover_text(
                            "A jumper at a section's end at that section's charge per metre, as \
                             solved in the weather case (3D). Off: left out (not conservative \
                             near the structures). Jumpers not at a section's end are left out, \
                             said",
                        );
                    ui.end_row();
                    ui.label("Ground");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.tiled, true, "tiled floor");
                        ui.radio_value(&mut self.tiled, false, "plane under each point");
                        if self.tiled {
                            ui.label("tiles: profiles and points");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.tile)
                                    .range(1.0..=8.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            );
                            ui.label("cuts");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.cut_tile)
                                    .range(1.0..=32.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            );
                            ui.label("volumes");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.volume_tile)
                                    .range(2.0..=32.0)
                                    .speed(0.1)
                                    .suffix(" m"),
                            );
                        } else {
                            ui.label("fitted over ±");
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut self.ground_radius)
                                    .range(0.5..=500.0)
                                    .speed(0.5)
                                    .suffix(" m"),
                            );
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    if self.tiled {
                        let floor = electric::TiledFloor {
                            fine: self.tile,
                            ..Default::default()
                        };
                        let tiles = floor.tiles(self.width);
                        let floors = match self.stations {
                            Stations::EverySpan => {
                                let spans = self.spans(model);
                                self.span_pick.chosen(model, &spans).0.len()
                            }
                            _ => 1,
                        };
                        help::explain(
                            ui,
                            &(format!(
                                "The terrain as a conducting floor of tiles around each profile, \
                                 group of points and cut (owner, 2026-10-29: \"keep the tiled \
                                 implementation\"): ≈ {tiles} tiles, {} per floor, {floors} \
                                 floor(s) for the profiles. It follows hollows and ridges: on a \
                                 real line the plane read the largest field up to 8 % low on bumps \
                                 and up to 24 % high in a hollow. From 4 m to 2 m tiles the \
                                 profiles moved 1–3 %; 1 m tiles take about 8 times as long.",
                                magnetic_duration(self.floor_seconds(tiles))
                            ) + &format!(
                                " Cuts across the line and volumes on coarser tiles by default \
                                 (owner, 2026-10-29): a cut ≈ {} tiles, {}; a volume over one \
                                 span ≈ {} tiles, {}. Measured against the exact field on even \
                                 ground, a volume's floor of 16 m tiles is 8.3 % off the largest \
                                 value, of 8 m 2.8 %, of 4 m 0.16 % (some 40 times as long).",
                                electric::TiledFloor {
                                    fine: self.cut_tile,
                                    coarse: self.cut_tile.max(8.0),
                                    ..Default::default()
                                }
                                .tiles(self.width),
                                magnetic_duration(
                                    self.floor_seconds(
                                        electric::TiledFloor {
                                            fine: self.cut_tile,
                                            coarse: self.cut_tile.max(8.0),
                                            ..Default::default()
                                        }
                                        .tiles(self.width)
                                    )
                                ),
                                electric::Floor::tiles_under(400.0, self.width, self.volume_tile),
                                magnetic_duration(self.floor_seconds(
                                    electric::Floor::tiles_under(
                                        400.0,
                                        self.width,
                                        self.volume_tile
                                    )
                                )),
                            )),
                            "Square tiles of the fine size within 30 m along the line and 10 m \
                             beyond the profiles across, 8 m beyond, to 200 m every way, each \
                             carrying a uniform charge solved so that the floor is at zero \
                             potential. Volumes take a floor of their own tiles along their \
                             whole run and 100 m beyond. The times at the speed of the last run's \
                             floors here (before one: as measured on a laptop). See the theory \
                             page, Electric field.",
                        );
                    } else {
                        help::explain(
                            ui,
                            "The ground under each point a plane fitted over ± this distance, \
                             the charges' images in it: quick (no floor to solve), exact for \
                             level or evenly sloping ground; on a real line it read the largest \
                             field up to 8 % low on bumps (not conservative) and in a hollow up \
                             to 24 % high.",
                            "A short distance follows the ground right under the point (a bank, \
                             a ditch), a long one the slope at large. See the theory page, \
                             Electric field.",
                        );
                    }
                    ui.end_row();
                }
                // The spans computed (owner, 2026-10-30): the profiles,
                // the isolines from above and the volumes over them.
                let spans = self.spans(model);
                ui.label("Spans");
                self.span_pick.ui(ui, "electric-field", model, &spans);
                ui.end_row();
                ui.label("Profiles");
                ui.horizontal(|ui| {
                    let mut every = self.stations == Stations::EverySpan;
                    ui.radio_value(&mut every, true, "at each ticked span's lowest point");
                    ui.radio_value(&mut every, false, "one span");
                    if every {
                        self.stations = Stations::EverySpan;
                    } else {
                        if self.stations == Stations::EverySpan {
                            self.stations = Stations::One {
                                span: 0,
                                fraction: None,
                            };
                        }
                        if let Stations::One { span, fraction } = &mut self.stations {
                            *span = (*span).min(spans.len().saturating_sub(1));
                            let name = |k: usize| {
                                spans
                                    .get(k)
                                    .map_or(String::new(), |(a, b)| format!("{a}–{b}"))
                            };
                            egui::ComboBox::from_id_salt("electric-field-span")
                                .selected_text(name(*span))
                                .show_ui(ui, |ui| {
                                    for k in 0..spans.len() {
                                        ui.selectable_value(span, k, name(k));
                                    }
                                });
                            let mut lowest = fraction.is_none();
                            ui.checkbox(&mut lowest, "at its lowest point");
                            if lowest {
                                *fraction = None;
                            } else {
                                let f = fraction.get_or_insert(0.5);
                                crate::style::field(
                                    ui,
                                    egui::DragValue::new(f).range(0.0..=1.0).speed(0.01),
                                )
                                .on_hover_text(
                                    "The place in the span from its first structure along the \
                                     line (0–1)",
                                );
                            }
                        }
                    }
                });
                ui.end_row();
                ui.label("Across");
                ui.horizontal(|ui| {
                    ui.label("±");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.width)
                            .range(1.0..=2000.0)
                            .speed(1.0)
                            .suffix(" m"),
                    );
                    ui.label("every");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.step)
                            .range(0.1..=50.0)
                            .speed(0.1)
                            .suffix(" m"),
                    );
                    ui.label("at");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.height)
                            .range(0.0..=200.0)
                            .speed(0.1)
                            .suffix(" m"),
                    );
                    ui.label("above the ground");
                });
                ui.end_row();
                ui.label("Limit");
                ui.horizontal(|ui| {
                    let mut given = self.limit.is_some();
                    ui.checkbox(&mut given, "given").on_hover_text(
                        "Your limit, kV/m: the margin at every point. None is built in (an \
                         ordinance's values come from its parameter file)",
                    );
                    if !given {
                        self.limit = None;
                    } else {
                        let l = self.limit.get_or_insert(5.0);
                        crate::style::field(
                            ui,
                            egui::DragValue::new(l)
                                .range(0.001..=1000.0)
                                .speed(0.01)
                                .suffix(" kV/m"),
                        );
                    }
                });
                ui.end_row();
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
                            .and_then(|t| magnetic::parse_points(&t).map(|p| (t, p)))
                        {
                            Ok((text, p)) => {
                                let name = path
                                    .file_name()
                                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                                self.points = Some((name, p));
                                self.points_text = Some(text);
                                self.points_error = None;
                            }
                            Err(e) => self.points_error = Some(format!("{}: {e}", path.display())),
                        }
                    }
                    if let Some((name, p)) = &self.points {
                        ui.label(format!("{} points from {name}", p.len()));
                        if ui.small_button("Clear").clicked() {
                            self.points = None;
                            self.points_text = None;
                        }
                    }
                });
                ui.end_row();
            });
        if let Some(e) = &self.points_error {
            crate::menu::error_label(ui, e.as_str());
        }
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, stale: &Option<String>, run: &mut bool) {
        let answer = match &self.job.result {
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
            *run |= crate::revision::banner(ui, what, !self.job.running());
        }
        let m = &answer.field;
        ui.horizontal(|ui| {
            ui.label(format!(
                "In \"{}\"; {}; {} m above the ground{}",
                answer.case,
                match m.model {
                    FieldModel::ThreeD => "3D (the wires as solved, images in the ground)",
                    FieldModel::TwoD => "2D (infinite line charges over level ground)",
                },
                m.height,
                m.limit_kv_m
                    .map_or(String::new(), |l| format!("; limit {l} kV/m"))
            ));
            if ui.button("Export CSV…").clicked() {
                save_csv("electric-field.csv", &electric::csv(m));
            }
        });
        if !m.charges.is_empty() {
            egui::CollapsingHeader::new(format!("Charges per wire ({})", m.charges.len()))
                .id_salt("electric-field-charges")
                .show(ui, |ui| {
                    help::list(
                        ui,
                        "electric-field-charges",
                        help::LIST_HEIGHT,
                        egui::ScrollArea::vertical().id_salt("electric-field-charges-scroll"),
                        |ui| {
                            egui::Grid::new("electric-field-charges-grid")
                                .striped(true)
                                .spacing(help::GRID_SPACING)
                                .show(ui, |ui| {
                                    for h in ["section", "span", "kV to earth", "nC/m", "Hz"] {
                                        ui.strong(h);
                                    }
                                    ui.end_row();
                                    for c in &m.charges {
                                        ui.label(&c.section);
                                        ui.label(format!("{}–{}", c.from, c.to));
                                        ui.label(format!("{:.2}", c.kv));
                                        ui.label(format!("{:.2}", c.nc_per_m));
                                        ui.label(format!("{}", c.frequency_hz));
                                        ui.end_row();
                                    }
                                });
                        },
                    );
                });
        }
        for n in &m.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        ui.label(
            egui::RichText::new(
                "Not yet: isolines from above (the plan); places of sensitive use as objects of \
                 the project.",
            )
            .small()
            .weak(),
        );
        self.views_results_ui(ui, m, &answer.meshes);
        if !m.profiles.is_empty() {
            ui.separator();
            self.shown = self.shown.min(m.profiles.len() - 1);
            ui.horizontal(|ui| {
                ui.strong("Profile");
                let name = |p: &electric::Profile| format!("{}–{} at {:.1} m", p.from, p.to, p.at);
                egui::ComboBox::from_id_salt("electric-field-profile")
                    .selected_text(name(&m.profiles[self.shown]))
                    .show_ui(ui, |ui| {
                        for (k, p) in m.profiles.iter().enumerate() {
                            ui.selectable_value(&mut self.shown, k, name(p));
                        }
                    });
            });
            plot(ui, &m.profiles[self.shown], m.limit_kv_m);
            help::list(
                ui,
                "electric-field-profiles",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("electric-field-profiles-scroll"),
                |ui| {
                    egui::Grid::new("electric-field-profiles")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in ["span", "station m", "max kV/m", "at m", "margin kV/m"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (k, p) in m.profiles.iter().enumerate() {
                                if ui
                                    .selectable_label(
                                        k == self.shown,
                                        format!("{}–{}", p.from, p.to),
                                    )
                                    .clicked()
                                {
                                    self.shown = k;
                                }
                                ui.label(format!("{:.1}", p.at));
                                ui.label(format!("{:.3}", p.max_kv_m));
                                ui.label(format!("{:.1}", p.max_offset));
                                margin(ui, m.limit_kv_m.map(|l| l - p.max_kv_m));
                                ui.end_row();
                            }
                        });
                },
            );
        }
        if !m.points.is_empty() {
            ui.separator();
            ui.strong("Points");
            help::list(
                ui,
                "electric-field-points",
                help::LIST_HEIGHT,
                egui::ScrollArea::vertical().id_salt("electric-field-points-scroll"),
                |ui| {
                    egui::Grid::new("electric-field-points")
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in [
                                "name",
                                "x m",
                                "y m",
                                "z m",
                                "height m",
                                "E kV/m",
                                "margin kV/m",
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
                                ui.label(format!("{:.4}", p.e_kv_m));
                                margin(ui, p.margin_kv_m);
                                ui.end_row();
                            }
                        });
                },
            );
        }
    }
}

/// A margin, kV/m: red when below zero, a dash without a limit.
fn margin(ui: &mut egui::Ui, m: Option<f64>) {
    match m {
        Some(x) if x < 0.0 => {
            ui.label(egui::RichText::new(format!("{x:.4}")).color(ui.visuals().error_fg_color));
        }
        Some(x) => {
            ui.label(format!("{x:.4}"));
        }
        None => {
            ui.label("—");
        }
    }
}

/// A profile's plot: E against the distance from the axis, the limit as a
/// dashed line, the value under the pointer.
fn plot(ui: &mut egui::Ui, p: &electric::Profile, limit: Option<f64>) {
    let width = ui.available_width().clamp(300.0, 900.0);
    let (response, painter) = ui.allocate_painter(egui::vec2(width, 240.0), egui::Sense::hover());
    let r = response.rect;
    let v = ui.visuals();
    let text = v.text_color();
    let faint = v.weak_text_color();
    let line = v.hyperlink_color;
    let font = egui::FontId::proportional(11.0);
    let frame = egui::Rect::from_min_max(
        r.min + egui::vec2(48.0, 8.0),
        r.max - egui::vec2(10.0, 24.0),
    );
    painter.rect_stroke(
        frame,
        0.0,
        egui::Stroke::new(1.0, faint),
        egui::StrokeKind::Inside,
    );
    let (Some(first), Some(last)) = (p.points.first(), p.points.last()) else {
        return;
    };
    let (x0, x1) = (first.offset, last.offset.max(first.offset + 1e-9));
    let top = p
        .points
        .iter()
        .map(|q| q.e_kv_m)
        .fold(limit.unwrap_or(0.0), f64::max)
        .max(1e-6)
        * 1.1;
    let to_screen = |x: f64, y: f64| {
        egui::pos2(
            frame.left() + ((x - x0) / (x1 - x0)) as f32 * frame.width(),
            frame.bottom() - (y / top) as f32 * frame.height(),
        )
    };
    // Ticks: 5 across, 4 up.
    for k in 0..=4 {
        let x = x0 + (x1 - x0) * f64::from(k) / 4.0;
        let at = to_screen(x, 0.0);
        painter.line_segment(
            [at, egui::pos2(at.x, frame.top())],
            egui::Stroke::new(0.5, faint.gamma_multiply(0.4)),
        );
        painter.text(
            egui::pos2(at.x, frame.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            format!("{x:.0}"),
            font.clone(),
            faint,
        );
        let y = top * f64::from(k) / 4.0;
        let at = to_screen(x0, y);
        painter.line_segment(
            [at, egui::pos2(frame.right(), at.y)],
            egui::Stroke::new(0.5, faint.gamma_multiply(0.4)),
        );
        painter.text(
            egui::pos2(frame.left() - 4.0, at.y),
            egui::Align2::RIGHT_CENTER,
            format!("{y:.2}"),
            font.clone(),
            faint,
        );
    }
    painter.text(
        egui::pos2(frame.left() + 4.0, frame.top() + 2.0),
        egui::Align2::LEFT_TOP,
        "E kV/m",
        font.clone(),
        text,
    );
    painter.text(
        egui::pos2(frame.right(), r.bottom()),
        egui::Align2::RIGHT_BOTTOM,
        "m from the axis (right +)",
        font.clone(),
        faint,
    );
    let e: Vec<egui::Pos2> = p
        .points
        .iter()
        .map(|q| to_screen(q.offset, q.e_kv_m))
        .collect();
    painter.add(egui::Shape::line(e, egui::Stroke::new(2.0, line)));
    if let Some(l) = limit {
        let y = to_screen(x0, l).y;
        painter.extend(egui::Shape::dashed_line(
            &[egui::pos2(frame.left(), y), egui::pos2(frame.right(), y)],
            egui::Stroke::new(1.0, v.warn_fg_color),
            6.0,
            4.0,
        ));
        painter.text(
            egui::pos2(frame.right() - 4.0, y - 2.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("limit {l} kV/m"),
            font.clone(),
            v.warn_fg_color,
        );
    }
    if let Some(pos) = response.hover_pos()
        && frame.contains(pos)
    {
        let x = x0 + f64::from((pos.x - frame.left()) / frame.width()) * (x1 - x0);
        if let Some(q) = p
            .points
            .iter()
            .min_by(|a, b| (a.offset - x).abs().total_cmp(&(b.offset - x).abs()))
        {
            let at = to_screen(q.offset, q.e_kv_m);
            painter.circle_filled(at, 3.0, line);
            painter.text(
                at + egui::vec2(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{:.1} m: {:.4} kV/m", q.offset, q.e_kv_m),
                font,
                text,
            );
        }
    }
}
