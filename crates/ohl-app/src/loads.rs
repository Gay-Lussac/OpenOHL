// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Structure loads: the loads the wires and strings put on the
//! structures in each load case (the load case file used: the project's,
//! Line › Criteria › Load cases), computed on demand in the background — the
//! model is not changed. One load case shown at a time; factored or not;
//! a click goes to the structure; CSV export. **Usage**: how much of its
//! capacity each structure uses (its type's allowable spans and loads,
//! structure model and pole, [`ohl_loads::capacity`]), the governing load
//! case or one case, worst first. A structure whose loads in a case lack
//! a section or jumper that did not solve there is **incomplete**: its
//! rows are greyed, "not checked: S12 not solved in ice", never a pass.
//! **Colour structures by usage** colours them in the plan and 3D views
//! ([`UsageMark`], with a legend): incomplete ones in a neutral colour,
//! "not checked".

use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui;
use ohl_loads::CaseLoads;
use ohl_loads::capacity::{CaseUsage, StructureUsage, UsageStatus};
// Usage from `ohl_structures_fe::check::NEAR_FULL` is shown as a warning
// (orange): one classification for both applications.
use ohl_structures_fe::check::{UsageClass, classify_usage};

use crate::check_help as help;
use crate::model::Model;

/// What its results are computed from ([`crate::revision`]).
pub(crate) const USES: crate::revision::Uses = crate::revision::Uses::LINE;

/// Its **Use local conditions**: the wind taken, the check's name for
/// the run times, what the wind is for.
const LOCAL: (ohl_line::local::Purpose, &str, &str) = (
    ohl_line::local::Purpose::Strength,
    "structure-loads",
    "every load case as if it said use_local = true: the wires solved with their spans' wind \
     and ice for strength, and the wind on the structures' bodies (wind bands, the members of \
     the usage check, the poles) times each structure's factor, once; a case's own \
     [case.structure] ice = true takes each structure's ice",
);

/// What a run computes from (taken on the main thread).
pub struct Inputs {
    /// The line (not localized: the load cases localize it themselves).
    pub line: ohl_line::Line,
    /// Its wires.
    pub library: ohl_cables::Library,
    /// The structure types.
    pub types: ohl_structures::StructureLibrary,
    /// The weather cases.
    pub weather: Vec<ohl_weather::WeatherCase>,
    /// The cross-section library (or why it does not read).
    pub sections: Result<ohl_sections::SectionLibrary, String>,
    /// The load cases, or why there are none.
    pub cases: Result<Vec<ohl_loads::LoadCase>, String>,
    /// **Use local conditions** ticked.
    pub local: bool,
    /// The structures kept (View › Work on route).
    pub scope: Option<std::collections::BTreeSet<String>>,
    /// The structures ticked (2 Scope).
    pub structures: Option<Vec<String>>,
}

/// The loads and their usage (one rule for the local conditions, owner
/// 2026-11-03): the window's switch sets `use_local` on every case
/// ([`ohl_loads::with_local`]) and the line goes in as it is, so
/// `ohl_loads` solves the localized wires and takes each structure's
/// `wind_static` once on its body wind (bands, members, poles); the
/// structures' ice and snow rows come with
/// [`ohl_loads::add_structure_loads`].
pub fn compute(i: &Inputs) -> (Outcome, Vec<CaseUsage>) {
    let line = &i.line;
    let r = i.sections.clone().and_then(|sections| {
        let cases = ohl_loads::with_local(i.cases.clone()?, i.local);
        let mut loads = ohl_loads::structure_loads(
            line,
            &i.library,
            &i.weather,
            &cases,
            &ohl_loads::default_budget(),
        )?;
        ohl_loads::add_structure_loads(&mut loads, line, &i.types, &sections, &i.weather);
        for c in loads.iter_mut() {
            if let Some(on) = &i.scope {
                c.loads.retain(|l| on.contains(&l.structure));
                c.missing.retain(|m| on.contains(&m.structure));
            }
            c.loads
                .retain(|l| crate::tick_list::ticked(&i.structures, &l.structure));
            c.missing
                .retain(|m| crate::tick_list::ticked(&i.structures, &m.structure));
        }
        Ok(loads)
    });
    let usage = r.as_ref().map_or_else(
        |_| Vec::new(),
        |loads| {
            let sections = i.sections.clone().unwrap_or_default();
            ohl_loads::capacity::usage_with_sections(
                line, &i.types, &sections, &i.library, &i.weather, loads,
            )
        },
    );
    (r, usage)
}

type Outcome = Result<Vec<CaseLoads>, String>;

/// A computation's answer: the loads, and the usage computed from them.
type Finished = (Outcome, Vec<CaseUsage>, Vec<String>);

/// Colour of what is not checked because a wire did not solve (grey).
const INCOMPLETE: egui::Color32 = egui::Color32::from_rgb(150, 156, 170);

/// How a structure is coloured by usage in the plan and 3D views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageMark {
    /// Below 90 %.
    Within,
    /// From 90 % to 100 %.
    Near,
    /// Over 100 % (or not allowed).
    Over,
    /// Not checked: a section or jumper at it did not solve (its loads
    /// are incomplete).
    Incomplete,
    /// Approximate: its finite-element check kept tension-only members
    /// active because their set did not settle.
    Approximate,
    /// Within its capacity, but part of a member's check not done (not on
    /// the safe side, 2026-11-08): never shown as a pass.
    NotSafe,
    /// Nothing to check against (no type, no capacity).
    Unchecked,
}

impl UsageMark {
    /// Every mark, in the legend's order.
    pub const ALL: [UsageMark; 7] = [
        UsageMark::Within,
        UsageMark::Near,
        UsageMark::Over,
        UsageMark::NotSafe,
        UsageMark::Incomplete,
        UsageMark::Approximate,
        UsageMark::Unchecked,
    ];

    /// The mark of a structure's usage (in one case, or its governing one).
    pub fn of(s: &StructureUsage) -> Self {
        match s.status() {
            UsageStatus::Incomplete => Self::Incomplete,
            UsageStatus::Approximate => Self::Approximate,
            UsageStatus::NotSafe => Self::NotSafe,
            UsageStatus::NotChecked => Self::Unchecked,
            UsageStatus::Over => Self::Over,
            UsageStatus::Pass => match s.governing() {
                Some(g) if classify_usage(g.usage) == UsageClass::Near => Self::Near,
                _ => Self::Within,
            },
        }
    }

    /// Its colour.
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Within => [110, 200, 120],
            Self::Near => [255, 170, 60],
            Self::Over => [235, 70, 60],
            Self::Incomplete => [150, 156, 170],
            Self::Approximate => [175, 120, 220],
            Self::NotSafe => [215, 110, 200],
            Self::Unchecked => [95, 100, 110],
        }
    }

    /// In the legend.
    pub fn label(self) -> &'static str {
        match self {
            Self::Within => "below 90 %",
            Self::Near => "90 – 100 %",
            Self::Over => "over 100 %",
            Self::Incomplete => "not checked: a wire not solved",
            Self::Approximate => "approximate (tension-only kept)",
            Self::NotSafe => "not all checked (not safe side)",
            Self::Unchecked => "no capacity to check",
        }
    }
}

/// The legend of the colours by usage, at the bottom right of `rect`.
pub fn paint_usage_legend(painter: &egui::Painter, rect: egui::Rect, out_of_date: bool) {
    let font = egui::FontId::proportional(11.0);
    let row = 15.0;
    let width = 190.0;
    let height = row * UsageMark::ALL.len() as f32 + 22.0;
    let at = rect.right_bottom() + egui::vec2(-8.0 - width, -8.0 - height);
    let frame = egui::Rect::from_min_size(at, egui::vec2(width, height));
    painter.rect_filled(frame, 4.0, egui::Color32::from_black_alpha(170));
    painter.text(
        at + egui::vec2(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        if out_of_date {
            "structures by usage: OUT OF DATE"
        } else {
            "structures by usage"
        },
        font.clone(),
        if out_of_date {
            egui::Color32::from_rgb(255, 190, 60)
        } else {
            egui::Color32::WHITE
        },
    );
    for (k, m) in UsageMark::ALL.iter().enumerate() {
        let y = at.y + 20.0 + row * k as f32;
        let [r, g, b] = m.color();
        painter.circle_filled(
            egui::pos2(at.x + 12.0, y + 6.0),
            4.5,
            egui::Color32::from_rgb(r, g, b),
        );
        painter.text(
            egui::pos2(at.x + 22.0, y),
            egui::Align2::LEFT_TOP,
            m.label(),
            font.clone(),
            egui::Color32::WHITE,
        );
    }
}

/// The loads of case `r` at the structures whose id contains `filter`
/// (lower case; empty: all), in the case's order.
fn loads_shown<'a>(
    r: &'a CaseLoads,
    filter: &'a str,
) -> impl Iterator<Item = &'a ohl_loads::AttachmentLoad> {
    r.loads
        .iter()
        .filter(move |l| filter.is_empty() || l.structure.to_lowercase().contains(filter))
}

/// A row of the loads table: load `l` of case `r`, factored or not;
/// incomplete (a wire's forces missing at its structure) as a violation.
fn load_row(
    r: &CaseLoads,
    l: &ohl_loads::AttachmentLoad,
    unfactored: bool,
) -> crate::check_ui::Row {
    use crate::check_ui::Cell;
    let missing: Vec<String> = r.missing_at(&l.structure).map(|m| m.short()).collect();
    let v = if unfactored { l.local } else { l.factored };
    let sections = if missing.is_empty() {
        l.sections.join(", ")
    } else {
        format!(
            "{} — incomplete: {}",
            l.sections.join(", "),
            missing.join(", ")
        )
    };
    crate::check_ui::Row::new(
        vec![
            Cell::text(&l.structure).bad(!missing.is_empty()),
            Cell::text(&l.label),
            Cell::text(
                l.sets
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            Cell::num(Some(v[0] / 1000.0), 2),
            Cell::num(Some(v[1] / 1000.0), 2),
            Cell::num(Some(v[2] / 1000.0), 2),
            Cell::text(sections).bad(!missing.is_empty()),
        ],
        !missing.is_empty(),
    )
}

/// The Structure loads window (a page until 2026-10-27; now docked as a
/// panel or floating like every check window, [`crate::check_help::window`]).
#[derive(Default)]
pub struct LoadsPage {
    /// Shown.
    pub open: bool,
    /// The last results (or why there are none).
    pub results: Option<Outcome>,
    /// **Use local conditions** (owner, 2026-11-03): the loads with the
    /// structures' wind for strength and wire ice on their spans, and their
    /// wind for strength on their own wind bands.
    pub local: bool,
    /// A computation running: since, its answer.
    running: Option<(Instant, Receiver<Finished>)>,
    /// How much of their capacity the structures use, per load case (with
    /// the last results).
    pub usage: Vec<CaseUsage>,
    /// The usage shown instead of the loads.
    pub show_usage: bool,
    /// How long the last one took.
    pub took: Option<std::time::Duration>,
    /// The last run's notes (Use local conditions, the weather groups no
    /// longer read).
    pub notes: Vec<String>,
    /// The load case shown.
    pub case: usize,
    /// Structures shown (id contains this text).
    pub filter: String,
    /// Factored loads shown (else unfactored).
    pub unfactored: bool,
    /// The usage's load case: 0 the governing one, k + 1 case k.
    pub usage_case: usize,
    /// The structures coloured by usage in the plan and 3D views (the
    /// usage's load case).
    pub colour_structures: bool,
    /// What the results were computed from ([`crate::revision`]).
    stamp: crate::revision::Stamp,
    /// The load cases computed, by name (`None`: every one; 2 Scope,
    /// owner 2026-10-31).
    pub cases_ticked: Option<Vec<String>>,
    /// The structures computed, by id (`None`: every one).
    pub structures: Option<Vec<String>>,
    /// The load case file's case names, as last read (file, names or why
    /// not).
    case_names: Option<(std::path::PathBuf, Result<Vec<String>, String>)>,
    /// Counts the answers taken (a new one opens the Results tab).
    generation: u64,
    /// The loads table's rows, by the answers taken, the load case
    /// shown, factored or not and the structure filter.
    rows: crate::check_ui::CachedRows<(u64, usize, bool, String)>,
    /// How much the last run computed (cases × structures): its time
    /// estimates the next.
    units: (usize, usize),
}

impl LoadsPage {
    /// Whether a computation runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// The colour of each structure by usage in the usage's load case (the
    /// governing one: incomplete in any case is incomplete), when
    /// colouring by usage is on (else none).
    pub fn usage_marks(&self) -> std::collections::BTreeMap<String, UsageMark> {
        let mut out = std::collections::BTreeMap::new();
        if !self.colour_structures {
            return out;
        }
        match self.usage_case {
            0 => {
                for c in &self.usage {
                    for s in &c.structures {
                        out.entry(s.structure.clone())
                            .or_insert(UsageMark::Unchecked);
                    }
                }
                for (_, s) in ohl_loads::capacity::governing(&self.usage) {
                    out.insert(s.structure.clone(), UsageMark::of(s));
                }
                for c in &self.usage {
                    for s in c.structures.iter().filter(|s| s.is_incomplete()) {
                        out.insert(s.structure.clone(), UsageMark::Incomplete);
                    }
                }
            }
            k => {
                if let Some(c) = self.usage.get(k - 1) {
                    for s in &c.structures {
                        out.insert(s.structure.clone(), UsageMark::of(s));
                    }
                }
            }
        }
        out
    }

    /// Computes the loads of the model as it is now (in the background).
    pub fn start(&mut self, model: &Model, ctx: &egui::Context) {
        let Some(path) = model.sources.load_cases.clone() else {
            self.results = Some(Err(
                "no load case file: Line › Criteria › Load cases library (the project's own, or a \
                 .ohllc file next to a line file)"
                    .into(),
            ));
            return;
        };
        // Its notes and run times (the line itself goes in as it is: the
        // load cases take the local conditions, one rule).
        let local = crate::local_solve::prepare(&model.line, self.local, LOCAL.0);
        let ticked = self.cases_ticked.clone();
        let cases = ohl_loads::LoadCases::load(&path).and_then(|set| {
            let cases: Vec<_> = set
                .cases
                .into_iter()
                .filter(|c| crate::tick_list::ticked(&ticked, &c.name))
                .collect();
            if cases.is_empty() {
                return Err("no load case ticked (2 Scope)".to_owned());
            }
            Ok(cases)
        });
        let inputs = Inputs {
            line: model.line.clone(),
            library: model.library.clone(),
            types: model
                .types
                .as_ref()
                .map(|(t, _)| t.clone())
                .unwrap_or_default(),
            weather: model.cases.iter().map(|c| c.case.clone()).collect(),
            // The structure models' section library (the project's).
            sections: model
                .section_library()
                .map_err(|e| format!("cross-section library: {e}")),
            cases,
            local: self.local,
            // The route worked on only (View › Work on route).
            scope: model.scope().map(|s| s.structures),
            // The structures ticked (2 Scope).
            structures: self.structures.clone(),
        };
        self.units.1 = self.plan_units(model);
        let (tx, rx) = channel();
        let c = ctx.clone();
        self.stamp.started(model.revision());
        std::thread::spawn(move || {
            let started = Instant::now();
            let (r, usage) = compute(&inputs);
            local.finish(LOCAL.1, started.elapsed());
            let _ = tx.send((r, usage, local.notes()));
            c.request_repaint();
        });
        self.running = Some((Instant::now(), rx));
    }

    /// Takes a finished computation.
    pub fn poll(&mut self) {
        let Some((t, rx)) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok((r, usage, notes)) => {
                self.usage = usage;
                self.notes = notes;
                Some(r)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.usage.clear();
                Some(Err("the computation stopped".into()))
            }
        };
        if let Some(r) = r {
            self.took = Some(t.elapsed());
            if let Ok(v) = &r
                && self.case >= v.len()
            {
                self.case = 0;
            }
            self.results = Some(r);
            self.stamp.arrived();
            self.running = None;
            self.generation += 1;
            self.units.0 = self.units.1;
        }
    }

    /// What the results were computed from that changed since, in words
    /// (`None`: nothing, or no results): the line, its cases and files
    /// ([`crate::revision`]).
    pub fn out_of_date(&self, model: &Model) -> Option<String> {
        matches!(self.results, Some(Ok(_)))
            .then(|| self.stamp.out_of_date(model, USES))
            .flatten()
    }

    /// The load case file's case names (read again when the file
    /// changed).
    fn names(&mut self, model: &Model) -> Option<&Result<Vec<String>, String>> {
        let path = model.sources.load_cases.clone()?;
        if self.case_names.as_ref().is_none_or(|(p, _)| *p != path) {
            let names = ohl_loads::LoadCases::load(&path)
                .map(|set| set.cases.into_iter().map(|c| c.name).collect());
            self.case_names = Some((path, names));
        }
        self.case_names.as_ref().map(|(_, n)| n)
    }

    /// The load cases ticked and the structures ticked.
    fn counts(&mut self, model: &Model) -> (usize, usize) {
        let ticked = self.cases_ticked.clone();
        let cases = match self.names(model) {
            Some(Ok(n)) => n
                .iter()
                .filter(|c| crate::tick_list::ticked(&ticked, c))
                .count(),
            _ => 0,
        };
        let structures = model
            .line
            .structures
            .iter()
            .filter(|s| crate::tick_list::ticked(&self.structures, &s.id))
            .count();
        (cases, structures)
    }

    /// How much a run computes: load cases × structures.
    fn plan_units(&mut self, model: &Model) -> usize {
        let (c, s) = self.counts(model);
        c * s
    }

    /// The window: Setup and Results tabs (owner, 2026-10-31, the check
    /// pages' guidelines), Run in the footer at the bottom, always
    /// visible; returns a structure to go to.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut go = None;
        let (cases, structures) = self.counts(model);
        let why = if model.sources.load_cases.is_none() {
            Some(
                "no load case file: add one in the Load cases library (Line › Criteria)".to_owned(),
            )
        } else if cases == 0 {
            Some("tick a load case (2 Scope)".to_owned())
        } else if structures == 0 {
            Some("tick a structure (2 Scope)".to_owned())
        } else {
            None
        };
        let findings = match &self.results {
            Some(Ok(r)) => Some(r.iter().map(|c| c.loads.len()).sum()),
            _ => None,
        };
        let tab = crate::check_ui::tabs(
            ui,
            "loads-page",
            findings,
            self.generation,
            why.is_some() && !self.running(),
        );
        match tab {
            crate::check_ui::Tab::Setup => {
                help::body(ui, "loads-page", |ui| self.setup_ui(ui, model));
            }
            crate::check_ui::Tab::Results => {
                go = crate::check_ui::results_area(ui, "loads-page", |ui| self.body_ui(ui, model));
            }
        }
        let units = cases * structures;
        let estimate = match self.took {
            Some(t) if self.units.0 > 0 && matches!(self.results, Some(Ok(_))) => format!(
                " — {} (from the last run)",
                crate::check_ui::plan_duration(
                    t.as_secs_f64() * units as f64 / self.units.0 as f64
                )
            ),
            _ => " — time measured on the first run".to_owned(),
        };
        let last = self
            .took
            .map(|t| format!(" · last run {:.1} s", t.as_secs_f64()))
            .unwrap_or_default();
        help::footer(ui, "loads-page", |ui| {
            // The shared Run footer (2026-10-27): Run greyed with its
            // reason, Stop while it runs (the previous results stay).
            let f = crate::check_ui::RunFooter {
                running: self
                    .running
                    .as_ref()
                    .map(|(t, _)| crate::check_ui::Progress::elapsed(t.elapsed())),
                why_not: why,
                hover: "Solve every section in every load case ticked and add up the loads at \
                        the structures ticked",
                scope: None,
                info: Some(format!(
                    "{} × {}{estimate}{last}",
                    crate::check_ui::count(cases, "load case"),
                    crate::check_ui::count(structures, "structure")
                )),
            };
            let local = &mut self.local;
            match crate::check_ui::run_footer(ui, f, |ui| {
                crate::local_solve::switch(ui, local, LOCAL.1, LOCAL.0, LOCAL.2, &model.line);
            }) {
                crate::check_ui::Action::Run => self.start(model, ui.ctx()),
                crate::check_ui::Action::Stop => self.running = None,
                crate::check_ui::Action::None => {}
            }
        });
        go
    }

    /// The Setup tab: 1 What it checks (with its drawing), 2 Scope (the
    /// load case file, the load cases and structures ticked).
    fn setup_ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        use crate::check_ui;
        check_ui::section(ui, 1, "What it checks", None);
        help::explain(
            ui,
            "The loads the wires and strings put on each structure, at every point they hang \
             from, in each load case; and how much of its capacity each structure uses.",
            "Every section is solved in the load case's weather case (finite elements); the \
             forces at its clamps are carried up the strings to the structure points, \
             transverse, longitudinal and vertical in the structure's axes, times the load \
             case's factors; the wind on the structure is added. The usage compares them with \
             the type's allowable spans and loads, its structure model and its pole. The model \
             is not changed.",
        );
        check_ui::how_computed(
            ui,
            "loads-page",
            "Per load case: the weather case's wires solved; at each structure point the sum of \
             its strings' forces, split into T (across the line, the bisector at an angle), L \
             (along it) and V (down); factored by the case's V, T and L factors (unfactored \
             shown on request).",
            help::structure_loads,
        );
        ui.separator();
        check_ui::section(ui, 2, "Scope", None);
        match &model.sources.load_cases {
            Some(p) => {
                ui.horizontal(|ui| {
                    ui.label("Load case file");
                    ui.label(
                        egui::RichText::new(
                            p.file_name()
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                        )
                        .strong(),
                    );
                    if ui.button("Load cases library…").clicked() {
                        crate::menu::request_page(
                            ui.ctx(),
                            crate::app::Tab::Library(crate::library::LibraryKind::LoadCases),
                        );
                    }
                });
            }
            None => {
                // Expected in a new project: not an error.
                ui.label("No load case file yet: the structure loads are computed for its cases.");
                ui.label(
                    egui::RichText::new(
                        "Add one in the Load cases library (Line › Criteria): Browse… for a \
                         .ohllc file, or New file… from a template.",
                    )
                    .small()
                    .weak(),
                );
                if ui.button("Open the Load cases library").clicked() {
                    crate::menu::request_page(
                        ui.ctx(),
                        crate::app::Tab::Library(crate::library::LibraryKind::LoadCases),
                    );
                }
                return;
            }
        }
        let running = self.running();
        let names = self.names(model).cloned();
        ui.add_enabled_ui(!running, |ui| {
            ui.columns(2, |cols| {
                cols[0].strong("Load cases");
                match &names {
                    Some(Ok(n)) => {
                        let items: Vec<crate::tick_list::Item> = n
                            .iter()
                            .map(|c| crate::tick_list::Item::new(c, c))
                            .collect();
                        crate::tick_list::tick_list(
                            &mut cols[0],
                            "loads-cases",
                            &items,
                            &mut self.cases_ticked,
                            180.0,
                        );
                    }
                    Some(Err(e)) => {
                        crate::menu::error_label(&mut cols[0], e.as_str());
                    }
                    None => {}
                }
                cols[1].strong("Structures");
                let items: Vec<crate::tick_list::Item> = model
                    .line
                    .structures
                    .iter()
                    .map(|s| {
                        crate::tick_list::Item::new(
                            &s.id,
                            match &s.structure_type {
                                Some(t) => format!("{} — {t}", s.id),
                                None => s.id.clone(),
                            },
                        )
                    })
                    .collect();
                crate::tick_list::tick_list(
                    &mut cols[1],
                    "loads-structures",
                    &items,
                    &mut self.structures,
                    180.0,
                );
            });
        });
        help::explain(
            ui,
            "Only the load cases ticked are solved; the loads are kept at the structures ticked.",
            "Every section is still solved in each case ticked (a structure's loads come from \
             the spans on both sides), so fewer structures do not make a run much faster; fewer \
             load cases do. With View › Work on route, only that route's structures are kept.",
        );
    }

    /// The Results tab.
    pub(crate) fn body_ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut go = None;
        if model.sources.load_cases.is_none() {
            ui.label("No load case file yet (Setup › 2 Scope).");
            return None;
        }
        let results = match &self.results {
            None => {
                ui.label(
                    "The loads the wires and strings put on the structures, at every \
                     attachment, in each load case. Run to see them.",
                );
                return None;
            }
            Some(Err(e)) => {
                crate::menu::error_label(ui, e);
                return None;
            }
            Some(Ok(r)) => r,
        };
        if let Some(what) = self.out_of_date(model)
            && crate::revision::banner(ui, &what, !self.running())
        {
            self.start(model, ui.ctx());
            return None;
        }
        if results.is_empty() {
            ui.label("The load case file has no case.");
            return None;
        }
        for n in &self.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.show_usage, false, "Loads");
            ui.selectable_value(&mut self.show_usage, true, "Usage")
                .on_hover_text(
                    "How much of its capacity each structure uses: its type's allowable spans \
                     (Method 1), allowable loads (Method 2), structure model (fe) and pole",
                );
        });
        if self.show_usage {
            let names: Vec<String> = results.iter().map(|r| r.case.name.clone()).collect();
            // A case with sections not solved misses their loads: said so.
            let incomplete: Vec<String> = results
                .iter()
                .filter(|r| !r.failed.is_empty())
                .map(|r| {
                    let mut at: Vec<&str> =
                        r.missing.iter().map(|m| m.structure.as_str()).collect();
                    at.dedup();
                    format!(
                        "{} ({} sections or jumpers; incomplete at {} structures)",
                        r.case.name,
                        r.failed.len(),
                        at.len()
                    )
                })
                .collect();
            if !incomplete.is_empty() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "not solved in: {} — the structures they hang on are not checked there \
                         (grey); see Loads",
                        incomplete.join(", ")
                    ),
                );
            }
            return self.usage_ui(ui, &names);
        }
        ui.horizontal(|ui| {
            let names: Vec<&str> = results.iter().map(|r| r.case.name.as_str()).collect();
            // Labels before their controls (the desktop app's rule).
            ui.label("Load case");
            egui::ComboBox::from_id_salt("structure-loads-case")
                .selected_text(names[self.case.min(names.len() - 1)])
                .width(220.0)
                .show_ui(ui, |ui| {
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.case, k, *n);
                    }
                });
            ui.label("Structure");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(100.0));
            ui.checkbox(&mut self.unfactored, "unfactored");
            // The table's Export CSV… writes the case shown; this one
            // every case (named apart, check pages' guidelines §4).
            if ui
                .button("Export every case (CSV)…")
                .on_hover_text(
                    "Every load case, every attachment, as one CSV file (the table's Export \
                     CSV… writes the case shown)",
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("structure-loads.csv")
                    .save_file()
                && let Err(e) = std::fs::write(&path, ohl_loads::to_csv(results))
            {
                log::warn!("{}: {e}", path.display());
            }
        });
        let r = &results[self.case.min(results.len() - 1)];
        let c = &r.case;
        ui.label(
            egui::RichText::new(format!(
                "{} — weather {}, wires {}, factors V {} T {} L {}, strength {}{}",
                if c.description.is_empty() {
                    &c.name
                } else {
                    &c.description
                },
                c.weather,
                c.condition.as_str(),
                c.vertical,
                c.transverse,
                c.longitudinal,
                c.strength,
                if c.tags.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", c.tags.join(", "))
                }
            ))
            .small(),
        );
        // Per-pylon sets: only the structures whose set has this case.
        if model.line.has_load_case_sets() {
            let (with, without): (Vec<&ohl_line::Structure>, Vec<&ohl_line::Structure>) = model
                .line
                .structures
                .iter()
                .partition(|s| model.line.takes_load_case(&s.id, &c.name));
            ui.label(
                egui::RichText::new(format!(
                    "taken by {} of {} structures (their load case sets); not by: {}",
                    with.len(),
                    model.line.structures.len(),
                    if without.is_empty() {
                        "—".to_owned()
                    } else {
                        without
                            .iter()
                            .map(|s| s.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ))
                .small()
                .weak(),
            )
            .on_hover_text(
                "Each structure takes the load cases of its set (Structures table › load \
                 cases, or its type's in Line › Criteria › Load cases per structure type; \
                 none: every case). A structure not taking this case has no loads in it.",
            );
        }
        // What the case assumed (wind side, local conditions, removed and
        // broken wires, ice and snow on the structures), in a box of its
        // own height.
        if !r.notes.is_empty() {
            help::list(
                ui,
                "loads-case-notes",
                90.0,
                egui::ScrollArea::vertical()
                    .id_salt("loads-case-notes")
                    .auto_shrink([false, true]),
                |ui| {
                    for n in &r.notes {
                        ui.label(egui::RichText::new(format!("• {n}")).small());
                    }
                },
            );
        }
        if !r.failed.is_empty() {
            // In a box of its own height, scrolled: a long list stays small.
            help::list(
                ui,
                "loads-list-1",
                130.0,
                egui::ScrollArea::vertical()
                    .id_salt("loads-not-solved")
                    .auto_shrink([false, true]),
                |ui| {
                    for f in &r.failed {
                        crate::menu::error_label(ui, format!("not solved: {f}"));
                    }
                    // The structures whose loads lack them.
                    let mut by: std::collections::BTreeMap<&str, Vec<String>> =
                        std::collections::BTreeMap::new();
                    for m in &r.missing {
                        by.entry(m.structure.as_str()).or_default().push(m.short());
                    }
                    for (s, wires) in by {
                        ui.colored_label(
                            INCOMPLETE,
                            format!(
                                "{s}: incomplete — not checked: {} in {}",
                                wires.join(", "),
                                r.case.name
                            ),
                        );
                    }
                },
            );
        }
        // The shared results table (owner, 2026-10-28): sortable, units in
        // the headers, searched, grouped by structure, CSV; a row clicked
        // goes to its structure; incomplete loads in the warning colour.
        // Built once per answer, load case shown, structure filter and
        // factored or not; not every frame.
        let filter = self.filter.trim().to_lowercase();
        use crate::check_ui::Col;
        let (case, unfactored) = (self.case.min(results.len() - 1), self.unfactored);
        let key = (self.generation, case, unfactored, filter.clone());
        let rows = self.rows.get_or_build(key, || {
            loads_shown(r, &filter)
                .map(|l| load_row(r, l, unfactored))
                .collect()
        });
        if let Some(k) = crate::check_ui::ResultTable::new(
            "structure-loads-rows",
            vec![
                Col::text("Structure"),
                Col::text("Point"),
                Col::text("Sets"),
                Col::num("Transverse [kN]"),
                Col::num("Longitudinal [kN]"),
                Col::num("Vertical [kN]"),
                Col::text("Sections"),
            ],
        )
        .rows_kept(rows)
        .no_filter()
        .group_by(&[0])
        .csv("structure-loads-table.csv")
        .ui(ui)
        {
            go = loads_shown(r, &filter).nth(k).map(|l| l.structure.clone());
        }
        go
    }

    /// The usage view; returns a structure to go to.
    fn usage_ui(&mut self, ui: &mut egui::Ui, names: &[String]) -> Option<String> {
        let mut go = None;
        // Load case: 0 = the governing one of each structure, k + 1 = case k.
        ui.horizontal_wrapped(|ui| {
            let shown = match self.usage_case {
                0 => "governing (every case)".to_owned(),
                k => names.get(k - 1).cloned().unwrap_or_default(),
            };
            ui.label("Load case");
            egui::ComboBox::from_id_salt("structure-usage-case")
                .selected_text(shown)
                .width(220.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.usage_case, 0, "governing (every case)");
                    for (k, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.usage_case, k + 1, n);
                    }
                });
            ui.label("Structure");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(100.0));
            if ui
                .button("Export CSV…")
                .on_hover_text("Every load case, every structure")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("structure-usage.csv")
                    .save_file()
                && let Err(e) = std::fs::write(&path, ohl_loads::capacity::usage_csv(&self.usage))
            {
                log::warn!("{}: {e}", path.display());
            }
            ui.checkbox(&mut self.colour_structures, "Colour structures by usage")
                .on_hover_text(
                    "The structures coloured in the plan and 3D views by their usage in the \
                     load case chosen here (a legend in the views); those not checked because a \
                     wire at them did not solve in a neutral colour",
                );
        });
        let rows: Vec<(&str, &StructureUsage)> = match self.usage_case {
            0 => ohl_loads::capacity::governing(&self.usage),
            k => self.usage.get(k - 1).map_or_else(Vec::new, |c| {
                let mut v: Vec<(&str, &StructureUsage)> =
                    c.structures.iter().map(|s| (c.case.as_str(), s)).collect();
                v.sort_by(|a, b| {
                    let u = |s: &StructureUsage| s.governing().map_or(-1.0, |g| g.usage);
                    u(b.1).total_cmp(&u(a.1))
                });
                v
            }),
        };
        // Incomplete: in the case shown, or (governing) in any case — with
        // the cases and the wires not solved.
        let not_checked = |s: &StructureUsage| -> Option<String> {
            match self.usage_case {
                0 => {
                    let cases = ohl_loads::capacity::incomplete_cases(&self.usage, &s.structure);
                    (!cases.is_empty()).then(|| {
                        let mut wires: Vec<String> = Vec::new();
                        for (_, u) in &cases {
                            for m in &u.missing {
                                let w = m.short();
                                if !wires.contains(&w) {
                                    wires.push(w);
                                }
                            }
                        }
                        format!(
                            "not checked: {} in {}",
                            wires.join(", "),
                            cases.iter().map(|c| c.0).collect::<Vec<_>>().join(", ")
                        )
                    })
                }
                k => s.is_incomplete().then(|| {
                    format!(
                        "{} in {}",
                        s.not_checked_text(),
                        names.get(k - 1).map_or("", String::as_str)
                    )
                }),
            }
        };
        let checked = rows
            .iter()
            .filter(|r| r.1.governing().is_some() && not_checked(r.1).is_none())
            .count();
        let over = rows
            .iter()
            .filter(|r| {
                r.1.governing()
                    .is_some_and(|g| classify_usage(g.usage) == UsageClass::Over)
            })
            .count();
        let incomplete: Vec<&StructureUsage> = match self.usage_case {
            0 => {
                let mut seen = std::collections::BTreeSet::new();
                self.usage
                    .iter()
                    .flat_map(|c| c.structures.iter())
                    .filter(|s| s.is_incomplete() && seen.insert(s.structure.clone()))
                    .collect()
            }
            k => self.usage.get(k - 1).map_or_else(Vec::new, |c| {
                c.structures.iter().filter(|s| s.is_incomplete()).collect()
            }),
        };
        ui.label(
            egui::RichText::new(format!(
                "{checked} structures checked, {over} over 100 %, {} not checked (a wire not \
                 solved){}",
                incomplete.len(),
                if checked == 0 {
                    " — give the structure types a [capacity] (allowable spans, \
                     allowable loads) to check them"
                } else {
                    ""
                }
            ))
            .small(),
        );
        // Not checked: at the end, with why.
        let unchecked: Vec<&StructureUsage> = match self.usage_case {
            0 => self
                .usage
                .first()
                .map(|c| {
                    c.structures
                        .iter()
                        .filter(|s| s.governing().is_none())
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let filter = self.filter.trim().to_lowercase();
        let (error, ok) = (ui.visuals().error_fg_color, crate::menu::ok_color(ui));
        let pct = |c: Option<&ohl_loads::capacity::Check>| match c {
            None => egui::RichText::new("—").weak(),
            Some(c) => {
                let text = if c.usage.is_finite() {
                    format!("{:.0} %", c.usage * 100.0)
                } else {
                    "not allowed".into()
                };
                let colour = match classify_usage(c.usage) {
                    UsageClass::Over => error,
                    _ if c.not_safe > 0 => {
                        let [r, g, b] = UsageMark::NotSafe.color();
                        egui::Color32::from_rgb(r, g, b)
                    }
                    UsageClass::Near => egui::Color32::from_rgb(255, 170, 60),
                    UsageClass::Pass | UsageClass::NotSafe => ok,
                };
                egui::RichText::new(text).monospace().color(colour)
            }
        };
        // Both ways: its many columns scroll sideways in the window's
        // 860 px rather than widening it.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("structure-usage")
                    .striped(true)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        for h in [
                            "structure",
                            "type",
                            "usage",
                            "spans",
                            "loads",
                            "fe",
                            "pole",
                            "governs",
                            "load case",
                            "angle °",
                            "wind span m",
                            "weight span m",
                        ] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        let listed: std::collections::BTreeSet<&str> = rows
                            .iter()
                            .filter(|r| r.1.governing().is_some())
                            .map(|r| r.1.structure.as_str())
                            .chain(unchecked.iter().map(|s| s.structure.as_str()))
                            .collect();
                        // Incomplete without any check: listed too.
                        let extra: Vec<(&str, &StructureUsage)> = incomplete
                            .iter()
                            .filter(|s| !listed.contains(s.structure.as_str()))
                            .map(|s| ("", *s))
                            .collect();
                        let shown = rows
                            .iter()
                            .filter(|r| r.1.governing().is_some())
                            .chain(
                                unchecked
                                    .iter()
                                    .map(|s| ("", *s))
                                    .collect::<Vec<_>>()
                                    .iter(),
                            )
                            .chain(extra.iter())
                            .filter(|r| {
                                filter.is_empty() || r.1.structure.to_lowercase().contains(&filter)
                            })
                            .take(5000)
                            .cloned()
                            .collect::<Vec<_>>();
                        for (case, s) in shown {
                            let not = not_checked(s);
                            // Incomplete: grey, "not checked", the usage
                            // of the loads there are only on hover.
                            let grey = |t: egui::RichText| {
                                if not.is_some() {
                                    t.color(INCOMPLETE).italics()
                                } else {
                                    t
                                }
                            };
                            if ui
                                .link(grey(egui::RichText::new(&s.structure)))
                                .on_hover_text("Go to this structure")
                                .clicked()
                            {
                                go = Some(s.structure.clone());
                            }
                            ui.label(grey(egui::RichText::new(
                                s.structure_type.as_deref().unwrap_or("—"),
                            )));
                            let g = s.governing();
                            if not.is_some() {
                                let r = ui.label(
                                    egui::RichText::new("not checked")
                                        .color(INCOMPLETE)
                                        .italics(),
                                );
                                if let Some(g) = g {
                                    r.on_hover_text(format!(
                                        "{:.0} % from the loads there are (a wire is missing)",
                                        g.usage * 100.0
                                    ));
                                }
                                for c in [&s.spans, &s.loads, &s.fe, &s.pole] {
                                    ui.label(
                                        egui::RichText::new(if c.is_some() { "?" } else { "—" })
                                            .color(INCOMPLETE),
                                    );
                                }
                            } else {
                                ui.label(pct(g));
                                ui.label(pct(s.spans.as_ref()));
                                ui.label(pct(s.loads.as_ref()));
                                ui.label(pct(s.fe.as_ref()));
                                ui.label(pct(s.pole.as_ref()));
                            }
                            match (&not, g) {
                                (Some(text), _) => ui.colored_label(INCOMPLETE, text),
                                (None, Some(g)) => ui.label(&g.governs),
                                (None, None) => {
                                    ui.label(egui::RichText::new(s.notes.join("; ")).weak())
                                }
                            };
                            ui.label(case);
                            ui.label(format!("{:.1}", s.line_angle_deg));
                            ui.label(format!("{:.0}", s.wind_span));
                            ui.label(s.weight_span.map_or(String::new(), |[lo, hi]| {
                                if (hi - lo).abs() < 0.5 {
                                    format!("{hi:.0}")
                                } else {
                                    format!("{lo:.0} – {hi:.0}")
                                }
                            }));
                            ui.end_row();
                        }
                    });
            });
        go
    }
}
