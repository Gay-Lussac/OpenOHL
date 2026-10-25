// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Load cases panel (owner, 2026-10-25; spec:
//! `docs/development/structure-load-cases-spec.md`, UI): the cases on the
//! left (name, the first line of the description), the case on the right
//! — its name, description and strength factor, the sentence of what it
//! does ([`ohl_structure_loads::describe`]), **1. Line forces** (a line
//! result the line sent, a factor per axis, at every point or some sets,
//! a tick per attachment point), **2. Loads on the structure itself**
//! (self weight, wind on the body, ice and additional weight, point
//! loads) and, only when the case has some, **Loads in the older form**
//! with Convert. Case buttons: new, duplicate, delete, Mirror wind.
//!
//! Every change is one edit of the structure file (one undo step): the
//! case as read, changed, written back part by part
//! ([`ohl_structure_edit::set_case_tables`]: the case's other keys and
//! comments stay).

use eframe::egui;
use ohl_core::units::{Density, Force, ForcePerLength, Length, Quantity, Stress};
use ohl_structure_edit::{self as edit, CaseText};
use ohl_structure_loads::{
    BodyWindLoad, Direction, IceLoad, LineForces, LoadCaseRecipe, OwnLoads, Pick, PointLoad,
    StructureLoadCases, Target, WindFrom, convert_older, describe, mirror_wind,
};

use crate::app::{StructureApp, error_label};
use crate::load_cases_tab::{cell, readable};

/// The result a new line forces row takes when the line sent none (to
/// be typed over).
pub const NO_RESULT: &str = "line result";

/// A change of the case shown: in words, the case changed, the keys of
/// its table written.
type Change = (String, LoadCaseRecipe, &'static [&'static str]);

/// The keys of a case's parts.
const PARTS: &[&str] = &["line", "structure"];
/// The parts and the older self weight (moved into the structure's).
const PARTS_AND_WEIGHT: &[&str] = &["line", "structure", "self_weight"];
/// What Convert writes.
const CONVERTED: &[&str] = &["line", "structure", "load", "self_weight", "line_case"];

/// A number typed: a plain number in `unit` (× `unit` to SI), or text with
/// a unit of `Q`.
fn amount<Q: Quantity>(typed: &str, unit: f64, what: &str) -> Result<f64, String> {
    let t = typed.trim();
    if let Ok(x) = t.parse::<f64>() {
        return if x.is_finite() {
            Ok(x * unit)
        } else {
            Err(format!("{what}: \"{t}\" is not a number"))
        };
    }
    t.parse::<Q>()
        .map(Quantity::si)
        .map_err(|e| format!("{what} \"{t}\": {e}"))
}

fn factor(typed: &str, what: &str) -> Result<f64, String> {
    typed
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{what}: \"{}\" is not a number", typed.trim()))
}

/// A number shown without trailing zeros.
fn shown(x: f64) -> String {
    let t = format!("{x:.4}");
    t.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Set numbers typed (`1; 2`, `1, 2`; empty: none).
fn sets(typed: &str) -> Result<Vec<u32>, String> {
    typed
        .split([';', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse()
                .map_err(|_| format!("\"{s}\" is not a set number"))
        })
        .collect()
}

fn sets_text(s: &[u32]) -> String {
    s.iter().map(u32::to_string).collect::<Vec<_>>().join("; ")
}

/// `all` or labels typed.
fn pick(typed: &str) -> Option<Pick> {
    let t = typed.trim();
    match t {
        "" => None,
        "all" => Some(Pick::All),
        _ => Some(Pick::These(
            t.split([';', ','])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        )),
    }
}

fn pick_text(p: &Option<Pick>) -> String {
    match p {
        None => String::new(),
        Some(Pick::All) => "all".into(),
        Some(Pick::These(v)) => v.join("; "),
    }
}

fn direction_words(d: Direction) -> &'static str {
    match d {
        Direction::Right => "to the right",
        Direction::Left => "to the left",
        Direction::Ahead => "ahead",
        Direction::Back => "back",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

const DIRECTIONS: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::Right,
    Direction::Left,
    Direction::Ahead,
    Direction::Back,
];

/// A copy of `name` not among `taken`: "X (copy)", "X (copy 2)", …
fn free_name(name: &str, taken: &[String], tag: &str) -> String {
    let first = format!("{name} ({tag})");
    if !taken.contains(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("{name} ({tag} {n})"))
        .find(|n| !taken.contains(n))
        .expect("a free name")
}

/// What the line sent, for the case editor.
struct LineInfo {
    /// The line results (names), in the line's order.
    results: Vec<String>,
    /// Per result, its weather case's values in words (hover).
    about: Vec<String>,
    /// The results with a wind pressure.
    windy: Vec<String>,
}

impl StructureApp {
    fn line_info(&self) -> LineInfo {
        let line = &self.loads.line;
        let mut results: Vec<String> = self
            .loads
            .info
            .as_ref()
            .map(|i| i.cases.clone())
            .unwrap_or_default();
        for v in &line.values {
            if !results.contains(&v.case) {
                results.push(v.case.clone());
            }
        }
        let about = results
            .iter()
            .map(|r| match line.structure_wind(r) {
                Some(w) if w.pressure > 0.0 => format!(
                    "{r}: its weather case's wind {} Pa, blowing {} (unfactored)",
                    shown(w.pressure),
                    if w.towards == Direction::Right {
                        "to the right"
                    } else {
                        "to the left"
                    }
                ),
                _ => format!("{r}: no wind (unfactored)"),
            })
            .collect();
        let windy = results
            .iter()
            .filter(|r| line.structure_wind(r).is_some_and(|w| w.pressure > 0.0))
            .cloned()
            .collect();
        LineInfo {
            results,
            about,
            windy,
        }
    }

    /// The Load cases panel: the case buttons, the list, the case.
    pub(crate) fn load_cases_window(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        let cases = edit::load_cases(f.doc());
        let parsed: Vec<LoadCaseRecipe> =
            self.loads.recipes().map(<[_]>::to_vec).unwrap_or_default();
        let read_error = self.loads.recipes().err().map(str::to_owned);
        let k = self.loads.recipe.min(cases.len().saturating_sub(1));
        self.loads.recipe = k;
        let current = parsed.get(k).cloned();
        let mirrorable = current.as_ref().is_some_and(|c| {
            let m = mirror_wind(c);
            m.line != c.line || m.structure != c.structure
        });
        let (mut add, mut duplicate, mut delete, mut mirror, mut import, mut export) =
            (false, false, false, false, false, false);
        ui.horizontal_wrapped(|ui| {
            add = ui
                .button("+ Case")
                .on_hover_text("A new load case, empty")
                .clicked();
            duplicate = ui
                .add_enabled(current.is_some(), egui::Button::new("Duplicate"))
                .on_hover_text("A copy of this case, to change")
                .on_disabled_hover_text("No case, or the cases do not read")
                .clicked();
            delete = ui
                .add_enabled(!cases.is_empty(), egui::Button::new("🗑 Case"))
                .on_hover_text("Delete this load case (Undo takes it back)")
                .clicked();
            mirror = ui
                .add_enabled(mirrorable, egui::Button::new("Mirror wind"))
                .on_hover_text(
                    "A copy of this case with the wind from the other side: left and right \
                     swapped in its name, its line results and its wind on the body (ahead and \
                     back stay)",
                )
                .on_disabled_hover_text(
                    "The case takes no line result or body wind with a side (or does not read)",
                )
                .clicked();
            import = ui
                .button("Import recipes…")
                .on_hover_text(
                    "Load cases from a structure load-case file (.ohlslc) or CSV: a case with \
                     the name of one here replaces it, the others are added",
                )
                .clicked();
            export = ui
                .button("Export recipes…")
                .on_hover_text(
                    "The load cases as a structure load-case file (.ohlslc), or CSV (the older \
                     form only)",
                )
                .clicked();
        });
        if add {
            self.add_case();
            return;
        }
        if import {
            self.import_recipes_dialog();
            return;
        }
        if export {
            self.export_recipes_dialog();
            return;
        }
        if delete && !cases.is_empty() {
            let name = cases[k].get("name").to_owned();
            if let Some(f) = &mut self.file {
                f.edit(&format!("delete load case {name}"), |doc, _| {
                    edit::remove_load_case(doc, k);
                });
            }
            self.loads.recipe = k.saturating_sub(1);
            self.message = Some((format!("deleted load case \"{name}\""), false));
            return;
        }
        if let (true, Some(c)) = (duplicate, &current) {
            let taken: Vec<String> = parsed.iter().map(|c| c.name.clone()).collect();
            let mut copy = c.clone();
            copy.name = free_name(&c.name, &taken, "copy");
            self.add_whole_case(copy, "duplicate load case");
            return;
        }
        if let (true, Some(c)) = (mirror, &current) {
            self.mirror_case(c, &parsed);
            return;
        }
        if let Some(e) = &read_error {
            error_label(ui, format!("the load cases do not read: {e}"));
        }
        if cases.is_empty() {
            self.no_case_yet(ui);
            return;
        }
        let mut pick = k;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(210.0);
                egui::ScrollArea::vertical()
                    .id_salt("case-list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (i, c) in cases.iter().enumerate() {
                            let first = c.get("description").lines().next().unwrap_or("");
                            let mut text = egui::text::LayoutJob::default();
                            text.append(
                                c.get("name"),
                                0.0,
                                egui::TextFormat {
                                    color: ui.visuals().strong_text_color(),
                                    ..Default::default()
                                },
                            );
                            if !first.is_empty() {
                                text.append(
                                    &format!("\n{first}"),
                                    0.0,
                                    egui::TextFormat {
                                        color: ui.visuals().weak_text_color(),
                                        ..Default::default()
                                    },
                                );
                            }
                            if ui.selectable_label(i == k, text).clicked() {
                                pick = i;
                            }
                        }
                    });
            });
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::both()
                    .id_salt("case")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.case_ui(ui, k, &cases[k], current.as_ref());
                    });
            });
        });
        if pick != k {
            self.loads.recipe = pick;
            self.loads.shown = Some(cases[pick].get("name").to_owned());
        }
    }

    /// No case yet: said, with the line's results to take.
    fn no_case_yet(&mut self, ui: &mut egui::Ui) {
        let line = self.loads.line_cases();
        ui.label(
            "This structure has no load cases of its own yet. The line's forces are in the load \
             table without any; a load case takes them with its own factors (1. line forces) \
             and adds the loads on the structure itself (2. self weight, wind on the body, ice, \
             point loads). + Case adds one, Import recipes… takes them from another structure.",
        );
        if ui
            .add_enabled(
                !line.is_empty(),
                egui::Button::new("Take the line's load cases as they are"),
            )
            .on_hover_text(
                "One load case per line result: its forces at every point, every factor 1, and \
                 on a structure with members the wind on the body at its weather case's pressure \
                 (one undo step)",
            )
            .on_disabled_hover_text("The line gives no line result (see above)")
            .clicked()
        {
            self.take_line_cases();
        }
    }

    /// Writes case `case` at the end of the file (a duplicate, a mirror):
    /// one edit; it is shown.
    fn add_whole_case(&mut self, case: LoadCaseRecipe, what: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let name = case.name.clone();
        let text = StructureLoadCases::new("", vec![case]).to_toml();
        match f.edit(what, |doc, _| edit::merge_load_cases(doc, &text)) {
            Ok(_) => {
                self.loads.recipe = edit::load_cases(f.doc())
                    .iter()
                    .position(|c| c.get("name") == name)
                    .unwrap_or(0);
                self.loads.shown = Some(name.clone());
                self.message = Some((format!("added load case \"{name}\""), false));
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// Mirror wind: case `case` copied with the wind from the other side
    /// (a case of that name there already is not replaced: said).
    pub fn mirror_case(&mut self, case: &LoadCaseRecipe, all: &[LoadCaseRecipe]) {
        let m = mirror_wind(case);
        if all.iter().any(|c| c.name == m.name) {
            self.message = Some((
                format!(
                    "a load case \"{}\" is there already: rename or delete it first",
                    m.name
                ),
                true,
            ));
            return;
        }
        let older = if case.loads.is_empty() {
            ""
        } else {
            " (its loads in the older form copied as they are)"
        };
        self.add_whole_case(m.clone(), &format!("mirror wind of {}", case.name));
        if self.message.as_ref().is_some_and(|m| !m.1) {
            self.message = Some((
                format!("\"{}\": the wind from the other side{older}", m.name),
                false,
            ));
        }
    }

    /// Applies a change of case `k`: one edit (`what`).
    fn change_case(&mut self, k: usize, (what, case, keys): Change) {
        let Some(f) = &mut self.file else {
            return;
        };
        let text = case.to_toml();
        match f.edit(&format!("{what} of {}", case.name), |doc, _| {
            edit::set_case_tables(doc, k, &text, keys)
        }) {
            Ok(()) => {
                // Read back: say what does not read.
                self.message = match StructureLoadCases::from_structure(f.structure()) {
                    Ok(_) => None,
                    Err(e) => Some((e, true)),
                };
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// The case shown: its fields, sentence and parts.
    fn case_ui(
        &mut self,
        ui: &mut egui::Ui,
        k: usize,
        text: &CaseText,
        case: Option<&LoadCaseRecipe>,
    ) {
        let name = text.get("name").to_owned();
        let mut written: Option<(String, Result<(), String>)> = None;
        // Name and strength factor.
        let mut fields: Vec<(&str, String)> = Vec::new();
        egui::Grid::new("case-head").num_columns(4).show(ui, |ui| {
            let buffers = &mut self.loads.buffers;
            ui.label("Name")
                .on_hover_text("Unique; model is the name of the [[load]] tables' case");
            if let Some(t) = cell(ui, buffers, format!("case/{k}/name"), &name, 200.0, "") {
                fields.push(("name", t));
            }
            ui.label("Strength factor")
                .on_hover_text("Strength (resistance) factor of the checks (default 1)");
            if let Some(t) = cell(
                ui,
                buffers,
                format!("case/{k}/strength"),
                text.get("strength"),
                60.0,
                "1",
            ) {
                fields.push(("strength", t));
            }
            ui.end_row();
        });
        ui.label("Description")
            .on_hover_text("What it stands for (several lines)");
        {
            let current = text.get("description");
            let e = self
                .loads
                .buffers
                .entry(format!("case/{k}/description"))
                .or_insert_with(|| (current.to_owned(), current.to_owned()));
            if e.0 != current {
                *e = (current.to_owned(), current.to_owned());
            }
            let r = ui.add(
                egui::TextEdit::multiline(&mut e.1)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if r.lost_focus() && e.1 != e.0 {
                fields.push(("description", e.1.clone()));
            }
        }
        if let Some(f) = &mut self.file {
            for (field, typed) in fields {
                let r = f.edit(&format!("{field} of {name}"), |doc, _| {
                    edit::set_load_case_field(doc, k, field, &typed)
                });
                if field == "name" && r.is_ok() {
                    self.loads.shown = Some(typed.trim().to_owned());
                }
                written = Some((field.to_owned(), r));
            }
        }
        match written {
            Some((field, Err(e))) => self.message = Some((format!("{field}: {e}"), true)),
            Some((_, Ok(()))) => self.message = None,
            None => {}
        }
        let Some(case) = case else {
            ui.label("Its parts are shown when the load cases read (see above).");
            self.older_ui(ui, k, text, None);
            return;
        };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(describe(case)).italics())
                .on_hover_text("What the case does, in words (made from it)");
        });
        let info = self.line_info();
        let targets = self
            .file
            .as_ref()
            .map(|f| ohl_structure_loads::targets(f.structure(), &self.loads.line))
            .unwrap_or_default();
        let mut change: Option<Change> = None;
        egui::CollapsingHeader::new("1. Line forces")
            .id_salt(("line-forces", k))
            .default_open(!case.line.is_empty() || case.loads.is_empty())
            .show(ui, |ui| {
                change = change
                    .take()
                    .or(self.line_forces_ui(ui, k, case, &info, &targets));
            });
        egui::CollapsingHeader::new("2. Loads on the structure itself")
            .id_salt(("own-loads", k))
            .default_open(case.structure.is_some() || case.loads.is_empty())
            .show(ui, |ui| {
                change = change.take().or(self.own_loads_ui(ui, k, case, &info));
            });
        if let Some(c) = change {
            self.change_case(k, c);
            return;
        }
        self.older_ui(ui, k, text, Some(case));
    }

    fn line_forces_ui(
        &mut self,
        ui: &mut egui::Ui,
        k: usize,
        case: &LoadCaseRecipe,
        info: &LineInfo,
        targets: &[Target],
    ) -> Option<Change> {
        ui.label(
            egui::RichText::new(
                "A line result's forces at each attachment point (unfactored, as the line sends \
                 them) × the factor of each axis: rows \"line\" of the load table (line 1, …), \
                 one per point.",
            )
            .weak(),
        );
        if info.results.is_empty() {
            ui.label(
                "No line results here: they come from the line (OHL-Design › Criteria › Loads \
                 for structures), with this structure opened from a placed structure. A result \
                 can be typed by its name.",
            );
        }
        let mut change: Option<Change> = None;
        let mut next = case.clone();
        let buffers = &mut self.loads.buffers;
        egui::Grid::new(("line-forces-grid", k))
            .striped(true)
            .show(ui, |ui| {
                for h in [
                    (
                        "Line result",
                        "The line result (a weather case; a wind case twice)",
                    ),
                    ("Vertical", "Factor on the vertical forces (down)"),
                    (
                        "Transverse",
                        "Factor on the transverse forces (right of the line)",
                    ),
                    ("Longitudinal", "Factor on the longitudinal forces (ahead)"),
                    (
                        "At",
                        "Every point, or the points of these sets (numbers, ;)",
                    ),
                    (
                        "Points",
                        "A tick per attachment point: its line forces on or off",
                    ),
                    ("", ""),
                ] {
                    ui.strong(h.0).on_hover_text(h.1);
                }
                ui.end_row();
                for (i, l) in case.line.iter().enumerate() {
                    // The result: chosen among the line's, or typed.
                    if info.results.is_empty() {
                        if let Some(t) = cell(
                            ui,
                            buffers,
                            format!("line/{k}/{i}/result"),
                            &l.result,
                            220.0,
                            "",
                        ) {
                            next.line[i].result = t.trim().to_owned();
                            change = Some(("line result".into(), next.clone(), PARTS));
                        }
                    } else {
                        let mut chosen = l.result.clone();
                        egui::ComboBox::from_id_salt(("line-result", k, i))
                            .selected_text(&l.result)
                            .width(230.0)
                            .show_ui(ui, |ui| {
                                for (r, about) in info.results.iter().zip(&info.about) {
                                    ui.selectable_value(&mut chosen, r.clone(), r)
                                        .on_hover_text(about);
                                }
                            });
                        if chosen != l.result {
                            next.line[i].result = chosen;
                            change = Some(("line result".into(), next.clone(), PARTS));
                        }
                    }
                    for (axis, value) in [
                        ("vertical", l.vertical),
                        ("transverse", l.transverse),
                        ("longitudinal", l.longitudinal),
                    ] {
                        if let Some(t) = cell(
                            ui,
                            buffers,
                            format!("line/{k}/{i}/{axis}"),
                            &shown(value),
                            50.0,
                            "1",
                        ) {
                            match factor(&t, axis) {
                                Ok(x) => {
                                    let r = &mut next.line[i];
                                    match axis {
                                        "vertical" => r.vertical = x,
                                        "transverse" => r.transverse = x,
                                        _ => r.longitudinal = x,
                                    }
                                    change = Some((format!("{axis} factor"), next.clone(), PARTS));
                                }
                                Err(e) => self.message = Some((e, true)),
                            }
                        }
                    }
                    if let Some(t) = cell(
                        ui,
                        buffers,
                        format!("line/{k}/{i}/sets"),
                        &sets_text(&l.sets),
                        80.0,
                        "every point",
                    ) {
                        match sets(&t) {
                            Ok(s) => {
                                next.line[i].sets = s;
                                change = Some(("line forces' sets".into(), next.clone(), PARTS));
                            }
                            Err(e) => self.message = Some((format!("sets: {e}"), true)),
                        }
                    }
                    let mut toggled: Option<String> = None;
                    crate::layout::options_menu(
                        ui,
                        format!("Points ({} off)", l.off.len()),
                        |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Unticked: the point's line forces are left out (works on \
                                     the line with a conductor removed)",
                                )
                                .weak(),
                            );
                            for t in targets {
                                let mut on = !l.off.contains(&t.label);
                                let mut words = t.label.clone();
                                if !t.sets.is_empty() {
                                    words.push_str(&format!(" · set {}", sets_text(&t.sets)));
                                }
                                if !t.phases.is_empty() {
                                    words.push_str(&format!(" · phase {}", t.phases.join(", ")));
                                }
                                if ui.checkbox(&mut on, words).changed() {
                                    toggled = Some(t.label.clone());
                                }
                            }
                        },
                    );
                    if let Some(label) = toggled {
                        let off = &mut next.line[i].off;
                        let what = if let Some(p) = off.iter().position(|o| *o == label) {
                            off.remove(p);
                            format!("{label}'s line forces on")
                        } else {
                            off.push(label.clone());
                            format!("{label}'s line forces off")
                        };
                        change = Some((what, next.clone(), PARTS));
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Remove these line forces")
                        .clicked()
                    {
                        next.line.remove(i);
                        change = Some(("remove line forces".into(), next.clone(), PARTS));
                    }
                    ui.end_row();
                    if change.is_some() {
                        break;
                    }
                }
            });
        if ui
            .button("Add line forces")
            .on_hover_text(
                "A line result's forces at every point, every factor 1 (the next result the case \
                 does not take yet)",
            )
            .clicked()
        {
            let result = info
                .results
                .iter()
                .find(|r| !case.line.iter().any(|l| &l.result == *r))
                .or(info.results.first())
                .cloned()
                .unwrap_or_else(|| NO_RESULT.to_owned());
            let mut next = case.clone();
            next.line.push(LineForces::new(&result));
            change = Some(("add line forces".into(), next, PARTS));
        }
        change
    }

    fn own_loads_ui(
        &mut self,
        ui: &mut egui::Ui,
        k: usize,
        case: &LoadCaseRecipe,
        info: &LineInfo,
    ) -> Option<Change> {
        let mut next = case.clone();
        let own = case.structure.clone().unwrap_or_default();
        let mut change: Option<Change> = None;
        let with = |next: &mut LoadCaseRecipe, f: &mut dyn FnMut(&mut OwnLoads)| {
            let s = next.structure.get_or_insert_with(OwnLoads::default);
            f(s);
        };
        let buffers = &mut self.loads.buffers;
        let note = |ui: &mut egui::Ui, text: &str| {
            ui.label(egui::RichText::new(text).weak());
        };

        // Self weight.
        ui.strong("Self weight");
        note(
            ui,
            "Each member's own weight × this factor, put on the members by the solve (not rows \
             of the load table).",
        );
        ui.horizontal(|ui| {
            ui.label("Factor");
            if let Some(t) = cell(
                ui,
                buffers,
                format!("own/{k}/self_weight"),
                &shown(case.self_weight_factor()),
                60.0,
                "1",
            ) {
                match factor(&t, "self weight") {
                    Ok(x) => {
                        with(&mut next, &mut |s| s.self_weight = Some(x));
                        next.self_weight = 1.0;
                        change = Some(("self weight".into(), next.clone(), PARTS_AND_WEIGHT));
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            if own.self_weight.is_none() && case.self_weight != 1.0 {
                ui.label(egui::RichText::new("(the older form's)").weak());
            }
        });
        ui.add_space(4.0);

        // Wind on the body.
        ui.strong("Wind on the body");
        note(
            ui,
            "The member wind of the line's own structure checks (faces, solidity, shielding: the \
             cross-section library's wind factors) for this direction, pressure × factor: rows \
             \"wind\" (wind 1, …), one per member.",
        );
        egui::Grid::new(("wind-grid", k)).show(ui, |ui| {
            for (i, w) in own.wind.iter().enumerate() {
                let mut from = w.from;
                egui::ComboBox::from_id_salt(("wind-from", k, i))
                    .selected_text(w.from.words())
                    .show_ui(ui, |ui| {
                        for d in WindFrom::ALL {
                            ui.selectable_value(&mut from, d, d.words());
                        }
                    });
                if from != w.from {
                    with(&mut next, &mut |s| s.wind[i].from = from);
                    change = Some(("wind direction".into(), next.clone(), PARTS));
                }
                let own_text = "its own pressure";
                let mut of = w.pressure_of.clone().unwrap_or_default();
                egui::ComboBox::from_id_salt(("wind-of", k, i))
                    .selected_text(match &w.pressure_of {
                        Some(r) => format!("the pressure of {r}"),
                        None => own_text.to_owned(),
                    })
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut of, String::new(), own_text);
                        for r in &info.windy {
                            ui.selectable_value(&mut of, r.clone(), format!("the pressure of {r}"));
                        }
                    });
                if of != w.pressure_of.clone().unwrap_or_default() {
                    with(&mut next, &mut |s| {
                        let w = &mut s.wind[i];
                        if of.is_empty() {
                            w.pressure_of = None;
                            w.pressure = Some(w.pressure.unwrap_or(0.0));
                        } else {
                            w.pressure_of = Some(of.clone());
                            w.pressure = None;
                        }
                    });
                    change = Some(("wind pressure".into(), next.clone(), PARTS));
                }
                if w.pressure_of.is_none() {
                    if let Some(t) = cell(
                        ui,
                        buffers,
                        format!("wind/{k}/{i}/pressure"),
                        &w.pressure
                            .map_or(String::new(), |p| format!("{} Pa", shown(p))),
                        80.0,
                        "Pa",
                    ) {
                        match amount::<Stress>(&t, 1.0, "pressure") {
                            Ok(p) => {
                                with(&mut next, &mut |s| s.wind[i].pressure = Some(p));
                                change = Some(("wind pressure".into(), next.clone(), PARTS));
                            }
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                } else {
                    ui.label("");
                }
                ui.label("×");
                if let Some(t) = cell(
                    ui,
                    buffers,
                    format!("wind/{k}/{i}/factor"),
                    &shown(w.factor),
                    50.0,
                    "1",
                ) {
                    match factor(&t, "factor") {
                        Ok(x) => {
                            with(&mut next, &mut |s| s.wind[i].factor = x);
                            change = Some(("wind factor".into(), next.clone(), PARTS));
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("Remove this wind")
                    .clicked()
                {
                    with(&mut next, &mut |s| {
                        s.wind.remove(i);
                    });
                    change = Some(("remove a wind on the body".into(), next.clone(), PARTS));
                }
                ui.end_row();
                if change.is_some() {
                    break;
                }
            }
        });
        if ui
            .button("Add wind")
            .on_hover_text(
                "A wind on the body from the left, at the pressure of the first line result with \
                 wind (else 0 Pa, to type), factor 1",
            )
            .clicked()
        {
            let of = info.windy.first().cloned();
            with(&mut next, &mut |s| {
                s.wind.push(BodyWindLoad {
                    from: WindFrom::Left,
                    pressure: if of.is_some() { None } else { Some(0.0) },
                    pressure_of: of.clone(),
                    factor: 1.0,
                });
            });
            change = Some(("add a wind on the body".into(), next.clone(), PARTS));
        }
        ui.add_space(4.0);

        // Ice and additional weight.
        ui.strong("Ice and additional weight");
        note(
            ui,
            "A radial thickness of a density all round each member's outline (its full \
             perimeter, an angle's inside faces too: conservative), or a load per metre, × \
             factor, down: rows \"ice\" (ice 1, …), one per member. With a wind on the body in \
             the case, iced members are 2 t wider for it (conservative).",
        );
        egui::Grid::new(("ice-grid", k)).show(ui, |ui| {
            for (i, ice) in own.ice.iter().enumerate() {
                let by_thickness = ice.load.is_none();
                let mut kind = by_thickness;
                egui::ComboBox::from_id_salt(("ice-kind", k, i))
                    .selected_text(if by_thickness {
                        "thickness and density"
                    } else {
                        "load per metre"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut kind, true, "thickness and density");
                        ui.selectable_value(&mut kind, false, "load per metre");
                    });
                if kind != by_thickness {
                    with(&mut next, &mut |s| {
                        let ice = &mut s.ice[i];
                        if kind {
                            ice.load = None;
                            ice.thickness = Some(0.01);
                            ice.density = Some(1.0);
                        } else {
                            ice.thickness = None;
                            ice.density = None;
                            ice.load = Some(0.0);
                        }
                    });
                    change = Some(("ice kind".into(), next.clone(), PARTS));
                }
                if by_thickness {
                    if let Some(t) = cell(
                        ui,
                        buffers,
                        format!("ice/{k}/{i}/thickness"),
                        &ice.thickness
                            .map_or(String::new(), |t| format!("{} mm", shown(t * 1000.0))),
                        70.0,
                        "mm",
                    ) {
                        match amount::<Length>(&t, 1e-3, "thickness") {
                            Ok(x) => {
                                with(&mut next, &mut |s| s.ice[i].thickness = Some(x));
                                change = Some(("ice thickness".into(), next.clone(), PARTS));
                            }
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                    if let Some(t) = cell(
                        ui,
                        buffers,
                        format!("ice/{k}/{i}/density"),
                        &ice.density
                            .map_or(String::new(), |r| format!("{} kg/m³", shown(r))),
                        90.0,
                        "kg/m³",
                    ) {
                        match amount::<Density>(&t, 1.0, "density") {
                            Ok(x) => {
                                with(&mut next, &mut |s| s.ice[i].density = Some(x));
                                change = Some(("ice density".into(), next.clone(), PARTS));
                            }
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                } else {
                    if let Some(t) = cell(
                        ui,
                        buffers,
                        format!("ice/{k}/{i}/load"),
                        &ice.load
                            .map_or(String::new(), |w| format!("{} N/m", shown(w))),
                        80.0,
                        "N/m",
                    ) {
                        match amount::<ForcePerLength>(&t, 1.0, "load") {
                            Ok(x) => {
                                with(&mut next, &mut |s| s.ice[i].load = Some(x));
                                change = Some(("weight per metre".into(), next.clone(), PARTS));
                            }
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                    ui.label("");
                }
                ui.label("on");
                if let Some(t) = cell(
                    ui,
                    buffers,
                    format!("ice/{k}/{i}/members"),
                    &pick_text(&Some(ice.members.clone())),
                    90.0,
                    "all",
                ) {
                    with(&mut next, &mut |s| {
                        s.ice[i].members = pick(&t).unwrap_or(Pick::All);
                    });
                    change = Some(("ice members".into(), next.clone(), PARTS));
                }
                ui.label("×");
                if let Some(t) = cell(
                    ui,
                    buffers,
                    format!("ice/{k}/{i}/factor"),
                    &shown(ice.factor),
                    50.0,
                    "1",
                ) {
                    match factor(&t, "factor") {
                        Ok(x) => {
                            with(&mut next, &mut |s| s.ice[i].factor = x);
                            change = Some(("ice factor".into(), next.clone(), PARTS));
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("Remove this weight")
                    .clicked()
                {
                    with(&mut next, &mut |s| {
                        s.ice.remove(i);
                    });
                    change = Some(("remove ice".into(), next.clone(), PARTS));
                }
                ui.end_row();
                if change.is_some() {
                    break;
                }
            }
        });
        if ui
            .button("Add ice")
            .on_hover_text("Ice on every member: a thickness and a density to type, factor 1")
            .clicked()
        {
            with(&mut next, &mut |s| {
                s.ice.push(IceLoad {
                    thickness: Some(0.01),
                    density: Some(1.0),
                    load: None,
                    members: Pick::All,
                    factor: 1.0,
                });
            });
            change = Some(("add ice".into(), next.clone(), PARTS));
        }
        ui.add_space(4.0);

        // Point loads.
        ui.strong("Point loads");
        note(
            ui,
            "A force of the case at points (all, labels, or the points of sets): rows \"point\" \
             (point 1, …), one per point.",
        );
        egui::Grid::new(("point-grid", k)).show(ui, |ui| {
            for (i, p) in own.point.iter().enumerate() {
                if let Some(t) = cell(
                    ui,
                    buffers,
                    format!("point/{k}/{i}/value"),
                    &format!("{} kN", shown(p.value / 1000.0)),
                    70.0,
                    "kN",
                ) {
                    match amount::<Force>(&t, 1000.0, "value") {
                        Ok(x) => {
                            with(&mut next, &mut |s| s.point[i].value = x);
                            change = Some(("point load".into(), next.clone(), PARTS));
                        }
                        Err(e) => self.message = Some((e, true)),
                    }
                }
                let mut d = p.direction;
                egui::ComboBox::from_id_salt(("point-direction", k, i))
                    .selected_text(direction_words(p.direction))
                    .show_ui(ui, |ui| {
                        for x in DIRECTIONS {
                            ui.selectable_value(&mut d, x, direction_words(x));
                        }
                    });
                if d != p.direction {
                    with(&mut next, &mut |s| s.point[i].direction = d);
                    change = Some(("point load direction".into(), next.clone(), PARTS));
                }
                ui.label("at");
                if let Some(t) = cell(
                    ui,
                    buffers,
                    format!("point/{k}/{i}/points"),
                    &pick_text(&p.points),
                    90.0,
                    "all / labels",
                ) {
                    with(&mut next, &mut |s| s.point[i].points = pick(&t));
                    change = Some(("point load points".into(), next.clone(), PARTS));
                }
                ui.label("sets");
                if let Some(t) = cell(
                    ui,
                    buffers,
                    format!("point/{k}/{i}/sets"),
                    &sets_text(&p.sets),
                    60.0,
                    "",
                ) {
                    match sets(&t) {
                        Ok(v) => {
                            with(&mut next, &mut |s| s.point[i].sets = v.clone());
                            change = Some(("point load sets".into(), next.clone(), PARTS));
                        }
                        Err(e) => self.message = Some((format!("sets: {e}"), true)),
                    }
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("Remove this point load")
                    .clicked()
                {
                    with(&mut next, &mut |s| {
                        s.point.remove(i);
                    });
                    change = Some(("remove a point load".into(), next.clone(), PARTS));
                }
                ui.end_row();
                if change.is_some() {
                    break;
                }
            }
        });
        if ui
            .button("Add point load")
            .on_hover_text("1 kN down at every point, to change")
            .clicked()
        {
            with(&mut next, &mut |s| {
                s.point.push(PointLoad {
                    value: 1000.0,
                    direction: Direction::Down,
                    points: Some(Pick::All),
                    sets: Vec::new(),
                    phases: Vec::new(),
                    note: String::new(),
                });
            });
            change = Some(("add a point load".into(), next.clone(), PARTS));
        }
        change
    }

    /// Loads in the older form, when the case has some: its older fields,
    /// its loads, Convert.
    fn older_ui(
        &mut self,
        ui: &mut egui::Ui,
        k: usize,
        text: &CaseText,
        case: Option<&LoadCaseRecipe>,
    ) {
        let has = !text.loads.is_empty()
            || !text.get("line_case").is_empty()
            || !text.get("self_weight").is_empty();
        if !has {
            return;
        }
        let mut convert = false;
        egui::CollapsingHeader::new("Loads in the older form")
            .id_salt(("older", k))
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "Each load: what (a value the line gives, or one of its own), how, \
                             where — evaluated as before.",
                        )
                        .weak(),
                    );
                    convert = ui
                        .add_enabled(case.is_some(), egui::Button::new("Convert"))
                        .on_hover_text(
                            "The older loads into the parts where that is exact (the line's \
                             unfactored values of one line result at the same points, a value \
                             at points, the self weight); the rest stays here, said (one undo \
                             step)",
                        )
                        .clicked();
                });
                self.older_loads_grid(ui, k, text);
            });
        if let (true, Some(c)) = (convert, case) {
            let info = self.line_info();
            let done = convert_older(c, &info.results);
            let summary = match (done.done.is_empty(), done.kept.is_empty()) {
                (true, _) => format!("nothing converted: {}", done.kept.join("; ")),
                (false, true) => format!("converted: {}", done.done.join("; ")),
                (false, false) => format!(
                    "converted: {} — kept in the older form: {}",
                    done.done.join("; "),
                    done.kept.join("; ")
                ),
            };
            let nothing = done.done.is_empty();
            if !nothing {
                self.change_case(k, ("convert".into(), done.case, CONVERTED));
            }
            if self.message.as_ref().is_none_or(|m| !m.1) {
                self.message = Some((summary, nothing));
            }
        }
    }

    /// The older loads' grid: the case's older fields, one row per load,
    /// + Load.
    fn older_loads_grid(&mut self, ui: &mut egui::Ui, k: usize, case: &CaseText) {
        let Some(f) = &mut self.file else {
            return;
        };
        let buffers = &mut self.loads.buffers;
        let mut written: Option<(String, Result<(), String>)> = None;
        ui.horizontal_wrapped(|ui| {
            for (field, tip) in [
                (
                    "line_case",
                    "The line's case (result) whose values the older loads take",
                ),
                (
                    "self_weight",
                    "The older factor on the structure's own weight (the structure's self \
                     weight, when given, is the case's)",
                ),
            ] {
                ui.label(readable(field))
                    .on_hover_text(format!("{tip} (in the file: {field})"));
                if let Some(typed) = cell(
                    ui,
                    buffers,
                    format!("case/{k}/{field}"),
                    case.get(field),
                    if field == "line_case" { 160.0 } else { 50.0 },
                    "",
                ) {
                    let r = f.edit(&format!("{} of {}", field, case.get("name")), |doc, _| {
                        edit::set_load_case_field(doc, k, field, &typed)
                    });
                    written = Some((field.to_owned(), r));
                }
            }
            if ui
                .button("+ Load")
                .on_hover_text(
                    "A new load in the older form, to change: with the line's values, the \
                     line's vertical load at every point; opened on its own, 1 kN down at every \
                     point",
                )
                .clicked()
            {
                let name = case.get("name").to_owned();
                let from_line = self.line_values.is_some();
                match f.edit(&format!("add a load to {name}"), |doc, _| {
                    edit::add_load(doc, k, from_line)
                }) {
                    Ok(n) => self.message = Some((format!("{name}: load {} added", n + 1), false)),
                    Err(e) => self.message = Some((e, true)),
                }
            }
        });
        crate::load_cases_tab::older_loads_table(ui, f, buffers, k, case, &mut written);
        match written {
            Some((field, Err(e))) => self.message = Some((format!("{field}: {e}"), true)),
            Some((_, Ok(()))) => self.message = None,
            None => {}
        }
    }
}
