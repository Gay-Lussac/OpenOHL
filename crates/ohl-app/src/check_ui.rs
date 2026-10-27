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
//!   *4 Settings*.
//! - [`ResultTable`]: the verdict banner ([`verdict`]), a toolbar (*Only
//!   violations*, on by default; *Search*; *Group by*; *Export CSV…*),
//!   columns sorted by a click on their header, units in brackets in the
//!   headers, a row clicked returned to the window (it selects the object
//!   and draws the finding's weather case, [`crate::result_case`]).
//! - [`run_footer`]: Run, Stop (every check), the progress and the time
//!   left, "Before Run: …" when Run is grey, the scope in one line, then
//!   what the window adds (overrides, *Save in Performed checks*).
//! - [`Kept`]: a window's setup kept in the project, a project setting
//!   per project line (`app.<check>.<line>`), as the magnetic field's.

#![allow(dead_code)] // TEMP while windows move over

use std::collections::BTreeSet;
use std::time::Duration;

use eframe::egui;

use crate::check_help as help;
use crate::tick_list::{self, Item};

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

/// The tab window `id` shows (tests, and windows that need it before
/// drawing [`tabs`]).
pub fn current_tab(ctx: &egui::Context, id: &str) -> Tab {
    ctx.data(|d| d.get_temp::<TabState>(tab_key(id)))
        .map(|s| s.tab)
        .unwrap_or_default()
}

/// Shows tab `tab` of window `id` (a window that wants its results
/// shown, tests).
pub fn set_tab(ctx: &egui::Context, id: &str, tab: Tab) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<TabState>(tab_key(id)).tab = tab);
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
/// "3 Rules", "4 Settings"): its heading, with the user guide's button
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
            sections: None,
        }
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
    }
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
        }
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
        } = self;
        let mut changed = false;
        let mut summary: Vec<String> = Vec::new();
        ui.horizontal_wrapped(|ui| {
            if let Some(scope) = scope {
                changed |= circuits_menu(ui, id, line, scope);
                changed |= routes_menu(ui, id, line, scope);
                if sections {
                    changed |= sections_menu(ui, id, line, scope);
                }
                summary.push(scope_words(line, scope, sections));
            }
            if let Some(cases) = cases {
                let (c, words) = cases_menu(ui, id, cases);
                changed |= c;
                summary.push(words);
            }
        });
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

/// A tick menu of the scope: `title` on its button, `list` inside.
fn tick_menu(ui: &mut egui::Ui, id: (&str, &str), title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.push_id(id, |ui| {
        crate::menu::options_menu(ui, title, |ui| {
            ui.set_min_width(260.0);
            add(ui);
        });
    });
}

fn circuits_menu(ui: &mut egui::Ui, id: &str, line: &ohl_line::Line, scope: &mut Scope) -> bool {
    let names = circuits_of(line);
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
    changed
}

fn routes_menu(ui: &mut egui::Ui, id: &str, line: &ohl_line::Line, scope: &mut Scope) -> bool {
    if line.routes.is_empty() && line.alignments.is_empty() {
        return false;
    }
    let name = |id: &str, name: &str| {
        if name.is_empty() || name == id {
            id.to_owned()
        } else {
            format!("{name} ({id})")
        }
    };
    let mut items: Vec<Item> = Vec::new();
    for r in &line.routes {
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
        if !line.routes.iter().any(|r| r.id == a.route) {
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

fn sections_menu(ui: &mut egui::Ui, id: &str, line: &ohl_line::Line, scope: &mut Scope) -> bool {
    let items: Vec<Item> = line
        .sections
        .iter()
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
}

impl Cell {
    /// A text cell.
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            key: Key::Text(text.to_lowercase()),
            text,
            bad: false,
        }
    }

    /// A number with `digits` decimals (`None`: "-", sorted last).
    pub fn num(v: Option<f64>, digits: usize) -> Self {
        Self {
            text: crate::job::or_dash(v, digits),
            key: Key::Num(v.unwrap_or(f64::INFINITY)),
            bad: false,
        }
    }

    /// A cell shown as `text`, sorted by `v`.
    pub fn sorted(text: impl Into<String>, v: f64) -> Self {
        Self {
            text: text.into(),
            key: Key::Num(v),
            bad: false,
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
        }
    }

    /// Shown in the error colour.
    pub fn bad(mut self, bad: bool) -> Self {
        self.bad = bad;
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
enum Line {
    Group(String, usize),
    Row(usize),
}

/// The results of a check as one table (owner, 2026-10-27; built on the
/// ground windows' table, the best one): a toolbar (*Only violations*,
/// *Search*, *Group by*, *Export CSV…*), headers clicked to sort, rows
/// drawn only in view, a row clicked selected and returned.
pub struct ResultTable<'a> {
    id: &'a str,
    cols: Vec<Col>,
    rows: Vec<Row>,
    filter: bool,
    groups: Vec<usize>,
    csv: Option<&'a str>,
    /// Kept free under the table, px (the folds under it).
    below: f32,
}

/// The memory key of a table's view.
fn view_key(id: &str) -> egui::Id {
    egui::Id::new(("result-table-view", id))
}

/// Whether table `id` shows only the violations.
pub fn only_violations(ctx: &egui::Context, id: &str) -> bool {
    ctx.data(|d| d.get_temp::<View>(view_key(id)))
        .unwrap_or_default()
        .only
}

/// Sets *Only violations* of table `id` (tests).
pub fn set_only_violations(ctx: &egui::Context, id: &str, only: bool) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<View>(view_key(id)).only = only);
}

impl<'a> ResultTable<'a> {
    /// The table `id` of `cols`.
    pub fn new(id: &'a str, cols: Vec<Col>) -> Self {
        Self {
            id,
            cols,
            rows: Vec::new(),
            filter: true,
            groups: Vec::new(),
            csv: None,
            below: 0.0,
        }
    }

    /// Its rows.
    pub fn rows(mut self, rows: Vec<Row>) -> Self {
        self.rows = rows;
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

    /// `px` kept free under the table (for what the window shows below).
    pub fn below(mut self, px: f32) -> Self {
        self.below = px;
        self
    }

    /// The rows shown, in order (indices in the rows given), and the
    /// lines drawn.
    fn shown(&self, view: &View) -> (Vec<usize>, Vec<Line>) {
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
                let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
                for &k in &idx {
                    let t = text(k);
                    match groups.iter_mut().find(|(n, _)| *n == t) {
                        Some((_, v)) => v.push(k),
                        None => groups.push((t, vec![k])),
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
        let violations = self.rows.iter().filter(|r| r.violation).count();
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
        let (idx, lines) = self.shown(&view);
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
                crate::job::save_csv(name, &self.csv_text(&idx));
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
        let height = (ui.available_height() - self.below).max(80.0);
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
                                let add = |ui: &mut egui::Ui| {
                                    ui.add(egui::Label::new(t).sense(egui::Sense::click()))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(tip)
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

/// The footer of a window computed by a [`crate::job::Job`]: its
/// progress while it runs (Stop discards the run: the previous results
/// stay), else how long the last run took.
pub fn job_footer<T: Send + 'static>(
    ui: &mut egui::Ui,
    job: &mut crate::job::Job<T>,
    why_not: Option<String>,
    hover: &str,
    scope: Option<String>,
    extra: impl FnOnce(&mut egui::Ui),
) -> bool {
    let f = RunFooter {
        running: job.elapsed().map(Progress::elapsed),
        why_not,
        hover,
        scope,
        info: job.took.map(|t| format!("computed in {t:.1?}")),
    };
    match run_footer(ui, f, extra) {
        Action::Run => true,
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
