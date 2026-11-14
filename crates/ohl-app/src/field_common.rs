// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What the Electric and the Magnetic field windows have in common: the
//! inputs of their setup, how those are kept in the project, the span and
//! profile setup, the profile plot and the notes of a run. Each window keeps
//! what is its own: its circuits, its model's settings, its results.

use std::sync::Arc;

use eframe::egui;
use ohl_electrical::magnetic::{self, GivenPoint, Progress, StationSpec};

use crate::field_overlay::Level;
use crate::field_results::{self, FieldRequest};
use crate::job::Job;
use crate::magnetic_field::views::{PanelCuts, PieceStats, VolumeSetup};
use crate::model::Model;
use crate::span_pick::{Columns, CutAt, SpanPick};

/// What their results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE_GROUND;

type SpanIds = (String, String);

/// The line's spans by their first and last structure.
pub(crate) type Spans = Arc<Vec<(String, String)>>;

/// The values a window starts with where the two differ.
#[derive(Debug, Clone)]
pub struct FieldStart {
    /// The jumpers carry their phase's charge or current.
    pub jumpers: bool,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The profiles' half width, m.
    pub width: f64,
    /// The profiles' step, m.
    pub step: f64,
    /// The levels drawn.
    pub levels: Vec<Level>,
    /// The volumes' half width, m.
    pub volume_width: f64,
    /// The cuts' half width, m.
    pub cut_width: f64,
}

/// The inputs and the state both field windows have, `A` a run's answer.
pub struct FieldShared<A> {
    /// The values it started with (what kept inputs fall back to).
    start: FieldStart,
    /// Shown.
    pub open: bool,
    /// The weather case (index into the model's cases).
    pub case: usize,
    /// The jumpers carry their phase's charge (electric) or current
    /// (magnetic).
    pub jumpers: bool,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The profiles' half width across the line, m.
    pub width: f64,
    /// The profiles' step across the line, m.
    pub step: f64,
    /// Where the profiles are taken.
    pub stations: Stations,
    /// The spans ticked for the profiles (every span's).
    pub span_pick: SpanPick,
    /// The limit drawn and checked, in the field's unit (none given).
    pub limit: Option<f64>,
    /// The profiles' method: 2D only (infinite line charges or straight
    /// conductors), else from the 3D model.
    pub two_d: bool,
    /// The page of the setup shown, what a run makes (owner, 2026-11-12:
    /// three pages, 3D volumes, 2D profiles, 2D cuts).
    pub page: FieldPage,
    /// Points read from a CSV file, and its name.
    pub points: Option<(String, Vec<GivenPoint>)>,
    /// The cuts and places set up by Checks › Electrical › Buildings
    /// (owner, 2026-11-03), run with the window's own.
    pub buildings: Option<crate::field_preset::Preset>,
    /// The points file's text (kept in the project).
    pub points_text: Option<String>,
    /// The project setting's key and the inputs as last read or written
    /// (none: not read yet).
    pub saved: Option<(String, String)>,
    /// The weather case came from the project (opening keeps it).
    pub case_kept: bool,
    /// Why the last points file could not be read.
    pub points_error: Option<String>,
    /// The profile shown in the plot.
    pub shown: usize,
    /// Scroll the results to the plot (its row's button clicked).
    pub reveal: bool,
    /// The run's progress and Stop.
    pub progress: Option<Arc<Progress>>,
    /// The run.
    pub job: Job<A>,
    /// The line's spans (first and last structure), by the line's version
    /// they were listed for: listed again only when the line changes (they
    /// take every section's place along the alignments).
    pub spans: Option<(u64, Spans)>,
    /// The inputs are written into the project at the next frame (a run
    /// started, the window closed; owner, 2026-10-28: not at every change,
    /// the page stays quick).
    pub commit: bool,
    /// The longest piece of a wire, m (none: as solved).
    pub piece_length: Option<f64>,
    /// The levels drawn.
    pub levels: Vec<Level>,
    /// The cuts coloured by band.
    pub fill: bool,
    /// The field in the cross-section panels: the step, m.
    pub in_cuts: Option<f64>,
    /// The volumes' step (their spans: ticked in the table).
    pub volume: Option<VolumeSetup>,
    /// The cuts asked in the table (owner, 2026-10-31): ± this across, m;
    /// every this, m; up to this over the highest wire, m.
    pub cut_width: f64,
    /// The cuts' step, m.
    pub cut_step: f64,
    /// The cuts' reach over the highest wire, m.
    pub cut_above: f64,
    /// What the window asks of the application (a cut opened, the 3D
    /// view turned to a volume), taken each frame.
    pub requests: Vec<FieldRequest>,
    /// The panels' cuts.
    pub panel: PanelCuts,
    /// Seconds per piece and point of the last run here.
    pub rate: Option<f64>,
    /// The share of a volume's lattice the last run computed.
    pub volume_share: Option<f64>,
    /// A section of the window's user guide page to open (a heading's
    /// **user guide ↗** clicked; the application opens it).
    pub guide: Option<&'static str>,
    /// The wires' pieces, by line version and case.
    pub pieces_seen: Option<((u64, usize), PieceStats)>,
    /// The overlay as last handed out.
    pub drawn: Option<(bool, u64, u64, String, bool, bool, String)>,
    /// Save in Performed checks.
    pub saving: crate::field_saved::Saving,
    /// A report saved: the command the app applies.
    pub apply: Option<(String, ohl_session::Command)>,
    /// How the wires are solved for a run (owner, 2026-11-01; none: each
    /// section's own, the case as shown).
    pub calculation: ohl_clearance::calculation::Calc,
    /// Per electrical system its own weather case and calculation
    /// (owner, 2026-11-01).
    pub per_system: crate::field_systems::PerSystem,
    /// Where the ground comes from (owner, 2026-11-01).
    pub ground_from: crate::field_ground::FieldGround,
    /// The outputs of the run unticked in its list (not drawn).
    pub hidden: field_results::Hidden,
}

/// The inputs both field windows keep in the project (JSON, part of each
/// window's own; the same keys as before they were put together).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StoredCommon {
    pub case: Option<String>,
    pub jumpers: Option<bool>,
    pub height: Option<f64>,
    pub width: Option<f64>,
    pub step: Option<f64>,
    /// One span's stations: its structures and the place in it (none:
    /// every span at its lowest point).
    pub span: Option<(String, String, Option<f64>)>,
    /// Profiles beside the structures: the distance into the spans, m
    /// (2026-10-31).
    pub beside: Option<f64>,
    /// The spans on an alignment unticked and those on none ticked.
    pub spans_off: Vec<(String, String)>,
    pub spans_on: Vec<(String, String)>,
    pub limit: Option<f64>,
    pub two_d: bool,
    /// 2D (cuts, profiles) or 3D (volumes) (2026-11-01; absent: 2D).
    pub planes: Option<bool>,
    /// 2D: the profiles too (absent: yes).
    pub profiles: Option<bool>,
    /// The page shown, what a run makes (2026-11-12; absent: from
    /// `planes` and `profiles`, [`FieldPage::of_old`]).
    pub page: Option<FieldPage>,
    /// The points file's name and text.
    pub points: Option<(String, String)>,
    /// The buildings' preset (2026-11-03).
    pub buildings: Option<crate::field_preset::Preset>,
    pub piece_length: Option<f64>,
    pub levels: Option<Vec<Level>>,
    pub fill: Option<bool>,
    pub in_cuts: Option<f64>,
    /// A volume: its first and last spans and the step.
    pub volume: Option<(SpanIds, SpanIds, f64)>,
    /// A volume over the spans ticked: its step (2026-10-30; `volume`
    /// before: its spans now the ticked ones, its step read).
    pub volume_step: Option<f64>,
    /// The volumes' half width (2026-11-01; before: the profiles').
    pub volume_width: Option<f64>,
    /// How the wires are solved (2026-11-01; none: each section's own).
    pub calculation: Option<ohl_line::condition::Calculation>,
    /// Per system (id; "": the earthed wires) its weather case by name
    /// and calculation (2026-11-01; absent: the window's).
    pub per_system: crate::field_systems::PerSystem,
    /// Where the ground comes from (2026-11-01; absent: the checks').
    pub ground_from: crate::field_ground::FieldGround,
    /// Per span the stations of its 2D cuts (2026-10-31).
    pub cuts: Vec<(SpanIds, Vec<CutAt>)>,
    /// The spans with a volume (2026-10-31).
    pub volume_spans: Vec<SpanIds>,
    /// The cuts' half width, step and reach over the highest wire, m.
    pub cut_width: Option<f64>,
    pub cut_step: Option<f64>,
    pub cut_above: Option<f64>,
}

/// What keeping the inputs asks next ([`FieldShared::keep_step`]).
pub(crate) enum KeepStep {
    /// Nothing now.
    Nothing,
    /// Read them from the project (none read yet for this line).
    Read,
    /// Write them under this key.
    Write(String),
}

impl<A> FieldShared<A> {
    /// A window's start, `start` the values where the two differ.
    pub fn new(start: FieldStart) -> Self {
        Self {
            open: false,
            case: 0,
            jumpers: start.jumpers,
            height: start.height,
            width: start.width,
            step: start.step,
            stations: Stations::EverySpan,
            span_pick: SpanPick::default(),
            limit: None,
            two_d: false,
            page: FieldPage::default(),
            points: None,
            buildings: None,
            points_text: None,
            saved: None,
            case_kept: false,
            points_error: None,
            shown: 0,
            reveal: false,
            progress: None,
            job: Job::default(),
            spans: None,
            commit: false,
            piece_length: None,
            levels: start.levels.clone(),
            fill: true,
            in_cuts: None,
            volume: Some(VolumeSetup {
                step: 2.0,
                width: start.volume_width,
            }),
            cut_width: start.cut_width,
            // 1 m (owner, 2026-11-01: quicker at the same accuracy where it
            // matters: the low levels' isolines within 0.03 %, measured).
            cut_step: 1.0,
            cut_above: crate::magnetic_field::views::ABOVE,
            requests: Vec::new(),
            panel: PanelCuts::default(),
            rate: None,
            volume_share: None,
            guide: None,
            pieces_seen: None,
            drawn: None,
            hidden: Default::default(),
            calculation: None,
            per_system: Default::default(),
            ground_from: Default::default(),
            saving: Default::default(),
            apply: None,
            start,
        }
    }

    /// What keeping the inputs under `setting` asks now: read when not read
    /// yet for the line worked on; written once a run starts or the window
    /// closes, not while a value is dragged or typed.
    pub(crate) fn keep_step(
        &mut self,
        model: &Model,
        ctx: &egui::Context,
        setting: &str,
    ) -> KeepStep {
        let Some(line) = model.session().map(|s| s.current_line().to_owned()) else {
            return KeepStep::Nothing;
        };
        let key = format!("{setting}.{line}");
        if self.saved.as_ref().is_none_or(|(k, _)| *k != key) {
            return KeepStep::Read;
        }
        if !self.commit
            || ctx.input(|i| i.pointer.any_down())
            || ctx.memory(|m| m.focused().is_some())
        {
            return KeepStep::Nothing;
        }
        self.commit = false;
        KeepStep::Write(key)
    }

    /// Writes `stored` under `key` when it changed. A project this user
    /// cannot change keeps it for the session.
    pub(crate) fn write_inputs(
        &mut self,
        model: &mut Model,
        key: String,
        stored: &impl serde::Serialize,
        what: &str,
    ) {
        let Ok(json) = serde_json::to_string(stored) else {
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
        let _ = model.project_op(what, op);
        self.saved = Some((key, json));
    }

    /// The spans' table with the page's column only; on 2D profiles the
    /// profiles' setup under it.
    pub(crate) fn spans_ui(&mut self, ui: &mut egui::Ui, id: &str, model: &Model, spans: &Spans) {
        self.span_pick.table(
            ui,
            id,
            model,
            spans,
            Columns {
                profile: self.page == FieldPage::Profiles,
                cuts: self.page == FieldPage::Cuts,
                volume: self.page == FieldPage::Volumes,
            },
        );
        if self.page == FieldPage::Profiles {
            self.profiles_setup_ui(ui, id, spans);
        }
    }

    /// What the page's run waits for in the span table (none: it can
    /// run): a span ticked for the profiles, a cut asked, a volume ticked.
    pub(crate) fn page_blocker(&mut self, model: &Model) -> Option<String> {
        let spans = self.spans(model);
        match self.page {
            FieldPage::Profiles => {
                (matches!(self.stations, Stations::EverySpan | Stations::Beside { .. })
                    && self.span_pick.chosen(model, &spans).0.is_empty())
                .then(|| "no span ticked (3 What and where)".to_owned())
            }
            FieldPage::Cuts => (self.span_pick.cut_places(model, &spans).0.is_empty()
                && self.buildings.is_none())
            .then(|| "no cut asked: + station in the table (3 What and where)".to_owned()),
            FieldPage::Volumes => (self.volume.is_none()
                || self.span_pick.volume_runs(model, &spans).is_empty())
            .then(|| "no span with Volume ticked (3 What and where)".to_owned()),
        }
    }

    /// The inputs both windows keep, as kept in the project.
    pub(crate) fn stored_common(&mut self, model: &Model) -> StoredCommon {
        let spans = self.spans(model);
        let span = match &self.stations {
            Stations::EverySpan | Stations::Beside { .. } => None,
            Stations::One { span, fraction } => spans
                .get(*span)
                .map(|(from, to)| (from.clone(), to.clone(), *fraction)),
        };
        StoredCommon {
            case: model.cases.get(self.case).map(|c| c.case.name.clone()),
            jumpers: Some(self.jumpers),
            height: Some(self.height),
            width: Some(self.width),
            step: Some(self.step),
            span,
            beside: match self.stations {
                Stations::Beside { distance } => Some(distance),
                _ => None,
            },
            spans_off: self.span_pick.off.iter().cloned().collect(),
            spans_on: self.span_pick.on.iter().cloned().collect(),
            limit: self.limit,
            two_d: self.two_d,
            page: Some(self.page),
            // As older versions read them: the page's outputs.
            planes: Some(self.page != FieldPage::Volumes),
            profiles: Some(self.page == FieldPage::Profiles),
            points: match (&self.points, &self.points_text) {
                (Some((name, _)), Some(text)) => Some((name.clone(), text.clone())),
                _ => None,
            },
            buildings: self.buildings.clone(),
            piece_length: self.piece_length,
            levels: Some(self.levels.clone()),
            fill: Some(self.fill),
            in_cuts: self.in_cuts,
            volume: None,
            volume_step: self.volume.as_ref().map(|v| v.step),
            volume_width: self.volume.as_ref().map(|v| v.width),
            calculation: self.calculation,
            per_system: self.per_system.clone(),
            ground_from: self.ground_from.clone(),
            cuts: self
                .span_pick
                .cuts
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            volume_spans: self.span_pick.volume.iter().cloned().collect(),
            cut_width: Some(self.cut_width),
            cut_step: Some(self.cut_step),
            cut_above: Some(self.cut_above),
        }
    }

    /// Takes the inputs kept (in the project or with a saved run); what is
    /// no longer there (a case, a span) falls back to the window's start
    /// values, as they were made.
    pub(crate) fn take_common(&mut self, model: &Model, st: StoredCommon) {
        let d = Self::new(self.start.clone());
        self.calculation = st.calculation;
        self.per_system = st.per_system.clone();
        self.ground_from = st.ground_from.clone();
        self.case_kept = false;
        if let Some(k) = st
            .case
            .as_ref()
            .and_then(|n| model.cases.iter().position(|c| &c.case.name == n))
        {
            self.case = k;
            self.case_kept = true;
        }
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
            .or(st.beside.map(|distance| Stations::Beside { distance }))
            .unwrap_or_default();
        self.limit = st.limit;
        self.two_d = st.two_d;
        self.page = st.page.unwrap_or(FieldPage::of_old(st.planes, st.profiles));
        self.piece_length = st.piece_length;
        self.levels = st.levels.unwrap_or(d.levels);
        self.fill = st.fill.unwrap_or(d.fill);
        self.in_cuts = st.in_cuts;
        self.volume = Some(VolumeSetup {
            step: st.volume_step.or(st.volume.map(|v| v.2)).unwrap_or(2.0),
            width: st.volume_width.unwrap_or(self.width),
        });
        self.span_pick.cuts = st.cuts.into_iter().collect();
        self.span_pick.volume = st.volume_spans.into_iter().collect();
        self.cut_width = st.cut_width.unwrap_or(d.cut_width);
        self.cut_step = st.cut_step.unwrap_or(d.cut_step);
        self.cut_above = st.cut_above.unwrap_or(d.cut_above);
        self.buildings = st.buildings.clone();
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
    }

    /// The line's spans by their first and last structure, listed once per
    /// version of the line.
    pub(crate) fn spans(&mut self, model: &Model) -> Spans {
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

    /// Opens the window on the weather case shown.
    pub fn show(&mut self, case: usize) {
        if !self.open && !self.case_kept {
            self.case = case;
        }
        self.open = true;
    }

    /// Asks a 2D cut in span `k` at `at` (tests).
    #[cfg(test)]
    pub fn add_cut(&mut self, model: &Model, k: usize, at: CutAt) {
        self.page = FieldPage::Cuts;
        let spans = self.spans(model);
        self.span_pick
            .cuts
            .entry(spans[k].clone())
            .or_default()
            .push(at);
    }

    /// The span table's state (tests): the spans ticked.
    #[cfg(test)]
    pub fn ticked_spans(&mut self, model: &Model) -> Vec<usize> {
        let spans = self.spans(model);
        let (mut c, _) = self.span_pick.chosen(model, &spans);
        c.sort_unstable();
        c
    }

    /// The panels' cuts computed (tests).
    #[cfg(test)]
    pub fn panel_cuts_done(&self) -> usize {
        self.panel.done.len()
    }

    /// What the last run was computed from that changed since, in words
    /// (`None`: nothing, or no answer).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        if !matches!(self.job.result, Some(Ok(_))) {
            return None;
        }
        // A saved run: compared with the line by its own state.
        match &self.saving.stale {
            Some(s) => Some(s.clone()),
            None => self.job.stamp.out_of_date(model, USES),
        }
    }

    /// The limit, µT (tests).
    #[cfg(test)]
    pub fn limit(&self) -> Option<f64> {
        self.limit
    }

    /// Sets the limit, µT.
    pub fn set_limit(&mut self, limit: Option<f64>) {
        self.limit = limit;
    }

    /// The buildings' preset set up (tests).
    #[cfg(test)]
    pub fn buildings(&self) -> Option<&crate::field_preset::Preset> {
        self.buildings.as_ref()
    }

    /// Sets the buildings' preset (Checks › Electrical › Buildings).
    pub fn set_buildings(&mut self, preset: Option<crate::field_preset::Preset>) {
        // The buildings' cuts and places: on the 2D cuts page.
        if preset.is_some() {
            self.page = FieldPage::Cuts;
        }
        self.buildings = preset;
        self.commit = true;
    }

    /// The profiles: where and how wide (2D, profiles asked).
    pub(crate) fn profiles_setup_ui(&mut self, ui: &mut egui::Ui, id: &str, spans: &Spans) {
        egui::Grid::new(format!("{id}-profiles-setup"))
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Profiles");
                profiles_ui(ui, id, &mut self.stations, spans);
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
                    )
                    .on_hover_text(
                        "The profile's half width across the line: wider, the low levels' reach \
                         inside it, more points",
                    );
                    ui.label("every");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.step)
                            .range(0.1..=50.0)
                            .speed(0.1)
                            .suffix(" m"),
                    )
                    .on_hover_text(
                        "The step between a profile's points: finer, more points, the time in \
                         proportion",
                    );
                    ui.label("at");
                    crate::style::field(
                        ui,
                        egui::DragValue::new(&mut self.height)
                            .range(0.0..=200.0)
                            .speed(0.1)
                            .suffix(" m"),
                    )
                    .on_hover_text(
                        "The profile's height above the ground: the field is largest nearest \
                         the wires",
                    );
                    ui.label("above the ground");
                });
                ui.end_row();
            });
    }
}

/// Where the profiles are taken.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum Stations {
    /// Every span at its lowest point.
    #[default]
    EverySpan,
    /// One span (index into the line's spans), at its lowest point or a
    /// fraction of it.
    One { span: usize, fraction: Option<f64> },
    /// Beside the structures (owner, 2026-10-31: where the steel matters):
    /// in each span ticked, `distance` into it from both ends (a span not
    /// twice as long: its middle).
    Beside { distance: f64 },
}

/// The profiles of [`Stations::Beside`] over the spans `chosen`
/// (indices into `spans`): `distance` into each from both ends, along the
/// line in plan.
pub(crate) fn beside_stations(
    model: &Model,
    spans: &Spans,
    chosen: &[usize],
    distance: f64,
) -> Vec<StationSpec> {
    let mut out = Vec::new();
    for &k in chosen {
        let (from, to) = &spans[k];
        let at = |id: &str| model.line.structure(id).map(|s| s.position);
        let (Some(a), Some(b)) = (at(from), at(to)) else {
            continue;
        };
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let spec = |fraction: f64| StationSpec {
            from: from.clone(),
            to: to.clone(),
            fraction: Some(fraction),
        };
        if len > 2.0 * distance && len > 0.0 {
            out.push(spec(distance / len));
            out.push(spec(1.0 - distance / len));
        } else {
            out.push(spec(0.5));
        }
    }
    out
}

/// The Profiles row: at each ticked span's lowest point, beside the
/// structures, or one span (at its lowest point or a place in it).
pub(crate) fn profiles_ui(ui: &mut egui::Ui, id: &str, stations: &mut Stations, spans: &Spans) {
    ui.horizontal(|ui| {
        #[derive(PartialEq, Clone, Copy)]
        enum Kind {
            Lowest,
            Beside,
            One,
        }
        let mut kind = match stations {
            Stations::EverySpan => Kind::Lowest,
            Stations::Beside { .. } => Kind::Beside,
            Stations::One { .. } => Kind::One,
        };
        ui.radio_value(
            &mut kind,
            Kind::Lowest,
            "at each ticked span's lowest point",
        );
        ui.radio_value(&mut kind, Kind::Beside, "beside its structures")
            .on_hover_text(
                "In each span ticked, a profile this far into it from each of its structures \
                 (where the structures' steel and the wires near them matter)",
            );
        ui.radio_value(&mut kind, Kind::One, "one span");
        match kind {
            Kind::Lowest => *stations = Stations::EverySpan,
            Kind::Beside => {
                if !matches!(stations, Stations::Beside { .. }) {
                    *stations = Stations::Beside { distance: 10.0 };
                }
                if let Stations::Beside { distance } = stations {
                    crate::style::field(
                        ui,
                        egui::DragValue::new(distance)
                            .range(1.0..=500.0)
                            .speed(0.5)
                            .suffix(" m"),
                    );
                    ui.label("into the span");
                }
            }
            Kind::One => {
                if !matches!(stations, Stations::One { .. }) {
                    *stations = Stations::One {
                        span: 0,
                        fraction: None,
                    };
                }
                if let Stations::One { span, fraction } = stations {
                    *span = (*span).min(spans.len().saturating_sub(1));
                    let name = |k: usize| {
                        spans
                            .get(k)
                            .map_or(String::new(), |(a, b)| format!("{a}–{b}"))
                    };
                    egui::ComboBox::from_id_salt((id, "span"))
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
                            "The place in the span from its first structure along the line (0–1)",
                        );
                    }
                }
            }
        }
    });
}

/// The three pages of a field window's setup (owner, 2026-11-12: "one
/// to make the 3D volumes, the 2D profiles and the 2D cuts"): each shows
/// only what its output needs; a run computes the page shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldPage {
    /// The field's surfaces around the wires over the spans ticked.
    Volumes,
    /// The field across the spans at a height above the ground.
    #[default]
    Profiles,
    /// The field's isolines in vertical planes across the line.
    Cuts,
}

impl FieldPage {
    /// The three, in the order of the tabs.
    pub const ALL: [FieldPage; 3] = [FieldPage::Volumes, FieldPage::Profiles, FieldPage::Cuts];

    /// Its tab's title.
    pub fn title(self) -> &'static str {
        match self {
            FieldPage::Volumes => "3D volumes",
            FieldPage::Profiles => "2D profiles",
            FieldPage::Cuts => "2D cuts",
        }
    }

    /// Its part of the window's user guide page (the heading's anchor).
    pub fn guide(self) -> &'static str {
        match self {
            FieldPage::Volumes => "3d-volumes",
            FieldPage::Profiles => "2d-profiles",
            FieldPage::Cuts => "2d-cuts",
        }
    }

    /// The page of inputs kept before the pages (2026-11-12): `planes`
    /// false the volumes; else the profiles when they were made (absent:
    /// yes), the cuts when not.
    pub fn of_old(planes: Option<bool>, profiles: Option<bool>) -> FieldPage {
        match (planes.unwrap_or(true), profiles.unwrap_or(true)) {
            (false, _) => FieldPage::Volumes,
            (true, true) => FieldPage::Profiles,
            (true, false) => FieldPage::Cuts,
        }
    }
}

/// The pages' tabs at the top of the setup.
pub(crate) fn page_ui(ui: &mut egui::Ui, page: &mut FieldPage) {
    ui.horizontal(|ui| {
        ui.label("Make");
        for p in FieldPage::ALL {
            let hover = match p {
                FieldPage::Volumes => {
                    "The surfaces of the levels around the wires over the spans with Volume \
                     ticked, from the 3D model"
                }
                FieldPage::Profiles => {
                    "The field across each span ticked at a height above the ground (and at \
                     the points from CSV): from the 3D model or 2D only"
                }
                FieldPage::Cuts => {
                    "Isolines in vertical planes across the line at the stations asked \
                     (+ station in the table), from the 3D model"
                }
            };
            ui.selectable_value(page, p, egui::RichText::new(p.title()).strong())
                .on_hover_text(hover);
        }
    });
}

/// A method in words: one short line beside it (what it assumes, when it
/// is enough, its time), the rest behind its (?).
pub(crate) struct MethodText {
    /// The line beside the choice.
    pub short: &'static str,
    /// Behind the (?).
    pub long: &'static str,
}

/// The profiles' method, each beside its explanation (owner, 2026-11-12:
/// "each method clearly explained in the page aside with the influence
/// of each parameter"): from the 3D model or 2D only.
pub(crate) fn profiles_method_ui(
    ui: &mut egui::Ui,
    two_d: &mut bool,
    three_d: &MethodText,
    flat: &MethodText,
) {
    egui::Grid::new("field-profiles-method")
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.label("Method");
            ui.radio_value(two_d, false, "from the 3D model")
                .on_hover_text(three_d.long);
            ui.end_row();
            ui.label("");
            crate::check_help::explain(ui, three_d.short, three_d.long);
            ui.end_row();
            ui.label("");
            ui.radio_value(two_d, true, "2D only")
                .on_hover_text(flat.long);
            ui.end_row();
            ui.label("");
            crate::check_help::explain(ui, flat.short, flat.long);
            ui.end_row();
        });
    // The 2D method's assumptions are not on the safe side everywhere:
    // said where it is chosen.
    if *two_d {
        ui.colored_label(ui.visuals().warn_fg_color, TWO_D_ASSUMES);
    }
}

/// What the 2D-only method assumes, said where it is chosen and in its
/// results.
pub(crate) const TWO_D_ASSUMES: &str = "2D only assumes infinitely long, straight, parallel \
     conductors at the section's heights (no sag along the span, no line angle, no jumpers, no \
     structures): NOT ON THE SAFE SIDE near a line angle, a structure or a crossing (warned per \
     profile).";

/// What the cuts' 2D-only method lacks: the engines compute cuts from the
/// 3D model only (2026-11-12).
pub(crate) const CUTS_TWO_D_MISSING: &str =
    "not available yet: the field engines compute cuts from the 3D model only";

/// The cuts' method: from the 3D model (the only one the engines have);
/// 2D only shown disabled with the reason.
pub(crate) fn cuts_method_ui(ui: &mut egui::Ui, three_d: &MethodText) {
    egui::Grid::new("field-cuts-method")
        .num_columns(2)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.label("Method");
            let _ = ui
                .radio(true, "from the 3D model")
                .on_hover_text(three_d.long);
            ui.end_row();
            ui.label("");
            crate::check_help::explain(ui, three_d.short, three_d.long);
            ui.end_row();
            ui.label("");
            ui.add_enabled(false, egui::RadioButton::new(false, "2D only"))
                .on_disabled_hover_text(CUTS_TWO_D_MISSING);
            ui.end_row();
            ui.label("");
            ui.label(egui::RichText::new(CUTS_TWO_D_MISSING).small().weak());
            ui.end_row();
        });
}

/// A profile to plot ([`plot`]): what differs between the fields.
pub(crate) struct ProfilePlot<'a> {
    /// (Offset from the axis, m; the value), drawn strong.
    pub values: &'a [(f64, f64)],
    /// A second curve, drawn faint under it (the largest over the cycle).
    pub faint: Option<&'a [(f64, f64)]>,
    /// The axis' title, as "E kV/m".
    pub axis: &'a str,
    /// The unit, as "kV/m".
    pub unit: &'a str,
    /// The text at point `k` under the pointer.
    pub hover: &'a dyn Fn(usize) -> String,
}

/// A profile's plot: B (and the cycle's largest value, fainter) against
/// the distance from the axis, the limit as a dashed line, the value under
/// the pointer.
pub(crate) fn plot(ui: &mut egui::Ui, p: &ProfilePlot<'_>, limit: Option<f64>) -> egui::Rect {
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
    let (Some(first), Some(last)) = (p.values.first(), p.values.last()) else {
        return r;
    };
    let (x0, x1) = (first.0, last.0.max(first.0 + 1e-9));
    let top = p
        .values
        .iter()
        .map(|q| q.1)
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
        p.axis,
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
    if let Some(faint_values) = p.faint {
        let pts: Vec<egui::Pos2> = faint_values.iter().map(|q| to_screen(q.0, q.1)).collect();
        painter.add(egui::Shape::line(
            pts,
            egui::Stroke::new(1.0, line.gamma_multiply(0.45)),
        ));
    }
    let main: Vec<egui::Pos2> = p.values.iter().map(|q| to_screen(q.0, q.1)).collect();
    painter.add(egui::Shape::line(main, egui::Stroke::new(2.0, line)));
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
            format!("limit {l} {}", p.unit),
            font.clone(),
            v.warn_fg_color,
        );
    }
    if let Some(pos) = response.hover_pos()
        && frame.contains(pos)
    {
        let x = x0 + f64::from((pos.x - frame.left()) / frame.width()) * (x1 - x0);
        if let Some((k, q)) = p
            .values
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1.0 - x).abs().total_cmp(&(b.1.0 - x).abs()))
        {
            let at = to_screen(q.0, q.1);
            painter.circle_filled(at, 3.0, line);
            painter.text(
                at + egui::vec2(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                (p.hover)(k),
                font,
                text,
            );
        }
    }
    r
}

/// Adds the notes of a run to `notes`: the sections the case does not hold,
/// once rather than one line each (owner, 2026-10-31); the systems' and the
/// ground's notes first; a word when there is no terrain surface.
pub(crate) fn add_run_notes(
    notes: &mut Vec<String>,
    line: &ohl_line::Line,
    shapes: &[ohl_line::solve::SectionShape],
    per_notes: Vec<String>,
    ground_note: Option<String>,
    has_ground: bool,
) {
    let missing: Vec<&str> = line
        .sections
        .iter()
        .map(|s| s.id.as_str())
        .filter(|id| !shapes.iter().any(|s| s.section == *id))
        .collect();
    if !missing.is_empty() {
        // The engine's own note per cross-section, said once here.
        notes.retain(|n| !n.ends_with(": not solved, left out"));
        let shown: Vec<&str> = missing.iter().take(5).copied().collect();
        notes.insert(
            0,
            format!(
                "{} section(s) not solved in this case, left out: {}{}",
                missing.len(),
                shown.join(", "),
                if missing.len() > 5 { ", …" } else { "" }
            ),
        );
    }
    for n in per_notes.into_iter().rev().chain(ground_note) {
        notes.insert(0, n);
    }
    if !has_ground {
        notes.push(
            "ground: straight between the structures' feet and level across (no terrain surface)"
                .into(),
        );
    }
}

/// What **Wires by** changes (its (?) in both field windows).
pub const WIRES_BY_HELP: &str = "How every wire is solved in the weather case for this window: \
    each section's own calculation, or another for all (e.g. Max Sag: the lowest wires, the \
    highest field at the ground). Another than each section's own solves the case for the run: \
    a solve per case.";

/// A field run under way as its footer says it (owner, 2026-11-10: "step
/// 1 of x increasing, then resetting to 0 with step 2 and so on"): the
/// step, its bar from its own points, the time gone and the time its
/// points left take. Before the first step (or a run without steps): the
/// points as a whole.
pub fn run_progress(p: &Progress, elapsed: std::time::Duration) -> crate::check_ui::Progress {
    use std::sync::atomic::Ordering;
    let stage = p.stage();
    let Some(step) = p.step_now() else {
        let total = p.total.load(Ordering::Relaxed).max(1);
        let done = p.done.load(Ordering::Relaxed);
        let mut pr = crate::check_ui::Progress::with_left(elapsed, p.seconds_left(elapsed));
        pr.text = if stage.is_empty() {
            format!("{done} of {total} points; {}", pr.text)
        } else {
            format!("{stage}; {}", pr.text)
        };
        return pr;
    };
    let mut text = format!("step {} of {}: {}", step.number, step.of, step.label);
    if !stage.is_empty() && stage != step.label {
        text.push_str(&format!(" — {stage}"));
    }
    text.push_str(&format!("; {:.0} s", elapsed.as_secs_f64()));
    if let Some(l) = step.left.filter(|l| *l >= 1.0) {
        text.push_str(&format!(
            ", about {} left of this step",
            crate::check_ui::left_text(l)
        ));
    }
    crate::check_ui::Progress {
        fraction: step.fraction,
        text,
    }
}

/// The steps a field run of `kinds` takes (each: its cuts, its volumes),
/// and one for the weather case when it is solved first and `more` for
/// what follows: a step per cut, two per volume (its coarse and its fine
/// lattice), one per other kind.
pub fn steps_planned(kinds: &[(usize, usize)], unsolved: bool, more: usize) -> usize {
    usize::from(unsolved)
        + more
        + kinds
            .iter()
            .map(|&(cuts, volumes)| (cuts + 2 * volumes).max(1))
            .sum::<usize>()
}

/// A note of a field run: what was not computed in the error colour,
/// what is not on the safe side in the warning colour (both in full,
/// never faint), the others small and faint (owner, 2026-11-14: a cut
/// that failed is said, not lost among the notes).
pub fn note_label(ui: &mut egui::Ui, n: &str) {
    if n.starts_with("NOT COMPUTED") {
        ui.colored_label(ui.visuals().error_fg_color, n);
    } else if n.starts_with("NOT ON THE SAFE SIDE") {
        ui.colored_label(ui.visuals().warn_fg_color, n);
    } else {
        ui.label(egui::RichText::new(n).small().weak());
    }
}
