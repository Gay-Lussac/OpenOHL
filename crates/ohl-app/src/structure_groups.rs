// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line › Structures › **Load cases per group…** (owner, 2026-11-15: "most
//! of the features here are doubled with simply defining load cases and
//! then have them attributed in groups of pylons … make them simpler and
//! more understandable"): the structure groups of the line
//! ([`ohl_line::StructureGroup`]) and the load cases each gives its
//! structures.
//!
//! The window follows the check pages' guidelines (numbered sections, the
//! explanations behind a (?), a *user guide ↗* button per section opening
//! its own page, [`GUIDE`]): 1 How it works (with the line's summary),
//! 2 Groups (the list: add, delete), then for the group picked 3 Name and
//! description, 4 Structures picked (from the selection in the Structures
//! table or a view: *Add the N selected*; per position, the first way),
//! 5 Rules (optional: types, line angle, station on an alignment, dead end
//! — all must hold, an automatic helper), 6 Load cases (the
//! library's, ticked) and 7 Who is in (the structures it holds now, why,
//! and the cases each takes; the structures in no group).
//!
//! A load case's own list (2026-11-15, the Load cases form's *8 Applies
//! to*) is a row *case (own list)*; picked, its picked and excluded
//! structures and who takes the case, changed with the form's command
//! ([`crate::load_case_sets::give_command`]).
//!
//! Every change is one command of the session
//! ([`ohl_session::Session::structure_groups_command`], one undo).

use std::collections::HashMap;

use eframe::egui;
use ohl_line::{CaseSet, CaseSets, GroupRules, StructureGroup};
use ohl_session::Command;

use crate::check_help as help;
use crate::model::Model;

/// The window's own page of the user guide.
pub const GUIDE: &str = "user-guide/load-cases-per-group";

/// The sections of [`GUIDE`] the window's **user guide ↗** buttons open
/// (each a heading of the page: tested).
pub mod anchors {
    /// *1 How it works*.
    pub const HOW: &str = "1-how-it-works";
    /// *2 Groups*.
    pub const GROUPS: &str = "2-groups";
    /// *3 Name and description*.
    pub const NAME: &str = "3-name-and-description";
    /// *4 Structures picked*.
    pub const PICKED: &str = "4-structures-picked";
    /// *5 Rules (optional)*.
    pub const RULES: &str = "5-rules-optional";
    /// *6 Load cases*.
    pub const CASES: &str = "6-load-cases";
    /// *7 Who is in*.
    pub const WHO: &str = "7-who-is-in";
}

/// The page and section a **user guide ↗** button opens.
pub fn guide(anchor: &'static str) -> (&'static str, &'static str) {
    (GUIDE, anchor)
}

/// The window's title.
pub const TITLE: &str = "Load cases per group";

/// *1 How it works*: the short line, the longer text behind the (?).
const HOW_HELP: (&str, &str) = (
    "A structure takes the load cases of every group it is in.",
    "Make the load cases once (Line › Structures › Load cases…), then say here which \
     structures take which: a group holds structures — picked by hand, or by rules (types, line \
     angle, station, dead end) that follow the line as it is edited — and ticks the load cases \
     its structures take. A structure in two groups takes the cases of both (more cases: the \
     safe side). A structure in no group takes every case of the library. A group ticking no \
     case gives its structures none: no loads, no structure check, said wherever they are \
     listed. Picking is per position: two structures of the same type can take different cases; \
     the rules are an optional automatic helper. Each load case also has its own list, ticked \
     where the case is defined (Load cases… › Applies to) and shown here as \"case (own list)\": \
     its picked structures take that case on top of their groups' cases, its excluded ones do \
     not take it whatever their groups give. The checks of the line (Checks › Structure loads) and OHL-Structure (Open in \
     OHL-Structure… on a placed structure) take exactly these cases. Nothing is solved again: \
     a case is solved once for the whole line.",
);

/// *4 Structures picked*: the help.
const PICKED_HELP: (&str, &str) = (
    "The first way: structures in the group by hand, per position, whatever their type.",
    "Select structures in the Structures table or in a view (Shift or Cmd/Ctrl + click for \
     several), then Add the N selected. A structure picked stays in the group when the line \
     changes; × takes it out. A picked id no longer in the line is shown and left out. Two \
     structures of the same type can be in different groups and take different cases.",
);

/// *5 Rules (optional)*: the help.
const RULES_HELP: (&str, &str) = (
    "Optional, automatic: every rule given must hold; bounds are included.",
    "A helper to fill the group without picking: a structure for which every rule given holds \
     is in it, on top of the structures picked. Types: its structure type is one of those ticked (none ticked: any type). Line angle: the \
     size of the angle the line turns there, left or right, in degrees (the largest over the \
     alignments it is on; a structure on no alignment has none and is not taken by an angle \
     rule). Station: its station on the alignment chosen (any alignment: on any it stands on), \
     in metres. Dead end: a section of the line ends at it (read from the line, not from the \
     type's name); not a dead end: no section ends there. A rule left empty does not count; a \
     group without rules holds its picked structures only. The rules are read from the line as \
     it is: move a structure past the angle given and its groups change.",
);

/// *6 Load cases*: the help.
const CASES_HELP: (&str, &str) = (
    "The load cases of the library the group's structures take.",
    "Ticked cases are taken by every structure in the group (with those of its other groups). \
     None ticked: its structures take no load case from this group — and none at all when no \
     other group gives them one (no loads computed, no structure check, listed as not checked, \
     never as passing). A ticked case no longer in the library is shown in the error colour: \
     the structures are not checked in it.",
);

/// The window's state.
#[derive(Debug, Default)]
pub struct GroupsWindow {
    /// Shown.
    pub open: bool,
    /// The group picked (its place in the list).
    pub picked: usize,
    /// Text being typed per field: (value when typing began, text).
    buffers: HashMap<String, (String, String)>,
    /// Why the last change was refused (shown in the window).
    pub error: Option<String>,
    /// The groups resolved, with what they were resolved from.
    cache: Option<(u64, Vec<StructureGroup>, CaseSets)>,
}

/// What the window asks for this frame.
#[derive(Debug, Default)]
pub struct Out {
    /// A change of the groups: in words, and its command (one undo).
    pub command: Option<(String, Command)>,
    /// A section of the user guide to open.
    pub guide: Option<&'static str>,
}

/// The memory key of a request to open the window.
fn request_key() -> egui::Id {
    egui::Id::new("structure-groups-open")
}

/// Asks the window to open (from a structure's Properties), with that
/// structure's first group picked.
pub fn request_open(ctx: &egui::Context, structure: Option<&str>) {
    ctx.data_mut(|d| {
        d.insert_temp(
            request_key(),
            Some(structure.unwrap_or_default().to_owned()),
        )
    });
}

/// The structures selected (the selection and the others selected), by id.
pub fn selected_structures(selection: &crate::selection::Selection, model: &Model) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in crate::selection::all_selected(selection, model) {
        if let crate::selection::Selection::Structure(id) = s
            && !out.contains(id)
        {
            out.push(id.clone());
        }
    }
    out
}

/// The window, when open; its change performed as one command.
pub fn window(ctx: &egui::Context, r: &mut crate::app::Ready) {
    if let Some(Some(id)) = ctx.data_mut(|d| d.remove_temp::<Option<String>>(request_key())) {
        r.structure_groups.open = true;
        if !id.is_empty() {
            let sets = r.model.line.case_sets();
            if let Some(name) = sets.groups_of(&id).first()
                && let Some(k) = r
                    .model
                    .line
                    .structure_groups
                    .iter()
                    .position(|g| g.name == *name)
            {
                r.structure_groups.picked = k;
            }
        }
    }
    if !r.structure_groups.open {
        return;
    }
    let selected = selected_structures(&r.selection, &r.model);
    let mut open = true;
    let mut out = Out::default();
    help::window(
        ctx,
        crate::dockable::Panel::StructureGroups,
        TITLE,
        [860.0, 760.0],
        &mut open,
        |ui| {
            out = r.structure_groups.ui(ui, &r.model, &selected);
        },
    );
    r.structure_groups.open = open;
    if let Some(anchor) = out.guide {
        let (page, anchor) = guide(anchor);
        r.status = Some(match crate::help::open_section(page, anchor) {
            Ok(m) => (format!("user guide opened: {m}"), false),
            Err(e) => (e, true),
        });
    }
    if let Some((what, command)) = out.command {
        r.structure_groups.error = None;
        if r.perform(crate::edit::Action::Command {
            what: what.clone(),
            command,
        }) {
            r.status = Some((what, false));
        }
    }
}

/// A number typed (`,` taken as the decimal point); empty: none.
fn parse_bound(text: &str) -> Result<Option<f64>, String> {
    let t = text.trim().replace(',', ".");
    if t.is_empty() {
        return Ok(None);
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(Some(v)),
        _ => Err(format!("\"{}\" is not a number", text.trim())),
    }
}

/// A bound as typed back (empty: none).
fn bound_text(v: Option<f64>) -> String {
    v.map(|v| {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    })
    .unwrap_or_default()
}

impl GroupsWindow {
    /// The groups as stored (the base of every change), else the model's.
    fn stored(model: &Model) -> Vec<StructureGroup> {
        model.stored_line().map_or_else(
            || model.line.structure_groups.clone(),
            |l| l.structure_groups.clone(),
        )
    }

    /// The command putting `groups` (one undo), named `what`; none when
    /// read-only (the window says why).
    fn command(
        &mut self,
        model: &Model,
        what: String,
        groups: Vec<StructureGroup>,
    ) -> Option<(String, Command)> {
        match &model.editing {
            Ok(e) => Some((what, e.session.structure_groups_command(groups))),
            Err(why) => {
                self.error = Some(format!("read-only: {why}"));
                None
            }
        }
    }

    /// A change of the group picked (`f` on a copy of it), as one command.
    fn change_picked(
        &mut self,
        model: &Model,
        what: &str,
        f: impl FnOnce(&mut StructureGroup),
    ) -> Option<(String, Command)> {
        let mut groups = Self::stored(model);
        let g = groups.get_mut(self.picked)?;
        let before = g.clone();
        f(g);
        if *g == before {
            return None;
        }
        if let Some(p) = g.rules.problem() {
            self.error = Some(format!("group \"{}\": {p}", g.label()));
            return None;
        }
        let name = g.label();
        self.command(model, format!("group \"{name}\": {what}"), groups)
    }

    /// Structures `take` made to take load case `case` and `drop` not to,
    /// through the case's own list (the same command as the Load cases
    /// form's *Applies to*).
    fn give(
        &mut self,
        model: &Model,
        case: &str,
        take: &[String],
        drop: &[String],
    ) -> Option<(String, Command)> {
        match crate::load_case_sets::give_command(model, case, take, drop) {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    /// The line's groups resolved (cached while the line and its groups
    /// are as they were).
    fn sets(&mut self, model: &Model) -> &CaseSets {
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|(v, g, _)| *v == model.line_version && *g == model.line.structure_groups);
        if !fresh {
            self.cache = Some((
                model.line_version,
                model.line.structure_groups.clone(),
                model.line.case_sets(),
            ));
        }
        &self.cache.as_ref().expect("set above").2
    }

    /// A text field committed when it loses the focus (or Enter): the
    /// text typed when it differs from `value`. `label` names it.
    fn field(
        &mut self,
        ui: &mut egui::Ui,
        key: String,
        label: &str,
        value: &str,
        width: f32,
        hover: &str,
    ) -> Option<String> {
        let entry = self
            .buffers
            .entry(key.clone())
            .or_insert_with(|| (value.to_owned(), value.to_owned()));
        if entry.0 != value {
            *entry = (value.to_owned(), value.to_owned());
        }
        let l = ui.label(label);
        let r = ui
            .add(egui::TextEdit::singleline(&mut entry.1).desired_width(width))
            .labelled_by(l.id)
            .on_hover_text(hover);
        if r.lost_focus() && entry.1 != value {
            let text = entry.1.clone();
            self.buffers.remove(&key);
            return Some(text);
        }
        None
    }

    /// The window's body; what it asks for.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model, selected: &[String]) -> Out {
        let mut out = Out::default();
        help::body(ui, "structure-groups-window", |ui| {
            self.body(ui, model, selected, &mut out);
        });
        out
    }

    fn body(&mut self, ui: &mut egui::Ui, model: &Model, selected: &[String], out: &mut Out) {
        let editable = model.can_edit().is_ok();
        if let Err(why) = model.can_edit() {
            crate::menu::error_label(ui, format!("read-only: {why}"));
        }
        if let Some(note) = model.line.unused_structure_loads_note() {
            ui.colored_label(ui.visuals().warn_fg_color, note);
        }
        if let Some(e) = &self.error {
            crate::menu::error_label(ui, e.as_str());
        }
        let known = crate::load_case_sets::library_names(model);
        let groups = model.line.structure_groups.clone();
        if self.picked >= groups.len() {
            self.picked = groups.len().saturating_sub(1);
        }
        // 1 How it works, with the line's summary.
        out.guide = out.guide.or(crate::check_ui::section_help(
            ui,
            1,
            "How it works",
            Some(anchors::HOW),
            Some(HOW_HELP),
        ));
        ui.label(HOW_HELP.0);
        let n = model.line.structures.len();
        let sets = self.sets(model).clone();
        let alone = sets.in_no_group().to_vec();
        let none = sets.without_load_cases().to_vec();
        ui.label(format!(
            "{} group(s) · {} of {n} structure(s) in a group · {} in no group: every case{}",
            groups.len(),
            n - alone.len(),
            alone.len(),
            if none.is_empty() {
                String::new()
            } else {
                format!(" · {} take no load case", none.len())
            }
        ));
        if !none.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "Take no load case (no loads, no structure check): {}",
                    first_ids(&none)
                ),
            )
            .on_hover_text(crate::load_case_sets::NO_CASE_HOVER);
        }
        if let Err(e) = &known {
            crate::menu::error_label(ui, e.as_str());
        }
        ui.separator();
        // 2 Groups: the list.
        out.guide = out.guide.or(crate::check_ui::section_help(
            ui,
            2,
            "Groups",
            Some(anchors::GROUPS),
            Some((
                "The line's groups: pick one to see and change it below.",
                "Each row: the group's name, how many structures it holds now, how many \
                 load cases it ticks, and its rules in words. New group adds an empty one \
                 (it holds nothing and ticks no case until you give it some); Delete takes \
                 the group picked away (its structures keep their other groups; in none, \
                 every case). Each is one step of Edit › Undo.",
            )),
        ));
        let alignment_name = |id: &str| {
            model
                .line
                .alignments
                .iter()
                .find(|a| a.id == id)
                .map_or_else(
                    || id.to_owned(),
                    |a| {
                        if a.name.is_empty() {
                            a.id.clone()
                        } else {
                            a.name.clone()
                        }
                    },
                )
        };
        if groups.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No group yet: every structure takes every case of the library.",
                )
                .weak(),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt("structure-groups-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    egui::Grid::new("structure-groups-grid")
                        .num_columns(4)
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in ["group", "structures", "load cases", "rules"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (k, g) in groups.iter().enumerate() {
                                if ui
                                    .selectable_label(k == self.picked, g.label())
                                    .on_hover_text(if g.description.is_empty() {
                                        "Pick it to see and change it below".to_owned()
                                    } else {
                                        g.description.clone()
                                    })
                                    .clicked()
                                {
                                    self.picked = k;
                                    self.buffers.clear();
                                }
                                ui.label(sets.members(k).len().to_string());
                                if let Some(case) = &g.load_case {
                                    if known.as_ref().is_ok_and(|k| !k.contains(case)) {
                                        crate::menu::error_label(
                                            ui,
                                            format!("{case} (not in the library)"),
                                        );
                                    } else {
                                        ui.label(case);
                                    }
                                    let excluded = sets.excluded(k).len();
                                    ui.label(format!(
                                        "— the load case's own list{} (Load cases… › Applies to)",
                                        if excluded == 0 {
                                            String::new()
                                        } else {
                                            format!(", {excluded} excluded")
                                        }
                                    ));
                                    ui.end_row();
                                    continue;
                                }
                                if g.load_cases.is_empty() {
                                    ui.colored_label(ui.visuals().warn_fg_color, "none");
                                } else {
                                    ui.label(g.load_cases.len().to_string());
                                }
                                let rules = g.rules.words(alignment_name);
                                ui.label(if rules.is_empty() {
                                    "— (picked only)".to_owned()
                                } else {
                                    rules
                                });
                                ui.end_row();
                            }
                        });
                });
        }
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("New group")
                    .on_hover_text("An empty group: name it, give it structures and load cases")
                    .clicked()
                {
                    let mut all = Self::stored(model);
                    let mut probe = model.line.clone();
                    probe.structure_groups = all.clone();
                    let name = probe.free_group_name("group 1");
                    all.push(StructureGroup::new(&name));
                    let k = all.len() - 1;
                    if let Some(c) =
                        self.command(model, format!("new structure group \"{name}\""), all)
                    {
                        out.command = Some(c);
                        self.picked = k;
                        self.buffers.clear();
                    }
                }
                if !groups.is_empty()
                    && ui
                        .button("Delete group")
                        .on_hover_text(
                            if groups
                                .get(self.picked)
                                .is_some_and(StructureGroup::is_own_list)
                            {
                                "The load case's own list taken away (one undo): its picked \
                                 structures keep the case only when a group gives it; its \
                                 excluded ones take it again"
                            } else {
                                "The group picked taken away (one undo); its structures keep \
                                 their other groups (in none: every case)"
                            },
                        )
                        .clicked()
                {
                    let mut all = Self::stored(model);
                    if self.picked < all.len() {
                        let gone = all.remove(self.picked);
                        out.command = self.command(
                            model,
                            format!("structure group \"{}\" deleted", gone.name),
                            all,
                        );
                        self.picked = self.picked.saturating_sub(1);
                        self.buffers.clear();
                    }
                }
            });
        });
        let Some(g) = groups.get(self.picked).cloned() else {
            return;
        };
        if g.is_own_list() {
            self.own_list_ui(ui, model, &g, selected, &sets, editable, out);
            return;
        }
        ui.separator();
        ui.heading(format!("Group \"{}\"", g.name));
        ui.add_enabled_ui(editable, |ui| {
            // 3 Name and description.
            out.guide = out.guide.or(crate::check_ui::section_help(
                ui,
                3,
                "Name and description",
                Some(anchors::NAME),
                Some((
                    "What the group is for, in your words.",
                    "The name is shown wherever a structure's groups are listed (the \
                     Structures table, its Properties, the notes of the checks, \
                     OHL-Structure's status block); unique in the line. The description is \
                     free text (a group made from the load case sets of before 2026-11-15 \
                     says where it came from).",
                )),
            ));
            egui::Grid::new("structure-group-name")
                .num_columns(2)
                .spacing(help::GRID_SPACING)
                .show(ui, |ui| {
                    if let Some(name) = self.field(
                        ui,
                        format!("name/{}", self.picked),
                        "name",
                        &g.name,
                        260.0,
                        "The group's name (unique)",
                    ) {
                        let name = name.trim().to_owned();
                        if name.is_empty() {
                            self.error = Some("a group needs a name".into());
                        } else if groups.iter().any(|x| x.name == name) {
                            self.error = Some(format!("another group is called \"{name}\""));
                        } else {
                            out.command =
                                self.change_picked(model, &format!("renamed \"{name}\""), |x| {
                                    x.name = name.clone();
                                });
                        }
                    }
                    ui.end_row();
                    if let Some(d) = self.field(
                        ui,
                        format!("description/{}", self.picked),
                        "description",
                        &g.description,
                        480.0,
                        "What the group is for (free text)",
                    ) {
                        out.command = self.change_picked(model, "description", |x| {
                            x.description = d.trim().to_owned();
                        });
                    }
                    ui.end_row();
                });
            ui.separator();
            // 4 Structures picked.
            out.guide = out.guide.or(crate::check_ui::section_help(
                ui,
                4,
                "Structures picked",
                Some(anchors::PICKED),
                Some(PICKED_HELP),
            ));
            if g.structures.is_empty() {
                ui.label(egui::RichText::new("None picked.").weak());
            } else {
                let mut gone: Option<String> = None;
                ui.horizontal_wrapped(|ui| {
                    for id in &g.structures {
                        let there = model.line.structure(id).is_some();
                        let text = if there {
                            format!("{id} ×")
                        } else {
                            format!("{id} (not in the line) ×")
                        };
                        if ui
                            .small_button(text)
                            .on_hover_text(format!("Take {id} out of the group"))
                            .clicked()
                        {
                            gone = Some(id.clone());
                        }
                    }
                });
                if let Some(id) = gone {
                    out.command = self.change_picked(model, &format!("{id} taken out"), |x| {
                        x.structures.retain(|s| *s != id);
                    });
                }
            }
            let new: Vec<&String> = selected
                .iter()
                .filter(|s| !g.structures.contains(s))
                .collect();
            let held: Vec<&String> = selected
                .iter()
                .filter(|s| g.structures.contains(s))
                .collect();
            ui.horizontal(|ui| {
                let add = ui
                    .add_enabled(
                        !new.is_empty(),
                        egui::Button::new(format!("Add the {} selected", new.len())),
                    )
                    .on_hover_text(format!(
                        "Pick {} (selected in the Structures table or a view)",
                        new.iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .on_disabled_hover_text(
                        "Select structures in the Structures table or in a view first (Shift or \
                         Cmd/Ctrl + click for several)",
                    );
                if add.clicked() {
                    let ids: Vec<String> = new.iter().map(|s| (*s).clone()).collect();
                    out.command =
                        self.change_picked(model, &format!("{} picked", ids.join(", ")), |x| {
                            x.structures.extend(ids.iter().cloned())
                        });
                }
                if !held.is_empty()
                    && ui
                        .button(format!("Remove the {} selected", held.len()))
                        .clicked()
                {
                    let ids: Vec<String> = held.iter().map(|s| (*s).clone()).collect();
                    out.command =
                        self.change_picked(model, &format!("{} taken out", ids.join(", ")), |x| {
                            x.structures.retain(|s| !ids.contains(s))
                        });
                }
            });
            ui.separator();
            // 5 Rules.
            out.guide = out.guide.or(crate::check_ui::section_help(
                ui,
                5,
                "Rules (optional)",
                Some(anchors::RULES),
                Some(RULES_HELP),
            ));
            if let Some(c) = self.rules_ui(ui, model, &g) {
                out.command = Some(c);
            }
            ui.separator();
            // 6 Load cases.
            out.guide = out.guide.or(crate::check_ui::section_help(
                ui,
                6,
                "Load cases",
                Some(anchors::CASES),
                Some(CASES_HELP),
            ));
            match &known {
                Err(e) => {
                    crate::menu::error_label(ui, e.as_str());
                }
                Ok(names) => {
                    if let Some(c) = self.cases_ui(ui, model, &g, names) {
                        out.command = Some(c);
                    }
                }
            }
        });
        ui.separator();
        // 7 Who is in.
        out.guide = out.guide.or(crate::check_ui::section_help(
            ui,
            7,
            "Who is in",
            Some(anchors::WHO),
            Some((
                "The structures in the group now, and the cases each takes.",
                "Read from the line as it is: picked, by its rules, or both. The last column \
                 is what the structure takes from all its groups (their union) — what the \
                 line's checks and OHL-Structure use. Below: the structures in no group, \
                 which take every case of the library.",
            )),
        ));
        let members = sets.members(self.picked).to_vec();
        ui.label(format!("{} structure(s) in \"{}\"", members.len(), g.name));
        if !members.is_empty() {
            let facts = model.line.structure_facts();
            let none = ohl_line::Facts::default();
            help::list(
                ui,
                "structure-groups-members",
                200.0,
                egui::ScrollArea::vertical()
                    .id_salt("structure-groups-members")
                    .auto_shrink([false, true]),
                |ui| {
                    egui::Grid::new("structure-groups-members-grid")
                        .num_columns(5)
                        .striped(true)
                        .spacing(help::GRID_SPACING)
                        .show(ui, |ui| {
                            for h in ["structure", "type", "why", "its groups", "its load cases"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for id in &members {
                                let Some(s) = model.line.structure(id) else {
                                    continue;
                                };
                                ui.label(id);
                                ui.label(s.structure_type.clone().unwrap_or_default());
                                let picked = g.structures.contains(id);
                                let ruled = g.rules.holds(s, facts.get(id).unwrap_or(&none));
                                ui.label(match (picked, ruled) {
                                    (true, true) => "picked, rules",
                                    (true, false) => "picked",
                                    _ => "rules",
                                });
                                ui.label(crate::load_case_sets::groups_words(&sets, id));
                                let excluded = sets.excluded_from(id);
                                let but = if excluded.is_empty() {
                                    String::new()
                                } else {
                                    format!(
                                        " — {} {}",
                                        excluded.join(", "),
                                        crate::load_case_sets::EXCLUDED
                                    )
                                };
                                match sets.of(id) {
                                    CaseSet::NoCase => {
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            format!("{}{but}", crate::load_case_sets::NO_CASE),
                                        );
                                    }
                                    c if excluded.is_empty() => {
                                        ui.label(c.names().join(", "));
                                    }
                                    c => {
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            format!("{}{but}", c.names().join(", ")),
                                        );
                                    }
                                }
                                ui.end_row();
                            }
                        });
                },
            );
        }
        ui.label(
            egui::RichText::new(if alone.is_empty() {
                "Every structure is in a group.".to_owned()
            } else {
                format!("In no group (they take every case): {}", first_ids(&alone))
            })
            .small(),
        );
    }

    /// A load case's own list `g` picked (2026-11-15): its description,
    /// its picked and excluded structures (changed as in the Load cases
    /// form's *Applies to*: the same command), who it gives the case to.
    #[allow(clippy::too_many_arguments)]
    fn own_list_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        g: &StructureGroup,
        selected: &[String],
        sets: &CaseSets,
        editable: bool,
        out: &mut Out,
    ) {
        let case = g.load_case.clone().unwrap_or_default();
        ui.separator();
        ui.heading(format!("Load case \"{case}\": its own list"));
        ui.label(
            egui::RichText::new(format!(
                "Made where the case is defined: Line › Structures › Load cases… › {case} › 8 \
                 Applies to (ticks there change this list). Its picked structures take {case} on \
                 top of their groups' cases (a structure in no group takes every case already); \
                 its excluded structures do not take {case}, whatever their groups give. It has \
                 no rules and gives no other case."
            ))
            .small(),
        );
        ui.add_enabled_ui(editable, |ui| {
            // 3 Name and description: the name is the case's.
            out.guide = out.guide.or(crate::check_ui::section_help(
                ui,
                3,
                "Name and description",
                Some(anchors::NAME),
                Some((
                    "Named after its load case; the description is free text.",
                    "A load case's own list has no name of its own: it is shown as \"case \
                     (own list)\" wherever a structure's groups are listed, and follows a \
                     rename of the case in the Load cases library.",
                )),
            ));
            egui::Grid::new("structure-group-own-description")
                .num_columns(2)
                .spacing(help::GRID_SPACING)
                .show(ui, |ui| {
                    if let Some(d) = self.field(
                        ui,
                        format!("description/{}", self.picked),
                        "description",
                        &g.description,
                        480.0,
                        "What the list is for (free text)",
                    ) {
                        out.command = self.change_picked(model, "description", |x| {
                            x.description = d.trim().to_owned();
                        });
                    }
                    ui.end_row();
                });
            ui.separator();
            // 4 Structures picked, and excluded.
            out.guide = out.guide.or(crate::check_ui::section_help(
                ui,
                4,
                "Structures picked",
                Some(anchors::PICKED),
                Some((
                    "Picked: they take the case; excluded: they do not, whatever their groups \
                     give.",
                    "Select structures in the Structures table or in a view, then Add the N \
                     selected (picked here when no group of theirs gives the case; an exclusion \
                     taken away) or Remove the N selected (taken out; excluded when a group or \
                     being in no group still gives them the case). × takes one pick or one \
                     exclusion away. The same as ticking them in Load cases… › Applies to.",
                )),
            ));
            let mut gone: Option<(String, bool)> = None;
            for (what, ids, excluded) in [
                ("picked (take it)", &g.structures, false),
                ("excluded (do not take it)", &g.excluded, true),
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(what);
                    if ids.is_empty() {
                        ui.label(egui::RichText::new("none").weak());
                    }
                    for id in ids.iter() {
                        let there = model.line.structure(id).is_some();
                        let text = if there {
                            format!("{id} ×")
                        } else {
                            format!("{id} (not in the line) ×")
                        };
                        if ui
                            .small_button(text)
                            .on_hover_text(if excluded {
                                format!("Take {id}'s exclusion away: it takes {case} again when its groups give it")
                            } else {
                                format!("Take {id} out of the list: it takes {case} only when its groups give it")
                            })
                            .clicked()
                        {
                            gone = Some((id.clone(), excluded));
                        }
                    }
                });
            }
            if let Some((id, excluded)) = gone {
                out.command = if excluded {
                    self.change_picked(model, &format!("{id} no longer excluded"), |x| {
                        x.excluded.retain(|s| *s != id);
                    })
                } else {
                    self.change_picked(model, &format!("{id} taken out"), |x| {
                        x.structures.retain(|s| *s != id);
                    })
                };
            }
            let (held, new): (Vec<String>, Vec<String>) = selected
                .iter()
                .filter(|s| model.line.structure(s).is_some())
                .cloned()
                .partition(|s| sets.takes(s, &case));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !new.is_empty(),
                        egui::Button::new(format!("Add the {} selected", new.len())),
                    )
                    .on_hover_text(format!("Give {case} to {}", new.join(", ")))
                    .on_disabled_hover_text(
                        "Select structures that do not take the case in the Structures table or \
                         in a view first",
                    )
                    .clicked()
                {
                    out.command = self.give(model, &case, &new, &[]);
                }
                if !held.is_empty()
                    && ui
                        .button(format!("Remove the {} selected", held.len()))
                        .on_hover_text(format!("{} not to take {case}", held.join(", ")))
                        .clicked()
                {
                    out.command = self.give(model, &case, &[], &held);
                }
            });
        });
        ui.separator();
        // 7 Who is in: who takes the case, and how.
        out.guide = out.guide.or(crate::check_ui::section_help(
            ui,
            7,
            "Who is in",
            Some(anchors::WHO),
            Some((
                "The structures taking the case, and how each gets it.",
                "Every structure taking the case — through a group, in no group, or picked \
                 here — and those excluded here.",
            )),
        ));
        let takers = sets.takers(&case);
        let excluded = sets.own_list(&case).map_or(&[][..], |k| sets.excluded(k));
        ui.label(format!(
            "{} structure(s) take \"{case}\"{}",
            takers.len(),
            if excluded.is_empty() {
                String::new()
            } else {
                format!(" · {} excluded here", excluded.len())
            }
        ));
        help::list(
            ui,
            "structure-groups-own-members",
            200.0,
            egui::ScrollArea::vertical()
                .id_salt("structure-groups-own-members")
                .auto_shrink([false, true]),
            |ui| {
                egui::Grid::new("structure-groups-own-members-grid")
                    .num_columns(3)
                    .striped(true)
                    .spacing(help::GRID_SPACING)
                    .show(ui, |ui| {
                        for h in ["structure", "type", "how"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for id in takers.iter().chain(excluded) {
                            let Some(s) = model.line.structure(id) else {
                                continue;
                            };
                            ui.label(id);
                            ui.label(s.structure_type.clone().unwrap_or_default());
                            let why = sets.why(id, &case);
                            let words = crate::load_case_sets::why_words(&model.line, &why);
                            if why.takes {
                                ui.label(words);
                            } else {
                                ui.colored_label(ui.visuals().warn_fg_color, words);
                            }
                            ui.end_row();
                        }
                    });
            },
        );
    }

    /// *5 Rules* of group `g`; the command of a change.
    fn rules_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        g: &StructureGroup,
    ) -> Option<(String, Command)> {
        let mut out = None;
        let rules = &g.rules;
        // Types: the line's, and those of the rule no structure has.
        let mut types: Vec<String> = model
            .line
            .structures
            .iter()
            .filter_map(|s| s.structure_type.clone())
            .collect();
        types.sort();
        types.dedup();
        let missing: Vec<String> = rules
            .types
            .iter()
            .filter(|t| !types.contains(t))
            .cloned()
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label("types")
                .on_hover_text("Its structure type is one of those ticked (none ticked: any)");
            if types.is_empty() && missing.is_empty() {
                ui.label(egui::RichText::new("no structure of the line has a type").weak());
            }
            for t in types.iter().chain(&missing) {
                let mut on = rules.types.contains(t);
                let text = if missing.contains(t) {
                    format!("type {t} (no structure)")
                } else {
                    format!("type {t}")
                };
                if ui.checkbox(&mut on, text).changed() {
                    let t = t.clone();
                    out = self.change_picked(
                        model,
                        &format!("type {t} {}", if on { "ticked" } else { "unticked" }),
                        |x| {
                            if on {
                                x.rules.types.push(t.clone());
                            } else {
                                x.rules.types.retain(|y| *y != t);
                            }
                        },
                    );
                }
            }
        });
        // Bounds: from–to, empty for none.
        let bounds = |ui: &mut egui::Ui,
                      this: &mut Self,
                      what: &str,
                      unit: &str,
                      from: Option<f64>,
                      to: Option<f64>,
                      set: fn(&mut GroupRules, Option<f64>, Option<f64>)|
         -> Option<(String, Command)> {
            let mut out = None;
            ui.horizontal(|ui| {
                ui.label(what);
                let hover = format!("{what}, {unit}: empty for no bound; the bound is included");
                let a = this.field(
                    ui,
                    format!("{what}/from/{}", this.picked),
                    &format!("from {unit}"),
                    &bound_text(from),
                    70.0,
                    &hover,
                );
                let b = this.field(
                    ui,
                    format!("{what}/to/{}", this.picked),
                    &format!("to {unit}"),
                    &bound_text(to),
                    70.0,
                    &hover,
                );
                let typed = match (a, b) {
                    (None, None) => None,
                    (a, b) => Some((
                        a.map_or(Ok(from), |t| parse_bound(&t)),
                        b.map_or(Ok(to), |t| parse_bound(&t)),
                    )),
                };
                match typed {
                    Some((Ok(f), Ok(t))) => {
                        out = this.change_picked(
                            model,
                            &format!("{what} {}–{}", bound_text(f), bound_text(t)),
                            |x| set(&mut x.rules, f, t),
                        );
                    }
                    Some((Err(e), _)) | Some((_, Err(e))) => {
                        this.error = Some(format!("{what}: {e}"));
                    }
                    None => {}
                }
            });
            out
        };
        if let Some(c) = bounds(
            ui,
            self,
            "line angle",
            "°",
            rules.angle_from_deg,
            rules.angle_to_deg,
            |r, f, t| {
                r.angle_from_deg = f;
                r.angle_to_deg = t;
            },
        ) {
            out = Some(c);
        }
        // The alignment of the station rule.
        let alignment_name = |a: &ohl_line::Alignment| {
            if a.name.is_empty() {
                a.id.clone()
            } else {
                a.name.clone()
            }
        };
        let shown = rules
            .alignment
            .as_deref()
            .map_or("any alignment".to_owned(), |id| {
                model
                    .line
                    .alignments
                    .iter()
                    .find(|a| a.id == id)
                    .map_or(format!("{id} (not in the line)"), alignment_name)
            });
        let mut chosen = rules.alignment.clone();
        ui.horizontal(|ui| {
            ui.label("on alignment").on_hover_text(
                "The station rule's alignment: the structure stands on it (any alignment: on \
                 any it stands on)",
            );
            egui::ComboBox::from_id_salt(("structure-group-alignment", self.picked))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "any alignment");
                    for a in &model.line.alignments {
                        ui.selectable_value(&mut chosen, Some(a.id.clone()), alignment_name(a));
                    }
                });
        });
        if chosen != rules.alignment {
            let c = chosen.clone();
            out = self.change_picked(model, "alignment of the station rule", |x| {
                x.rules.alignment = c;
            });
        }
        if let Some(c) = bounds(
            ui,
            self,
            "station",
            "m",
            rules.station_from,
            rules.station_to,
            |r, f, t| {
                r.station_from = f;
                r.station_to = t;
            },
        ) {
            out = Some(c);
        }
        let mut dead = rules.dead_end;
        ui.horizontal(|ui| {
            ui.label("dead end").on_hover_text(
                "Read from the line: a section ends at the structure (yes) or none does (no)",
            );
            ui.radio_value(&mut dead, None, "either");
            ui.radio_value(&mut dead, Some(true), "a dead end");
            ui.radio_value(&mut dead, Some(false), "not a dead end");
        });
        if dead != rules.dead_end {
            out = self.change_picked(model, "dead end rule", |x| x.rules.dead_end = dead);
        }
        if rules.is_empty() {
            ui.label(egui::RichText::new("No rule: it holds its picked structures only.").weak());
        }
        out
    }

    /// *6 Load cases* of group `g` (the library's `names`); the command
    /// of a change.
    fn cases_ui(
        &mut self,
        ui: &mut egui::Ui,
        model: &Model,
        g: &StructureGroup,
        names: &[String],
    ) -> Option<(String, Command)> {
        let mut out = None;
        let gone: Vec<&String> = g.load_cases.iter().filter(|n| !names.contains(n)).collect();
        ui.horizontal(|ui| {
            if ui
                .small_button("All")
                .on_hover_text("Tick every case of the library")
                .clicked()
            {
                let all = names.to_vec();
                out = self.change_picked(model, "every case ticked", |x| x.load_cases = all);
            }
            if ui
                .small_button("None")
                .on_hover_text("Untick every case: its structures take none from this group")
                .clicked()
            {
                out = self.change_picked(model, "no case ticked", |x| x.load_cases.clear());
            }
        });
        ui.horizontal_wrapped(|ui| {
            for n in names.iter().chain(gone.iter().copied()) {
                let mut on = g.load_cases.contains(n);
                let there = names.contains(n);
                let text = if there {
                    egui::RichText::new(n)
                } else {
                    egui::RichText::new(format!("{n} (not in the library)"))
                        .color(ui.visuals().error_fg_color)
                };
                if ui.checkbox(&mut on, text).changed() {
                    let n = n.clone();
                    // In the library's order, those not there last.
                    let next: Vec<String> = names
                        .iter()
                        .chain(gone.iter().copied())
                        .filter(|x| {
                            if **x == n {
                                on
                            } else {
                                g.load_cases.contains(x)
                            }
                        })
                        .cloned()
                        .collect();
                    out = self.change_picked(
                        model,
                        &format!("case {n} {}", if on { "ticked" } else { "unticked" }),
                        |x| x.load_cases = next,
                    );
                }
            }
        });
        if g.load_cases.is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "None ticked: its structures take no load case from this group (none at all \
                 unless another group of theirs gives some): no loads, no structure check.",
            )
            .on_hover_text(crate::load_case_sets::NO_CASE_HOVER);
        }
        out
    }
}

/// The first ids of `ids` (ten at most, then how many more).
fn first_ids(ids: &[String]) -> String {
    let mut s = ids
        .iter()
        .take(10)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if ids.len() > 10 {
        s.push_str(&format!(" and {} more", ids.len() - 10));
    }
    s
}
