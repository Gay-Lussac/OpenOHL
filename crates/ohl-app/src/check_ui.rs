// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The parts every check window is built of (owner, 2026-10-27: "Two
//! tabs, yes"; one layout for all of them), on top of
//! [`crate::check_help`]'s window, body and footer:
//!
//! - [`tabs`]: **Setup** and **Results (n)**. The setup scrolls; the
//!   results fill their tab ([`results_area`]: the table's height follows
//!   the window, no scroll inside a scroll, no handle). Results opens by
//!   itself when a run ends; Setup again when Run greys with a "Before
//!   Run" reason.
//! - The setup in a fixed order: [`section`] *1 What it checks* (one line,
//!   [`how_computed`] folded), *2 Scope* ([`ScopePicker`]), *3 Rules*,
//!   *4 Model* ([`model_section`], folded).
//! - [`ResultTable`]: the verdict banner ([`verdict`]), a toolbar (*Only
//!   violations*, on by default; *Search*; *Group by*; *Export CSV…*),
//!   columns sorted by a click on their header, units in brackets in the
//!   headers, a row clicked returned to the window (it selects the object
//!   and draws the finding's weather case, [`crate::result_case`]).
//! - [`run_footer`]: Run, Stop (every check), the progress and the time
//!   left, "Before Run: …" when Run is grey, the scope in one line, then
//!   what the window adds (overrides, *Save in Performed checks*).
//! - [`RuleTable`] (*3 Rules*): one compact row per rule, the selected
//!   rule's editor below the table, one *Preset ▾* with Merge / Replace,
//!   Import / Export ([`crate::rule_table`]).
//! - [`Kept`]: a window's setup kept in the project, a project setting
//!   per project line (`app.<check>.<line>`), as the magnetic field's.

use std::collections::BTreeSet;
use std::time::Duration;

use eframe::egui;

use crate::check_help as help;
use crate::model::Model;
use crate::tick_list::{self, Item};

pub use crate::rule_table::{PresetPick, RuleRow, RuleTable, Summary};

// ---------------------------------------------------------------- tabs

/// A check window's tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// What is checked and how: scrolls.
    #[default]
    Setup,
    /// The last run's results: the table fills the tab.
    Results,
}

/// What [`tabs`] keeps from frame to frame.
#[derive(Debug, Clone, Copy, Default)]
struct TabState {
    tab: Tab,
    /// The results seen (a new one opens Results).
    generation: u64,
    /// Run was grey with a reason.
    blocked: bool,
}

/// The memory key of a window's tab.
fn tab_key(id: &str) -> egui::Id {
    egui::Id::new(("check-tab", id))
}

/// The tab window `id` shows (tests).
#[cfg(test)]
pub fn current_tab(ctx: &egui::Context, id: &str) -> Tab {
    ctx.data(|d| d.get_temp::<TabState>(tab_key(id)))
        .map(|s| s.tab)
        .unwrap_or_default()
}

/// The **Setup** and **Results (n)** tabs of check window `id`, drawn at
/// its top; the tab shown. `findings`: the count said on Results (none:
/// no run yet). `generation`: counts the results arrived — a new one
/// opens Results by itself. `blocked`: Run is grey with a "Before Run"
/// reason — when it becomes so, Setup opens (where the reason is fixed).
pub fn tabs(
    ui: &mut egui::Ui,
    id: &str,
    findings: Option<usize>,
    generation: u64,
    blocked: bool,
) -> Tab {
    let key = tab_key(id);
    let mut st = ui
        .data(|d| d.get_temp::<TabState>(key))
        .unwrap_or(TabState {
            tab: if generation > 0 {
                Tab::Results
            } else {
                Tab::Setup
            },
            generation,
            blocked,
        });
    if generation != st.generation {
        st.generation = generation;
        if generation > 0 {
            st.tab = Tab::Results;
        }
    }
    if blocked && !st.blocked {
        st.tab = Tab::Setup;
    }
    st.blocked = blocked;
    ui.horizontal(|ui| {
        if ui
            .selectable_label(st.tab == Tab::Setup, "Setup")
            .on_hover_text("What is checked and how: scope, rules, settings")
            .clicked()
        {
            st.tab = Tab::Setup;
        }
        let results = match findings {
            Some(n) => format!("Results ({n})"),
            None => "Results".to_owned(),
        };
        if ui
            .selectable_label(st.tab == Tab::Results, results)
            .on_hover_text("The last run's results (opens by itself after a run)")
            .clicked()
        {
            st.tab = Tab::Results;
        }
    });
    ui.data_mut(|d| d.insert_temp(key, st));
    st.tab
}

/// The Results tab's area of check window `id` (the same id as its
/// [`help::body`] and [`help::footer`]): the room down to the footer,
/// not scrolled — a [`ResultTable`] in it fills what is left, so its
/// height follows the window's.
pub fn results_area<R>(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.spacing_mut().item_spacing = help::ITEM_SPACING;
    let height = help::room(ui, id);
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_size(egui::vec2(width, height));
            ui.set_max_height(height);
            add(ui)
        },
    )
    .inner
}

// ------------------------------------------------------------ the setup

/// A numbered part of the setup ("1 What it checks", "2 Scope",
/// "3 Rules"; *Model* is [`model_section`]): its heading, with the user guide's button
/// when `anchor` is given; the anchor when that is clicked.
pub fn section(
    ui: &mut egui::Ui,
    number: usize,
    title: &str,
    anchor: Option<&'static str>,
) -> Option<&'static str> {
    ui.add_space(4.0);
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{number}")).weak().strong());
        ui.strong(title);
        if let Some(a) = anchor
            && ui
                .small_button("user guide ↗")
                .on_hover_text(format!("Help › User guide: {title} (and its theory)"))
                .clicked()
        {
            out = Some(a);
        }
    });
    out
}

/// *How it is computed*, folded under *What it checks*: `text` (the
/// theory in short) and `draw` (the window's drawings, painted in the
/// theme's colours as [`help::falling_tree`]).
pub fn how_computed(ui: &mut egui::Ui, id: &str, text: &str, draw: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new("How it is computed")
        .id_salt((id, "how-computed"))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).small());
            draw(ui);
            ui.label(
                egui::RichText::new(
                    "Drawings of every measure and rule: Help › User guide › Check windows.",
                )
                .small()
                .weak(),
            );
        });
}

/// The numbered *Model* part of the setup (check pages' guidelines §1:
/// the modelling settings, **folded by default** — the defaults are
/// sound; a user opens it to change them): its heading is the fold's.
/// `add` draws the settings when it is open.
pub fn model_section<R>(
    ui: &mut egui::Ui,
    id: &str,
    number: usize,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    ui.add_space(4.0);
    egui::CollapsingHeader::new(egui::RichText::new(format!("{number}  Model")).strong())
        .id_salt((id, "model-section"))
        .default_open(false)
        .show(ui, add)
        .body_returned
}

/// Why a check of the LiDAR points cannot run without them, and where
/// they are brought in (Vegetation, As-built wires).
pub const NO_CLOUD: &str =
    "no point cloud in this model: import one with File › Import › LiDAR points (LAS/LAZ)";

// ---------------------------------------------------------------- scope

/// What a check covers besides its rules (owner, 2026-10-27: Vegetation
/// needs a circuit choice — "some of them we will model but we don't need
/// them checked"): the circuits, the routes and alignments, the sections.
/// Kept with the window's setup.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Scope {
    /// The circuits checked, by name (`""`: sections without one);
    /// `None`: every circuit (also those added later).
    pub circuits: Option<Vec<String>>,
    /// The routes checked (with [`Scope::alignments`] empty too: the
    /// whole line).
    pub routes: Vec<String>,
    /// The alignments checked besides whole routes.
    pub alignments: Vec<String>,
    /// The sections checked, by id; `None`: every section.
    pub sections: Option<Vec<String>>,
}

impl Scope {
    /// The sections of `line` it takes: of a circuit ticked, on a route
    /// or alignment ticked, ticked themselves.
    pub fn sections_of(&self, line: &ohl_line::Line) -> BTreeSet<String> {
        let on = (!(self.routes.is_empty() && self.alignments.is_empty()))
            .then(|| line.wires_on(&self.routes, &self.alignments).0);
        line.sections
            .iter()
            .filter(|s| tick_list::ticked(&self.circuits, &s.circuit_name))
            .filter(|s| tick_list::ticked(&self.sections, &s.id))
            .filter(|s| on.as_ref().is_none_or(|on| on.contains(&s.id)))
            .map(|s| s.id.clone())
            .collect()
    }

    /// Whether it takes every section of the line.
    pub fn whole(&self) -> bool {
        self.circuits.is_none()
            && self.sections.is_none()
            && self.routes.is_empty()
            && self.alignments.is_empty()
    }

    /// Why a check cannot run on it, if it cannot: nothing ticked.
    pub fn why_not(&self, line: &ohl_line::Line) -> Option<String> {
        (!line.sections.is_empty() && self.sections_of(line).is_empty())
            .then(|| "the scope takes no section: tick a circuit (Scope)".to_owned())
    }

    /// From a clearance rule file's `[check]` (an empty list: every one).
    pub fn of_check(check: &ohl_clearance::rules::CheckScope) -> Self {
        Self {
            circuits: (!check.circuits.is_empty()).then(|| check.circuits.clone()),
            routes: check.routes.clone(),
            alignments: check.alignments.clone(),
            sections: (!check.sections.is_empty()).then(|| check.sections.clone()),
        }
    }

    /// The sections of `line` it keeps; `None`: every one (the whole
    /// line).
    pub fn keeps(&self, line: &ohl_line::Line) -> Option<BTreeSet<String>> {
        (!self.whole()).then(|| self.sections_of(line))
    }

    /// The scope picker `id` last showed for `check`, while `check` is
    /// as it left it (a list of no circuit, which `[check]` cannot hold,
    /// is kept so): none when `check` changed since (another rule file).
    pub fn remembered(
        ctx: &egui::Context,
        id: &str,
        check: &ohl_clearance::rules::CheckScope,
    ) -> Option<Self> {
        let key = egui::Id::new(("check-scope", id));
        ctx.data(|d| d.get_temp::<(Scope, Scope)>(key))
            .filter(|(from, _)| *from == Self::of_check(check))
            .map(|(_, s)| s)
    }

    /// Remembers what scope picker `id` shows for `check`
    /// ([`Scope::remembered`]).
    pub fn remember(
        ctx: &egui::Context,
        id: &str,
        check: &ohl_clearance::rules::CheckScope,
        scope: Scope,
    ) {
        let key = egui::Id::new(("check-scope", id));
        ctx.data_mut(|d| d.insert_temp(key, (Self::of_check(check), scope)));
    }

    /// Into a clearance rule file's `[check]` (the rest of it kept). A
    /// list of no circuit cannot be written there (empty is every one):
    /// kept as it was, Run greyed until one is ticked.
    pub fn write_check(&self, check: &mut ohl_clearance::rules::CheckScope) {
        match &self.circuits {
            None => check.circuits.clear(),
            Some(c) if !c.is_empty() => {
                check.circuits = c.clone();
                check.circuits.sort();
            }
            Some(_) => {}
        }
        check.routes = self.routes.clone();
        check.alignments = self.alignments.clone();
        match &self.sections {
            None => check.sections.clear(),
            Some(c) if !c.is_empty() => {
                check.sections = c.clone();
                check.sections.sort();
            }
            Some(_) => {}
        }
    }
}

/// The `[check]` of a check's rule file holding its scope and its **Use
/// local conditions** (none when it changes nothing).
pub fn check_of(scope: &Scope, local: bool) -> Option<ohl_clearance::rules::CheckScope> {
    let mut check = ohl_clearance::rules::CheckScope {
        local,
        ..Default::default()
    };
    scope.write_check(&mut check);
    Some(check).filter(|c| !c.is_empty())
}

/// A circuit's name as shown (`""`: the sections without one).
pub fn circuit_label(name: &str) -> String {
    if name.is_empty() {
        "(no circuit name)".to_owned()
    } else {
        name.to_owned()
    }
}

/// The line's circuits, by name, sorted.
pub fn circuits_of(line: &ohl_line::Line) -> Vec<String> {
    let mut names: Vec<String> = line
        .sections
        .iter()
        .map(|s| s.circuit_name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// What the checks' rule editors offer of the line (owner, 2026-11-10:
/// five windows made these lists from the line again every frame, each
/// with its own copy of the code): made once per version of what they
/// read — the line ([`crate::model::Model::line_version`]), the route
/// worked on, the weather cases' names — and kept by the window. The
/// case groups are the weather case file's as read last, shared
/// ([`crate::case_groups::groups_shared`]).
#[derive(Debug, Clone, Default)]
pub struct LineChoices {
    /// What the lists were made from: the line's version and the route
    /// worked on (none: never made). The cases' names are compared
    /// themselves.
    made_for: Option<(u64, Option<String>)>,
    /// The weather cases' names, in the model's order.
    pub cases: Vec<String>,
    /// The weather case groups.
    pub groups: std::sync::Arc<Vec<ohl_weather::CaseGroup>>,
    /// The nominal voltages of the line's circuits that have one, kV,
    /// ascending, each once.
    pub voltages: Vec<f64>,
    /// As `voltages`, with 0 for what has none (earthed wires).
    pub voltages_or_zero: Vec<f64>,
    /// The wire types of the line's sections, sorted, each once.
    pub types: Vec<String>,
    /// The line's systems, then "none".
    pub systems: Vec<String>,
    /// The line's circuits.
    pub circuits: Vec<String>,
    /// The wire types of the sections on the route worked on (all
    /// without one), sorted, each once.
    pub route_types: Vec<String>,
    /// The systems with a section on the route worked on, sorted.
    pub route_systems: Vec<String>,
    /// The circuits with a section on the route worked on.
    pub route_circuits: Vec<String>,
}

impl LineChoices {
    /// The lists for `model` now: made again when its line, the route
    /// worked on or the cases' names changed; the groups asked of the
    /// weather case file's cache every time.
    pub fn of(&mut self, model: &Model) -> &Self {
        let same = self
            .made_for
            .as_ref()
            .is_some_and(|(v, r)| *v == model.line_version && *r == model.work_route)
            && self
                .cases
                .iter()
                .eq(model.cases.iter().map(|c| &c.case.name));
        if !same {
            *self = Self::made(model);
            #[cfg(test)]
            CHOICES_MADE.with(|n| n.set(n.get() + 1));
        }
        self.groups = crate::case_groups::groups_shared(model);
        self
    }

    /// The lists of `model` (the groups left empty).
    fn made(model: &Model) -> Self {
        let line = &model.line;
        let kv = |zero: bool| {
            let mut v: Vec<f64> = ohl_clearance::case::voltages_of(line)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| match p {
                    Some(p) => Some(p.nominal_kv),
                    None => zero.then_some(0.0),
                })
                .collect();
            v.sort_by(f64::total_cmp);
            v.dedup();
            v
        };
        let sorted_once = |mut v: Vec<String>| {
            v.sort();
            v.dedup();
            v
        };
        // With a route worked on: what has a section on it (2026-11-01).
        let on = model.scope();
        let sections: Vec<&ohl_line::Section> = line
            .sections
            .iter()
            .filter(|s| on.as_ref().is_none_or(|o| o.sections.contains(&s.id)))
            .collect();
        let mut systems: Vec<String> = line.systems.iter().map(|s| s.id.clone()).collect();
        systems.push("none".into());
        let mut route_systems: Vec<String> = line
            .systems
            .iter()
            .filter(|y| {
                sections
                    .iter()
                    .any(|s| s.system.as_deref() == Some(y.id.as_str()))
            })
            .map(|s| s.id.clone())
            .collect();
        route_systems.sort();
        Self {
            made_for: Some((model.line_version, model.work_route.clone())),
            cases: model.cases.iter().map(|c| c.case.name.clone()).collect(),
            groups: Default::default(),
            voltages: kv(false),
            voltages_or_zero: kv(true),
            types: sorted_once(line.sections.iter().map(|s| s.cable.clone()).collect()),
            systems,
            circuits: line.circuits.iter().map(|c| c.name.clone()).collect(),
            route_types: sorted_once(sections.iter().map(|s| s.cable.clone()).collect()),
            route_systems,
            route_circuits: line
                .circuits
                .iter()
                .filter(|c| sections.iter().any(|s| s.circuit_name == c.name))
                .map(|c| c.name.clone())
                .collect(),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Counts the times a [`LineChoices`] made its lists (tests).
    static CHOICES_MADE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The weather cases of a check's scope.
pub enum Cases<'a> {
    /// Several, ticked (indices in `names`).
    Many(&'a [String], &'a mut Vec<usize>),
    /// One (an index in `names`).
    One(&'a [String], &'a mut usize),
}

/// The scope of a check (owner, 2026-10-27: the same widget in every
/// window): circuits, routes and alignments, sections (optional), weather
/// cases, each a menu that stays open on a tick (the owner's standing
/// rule) over a [`tick_list`] (search, All / None / Invert, the count),
/// and the scope said in one line.
pub struct ScopePicker<'a> {
    id: &'a str,
    line: &'a ohl_line::Line,
    scope: Option<&'a mut Scope>,
    sections: bool,
    cases: Option<Cases<'a>>,
    on: Option<RouteOn>,
}

/// The route worked on as the scope menus offer it (owner, 2026-11-01:
/// "filter … the available circuits and systems … to the one being
/// worked on"): the circuits with a section on it, the route itself, its
/// sections. What is ticked outside it is kept, not run (the checks keep
/// to the route).
#[derive(Debug, Clone, Default)]
pub struct RouteOn {
    /// The route's id.
    pub route: String,
    /// The circuits (names) with a section on it.
    pub circuits: BTreeSet<String>,
    /// Its sections.
    pub sections: BTreeSet<String>,
}

impl RouteOn {
    /// The route `model` works on (none: all routes).
    pub fn of(model: &crate::model::Model) -> Option<Self> {
        let scope = model.scope()?;
        Some(Self {
            route: model.work_route.clone()?,
            circuits: model
                .line
                .sections
                .iter()
                .filter(|s| scope.sections.contains(&s.id))
                .map(|s| s.circuit_name.clone())
                .collect(),
            sections: scope.sections,
        })
    }
}

/// `chosen` after a tick list over `shown` of `all` changed it: what was
/// ticked among the items not shown kept ticked.
pub(crate) fn keep_hidden(
    before: &Option<Vec<String>>,
    chosen: &mut Option<Vec<String>>,
    all: &[String],
    shown: &[String],
) {
    if let Some(list) = chosen {
        for x in all.iter().filter(|x| !shown.contains(x)) {
            if before.as_ref().is_none_or(|b| b.contains(x)) && !list.contains(x) {
                list.push(x.clone());
            }
        }
    }
}

/// The height of a scope menu's list, px.
const SCOPE_LIST: f32 = 260.0;

impl<'a> ScopePicker<'a> {
    /// The scope picker of window `id` on `line`: nothing in it yet.
    pub fn new(id: &'a str, line: &'a ohl_line::Line) -> Self {
        Self {
            id,
            line,
            scope: None,
            sections: false,
            cases: None,
            on: None,
        }
    }

    /// Its lists kept to the route `model` works on ([`RouteOn`]).
    pub fn route_of(mut self, model: &crate::model::Model) -> Self {
        self.on = RouteOn::of(model);
        self
    }

    /// The circuits and the routes (and with `sections`, the sections).
    pub fn wires(mut self, scope: &'a mut Scope, sections: bool) -> Self {
        self.scope = Some(scope);
        self.sections = sections;
        self
    }

    /// The weather cases.
    pub fn cases(mut self, cases: Cases<'a>) -> Self {
        self.cases = Some(cases);
        self
    }

    /// The pickers in a row, the summary under them. True when something
    /// changed.
    pub fn ui(self, ui: &mut egui::Ui) -> bool {
        let Self {
            id,
            line,
            scope,
            sections,
            cases,
            on,
        } = self;
        let mut changed = false;
        let mut summary: Vec<String> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            if let Some(scope) = scope {
                changed |= circuits_menu(ui, id, line, scope, on.as_ref());
                changed |= routes_menu(ui, id, line, scope, on.as_ref());
                if sections {
                    changed |= sections_menu(ui, id, line, scope, on.as_ref());
                }
                summary.push(scope_words(line, scope, sections));
            }
            if let Some(cases) = cases {
                let (c, words) = cases_menu(ui, id, cases);
                changed |= c;
                summary.push(words);
            }
        });
        if let Some(on) = &on {
            summary.push(format!(
                "kept to route {} (View › Work on route; what is ticked outside it is kept, not run)",
                on.route
            ));
        }
        if !summary.is_empty() {
            ui.label(egui::RichText::new(summary.join(", ")).small().weak());
        }
        changed
    }
}

/// The scope in words: "2 of 3 circuits, the whole line, 4 of 12
/// sections".
pub fn scope_words(line: &ohl_line::Line, scope: &Scope, sections: bool) -> String {
    let all = circuits_of(line);
    let mut out = match &scope.circuits {
        None => "every circuit".to_owned(),
        Some(c) => {
            let n = all.iter().filter(|x| c.contains(x)).count();
            format!(
                "{n} of {} circuit{}",
                all.len(),
                if all.len() == 1 { "" } else { "s" }
            )
        }
    };
    out.push_str(&match (scope.routes.len(), scope.alignments.len()) {
        (0, 0) => ", the whole line".to_owned(),
        (r, a) => format!(", {r} route(s), {a} alignment(s)"),
    });
    if sections || scope.sections.is_some() {
        let n = scope.sections_of(line).len();
        out.push_str(&format!(", {n} of {} sections", line.sections.len()));
    }
    out
}

/// A tick per item of `all`, `chosen` the ticked; whether one changed (the
/// menu stays open).
pub(crate) fn ticks(ui: &mut egui::Ui, all: &[String], chosen: &mut Vec<String>) -> bool {
    let mut changed = false;
    for item in all {
        let mut on = chosen.contains(item);
        if ui.checkbox(&mut on, item).changed() {
            if on {
                chosen.push(item.clone());
            } else {
                chosen.retain(|x| x != item);
            }
            changed = true;
        }
    }
    for gone in chosen.clone().iter().filter(|x| !all.contains(x)) {
        let mut on = true;
        if ui
            .checkbox(&mut on, format!("{gone} (not in the model)"))
            .changed()
        {
            chosen.retain(|x| x != gone);
            changed = true;
        }
    }
    changed
}

/// The file of a window's rules in a project: in its clearance rules (its
/// working copy), else next to the model, named `{line}.{name}`.
pub(crate) fn project_rule_file(model: &Model, name: &str) -> std::path::PathBuf {
    let model_file = &model.sources.file;
    let line = model_file
        .file_name()
        .map_or(String::from("line"), |n| n.to_string_lossy().into_owned());
    let stem = line.split('.').next().unwrap_or("line").to_owned();
    let dir = model
        .library_dir
        .as_ref()
        .filter(|_| model.editing.is_ok())
        .map(|w| w.join("rules"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| ohl_core::files::dir_of(model_file));
    dir.join(format!("{stem}.{name}"))
}

/// The status line after writing a window's rule file: `written` is how
/// the write went; the project's working copy goes into the project.
pub(crate) fn rule_file_status(
    written: Result<(), String>,
    file: &std::path::Path,
    model: &mut Model,
) -> (String, bool) {
    let in_project = model
        .library_dir
        .as_ref()
        .is_some_and(|w| file.starts_with(w));
    match written {
        Ok(()) if in_project => match model.store_libraries() {
            Ok(_) => ("kept in the project".to_owned(), false),
            Err(e) => (format!("keeping the rules in the project: {e}"), true),
        },
        Ok(()) => (format!("kept in {}", file.display()), false),
        Err(e) => (e, true),
    }
}

/// A tick menu of the scope: `title` on its button, `list` inside.
fn tick_menu(ui: &mut egui::Ui, id: (&str, &str), title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.push_id(id, |ui| {
        crate::menu::options_menu(ui, title, |ui| {
            ui.set_min_width(260.0);
            add(ui);
        });
    });
}

fn circuits_menu(
    ui: &mut egui::Ui,
    id: &str,
    line: &ohl_line::Line,
    scope: &mut Scope,
    on: Option<&RouteOn>,
) -> bool {
    let all = circuits_of(line);
    let names: Vec<String> = all
        .iter()
        .filter(|n| on.is_none_or(|o| o.circuits.contains(*n)))
        .cloned()
        .collect();
    let before = scope.circuits.clone();
    let items: Vec<Item> = names
        .iter()
        .map(|n| Item::new(n.clone(), circuit_label(n)))
        .collect();
    let title = match &scope.circuits {
        None => "Circuits: every one".to_owned(),
        Some(c) => {
            let n = names.iter().filter(|x| c.contains(x)).count();
            format!("Circuits: {n} of {}", names.len())
        }
    };
    let mut changed = false;
    tick_menu(ui, (id, "circuits"), &title, |ui| {
        ui.label(
            egui::RichText::new("Only the wires of the circuits ticked are checked")
                .small()
                .weak(),
        );
        changed = tick_list::tick_list(
            ui,
            (id, "circuit-list"),
            &items,
            &mut scope.circuits,
            SCOPE_LIST,
        );
    });
    if changed {
        keep_hidden(&before, &mut scope.circuits, &all, &names);
    }
    changed
}

fn routes_menu(
    ui: &mut egui::Ui,
    id: &str,
    line: &ohl_line::Line,
    scope: &mut Scope,
    on: Option<&RouteOn>,
) -> bool {
    if line.routes.is_empty() && line.alignments.is_empty() {
        return false;
    }
    let shown = |route: &str| on.is_none_or(|o| o.route == route);
    let name = |id: &str, name: &str| {
        if name.is_empty() || name == id {
            id.to_owned()
        } else {
            format!("{name} ({id})")
        }
    };
    let mut items: Vec<Item> = Vec::new();
    for r in line.routes.iter().filter(|r| shown(&r.id)) {
        items.push(Item::new(
            format!("r:{}", r.id),
            format!("route {}", name(&r.id, &r.name)),
        ));
        for a in line.alignments.iter().filter(|a| a.route == r.id) {
            items.push(Item::new(
                format!("a:{}", a.id),
                format!("  alignment {}", name(&a.id, &a.name)),
            ));
        }
    }
    for a in &line.alignments {
        if on.is_none() && !line.routes.iter().any(|r| r.id == a.route) {
            items.push(Item::new(
                format!("a:{}", a.id),
                format!("alignment {}", name(&a.id, &a.name)),
            ));
        }
    }
    let mut chosen: Option<Vec<String>> =
        (!(scope.routes.is_empty() && scope.alignments.is_empty())).then(|| {
            scope
                .routes
                .iter()
                .map(|r| format!("r:{r}"))
                .chain(scope.alignments.iter().map(|a| format!("a:{a}")))
                .collect()
        });
    let title = match (scope.routes.len(), scope.alignments.len()) {
        (0, 0) => "Routes: the whole line".to_owned(),
        (1, 0) => format!("Routes: route {}", scope.routes[0]),
        (0, 1) => format!("Routes: alignment {}", scope.alignments[0]),
        (r, a) => format!("Routes: {r} route(s), {a} alignment(s)"),
    };
    let mut changed = false;
    tick_menu(ui, (id, "routes"), &title, |ui| {
        ui.label(
            egui::RichText::new(
                "None ticked (or all): the whole line. A route ticked takes all of it, an \
                 alignment its own sections",
            )
            .small()
            .weak(),
        );
        if tick_list::tick_list(ui, (id, "route-list"), &items, &mut chosen, SCOPE_LIST) {
            changed = true;
        }
    });
    if changed {
        let keys = chosen.unwrap_or_default();
        // Every one ticked is the whole line, as none.
        let all = keys.len() == items.len();
        scope.routes = if all {
            Vec::new()
        } else {
            keys.iter()
                .filter_map(|k| k.strip_prefix("r:").map(str::to_owned))
                .collect()
        };
        scope.alignments = if all {
            Vec::new()
        } else {
            keys.iter()
                .filter_map(|k| k.strip_prefix("a:").map(str::to_owned))
                .collect()
        };
        scope.routes.sort();
        scope.alignments.sort();
    }
    changed
}

fn sections_menu(
    ui: &mut egui::Ui,
    id: &str,
    line: &ohl_line::Line,
    scope: &mut Scope,
    on: Option<&RouteOn>,
) -> bool {
    let items: Vec<Item> = line
        .sections
        .iter()
        .filter(|s| on.is_none_or(|o| o.sections.contains(&s.id)))
        .map(|s| {
            let label = if s.circuit_name.is_empty() {
                s.id.clone()
            } else {
                format!("{} ({})", s.id, s.circuit_name)
            };
            Item::new(s.id.clone(), label)
        })
        .collect();
    let title = match &scope.sections {
        None => "Sections: every one".to_owned(),
        Some(c) => format!(
            "Sections: {} of {}",
            items.iter().filter(|x| c.contains(&x.key)).count(),
            items.len()
        ),
    };
    let mut changed = false;
    tick_menu(ui, (id, "sections"), &title, |ui| {
        ui.label(
            egui::RichText::new("Optional: only these sections (with the circuits and routes)")
                .small()
                .weak(),
        );
        changed = tick_list::tick_list(
            ui,
            (id, "section-list"),
            &items,
            &mut scope.sections,
            SCOPE_LIST,
        );
    });
    changed
}

fn cases_menu(ui: &mut egui::Ui, id: &str, cases: Cases<'_>) -> (bool, String) {
    let mut changed = false;
    match cases {
        Cases::One(names, case) => {
            if names.is_empty() {
                return (false, "no weather case".into());
            }
            ui.label("Weather case");
            crate::job::case_combo(ui, &format!("{id}-scope-case"), names, case);
            (false, format!("case {}", names[*case]))
        }
        Cases::Many(names, ticked) => {
            let items: Vec<Item> = names
                .iter()
                .enumerate()
                .map(|(k, n)| Item::new(k.to_string(), n.clone()))
                .collect();
            let mut chosen: Option<Vec<String>> =
                Some(ticked.iter().map(|k| k.to_string()).collect());
            let n = ticked.iter().filter(|k| **k < names.len()).count();
            let title = format!("Weather cases: {n} of {}", names.len());
            tick_menu(ui, (id, "cases"), &title, |ui| {
                changed =
                    tick_list::tick_list(ui, (id, "case-list"), &items, &mut chosen, SCOPE_LIST);
            });
            if changed {
                *ticked = match chosen {
                    None => (0..names.len()).collect(),
                    Some(c) => {
                        let mut v: Vec<usize> = c.iter().filter_map(|k| k.parse().ok()).collect();
                        v.sort_unstable();
                        v.dedup();
                        v
                    }
                };
            }
            (
                changed,
                format!("{n} weather case{}", if n == 1 { "" } else { "s" }),
            )
        }
    }
}

// --------------------------------------------------------- results

/// The verdict over the results, first (generalised from the ground
/// windows' banner): red with the worst finding when `failing`, green
/// when every rule is met. True when it was clicked (`clickable`: it
/// leads to the worst finding).
pub fn verdict(ui: &mut egui::Ui, failing: bool, text: &str, clickable: bool) -> bool {
    let color = if failing {
        ui.visuals().error_fg_color
    } else {
        crate::menu::ok_color(ui)
    };
    let r = egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.5, color))
        .fill(color.gamma_multiply(0.12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(egui::RichText::new(text).strong().color(color))
                    .sense(egui::Sense::click()),
            )
        })
        .inner;
    if clickable {
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Bring the views to it")
            .clicked()
    } else {
        false
    }
}

/// A column of a [`ResultTable`].
#[derive(Debug, Clone)]
pub struct Col {
    /// Its header, the unit in brackets (`Required [m]`).
    pub head: String,
    /// What it shows, on hover.
    pub tip: String,
    /// Numbers: right-aligned.
    pub num: bool,
}

impl Col {
    /// A column of text.
    pub fn text(head: impl Into<String>) -> Self {
        Self {
            head: head.into(),
            tip: String::new(),
            num: false,
        }
    }

    /// A column of numbers (right-aligned).
    pub fn num(head: impl Into<String>) -> Self {
        Self {
            head: head.into(),
            tip: String::new(),
            num: true,
        }
    }

    /// With a tip on its header.
    pub fn tip(mut self, tip: impl Into<String>) -> Self {
        self.tip = tip.into();
        self
    }
}

/// How a cell sorts.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// By value (a span number, a distance…).
    Num(f64),
    /// By text.
    Text(String),
}

/// A cell of a [`ResultTable`].
#[derive(Debug, Clone)]
pub struct Cell {
    /// What it shows.
    pub text: String,
    /// How it sorts.
    pub key: Key,
    /// Shown in the error colour (the value below its requirement).
    pub bad: bool,
    /// More about it on hover (e.g. how it was found), before the row's
    /// own hover.
    pub tip: Option<String>,
}

impl Cell {
    /// A text cell.
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            key: Key::Text(text.to_lowercase()),
            text,
            bad: false,
            tip: None,
        }
    }

    /// A number with `digits` decimals (`None`: "-", sorted last).
    pub fn num(v: Option<f64>, digits: usize) -> Self {
        Self {
            text: crate::job::or_dash(v, digits),
            key: Key::Num(v.unwrap_or(f64::INFINITY)),
            bad: false,
            tip: None,
        }
    }

    /// A cell shown as `text`, sorted by `v`.
    pub fn sorted(text: impl Into<String>, v: f64) -> Self {
        Self {
            text: text.into(),
            key: Key::Num(v),
            bad: false,
            tip: None,
        }
    }

    /// A span (or anything else) by its selection: its name 1-based
    /// ([`crate::selection::Selection::describe`]), sorted by its section
    /// then its number.
    pub fn span(section: &str, span: usize) -> Self {
        let sel = crate::selection::Selection::Span {
            section: section.to_owned(),
            span,
        };
        Self {
            text: sel.describe(),
            key: Key::Text(format!("{section}\u{0}{span:08}")),
            bad: false,
            tip: None,
        }
    }

    /// Shown in the error colour.
    pub fn bad(mut self, bad: bool) -> Self {
        self.bad = bad;
        self
    }

    /// `tip` on hover (none when empty).
    pub fn tip(mut self, tip: impl Into<String>) -> Self {
        let tip = tip.into();
        self.tip = (!tip.is_empty()).then_some(tip);
        self
    }
}

/// A row of a [`ResultTable`].
#[derive(Debug, Clone)]
pub struct Row {
    /// Its cells, one per column.
    pub cells: Vec<Cell>,
    /// A violation (kept by *Only violations*).
    pub violation: bool,
}

impl Row {
    /// A row of `cells`.
    pub fn new(cells: Vec<Cell>, violation: bool) -> Self {
        Self { cells, violation }
    }
}

/// How the user views a table: kept for the session per table.
#[derive(Debug, Clone, PartialEq)]
struct View {
    /// The column sorted by, ascending.
    sort: Option<(usize, bool)>,
    /// Only the violations.
    only: bool,
    search: String,
    /// The column grouped by.
    group: Option<usize>,
    /// The row selected (its index in the rows given).
    selected: Option<usize>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            sort: None,
            // Owner, 2026-10-27 (recommended, applied): on by default.
            only: true,
            search: String::new(),
            group: None,
            selected: None,
        }
    }
}

/// A line of the table drawn: a group's heading or a row.
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Group(String, usize),
    Row(usize),
}

/// The next stamp of rows built ([`CachedRows`]): never one given before,
/// whichever window builds them.
fn next_stamp() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A check window's result rows kept from frame to frame (2026-11-10:
/// every window made every row, each cell a text, again every frame).
/// Built when what they are made from changes, said by the window as a
/// key: the answer's count ([`crate::job::Job::generation`]) and what
/// else the cells are written from (the page shown, a comparison…).
/// What only chooses among the rows (*Only violations*, the search, the
/// sort, the groups) is not in the key: the table does that itself.
#[derive(Debug, Clone)]
pub struct CachedRows<K> {
    /// The key they were built for, their stamp, the rows.
    kept: Option<(K, u64, Vec<Row>)>,
}

impl<K> Default for CachedRows<K> {
    fn default() -> Self {
        Self { kept: None }
    }
}

impl<K: PartialEq> CachedRows<K> {
    /// The rows of `key`: those kept when they were built for it, else
    /// built now by `build` (and kept, the older ones dropped).
    pub fn get_or_build(&mut self, key: K, build: impl FnOnce() -> Vec<Row>) -> KeptRows<'_> {
        if self.kept.as_ref().is_some_and(|(k, ..)| *k != key) {
            self.kept = None;
        }
        let (_, stamp, rows) = self
            .kept
            .get_or_insert_with(|| (key, next_stamp(), build()));
        KeptRows {
            rows,
            stamp: *stamp,
        }
    }

    /// Drops them: the next [`CachedRows::get_or_build`] builds (tests
    /// that change a result in place).
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.kept = None;
    }

    /// The stamp of the rows kept, if any: another one after every build
    /// (tests).
    #[cfg(test)]
    pub fn stamp(&self) -> Option<u64> {
        self.kept.as_ref().map(|(_, stamp, _)| *stamp)
    }
}

/// Rows of a [`CachedRows`] with their stamp: other rows, another stamp.
/// A [`ResultTable`] given them ([`ResultTable::rows_kept`]) filters,
/// sorts and groups once per stamp and view, not every frame.
#[derive(Debug, Clone, Copy)]
pub struct KeptRows<'a> {
    /// The rows.
    pub rows: &'a [Row],
    stamp: u64,
}

/// What a table shows of kept rows, kept in the memory per table: for
/// which rows and view ([`ShownFor`]), and the answer.
type ShownKept = std::sync::Arc<(ShownFor, Shown)>;

/// What [`ResultTable::shown`] reads: the rows (their stamp), the view's
/// choices and the table's own.
#[derive(Debug, Clone, PartialEq)]
struct ShownFor {
    stamp: u64,
    sort: Option<(usize, bool)>,
    only: bool,
    search: String,
    group: Option<usize>,
    filter: bool,
    groups: Vec<usize>,
}

/// The rows shown, in order (indices in the rows given), the lines
/// drawn, and how many of all rows are violations.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    idx: Vec<usize>,
    lines: Vec<Line>,
    violations: usize,
}

#[cfg(test)]
thread_local! {
    /// Counts the times a table filtered, sorted and grouped (tests).
    static SHOWN_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The results of a check as one table (owner, 2026-10-27; built on the
/// ground windows' table, the best one): a toolbar (*Only violations*,
/// *Search*, *Group by*, *Export CSV…*), headers clicked to sort, rows
/// drawn only in view, a row clicked selected and returned.
pub struct ResultTable<'a> {
    id: &'a str,
    cols: Vec<Col>,
    rows: std::borrow::Cow<'a, [Row]>,
    /// The stamp of rows kept ([`ResultTable::rows_kept`]).
    stamp: Option<u64>,
    filter: bool,
    groups: Vec<usize>,
    csv: Option<&'a str>,
    /// *Full report (CSV)…*: the file's name, what it holds (on hover)
    /// and its text (made when clicked).
    full_csv: Option<FullCsv<'a>>,
    /// Kept free under the table, px (the folds under it).
    below: f32,
}

/// A window's full report as CSV beside the table's *Export CSV…*: the
/// file's name, what it holds, its text made when clicked.
type FullCsv<'a> = (&'a str, &'a str, Box<dyn Fn() -> String + 'a>);

/// The memory key of a table's view.
fn view_key(id: &str) -> egui::Id {
    egui::Id::new(("result-table-view", id))
}

/// Sets *Only violations* of table `id` (tests).
#[cfg(test)]
pub fn set_only_violations(ctx: &egui::Context, id: &str, only: bool) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<View>(view_key(id)).only = only);
}

impl<'a> ResultTable<'a> {
    /// The table `id` of `cols`.
    pub fn new(id: &'a str, cols: Vec<Col>) -> Self {
        Self {
            id,
            cols,
            rows: std::borrow::Cow::Borrowed(&[]),
            stamp: None,
            filter: true,
            groups: Vec::new(),
            csv: None,
            full_csv: None,
            below: 0.0,
        }
    }

    /// Its rows, made for this frame (a few rows, or rows of what is
    /// edited: filtered and sorted again every frame).
    pub fn rows(mut self, rows: Vec<Row>) -> Self {
        self.rows = std::borrow::Cow::Owned(rows);
        self.stamp = None;
        self
    }

    /// Its rows, kept by the window from frame to frame ([`CachedRows`]:
    /// built once per run, thousands of rows are not made again every
    /// frame; filtered, sorted and grouped once per view of them).
    pub fn rows_kept(mut self, kept: KeptRows<'a>) -> Self {
        self.rows = std::borrow::Cow::Borrowed(kept.rows);
        self.stamp = Some(kept.stamp);
        self
    }

    /// Without *Only violations* (a table of values, not findings).
    pub fn no_filter(mut self) -> Self {
        self.filter = false;
        self
    }

    /// *Group by* these columns.
    pub fn group_by(mut self, cols: &[usize]) -> Self {
        self.groups = cols.to_vec();
        self
    }

    /// *Export CSV…*, the file named `name`.
    pub fn csv(mut self, name: &'a str) -> Self {
        self.csv = Some(name);
        self
    }

    /// *Full report (CSV)…* beside *Export CSV…* (one place to export,
    /// check pages' guidelines §4): every row with every field, as the
    /// command line writes it — file `name`, `what` on hover, `text`
    /// made when clicked.
    pub fn full_csv(
        mut self,
        name: &'a str,
        what: &'a str,
        text: impl Fn() -> String + 'a,
    ) -> Self {
        self.full_csv = Some((name, what, Box::new(text)));
        self
    }

    /// `px` kept free under the table (for what the window shows below).
    pub fn below(mut self, px: f32) -> Self {
        self.below = px;
        self
    }

    /// The rows shown, in order (indices in the rows given), and the
    /// lines drawn.
    fn shown(&self, view: &View) -> (Vec<usize>, Vec<Line>) {
        #[cfg(test)]
        SHOWN_RUNS.with(|n| n.set(n.get() + 1));
        let q = view.search.trim().to_lowercase();
        let mut idx: Vec<usize> = (0..self.rows.len())
            .filter(|&k| !self.filter || !view.only || self.rows[k].violation)
            .filter(|&k| {
                q.is_empty()
                    || self.rows[k]
                        .cells
                        .iter()
                        .any(|c| c.text.to_lowercase().contains(&q))
            })
            .collect();
        let cmp = |a: &Key, b: &Key| match (a, b) {
            (Key::Num(x), Key::Num(y)) => x.total_cmp(y),
            (Key::Text(x), Key::Text(y)) => x.cmp(y),
            (Key::Num(_), Key::Text(_)) => std::cmp::Ordering::Less,
            (Key::Text(_), Key::Num(_)) => std::cmp::Ordering::Greater,
        };
        let key = |k: usize, c: usize| self.rows[k].cells.get(c).map(|c| &c.key);
        if let Some((c, asc)) = view.sort {
            idx.sort_by(|&a, &b| {
                let o = match (key(a, c), key(b, c)) {
                    (Some(x), Some(y)) => cmp(x, y),
                    _ => std::cmp::Ordering::Equal,
                };
                if asc { o } else { o.reverse() }
            });
        }
        let group = view.group.filter(|g| self.groups.contains(g));
        let mut lines = Vec::new();
        match group {
            None => lines.extend(idx.iter().map(|&k| Line::Row(k))),
            Some(g) => {
                // Stable: the sort kept within each group.
                let text = |k: usize| {
                    self.rows[k]
                        .cells
                        .get(g)
                        .map_or(String::new(), |c| c.text.clone())
                };
                // Each group found by its name in a map (a search through
                // the groups made thousands of rows slow).
                let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
                let mut at: std::collections::HashMap<String, usize> = Default::default();
                for &k in &idx {
                    let t = text(k);
                    match at.get(&t) {
                        Some(&j) => groups[j].1.push(k),
                        None => {
                            at.insert(t.clone(), groups.len());
                            groups.push((t, vec![k]));
                        }
                    }
                }
                groups.sort_by(|a, b| {
                    let (ka, kb) = (key(a.1[0], g), key(b.1[0], g));
                    match (ka, kb) {
                        (Some(x), Some(y)) => cmp(x, y),
                        _ => std::cmp::Ordering::Equal,
                    }
                });
                idx.clear();
                for (name, rows) in groups {
                    lines.push(Line::Group(name, rows.len()));
                    for k in rows {
                        idx.push(k);
                        lines.push(Line::Row(k));
                    }
                }
            }
        }
        (idx, lines)
    }

    /// The memory key of what the table showed last.
    fn shown_key(&self) -> egui::Id {
        egui::Id::new(("result-table-shown", self.id))
    }

    /// What the table showed last of these very rows (kept rows, the
    /// same stamp), in whatever view.
    fn shown_before(&self, ui: &egui::Ui) -> Option<ShownKept> {
        let stamp = self.stamp?;
        ui.data(|d| d.get_temp::<ShownKept>(self.shown_key()))
            .filter(|kept| kept.0.stamp == stamp)
    }

    /// How many of all rows are violations (counted once for kept rows).
    fn violations(&self, ui: &egui::Ui) -> usize {
        match self.shown_before(ui) {
            Some(kept) => kept.1.violations,
            None => self.rows.iter().filter(|r| r.violation).count(),
        }
    }

    /// [`ResultTable::shown`] and the count of violations: of kept rows
    /// ([`ResultTable::rows_kept`]) the answer of the last frame while
    /// the rows (their stamp), the view's choices and the table's are
    /// the same (kept in the memory per table); else made now.
    fn shown_kept(&self, ui: &egui::Ui, view: &View) -> ShownKept {
        let key = self.shown_key();
        if let Some(kept) = self.shown_before(ui)
            && let f = &kept.0
            && f.sort == view.sort
            && f.only == view.only
            && f.search == view.search
            && f.group == view.group
            && f.filter == self.filter
            && f.groups == self.groups
        {
            return kept;
        }
        let (idx, lines) = self.shown(view);
        let shown = Shown {
            idx,
            lines,
            violations: self.rows.iter().filter(|r| r.violation).count(),
        };
        let made_for = ShownFor {
            stamp: self.stamp.unwrap_or(0),
            sort: view.sort,
            only: view.only,
            search: view.search.clone(),
            group: view.group,
            filter: self.filter,
            groups: self.groups.clone(),
        };
        let kept: ShownKept = std::sync::Arc::new((made_for, shown));
        if self.stamp.is_some() {
            ui.data_mut(|d| d.insert_temp(key, kept.clone()));
        }
        kept
    }

    /// The rows shown as CSV (the headers first).
    fn csv_text(&self, idx: &[usize]) -> String {
        let field = |s: &str| {
            if s.contains([',', '"', '\n']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_owned()
            }
        };
        let mut out = self
            .cols
            .iter()
            .map(|c| field(&c.head))
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        for &k in idx {
            out.push_str(
                &self.rows[k]
                    .cells
                    .iter()
                    .map(|c| field(&c.text))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            out.push('\n');
        }
        out
    }

    /// The toolbar and the table, filling the room left (less
    /// [`ResultTable::below`]). The row clicked (its index in the rows
    /// given), if one was.
    pub fn ui(self, ui: &mut egui::Ui) -> Option<usize> {
        let key = view_key(self.id);
        let mut view: View = ui.data(|d| d.get_temp(key)).unwrap_or_default();
        let violations = self.violations(ui);
        ui.horizontal_wrapped(|ui| {
            if self.filter {
                ui.checkbox(&mut view.only, "Only violations")
                    .on_hover_text(format!(
                        "Only the rows below a requirement ({violations} of {})",
                        self.rows.len()
                    ));
            }
            let label = ui.label("Search");
            ui.add(
                egui::TextEdit::singleline(&mut view.search)
                    .hint_text("any cell")
                    .desired_width(140.0),
            )
            .labelled_by(label.id)
            .on_hover_text(
                "Shows the rows with a cell containing this (upper and lower case alike)",
            );
            if !self.groups.is_empty() {
                ui.label("Group by");
                let name = |g: Option<usize>| match g {
                    None => "none".to_owned(),
                    Some(c) => self.cols.get(c).map_or(String::new(), |c| c.head.clone()),
                };
                egui::ComboBox::from_id_salt((self.id, "group"))
                    .selected_text(name(view.group))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut view.group, None, name(None));
                        for &g in &self.groups {
                            ui.selectable_value(&mut view.group, Some(g), name(Some(g)));
                        }
                    });
            }
        });
        let shown = self.shown_kept(ui, &view);
        let (idx, lines) = (&shown.1.idx, &shown.1.lines);
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{} of {} rows shown", idx.len(), self.rows.len()))
                    .small()
                    .weak(),
            );
            if let Some(name) = self.csv
                && ui
                    .add_enabled(!idx.is_empty(), egui::Button::new("Export CSV…"))
                    .on_hover_text("The rows shown, as a CSV file (the headers first)")
                    .clicked()
            {
                crate::job::save_csv(name, &self.csv_text(idx));
            }
            if let Some((name, what, text)) = &self.full_csv
                && ui
                    .button("Full report (CSV)…")
                    .on_hover_text(*what)
                    .clicked()
            {
                crate::job::save_csv(name, &text());
            }
        });
        if self.rows.is_empty() {
            ui.label(egui::RichText::new("Nothing to list.").weak());
        } else if idx.is_empty() {
            ui.label(
                egui::RichText::new(if view.only && self.filter {
                    "No violation: untick Only violations to see every row."
                } else {
                    "No row matches the search."
                })
                .weak(),
            );
        }
        // The room left (in a scrolled body, which has no end: a list's).
        let room = ui.available_height();
        let height = if room.is_finite() {
            (room - self.below).max(80.0)
        } else {
            help::LIST_HEIGHT
        };
        egui::ScrollArea::both()
            .id_salt((self.id, "table"))
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                let mut table = crate::rows_in_view::Table::new(ui, self.id, self.cols.len())
                    .spacing(help::GRID_SPACING[0], help::GRID_SPACING[1]);
                table.header_cells(ui, |c| {
                    for (k, col) in self.cols.iter().enumerate() {
                        c.cell(|ui| {
                            let arrow = match view.sort {
                                Some((s, true)) if s == k => " ⏶",
                                Some((s, false)) if s == k => " ⏷",
                                _ => "",
                            };
                            let add = |ui: &mut egui::Ui| {
                                let r = ui
                                    .add(
                                        egui::Label::new(egui::RichText::new(&col.head).strong())
                                            .sense(egui::Sense::click()),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if !arrow.is_empty() {
                                    ui.label(egui::RichText::new(arrow).small());
                                }
                                let tip = if col.tip.is_empty() {
                                    "Click to sort by this column (again: the other way)".to_owned()
                                } else {
                                    format!("{} — click to sort (again: the other way)", col.tip)
                                };
                                r.on_hover_text(tip)
                            };
                            let r = if col.num { num(ui, add) } else { add(ui) };
                            if r.clicked() {
                                view.sort = match view.sort {
                                    Some((s, true)) if s == k => Some((k, false)),
                                    Some((s, false)) if s == k => None,
                                    _ => Some((k, true)),
                                };
                            }
                        });
                    }
                });
                table.rows(ui, lines.len(), None, |c, i| match &lines[i] {
                    Line::Group(name, n) => {
                        c.cell(|ui| ui.strong(format!("{name} ({n})")));
                    }
                    &Line::Row(k) => {
                        let row = &self.rows[k];
                        if view.selected == Some(k) {
                            let rect = c.ui.max_rect();
                            c.ui.painter().rect_filled(
                                rect,
                                2.0,
                                c.ui.visuals().selection.bg_fill.gamma_multiply(0.5),
                            );
                        }
                        let tip = "Select it and bring the views to it";
                        let mut hit = false;
                        for (j, cell) in row.cells.iter().enumerate() {
                            let numeric = self.cols.get(j).is_some_and(|c| c.num);
                            hit |= c.cell(|ui| {
                                let mut t = egui::RichText::new(&cell.text);
                                if cell.bad {
                                    t = t.color(ui.visuals().error_fg_color);
                                }
                                let hover = match &cell.tip {
                                    Some(more) => format!("{more}\n\n{tip}"),
                                    None => tip.to_owned(),
                                };
                                let add = |ui: &mut egui::Ui| {
                                    ui.add(egui::Label::new(t).sense(egui::Sense::click()))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(hover)
                                        .clicked()
                                };
                                if numeric { num(ui, add) } else { add(ui) }
                            });
                        }
                        if hit {
                            clicked = Some(k);
                        }
                    }
                });
            });
        if clicked.is_some() {
            view.selected = clicked;
        }
        ui.data_mut(|d| d.insert_temp(key, view));
        clicked
    }
}

/// `add` right-aligned in its cell (a number column).
fn num<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add)
        .inner
}

/// Folds under a results table: *Notes (n)* (fixed-height box: the
/// standing rule for long reports) and *Where the time went*. The room
/// they need closed, px (for [`ResultTable::below`]).
pub const FOLDS_BELOW: f32 = 64.0;

/// *Notes (n)* folded, the notes in a box of fixed height.
pub fn notes_fold(ui: &mut egui::Ui, id: &str, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("Notes ({})", notes.len()))
        .id_salt((id, "notes"))
        .show(ui, |ui| {
            crate::menu::message(
                ui,
                egui::RichText::new(notes.join("\n")).small().weak(),
                None,
            );
        });
}

// ---------------------------------------------------------------- footer

/// A run under way, as the footer says it.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// The share done (none: not known, a spinner).
    pub fraction: Option<f32>,
    /// What it does and the time left, in words.
    pub text: String,
}

/// A time left in words: seconds, then minutes.
fn left_text(l: f64) -> String {
    if l < 90.0 {
        format!("{l:.0} s")
    } else {
        format!("{:.0} min", l / 60.0)
    }
}

impl Progress {
    /// A run started `elapsed` ago, `done` of `total` done: the bar and
    /// the time left.
    pub fn of(elapsed: Duration, done: usize, total: usize) -> Self {
        let s = elapsed.as_secs_f64();
        let fraction = (total > 0).then(|| done as f32 / total as f32);
        let left = (done > 0 && total > done).then(|| s / done as f64 * (total - done) as f64);
        Self {
            fraction,
            text: match left {
                Some(l) => format!("{s:.0} s, about {l:.0} s left"),
                None => format!("{s:.1} s"),
            },
        }
    }

    /// A run started `elapsed` ago with about `left` s to go (none:
    /// not known yet): the bar the share of the time gone.
    pub fn with_left(elapsed: Duration, left: Option<f64>) -> Self {
        let s = elapsed.as_secs_f64();
        Self {
            fraction: left.map(|l| (s / (s + l).max(1e-9)) as f32),
            text: match left {
                Some(l) if l > 0.0 => format!("{s:.0} s, about {} left", left_text(l)),
                _ => format!("{s:.1} s"),
            },
        }
    }

    /// A run started `elapsed` ago, how far unknown.
    pub fn elapsed(elapsed: Duration) -> Self {
        Self {
            fraction: None,
            text: format!("{:.1} s", elapsed.as_secs_f64()),
        }
    }
}

/// What the Run footer was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Nothing.
    None,
    /// Run.
    Run,
    /// Stop the run.
    Stop,
}

/// The Run footer's content (owner, 2026-10-27: one for every check).
#[derive(Debug, Default)]
pub struct RunFooter<'a> {
    /// The run under way (none: idle).
    pub running: Option<Progress>,
    /// Why Run is grey ("Before Run: …").
    pub why_not: Option<String>,
    /// Run's hover text.
    pub hover: &'a str,
    /// The scope in one line.
    pub scope: Option<String>,
    /// Said after the last run (how long it took, what it solved), or
    /// what a run will do.
    pub info: Option<String>,
}

/// The Run footer: Run (grey with "Before Run: …"), or Stop and the
/// progress while a run is under way; the scope; then `extra` (the
/// overrides, *Save in Performed checks*). Draw it inside
/// [`help::footer`].
pub fn run_footer(
    ui: &mut egui::Ui,
    f: RunFooter<'_>,
    extra: impl FnOnce(&mut egui::Ui),
) -> Action {
    let mut action = Action::None;
    match &f.running {
        Some(p) => {
            match p.fraction {
                Some(x) => {
                    ui.add(
                        egui::ProgressBar::new(x)
                            .desired_width(140.0)
                            .show_percentage(),
                    );
                }
                None => {
                    ui.spinner();
                }
            }
            ui.label(&p.text);
            if help::stop_button(ui, "Stop this check: the previous results stay") {
                action = Action::Stop;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }
        None => {
            if help::run_button(ui, true, f.hover, f.why_not.as_deref()) {
                action = Action::Run;
            }
            if let Some(w) = &f.why_not {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    egui::RichText::new(format!("Before Run: {w}")).small(),
                );
            } else if let Some(i) = &f.info {
                ui.label(egui::RichText::new(i).small().weak());
            }
        }
    }
    if let Some(s) = &f.scope {
        ui.label(egui::RichText::new(format!("· {s}")).small().weak());
    }
    extra(ui);
    action
}

/// What a run will compute, said before Run (owner, 2026-10-31, the
/// check pages' guidelines: "4 spans · 3 cases — about 25 s"): the words,
/// and how much it computes (objects × cases, counted the same way from
/// run to run) for the time estimated from the last run.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub what: String,
    pub units: usize,
}

impl Plan {
    /// `what`, computing `units`.
    pub fn new(what: impl Into<String>, units: usize) -> Self {
        Self {
            what: what.into(),
            units,
        }
    }

    /// The footer's line: what, about how long (from the last run of
    /// `job`, else measured on the first), and the last run's time.
    pub fn words<T: Send + 'static>(&self, job: &crate::job::Job<T>) -> String {
        let time = match job.estimate(self.units) {
            Some(s) => format!(" — {} (from the last run)", plan_duration(s)),
            None if self.units > 0 => " — time measured on the first run".into(),
            None => String::new(),
        };
        let last = job
            .took
            .map(|t| format!(" · last run {:.1} s", t.as_secs_f64()))
            .unwrap_or_default();
        format!("{}{time}{last}", self.what)
    }
}

/// The plan of a check run on `scope` in `cases` weather cases: "3
/// sections (12 spans) in 2 cases". The line is solved whole in each case
/// (a scope only narrows what is checked), so the time goes with the
/// line's spans × the cases.
pub fn scope_plan(line: &ohl_line::Line, scope: &Scope, cases: usize) -> Plan {
    let (sections, spans) = scope_counts(line, scope);
    let all: usize = line
        .sections
        .iter()
        .map(|s| s.points.len().saturating_sub(1))
        .sum();
    Plan::new(
        format!(
            "{} ({}) in {}",
            count(sections, "section"),
            count(spans, "span"),
            count(cases, "case")
        ),
        all.max(1) * cases,
    )
}

/// The sections and their spans `scope` takes of `line`.
pub fn scope_counts(line: &ohl_line::Line, scope: &Scope) -> (usize, usize) {
    let ids = scope.sections_of(line);
    let spans = line
        .sections
        .iter()
        .filter(|s| ids.contains(&s.id))
        .map(|s| s.points.len().saturating_sub(1))
        .sum();
    (ids.len(), spans)
}

/// "about 12 s", "about 3.5 min".
pub fn plan_duration(s: f64) -> String {
    if s < 1.0 {
        "under a second".into()
    } else if s < 90.0 {
        format!("about {s:.0} s")
    } else if s < 5400.0 {
        format!("about {:.1} min", s / 60.0)
    } else {
        format!("about {:.1} h", s / 3600.0)
    }
}

/// `n` `one`s, the plural with an s ("1 span", "4 spans").
pub fn count(n: usize, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else if let Some(stem) = one.strip_suffix('y') {
        format!("{n} {stem}ies")
    } else if one.ends_with('s') || one.ends_with("sh") || one.ends_with("ch") {
        format!("{n} {one}es")
    } else {
        format!("{n} {one}s")
    }
}

/// The footer of a window computed by a [`crate::job::Job`]: its
/// progress while it runs (Stop discards the run: the previous results
/// stay), else what a run will compute and about how long it takes
/// (`plan`), or how long the last run took.
pub fn job_footer<T: Send + 'static>(
    ui: &mut egui::Ui,
    job: &mut crate::job::Job<T>,
    why_not: Option<String>,
    hover: &str,
    scope: Option<String>,
    plan: Option<Plan>,
    extra: impl FnOnce(&mut egui::Ui),
) -> bool {
    let units = plan.as_ref().map_or(0, |p| p.units);
    let f = RunFooter {
        running: job.elapsed().map(Progress::elapsed),
        why_not,
        hover,
        scope,
        info: match &plan {
            Some(p) => Some(p.words(job)),
            None => job
                .took
                .map(|t| format!("computed in {:.1} s", t.as_secs_f64())),
        },
    };
    match run_footer(ui, f, extra) {
        Action::Run => {
            job.run_units = units;
            true
        }
        Action::Stop => {
            job.stop();
            false
        }
        Action::None => false,
    }
}

// ------------------------------------------------------------ kept

/// A window's setup kept in the project (owner, 2026-10-27: Vegetation,
/// Safety volumes and Distance between conductors keep it, as the
/// magnetic field's): a project setting per project line,
/// `<setting>.<line>`, JSON; read when the project opens or the line
/// worked on changes, written when it changed (not while dragging or
/// typing). A project this user cannot change keeps it for the session.
#[derive(Debug, Default)]
pub struct Kept {
    /// The key and the JSON as last read or written (none: not read).
    saved: Option<(String, String)>,
}

/// What [`Kept::step`] asks of the window.
pub enum Step {
    /// Nothing.
    Kept,
    /// Take this setup (none kept yet: the defaults), then
    /// [`Kept::loaded`] with what it is now.
    Load(Option<String>),
}

impl Kept {
    /// The key of `setting` for the line worked on (none without a
    /// project).
    fn key(model: &crate::model::Model, setting: &str) -> Option<String> {
        model
            .session()
            .map(|s| format!("{setting}.{}", s.current_line()))
    }

    /// Each frame: [`Step::Load`] when the line's setup is not read yet,
    /// else `json` (the setup now) written when it changed.
    pub fn step(
        &mut self,
        model: &mut crate::model::Model,
        ctx: &egui::Context,
        setting: &str,
        what: &str,
        json: &str,
    ) -> Step {
        let Some(key) = Self::key(model, setting) else {
            return Step::Kept;
        };
        if self.saved.as_ref().is_none_or(|(k, _)| *k != key) {
            let text = model.session().and_then(|s| s.setting(&key).ok().flatten());
            self.saved = Some((key, String::new()));
            return Step::Load(text);
        }
        if self.saved.as_ref().is_some_and(|(_, j)| j == json)
            || ctx.input(|i| i.pointer.any_down())
            || ctx.memory(|m| m.focused().is_some())
        {
            return Step::Kept;
        }
        let op = ohl_server::Op::SetSetting {
            key: key.clone(),
            value: Some(json.to_owned()),
        };
        // (Read-only for this user: kept for the session.)
        let _ = model.project_op(what, op);
        self.saved = Some((key, json.to_owned()));
        Step::Kept
    }

    /// The setup taken after [`Step::Load`]: `json` is what is kept.
    pub fn loaded(&mut self, json: String) {
        if let Some((_, j)) = &mut self.saved {
            *j = json;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn rows() -> Vec<Row> {
        vec![
            Row::new(
                vec![Cell::span("S1", 0), Cell::num(Some(7.5), 2).bad(true)],
                true,
            ),
            Row::new(vec![Cell::span("S1", 1), Cell::num(Some(12.0), 2)], false),
            Row::new(
                vec![Cell::span("S2", 0), Cell::num(Some(3.25), 2).bad(true)],
                true,
            ),
        ]
    }

    fn table(ui: &mut egui::Ui, picked: &mut Option<usize>) {
        let t = ResultTable::new("test-table", vec![Col::text("Span"), Col::num("Found [m]")])
            .rows(rows())
            .csv("test.csv");
        if let Some(k) = t.ui(ui) {
            *picked = Some(k);
        }
    }

    /// Spans are said 1-based; *Only violations* is on by default and
    /// hides the row that is clear; a header clicked sorts by it, again
    /// the other way; a row clicked is returned.
    #[test]
    fn a_result_table_filters_sorts_and_picks() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui_state(table, None);
        h.run();
        h.get_by_label("S1 span 1");
        h.get_by_label("S2 span 1");
        assert!(
            h.query_by_label("S1 span 2").is_none(),
            "the clear span hidden"
        );
        h.get_by_label("2 of 3 rows shown");
        h.get_by_label("Only violations").click();
        h.run();
        h.get_by_label("S1 span 2");
        h.get_by_label("3 of 3 rows shown");
        // Sorted by Found: 3.25, 7.50, 12.00 top down.
        h.get_by_label("Found [m]").click();
        h.run();
        let y = |h: &Harness<'_, Option<usize>>, t: &str| h.get_by_label(t).rect().top();
        assert!(y(&h, "3.25") < y(&h, "7.50") && y(&h, "7.50") < y(&h, "12.00"));
        h.get_by_label("Found [m]").click();
        h.run();
        assert!(y(&h, "12.00") < y(&h, "7.50") && y(&h, "7.50") < y(&h, "3.25"));
        // Searched: one row.
        h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Search")
            .focus();
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Search")
            .type_text("s2");
        h.run();
        h.get_by_label("1 of 3 rows shown");
        h.get_by_label("S2 span 1").click();
        h.run();
        assert_eq!(*h.state(), Some(2));
    }

    /// Rows of three rules, for the kept-rows tests: `shift` moves the
    /// values (other rows for another key).
    fn rule_rows(shift: f64) -> Vec<Row> {
        [
            ("ice", 0, 7.5),
            ("wind", 1, 12.0),
            ("ice", 2, 3.25),
            ("cold", 0, 9.0),
            ("wind", 3, 1.0),
        ]
        .into_iter()
        .map(|(rule, span, v)| {
            let v = v + shift;
            Row::new(
                vec![
                    Cell::text(rule),
                    Cell::span("S1", span),
                    Cell::num(Some(v), 2).bad(v < 8.0),
                ],
                v < 8.0,
            )
        })
        .collect()
    }

    fn rule_cols() -> Vec<Col> {
        vec![Col::text("Rule"), Col::text("Span"), Col::num("Found [m]")]
    }

    /// Kept rows are built when their key changes and not otherwise, and
    /// every build has a stamp of its own.
    #[test]
    fn kept_rows_are_built_once_per_key() {
        let mut kept: CachedRows<(u64, bool)> = CachedRows::default();
        let mut builds = 0;
        let mut get = |kept: &mut CachedRows<(u64, bool)>, key: (u64, bool), shift: f64| {
            let k = kept.get_or_build(key, || {
                builds += 1;
                rule_rows(shift)
            });
            (k.stamp, k.rows[0].cells[2].text.clone())
        };
        let first = get(&mut kept, (1, false), 0.0);
        assert_eq!(first.1, "7.50");
        // The same key: the rows kept, whatever would be built now.
        for _ in 0..5 {
            assert_eq!(get(&mut kept, (1, false), 100.0), first);
        }
        // Another key (either part): built again, another stamp.
        let second = get(&mut kept, (2, false), 1.0);
        assert_eq!(second.1, "8.50");
        assert_ne!(second.0, first.0);
        let third = get(&mut kept, (2, true), 2.0);
        assert_eq!(third.1, "9.50");
        // Back to a key of before: built again (one answer is kept).
        let back = get(&mut kept, (1, false), 0.0);
        assert_eq!(back.1, "7.50");
        assert!(back.0 != first.0 && back.0 != third.0);
        drop(get);
        assert_eq!(builds, 4);
        // Another cache never gives a stamp given before.
        let mut other: CachedRows<u8> = CachedRows::default();
        assert_ne!(other.get_or_build(0, || rule_rows(0.0)).stamp, back.0);
    }

    /// Kept rows filtered, sorted and grouped give what rows made for the
    /// frame give, in every view; made once per view and rows, then taken
    /// from the memory.
    #[test]
    fn kept_rows_are_shown_as_fresh_ones_once_per_view() {
        let view = |sort, only, search: &str, group| View {
            sort,
            only,
            search: search.to_owned(),
            group,
            selected: None,
        };
        let views = [
            View::default(),
            view(None, false, "", None),
            view(Some((2, true)), false, "", None),
            view(Some((2, false)), true, "", None),
            view(Some((1, true)), false, " ICE ", None),
            view(None, false, "", Some(0)),
            view(Some((2, false)), false, "", Some(0)),
            view(Some((0, true)), true, "span", Some(0)),
            // Not a column grouped by: no groups.
            view(None, false, "", Some(2)),
        ];
        let runs = || SHOWN_RUNS.with(std::cell::Cell::get);
        let mut cache: CachedRows<u32> = CachedRows::default();
        egui::__run_test_ui(|ui| {
            for key in [1, 2] {
                for v in &views {
                    let kept = cache.get_or_build(key, || rule_rows(f64::from(key)));
                    let t = ResultTable::new("kept", rule_cols())
                        .rows_kept(kept)
                        .group_by(&[0, 1]);
                    let fresh = ResultTable::new("fresh", rule_cols())
                        .rows(rule_rows(f64::from(key)))
                        .group_by(&[0, 1])
                        .shown(v);
                    let before = runs();
                    let first = t.shown_kept(ui, v);
                    assert_eq!(runs(), before + 1, "{v:?}");
                    assert_eq!(
                        (&first.1.idx, &first.1.lines),
                        (&fresh.0, &fresh.1),
                        "{v:?}"
                    );
                    assert_eq!(
                        first.1.violations,
                        t.rows.iter().filter(|r| r.violation).count()
                    );
                    // Again, and what is selected changed: not made again.
                    let mut same = v.clone();
                    same.selected = Some(1);
                    for _ in 0..3 {
                        let again = t.shown_kept(ui, &same);
                        assert!(std::sync::Arc::ptr_eq(&first, &again), "{v:?}");
                    }
                    assert_eq!(runs(), before + 1, "{v:?}");
                    assert_eq!(t.violations(ui), first.1.violations);
                }
            }
            // The table's own choices are part of it: without the filter,
            // or grouped by other columns, made again.
            let v = view(None, true, "", Some(1));
            let kept = cache.get_or_build(2, Vec::new);
            let a = ResultTable::new("kept", rule_cols())
                .rows_kept(kept)
                .group_by(&[0, 1]);
            let with = a.shown_kept(ui, &v);
            let b = ResultTable::new("kept", rule_cols())
                .rows_kept(kept)
                .group_by(&[0, 1])
                .no_filter();
            let without = b.shown_kept(ui, &v);
            assert!(without.1.idx.len() > with.1.idx.len());
            let c = ResultTable::new("kept", rule_cols())
                .rows_kept(kept)
                .group_by(&[0]);
            let ungrouped = c.shown_kept(ui, &v);
            assert!(ungrouped.1.lines.iter().all(|l| matches!(l, Line::Row(_))));
        });
    }

    /// A table of kept rows drawn frame after frame filters once; rows of
    /// another key are shown at once, in the view chosen before.
    #[test]
    fn a_table_of_kept_rows_follows_its_rows() {
        type State = (CachedRows<u32>, u32, usize);
        let runs = || SHOWN_RUNS.with(std::cell::Cell::get);
        let before = runs();
        let mut h = Harness::builder()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui_state(
                |ui, s: &mut State| {
                    let key = s.1;
                    let kept = s.0.get_or_build(key, || {
                        s.2 += 1;
                        rule_rows(f64::from(key))
                    });
                    ResultTable::new("follows", rule_cols())
                        .rows_kept(kept)
                        .ui(ui);
                },
                (CachedRows::default(), 0, 0),
            );
        h.run();
        h.run();
        h.run();
        assert_eq!(h.state().2, 1, "built once");
        assert_eq!(runs(), before + 1, "filtered once");
        h.get_by_label("3 of 5 rows shown");
        h.get_by_label("7.50");
        // Every row, sorted by the value: one more filtering each.
        h.get_by_label("Only violations").click();
        h.run();
        h.get_by_label("Found [m]").click();
        h.run();
        h.run();
        assert_eq!((h.state().2, runs()), (1, before + 3));
        h.get_by_label("5 of 5 rows shown");
        // Other rows (a new run): built and filtered once more, shown in
        // the same view, the rows of before gone.
        h.state_mut().1 = 1;
        h.run();
        h.run();
        assert_eq!((h.state().2, runs()), (2, before + 4));
        h.get_by_label("5 of 5 rows shown");
        h.get_by_label("8.50");
        assert!(h.query_by_label("7.50").is_none(), "the rows of before");
        let y = |h: &Harness<'_, State>, t: &str| h.get_by_label(t).rect().top();
        assert!(y(&h, "2.00") < y(&h, "4.25") && y(&h, "4.25") < y(&h, "13.00"));
        h.get_by_label("Only violations").click();
        h.run();
        // 2.00 and 4.25 are below 8.
        h.get_by_label("2 of 5 rows shown");
    }

    /// What a frame of a results table cost before the rows were kept,
    /// and costs now (docs/development/performance.md), for 5000 rows of
    /// seven cells as Wire tensions writes them:
    ///
    /// ```bash
    /// cargo test --release -p ohl-app rows_kept_timing -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "a timing, printed"]
    fn rows_kept_timing() {
        const ROWS: usize = 5000;
        let build = || -> Vec<Row> {
            (0..ROWS)
                .map(|k| {
                    let tension = 20_000.0 + 7.0 * k as f64;
                    let bad = k % 3 == 0;
                    Row::new(
                        vec![
                            Cell::span(&format!("S{}", k % 40), k % 125),
                            Cell::text(format!("rule {}", k % 4)),
                            Cell::text(format!("case {}", k % 10)),
                            Cell::num(Some(tension / 1000.0), 2).bad(bad),
                            Cell::num(Some(55.0), 2),
                            Cell::num(Some(tension / 550.0), 1).bad(bad),
                            Cell::text(if bad { "fail" } else { "pass" }).bad(bad),
                        ],
                        bad,
                    )
                })
                .collect()
        };
        let cols = || {
            [
                "Span", "Rule", "Case", "Tension", "Allowed", "Usage", "Verdict",
            ]
            .map(Col::text)
            .to_vec()
        };
        let median = |mut t: Vec<f64>| {
            t.sort_by(f64::total_cmp);
            t[t.len() / 2]
        };
        let views = [
            ("only violations", View::default()),
            (
                "every row, sorted by tension, grouped by rule",
                View {
                    sort: Some((3, false)),
                    only: false,
                    group: Some(1),
                    ..View::default()
                },
            ),
        ];
        egui::__run_test_ui(|ui| {
            for (name, view) in &views {
                // Before: the rows made, then filtered, sorted and
                // grouped, every frame.
                let before = median(
                    (0..40)
                        .map(|_| {
                            let t = std::time::Instant::now();
                            let table = ResultTable::new("timing", cols())
                                .rows(build())
                                .group_by(&[1, 2, 0]);
                            let n = table.rows.iter().filter(|r| r.violation).count();
                            std::hint::black_box((table.shown(view), n));
                            t.elapsed().as_secs_f64() * 1e3
                        })
                        .collect(),
                );
                // Now: the rows kept, what is shown of them kept.
                let mut cache: CachedRows<u64> = CachedRows::default();
                let frame = |cache: &mut CachedRows<u64>| {
                    let t = std::time::Instant::now();
                    let table = ResultTable::new("timing", cols())
                        .rows_kept(cache.get_or_build(1, build))
                        .group_by(&[1, 2, 0]);
                    let n = table.violations(ui);
                    std::hint::black_box((table.shown_kept(ui, view), n));
                    t.elapsed().as_secs_f64() * 1e3
                };
                let first = frame(&mut cache);
                let now = median((0..40).map(|_| frame(&mut cache)).collect());
                println!(
                    "{ROWS} rows, {name}: {before:.3} ms a frame before; now {first:.3} ms the \
                     first frame, then {:.4} ms a frame",
                    now
                );
            }
        });
    }

    /// The lists the rule editors offer are made once per version of the
    /// line, route worked on and case names, and are those the windows
    /// made every frame before.
    #[test]
    fn line_choices_are_made_once_per_version() {
        let mut model = crate::tests::synthetic();
        let made = || CHOICES_MADE.with(std::cell::Cell::get);
        let before = made();
        let mut choices = LineChoices::default();
        for _ in 0..4 {
            choices.of(&model);
        }
        assert_eq!(made(), before + 1);
        let c = choices.of(&model).clone();
        let names: Vec<String> = model.cases.iter().map(|c| c.case.name.clone()).collect();
        assert_eq!(c.cases, names);
        assert_eq!(*c.groups, crate::case_groups::groups(&model));
        let mut types: Vec<String> = model
            .line
            .sections
            .iter()
            .map(|s| s.cable.clone())
            .collect();
        types.sort();
        types.dedup();
        assert!(!types.is_empty());
        assert_eq!(c.types, types);
        // No route worked on: the route's lists are the line's.
        assert_eq!(c.route_types, types);
        let mut systems: Vec<String> = model.line.systems.iter().map(|s| s.id.clone()).collect();
        let circuits: Vec<String> = model.line.circuits.iter().map(|c| c.name.clone()).collect();
        assert_eq!(c.circuits, circuits);
        systems.push("none".into());
        assert_eq!(c.systems, systems);
        let kv = |zero: bool| {
            let mut v: Vec<f64> = ohl_clearance::case::voltages_of(&model.line)
                .unwrap()
                .into_iter()
                .filter_map(|p| p.map(|p| p.nominal_kv).or(zero.then_some(0.0)))
                .collect();
            v.sort_by(f64::total_cmp);
            v.dedup();
            v
        };
        assert_eq!((c.voltages, c.voltages_or_zero), (kv(false), kv(true)));
        // The line changed: made again, with the change.
        model.line.sections[0].cable = "a wire type of the test".into();
        model.line_version += 1;
        assert!(
            choices
                .of(&model)
                .types
                .contains(&"a wire type of the test".to_owned())
        );
        assert_eq!(made(), before + 2);
        // A case renamed (the line's version as it was): made again.
        model.cases[0].case.name = "renamed".into();
        assert_eq!(choices.of(&model).cases[0], "renamed");
        assert_eq!(made(), before + 3);
        // A route worked on: made again; the same again: kept.
        model.work_route = Some("a route of the test".into());
        assert!(choices.of(&model).route_types.is_empty());
        assert!(!choices.of(&model).types.is_empty());
        assert_eq!(made(), before + 4);
    }

    /// The tabs: Setup first; a new result opens Results; Run greyed with
    /// a reason opens Setup again.
    #[test]
    fn tabs_follow_the_runs() {
        let mut h = Harness::new_ui_state(
            |ui, s: &mut (u64, bool, Tab)| {
                s.2 = tabs(ui, "t", Some(4), s.0, s.1);
            },
            (0, false, Tab::Setup),
        );
        h.run();
        assert_eq!(h.state().2, Tab::Setup);
        h.get_by_label("Results (4)");
        h.state_mut().0 = 1;
        h.run();
        assert_eq!(h.state().2, Tab::Results);
        h.get_by_label("Setup").click();
        h.run();
        assert_eq!(h.state().2, Tab::Setup);
        h.get_by_label("Results (4)").click();
        h.run();
        assert_eq!(h.state().2, Tab::Results);
        h.state_mut().1 = true;
        h.run();
        assert_eq!(h.state().2, Tab::Setup);
    }

    #[test]
    fn csv_quotes_and_heads() {
        let t = ResultTable::new("csv", vec![Col::text("Span"), Col::num("Found [m]")]).rows(vec![
            Row::new(vec![Cell::text("a, b"), Cell::num(Some(1.0), 1)], true),
        ]);
        assert_eq!(t.csv_text(&[0]), "Span,Found [m]\n\"a, b\",1.0\n");
    }
}
