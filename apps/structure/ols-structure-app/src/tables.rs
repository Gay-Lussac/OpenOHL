// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The panels of tables (Geometry, Body, Design): their tools and tables,
//! and the actions their menus share with the tools. Every change is one
//! edit of the file (one undo step; a value dragged is one step).

use eframe::egui;
use ols_structure_edit::{
    self as edit, Geometry, Handle, StringChange, StructureFile, geometry, polar,
};
use ols_structures::{AssemblyKind, InsulatorLibrary, StructureType};

use crate::app::{Sheet, StructureApp, error_label};
use crate::layout::{Area, Panel};

/// A length typed or dragged in a table.
fn coord(ui: &mut egui::Ui, v: &mut f64) -> egui::Response {
    ui.add(
        egui::DragValue::new(v)
            .speed(0.01)
            .max_decimals(3)
            .suffix(" m"),
    )
}

/// While the pointer is pressed on `r` its changes are one undo step
/// (`what`); the step ends when the pointer is let go.
fn hold(file: &mut StructureFile, r: &egui::Response, what: &str) {
    if r.is_pointer_button_down_on() {
        file.begin(what);
    }
}

/// A row's name: picks it (Shift + click: picks another, or drops it);
/// brought into view when asked.
pub(crate) fn row(
    ui: &mut egui::Ui,
    selected: &mut Vec<Handle>,
    scroll_to: &mut Option<Handle>,
    h: Handle,
    text: &str,
) {
    let r = ui.selectable_label(selected.contains(&h), text);
    if r.clicked() {
        if ui.input(|i| i.modifiers.shift) {
            match selected.iter().position(|x| *x == h) {
                Some(k) => {
                    selected.remove(k);
                }
                None => selected.push(h.clone()),
            }
        } else {
            *selected = vec![h.clone()];
        }
    }
    if scroll_to.as_ref() == Some(&h) {
        r.scroll_to_me(Some(egui::Align::Center));
        *scroll_to = None;
    }
}

impl StructureApp {
    /// A panel of tables: its tools, the table chosen (when it has
    /// several), the table.
    pub fn sheet_panel(&mut self, ui: &mut egui::Ui, panel: Panel) {
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure().clone();
        let g = geometry(&t, &self.insulators);
        if panel == Panel::Generators {
            // Shown while the generator's form is.
            self.body.preview.clear();
        }
        let area = panel.area().unwrap_or_default();
        if matches!(
            panel,
            Panel::PointsInsulators | Panel::WiresSets | Panel::Joints | Panel::Members
        ) {
            self.tools(ui, &t, area);
        }
        if matches!(panel, Panel::Joints | Panel::Members) {
            self.body_tools(ui, &t);
        }
        let sheets = panel.sheets();
        let Some(mut sheet) = self.sheet(panel) else {
            return;
        };
        if sheets.len() > 1 {
            ui.horizontal(|ui| {
                for (s, name, tip) in sheets {
                    ui.selectable_value(&mut sheet, *s, *name)
                        .on_hover_text(*tip);
                }
            });
            self.sheets.insert(panel, sheet);
        }
        egui::ScrollArea::both()
            .id_salt(("sheet", panel))
            .auto_shrink(false)
            .show(ui, |ui| match sheet {
                Sheet::Structure => self.structure_sheet(ui, &t),
                Sheet::Points => self.points_sheet(ui, &t),
                Sheet::Strings => self.strings_sheet(ui, &t, &g),
                Sheet::Wires => self.wires_sheet(ui, &t, &g),
                Sheet::Sets => self.sets_sheet(ui, &t),
                Sheet::Joints => self.joints_sheet(ui, &t),
                Sheet::Members => self.members_sheet(ui, &t, &g),
                Sheet::Generators => self.generators_sheet(ui, &t),
                Sheet::Groups => self.groups_sheet(ui, &t),
                Sheet::Supports => self.supports_sheet(ui, &t),
                Sheet::Design => self.design_sheet(ui, &t),
                Sheet::Sections => self.items_sheet(ui, "section"),
                Sheet::Materials => self.items_sheet(ui, "material"),
                Sheet::Pole => self.pole_sheet(ui, &t),
                Sheet::PoleDesign => self.pole_design_sheet(ui, &t),
                Sheet::Extensions => self.extensions_sheet(ui, &t),
            });
    }

    /// The points (or wire points) picked: where insulators start.
    pub fn picked_tops(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter_map(|h| match h {
                Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                _ => None,
            })
            .collect()
    }

    /// The joints picked, in order.
    pub fn picked_joints(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter_map(|h| match h {
                Handle::Joint(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Geometry › Add a point (+ Point): at the top of the stick figure.
    pub fn add_point(&mut self) {
        let Some(f) = &mut self.file else {
            return;
        };
        let t = f.structure();
        let top = t
            .points
            .iter()
            .map(|p| p.position()[2].si())
            .fold(t.height.map_or(10.0, |h| h.si()), f64::max);
        let l = f.edit("add point", |doc, t| {
            edit::add_point(doc, t, [2.0, 0.0, top])
        });
        self.selected = vec![Handle::Point(l.clone())];
        self.message = Some((format!("added point {l}"), false));
    }

    /// I (one point picked) or V (two): insulators down to a new wire
    /// point.
    pub fn add_insulators(&mut self) {
        let tops = self.picked_tops();
        let name = match tops.len() {
            1 => "I",
            2 => "V",
            _ => return,
        };
        let Some(f) = &mut self.file else {
            return;
        };
        let length = self.string_length;
        match f.edit(&format!("{name} insulator"), |doc, t| {
            edit::add_strings(doc, t, &tops, length)
        }) {
            Ok(w) => {
                self.message = Some((
                    format!("added the insulators to wire point {w}: put its set in Set points"),
                    false,
                ));
                self.selected = vec![Handle::Wire(w)];
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// Body › Add a joint: above the joint picked (or at the centre).
    pub fn add_joint(&mut self) {
        let Some(f) = &mut self.file else {
            return;
        };
        let at = match self.selected.last() {
            Some(Handle::Joint(id)) => f
                .structure()
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .map(|n| n.point.map(|v| v.si()))
                .map(|p| [p[0], p[1], p[2] + 1.0]),
            _ => None,
        }
        .unwrap_or([0.0; 3]);
        let ids = f.edit("add joint", |doc, t| edit::symmetry::add_joint(doc, t, at));
        self.selected = vec![Handle::Joint(ids[0].clone())];
        self.message = Some((format!("added joint {}", ids.join(", ")), false));
    }

    /// Body › Member: between the two joints picked.
    pub fn add_member(&mut self) {
        let joints = self.picked_joints();
        let ([a, b], Some(f)) = (
            <[String; 2]>::try_from(joints).unwrap_or_default(),
            &mut self.file,
        ) else {
            return;
        };
        if a.is_empty() {
            return;
        }
        match f.edit("add member", |doc, t| {
            edit::symmetry::add_member(doc, t, &a, &b)
        }) {
            Ok(ids) => {
                let more = if ids.len() > 1 {
                    format!(" and its images {}", ids[1..].join(", "))
                } else {
                    String::new()
                };
                self.message = Some((format!("added member {} ({a}–{b}){more}", ids[0]), false));
                // The next member starts at the second joint.
                self.selected = vec![Handle::Joint(b)];
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// Body › Point on the joint: a point on the joint picked.
    pub fn point_here(&mut self) {
        let (Some(f), [Handle::Joint(j)]) = (&mut self.file, self.selected.as_slice()) else {
            return;
        };
        let j = j.clone();
        let l = f.edit("add point", |doc, t| edit::add_point_at_joint(doc, t, &j));
        self.selected = vec![Handle::Point(l.clone())];
        self.message = Some((format!("added point {l} on joint {j}"), false));
    }

    /// Delete…: what is picked, asked first.
    pub fn ask_delete(&mut self) {
        if let (Some(f), [h]) = (&self.file, self.selected.as_slice()) {
            self.confirm = Some(edit::symmetry::removal(f.structure(), h));
        }
    }

    /// The tools of a Geometry or Body panel: snap, add, presets, delete,
    /// fit, what is picked.
    fn tools(&mut self, ui: &mut egui::Ui, _t: &StructureType, area: Area) {
        ui.horizontal_wrapped(|ui| {
            ui.label("snap");
            ui.add(
                egui::DragValue::new(&mut self.grid)
                    .range(0.0..=5.0)
                    .speed(0.01)
                    .max_decimals(3)
                    .suffix(" m"),
            )
            .on_hover_text("A dragged joint or point snaps to this grid (0: none), or onto a joint");
            ui.separator();
            if area == Area::Geometry {
                if ui
                    .button("+ Point")
                    .on_hover_text("A point where insulators hang, at the top of the stick figure (move it in the view or the table)")
                    .clicked()
                {
                    self.add_point();
                }
                let tops = self.picked_tops();
                ui.separator();
                ui.label("insulator");
                ui.add(
                    egui::DragValue::new(&mut self.string_length)
                        .range(0.05..=50.0)
                        .speed(0.05)
                        .max_decimals(3)
                        .suffix(" m"),
                );
                for (name, n, help) in [
                    ("I", 1, "An insulator from the point picked down to a new wire point (its set: Set points)"),
                    ("V", 2, "A V: two insulators from the two points picked to one wire point below their middle"),
                ] {
                    if ui
                        .add_enabled(tops.len() == n, egui::Button::new(name))
                        .on_hover_text(help)
                        .on_disabled_hover_text(if n == 1 {
                            "Pick one point (or a wire point: an insulator below it)"
                        } else {
                            "Pick two points (Shift + click the second)"
                        })
                        .clicked()
                    {
                        self.add_insulators();
                    }
                }
            }
            if area == Area::Body {
                if ui
                    .button("Add joint")
                    .on_hover_text("A joint above the joint picked (or at the centre)")
                    .clicked()
                {
                    self.add_joint();
                }
                let joints = self.picked_joints();
                if ui
                    .add_enabled(joints.len() == 2, egui::Button::new("Member"))
                    .on_hover_text("A member between the two joints picked")
                    .on_disabled_hover_text("Pick two joints (Shift + click the second)")
                    .clicked()
                {
                    self.add_member();
                }
                if matches!(self.selected.as_slice(), [Handle::Joint(_)])
                    && ui
                        .button("Point here")
                        .on_hover_text("A point where insulators hang, on this joint (it moves with it)")
                        .clicked()
                {
                    self.point_here();
                }
            }
            ui.separator();
            if ui
                .add_enabled(self.selected.len() == 1, egui::Button::new("Delete…"))
                .on_hover_text("Delete what is picked, with what goes with it (asks first)")
                .on_disabled_hover_text("Pick one thing")
                .clicked()
            {
                self.ask_delete();
            }
            if ui.button("Fit").on_hover_text("Show the whole structure").clicked() {
                self.view_fit = true;
            }
            if !self.selected.is_empty() {
                ui.label(
                    egui::RichText::new(
                        self.selected
                            .iter()
                            .map(Handle::describe)
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                    .color(egui::Color32::from_rgb(255, 235, 60)),
                );
            }
        });
    }

    /// The file's own fields: name, height of the stick figure, level of
    /// the feet, height extensions.
    fn structure_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        egui::Grid::new("structure-fields")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("id");
                ui.label(&t.id)
                    .on_hover_text("Its id: the line refers to it (renamed in the file only)");
                ui.end_row();
                ui.strong("name");
                let mut name = t.name.clone();
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
                if r.changed() {
                    f.edit("name", |doc, _| {
                        edit::set_top_field(doc, "name", Some(&name), false)
                    });
                }
                ui.end_row();
                for (key, label, tip, value) in [
                    (
                        "height",
                        "height",
                        "Top of the mast in the stick drawing (none: the highest point)",
                        t.height.map(|h| h.si()),
                    ),
                    (
                        "foot",
                        "feet at",
                        "Level of the feet in the structure's z (none: 0, the ground)",
                        t.foot.map(|h| h.si()),
                    ),
                ] {
                    ui.strong(label).on_hover_text(tip);
                    ui.horizontal(|ui| {
                        let mut on = value.is_some();
                        let mut v = value.unwrap_or(0.0);
                        let a = ui.checkbox(&mut on, "");
                        let b = ui.add_enabled(
                            on,
                            egui::DragValue::new(&mut v)
                                .speed(0.05)
                                .max_decimals(3)
                                .suffix(" m"),
                        );
                        hold(f, &b, label);
                        if a.changed() || b.changed() {
                            let text = format!("{v}");
                            f.edit(label, |doc, _| {
                                edit::set_top_field(doc, key, on.then_some(text.as_str()), true)
                            });
                        }
                    });
                    ui.end_row();
                }
            });
        ui.separator();
        ui.strong("Height extensions").on_hover_text(
            "Named heights added below the reference level: every point above the feet rises by dz",
        );
        let mut drop: Option<String> = None;
        egui::Grid::new("extensions").striped(true).show(ui, |ui| {
            for h in ["name", "dz", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for e in &t.extensions {
                ui.label(&e.name);
                let mut dz = e.dz.si();
                let r = ui.add(
                    egui::DragValue::new(&mut dz)
                        .speed(0.05)
                        .max_decimals(3)
                        .suffix(" m"),
                );
                hold(f, &r, "extension");
                if r.changed() {
                    f.edit("extension", |doc, _| edit::set_extension(doc, &e.name, dz));
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("Delete this extension")
                    .clicked()
                {
                    drop = Some(e.name.clone());
                }
                ui.end_row();
            }
        });
        if let Some(n) = drop {
            f.edit("delete extension", |doc, _| edit::remove_extension(doc, &n));
        }
        if ui.button("+ Extension").clicked() {
            let next = (1..)
                .map(|k| format!("+{}", 3 * k))
                .find(|n| !t.extensions.iter().any(|e| &e.name == n))
                .expect("a free name");
            let dz: f64 = next[1..].parse().unwrap_or(3.0);
            f.edit("add extension", |doc, _| {
                edit::set_extension(doc, &next, dz)
            });
        }
    }

    fn points_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        egui::Grid::new("points").striped(true).show(ui, |ui| {
            for h in ["attachment point", "x", "y", "z", "on", "insulators"] {
                ui.strong(h);
            }
            ui.end_row();
            for p in &t.points {
                let hanging = t.strings.iter().filter(|x| x.top == p.label).count();
                let h = Handle::Point(p.label.clone());
                row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    h.clone(),
                    &p.label,
                );
                let mut at = p.position().map(|v| v.si());
                let free = p.node.is_none() && p.member.is_none();
                let mut moved = false;
                for v in &mut at {
                    if free {
                        let r = coord(ui, v);
                        hold(f, &r, &format!("move point {}", p.label));
                        moved |= r.changed();
                    } else {
                        ui.label(format!("{v:.3} m"))
                            .on_hover_text("On a joint or member: moved with it");
                    }
                }
                if moved
                    && let Err(e) = f.edit(&format!("move point {}", p.label), |doc, t| {
                        edit::move_to(doc, t, &h, at)
                    })
                {
                    self.message = Some((e, true));
                }
                ui.label(match (&p.node, &p.member, p.at) {
                    (Some(n), _, _) => format!("joint {n}"),
                    (_, Some(m), Some(x)) => format!("member {m} at {x}"),
                    _ => "—".into(),
                });
                ui.label(hanging.to_string());
                ui.end_row();
            }
        });
        if t.points.is_empty() {
            ui.label(
                egui::RichText::new("No point yet: + Point (or Point here on a joint, Body).")
                    .weak(),
            );
        }
    }

    fn wires_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType, g: &Geometry) {
        let Some(f) = &mut self.file else {
            return;
        };
        egui::Grid::new("wires").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("wire point", "Its label"),
                (
                    "x",
                    "Placed by its insulator(s); typed only for one no insulator reaches (fixed)",
                ),
                ("y", ""),
                ("z", ""),
                ("held by", "The insulators ending here (two: a V)"),
                ("below it", "Insulators hanging from it"),
                ("set", "A set on it: a wire is strung here (Set points)"),
                ("counterweight", "A weight hanging at it"),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for w in &t.wires {
                let h = Handle::Wire(w.label.clone());
                row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    h.clone(),
                    &w.label,
                );
                let held: Vec<&str> = t
                    .strings
                    .iter()
                    .filter(|x| x.wire == w.label)
                    .map(|x| x.label.as_str())
                    .collect();
                let mut at = g.at(&w.label).unwrap_or([0.0; 3]);
                if held.is_empty() {
                    let mut moved = false;
                    for v in &mut at {
                        let r = coord(ui, v);
                        hold(f, &r, &format!("move wire point {}", w.label));
                        moved |= r.changed();
                    }
                    if moved {
                        let _ = f.edit(&format!("move wire point {}", w.label), |doc, t| {
                            edit::move_to(doc, t, &h, at)
                        });
                    }
                } else {
                    for v in at {
                        ui.label(format!("{v:.3} m")).on_hover_text(
                            "Placed by its insulator(s): change their length or direction",
                        );
                    }
                }
                ui.label(if held.is_empty() {
                    "— (fixed)".to_owned()
                } else {
                    held.join(", ")
                });
                let below: Vec<&str> = t
                    .strings
                    .iter()
                    .filter(|x| x.top == w.label)
                    .map(|x| x.label.as_str())
                    .collect();
                ui.label(below.join(", "));
                ui.label(w.set.map_or("—".to_owned(), |s| s.to_string()));
                let mut cw = w.weight.map_or(0.0, |x| x.si());
                let r = ui.add(
                    egui::DragValue::new(&mut cw)
                        .range(0.0..=1e6)
                        .speed(10.0)
                        .max_decimals(0)
                        .suffix(" N"),
                );
                hold(f, &r, "counterweight");
                if r.changed() {
                    f.edit("counterweight", |doc, _| {
                        edit::set_counterweight(doc, &w.label, cw)
                    });
                }
                ui.end_row();
            }
        });
    }

    fn sets_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let kind = |w: &str| {
            let on: Vec<&ols_structures::TypeString> =
                t.strings.iter().filter(|x| x.wire == w).collect();
            if on.is_empty() {
                "fixed (no insulator)"
            } else if on.iter().any(|x| x.strain) {
                "strain"
            } else {
                "suspension"
            }
        };
        let mut drop_set: Option<String> = None;
        egui::Grid::new("sets").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("set", "Its number: sections and jumpers end here (unique on the structure)"),
                ("on wire point", "The wire point it is on"),
                ("kind", "From its insulator: suspension (the wire runs through) or strain (the wire ends)"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            let mut sets: Vec<&ols_structures::TypeWire> =
                t.wires.iter().filter(|w| w.set.is_some()).collect();
            sets.sort_by_key(|w| w.set);
            for w in sets {
                let set = w.set.expect("filtered");
                let mut n = i64::from(set);
                let r = ui.add(egui::DragValue::new(&mut n).range(1..=99_999));
                if r.changed() {
                    let n = u32::try_from(n).unwrap_or(set);
                    if let Err(e) = f.edit("set number", |doc, t| edit::set_set(doc, t, &w.label, n)) {
                        self.message = Some((e, true));
                    }
                }
                let mut on = w.label.clone();
                egui::ComboBox::from_id_salt(("set-on", set))
                    .selected_text(&on)
                    .show_ui(ui, |ui| {
                        for x in t.wires.iter().filter(|x| x.set.is_none() || x.label == w.label) {
                            ui.selectable_value(&mut on, x.label.clone(), &x.label);
                        }
                    });
                if on != w.label {
                    f.edit("move set", |doc, _| edit::move_set(doc, &w.label, &on, set));
                }
                ui.label(kind(&w.label));
                if ui
                    .small_button("🗑")
                    .on_hover_text("Take the set off (the wire point stays)")
                    .clicked()
                {
                    drop_set = Some(w.label.clone());
                }
                ui.end_row();
            }
        });
        if let Some(l) = drop_set {
            f.edit("take the set off", |doc, _| edit::drop_set(doc, &l));
        }
        let free: Vec<&str> = t
            .wires
            .iter()
            .filter(|w| w.set.is_none())
            .map(|w| w.label.as_str())
            .collect();
        let mut add: Option<String> = None;
        ui.add_enabled_ui(!free.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("set-add")
                .selected_text("+ Set on wire point…")
                .show_ui(ui, |ui| {
                    for l in &free {
                        if ui.selectable_label(false, *l).clicked() {
                            add = Some((*l).to_owned());
                        }
                    }
                })
                .response
                .on_hover_text("A set where a wire will be strung: the next free number");
        });
        if let Some(l) = add
            && let Some(n) = f.edit("add set", |doc, t| edit::add_set(doc, t, &l))
        {
            self.message = Some((format!("set {n} on {l}"), false));
        }
    }

    fn strings_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType, g: &Geometry) {
        let Some(f) = &mut self.file else {
            return;
        };
        let insulators: &InsulatorLibrary = &self.insulators;
        let ahead = self.ahead;
        let tops: Vec<&str> = t
            .points
            .iter()
            .map(|p| p.label.as_str())
            .chain(t.wires.iter().map(|w| w.label.as_str()))
            .collect();
        let assemblies: Vec<&ols_structures::InsulatorAssembly> = insulators.iter().collect();
        let picked = self
            .selected
            .iter()
            .find_map(|h| match h {
                Handle::Point(l) | Handle::Wire(l) => Some(l.clone()),
                _ => None,
            })
            .or_else(|| t.points.first().map(|p| p.label.clone()));
        ui.horizontal_wrapped(|ui| {
            let from = picked.clone().unwrap_or_default();
            let can = picked.is_some();
            let length = self.string_length;
            if ui
                .add_enabled(can, egui::Button::new("+ New insulator"))
                .on_hover_text(format!(
                    "From {from} (the point picked), {length:.2} m, hanging, to a new wire point"
                ))
                .on_disabled_hover_text("Add an attachment point first")
                .clicked()
            {
                match f.edit("new insulator", |doc, t| {
                    edit::new_string(doc, t, &from, None, length, None)
                }) {
                    Ok((l, w)) => {
                        self.message = Some((format!("insulator {l} from {from} to {w}"), false));
                        self.selected = vec![Handle::Wire(w)];
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            let mut chosen: Option<String> = None;
            ui.add_enabled_ui(can && !assemblies.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("insulator-from-library")
                    .selected_text("From the library…")
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        for a in &assemblies {
                            if ui
                                .selectable_label(false, format!("{} — {}", a.id, a.name))
                                .clicked()
                            {
                                chosen = Some(a.id.clone());
                            }
                        }
                    })
                    .response
                    .on_hover_text(format!("An insulator of the library from {from}"))
                    .on_disabled_hover_text("No insulator assemblies next to the file");
            });
            if let Some(id) = chosen
                && let Some(a) = assemblies.iter().find(|a| a.id == id)
            {
                match f.edit("new insulator", |doc, t| {
                    edit::new_string(doc, t, &from, Some(a), a.length.si(), None)
                }) {
                    Ok((l, w)) => {
                        self.message =
                            Some((format!("insulator {l} ({id}) from {from} to {w}"), false));
                        self.selected = vec![Handle::Wire(w)];
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            ui.label("length");
            ui.add(
                egui::DragValue::new(&mut self.string_length)
                    .range(0.05..=50.0)
                    .speed(0.05)
                    .max_decimals(3)
                    .suffix(" m"),
            );
        });
        let mut changes: Vec<(String, StringChange, Option<String>)> = Vec::new();
        let mut delete: Option<String> = None;
        egui::Grid::new("strings").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("insulator", "Its label"),
                (
                    "hangs from",
                    "An attachment point, or another insulator's wire point",
                ),
                ("wire point", "Where it ends: another insulator's makes a V"),
                (
                    "kind",
                    "Suspension: the wire runs through its set. Strain: the wire ends at its set",
                ),
                (
                    "assembly",
                    "An assembly of the insulator library, or none: a length",
                ),
                (
                    "length",
                    "From its top to its wire point: it places the wire point",
                ),
                (
                    "angle",
                    "From straight down: 0° hanging, 90° level, 180° up",
                ),
                (
                    "direction",
                    "In plan, clockwise seen from above (ahead at the angle given)",
                ),
                ("weight", "Its weight (default: its assembly's)"),
                (
                    "wind area",
                    "The area the wind blows on (default: its assembly's)",
                ),
                (
                    "compression",
                    "The push it may take (V legs); none: tension only",
                ),
                (
                    "limits",
                    "Kept with the insulator, not checked by OLS CAD (for a plugin or a report): \
                     the tension it is rated for, the swing from straight down it may take (any \
                     direction), across and along the line. Unticked: not given — its \
                     assembly's, if it gives one (shown dimmed)",
                ),
                ("description", "What it is, in words"),
                ("", ""),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for s in &t.strings {
                let a = s
                    .insulator
                    .as_ref()
                    .and_then(|i| assemblies.iter().find(|a| &a.id == i).copied());
                let mut put = |c: StringChange, held: Option<String>| {
                    changes.push((s.label.clone(), c, held));
                };
                ui.label(&s.label);
                let mut top = s.top.clone();
                egui::ComboBox::from_id_salt(("string-top", &s.label))
                    .selected_text(&top)
                    .show_ui(ui, |ui| {
                        for l in tops.iter().filter(|l| **l != s.wire) {
                            ui.selectable_value(&mut top, (*l).to_owned(), *l);
                        }
                    });
                if top != s.top {
                    put(StringChange::Top(top), None);
                }
                let mut wire = s.wire.clone();
                egui::ComboBox::from_id_salt(("string-wire", &s.label))
                    .selected_text(&wire)
                    .show_ui(ui, |ui| {
                        for w in t.wires.iter().filter(|w| w.label != s.top) {
                            ui.selectable_value(&mut wire, w.label.clone(), &w.label);
                        }
                    });
                if wire != s.wire {
                    put(StringChange::Wire(wire), None);
                }
                let mut strain = s.strain;
                egui::ComboBox::from_id_salt(("string-clamp", &s.label))
                    .selected_text(if strain { "strain" } else { "suspension" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut strain, false, "suspension");
                        ui.selectable_value(&mut strain, true, "strain");
                    });
                if strain != s.strain {
                    put(StringChange::Strain(strain), None);
                }
                let mut pick = s.insulator.clone();
                egui::ComboBox::from_id_salt(("string-insulator", &s.label))
                    .selected_text(pick.as_deref().unwrap_or("— (a length)"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, None, "— (a length)");
                        for a in &assemblies {
                            ui.selectable_value(&mut pick, Some(a.id.clone()), &a.id)
                                .on_hover_text(format!(
                                    "{}: {:.3} m, {:.0} N{}",
                                    a.name,
                                    a.length.si(),
                                    a.weight.si(),
                                    if a.kind == AssemblyKind::Strain {
                                        ", strain"
                                    } else {
                                        ""
                                    }
                                ));
                        }
                    });
                if pick != s.insulator {
                    put(StringChange::Assembly(pick), None);
                }
                let ends = g.at(&s.top).zip(g.at(&s.wire));
                let len_now = ends.map_or(0.0, |(a, b)| polar(a, b).2);
                let mut l = a.map_or(s.length.map_or(len_now, |l| l.si()), |a| a.length.si());
                if a.is_some() {
                    ui.label(format!("{l:.3} m"))
                        .on_hover_text("Its assembly's");
                } else {
                    let r = ui.add(
                        egui::DragValue::new(&mut l)
                            .range(0.01..=50.0)
                            .speed(0.01)
                            .max_decimals(3)
                            .suffix(" m"),
                    );
                    if r.changed() {
                        put(
                            StringChange::Length(l),
                            r.is_pointer_button_down_on()
                                .then(|| "insulator length".into()),
                        );
                    }
                }
                let legs = t.strings.iter().filter(|x| x.wire == s.wire).count();
                if legs == 1 {
                    let (angle, direction) = s.angles();
                    let (mut dn, mut dr) = (angle, (direction + ahead).rem_euclid(360.0));
                    let a1 = ui.add(
                        egui::DragValue::new(&mut dn)
                            .range(0.0..=180.0)
                            .speed(0.5)
                            .max_decimals(2)
                            .suffix("°"),
                    );
                    let a2 = ui.add(
                        egui::DragValue::new(&mut dr)
                            .range(0.0..=360.0)
                            .speed(0.5)
                            .max_decimals(2)
                            .suffix("°"),
                    );
                    if a1.changed() || a2.changed() {
                        let held = (a1.is_pointer_button_down_on()
                            || a2.is_pointer_button_down_on())
                        .then(|| "insulator direction".into());
                        put(
                            StringChange::Direction(dn, (dr - ahead).rem_euclid(360.0)),
                            held,
                        );
                    }
                } else {
                    let (dn, dr, _) = ends.map_or((0.0, 0.0, 0.0), |(a, b)| polar(a, b));
                    let why = "A leg of a V: its direction follows where the legs meet";
                    ui.label(format!("{dn:.2}°")).on_hover_text(why);
                    ui.label(format!("{:.2}°", (dr + ahead).rem_euclid(360.0)))
                        .on_hover_text(why);
                }
                let mut w = s
                    .weight
                    .map(|w| w.si())
                    .or(a.map(|a| a.weight.si()))
                    .unwrap_or(0.0);
                let r = ui.add(
                    egui::DragValue::new(&mut w)
                        .range(0.0..=1e6)
                        .speed(1.0)
                        .max_decimals(1)
                        .suffix(" N"),
                );
                if r.changed() {
                    put(
                        StringChange::Weight(w),
                        r.is_pointer_button_down_on()
                            .then(|| "insulator weight".into()),
                    );
                }
                let mut wa = s
                    .wind_area
                    .map(|w| w.si())
                    .or(a.map(|a| a.wind_area.si()))
                    .unwrap_or(0.0);
                let r = ui.add(
                    egui::DragValue::new(&mut wa)
                        .range(0.0..=100.0)
                        .speed(0.005)
                        .max_decimals(3)
                        .suffix(" m²"),
                );
                if r.changed() {
                    put(
                        StringChange::WindArea(wa),
                        r.is_pointer_button_down_on().then(|| "wind area".into()),
                    );
                }
                ui.horizontal(|ui| {
                    let mut push = s.compression.is_some();
                    let mut c = s.compression.map_or(0.0, |c| c.si());
                    let a = ui.checkbox(&mut push, "");
                    let b = ui.add_enabled(
                        push,
                        egui::DragValue::new(&mut c)
                            .range(0.0..=1e7)
                            .speed(10.0)
                            .max_decimals(0)
                            .suffix(" N"),
                    );
                    if a.changed() || b.changed() {
                        put(
                            StringChange::Compression(push.then_some(c)),
                            b.is_pointer_button_down_on().then(|| "compression".into()),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let own = s.limits();
                    let from = a.map(|a| a.limits()).unwrap_or_default();
                    let fields = edit::limit_fields(own, from);
                    for edit::LimitField {
                        name,
                        own: v,
                        inherited,
                        start: default,
                        angle,
                        change: make,
                    } in fields
                    {
                        let mut on = v.is_some();
                        let mut x = v.or(inherited).unwrap_or(default);
                        let tick = ui.checkbox(&mut on, name);
                        let d = if on {
                            ui.add(if angle {
                                egui::DragValue::new(&mut x)
                                    .range(0.0..=180.0)
                                    .speed(0.5)
                                    .max_decimals(2)
                                    .suffix("°")
                            } else {
                                egui::DragValue::new(&mut x)
                                    .range(1.0..=1e9)
                                    .speed(100.0)
                                    .max_decimals(0)
                                    .suffix(" N")
                            })
                        } else if let Some(i) = inherited {
                            ui.weak(if angle {
                                format!("{i:.2}°")
                            } else {
                                format!("{i:.0} N")
                            })
                            .on_hover_text("Its assembly's")
                        } else {
                            ui.weak("—")
                        };
                        if tick.changed() || (on && d.changed()) {
                            put(
                                make(on.then_some(x)),
                                d.is_pointer_button_down_on()
                                    .then(|| format!("insulator {name} limit")),
                            );
                        }
                    }
                });
                let mut d = s.description.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut d).desired_width(160.0))
                    .changed()
                {
                    put(StringChange::Description(d), None);
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("Delete this insulator")
                    .clicked()
                {
                    delete = Some(s.label.clone());
                }
                ui.end_row();
            }
        });
        for (label, change, held) in changes {
            if let Some(what) = &held {
                f.begin(what);
            }
            if let Err(e) = f.edit(&format!("insulator {label}"), |doc, t| {
                edit::set_string(doc, t, g, insulators, &label, change)
            }) {
                self.message = Some((e, true));
            }
        }
        if let Some(label) = delete {
            f.edit("delete insulator", |doc, t| {
                edit::delete_string(doc, t, g, &label)
            });
            self.message = Some((format!("deleted insulator {label}"), false));
        }
        if t.strings.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No insulator yet: pick an attachment point, then + New insulator or From the library.",
                )
                .weak(),
            );
        }
    }

    fn joints_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        egui::Grid::new("joints").striped(true).show(ui, |ui| {
            for h in ["joint", "x", "y", "z", "members"] {
                ui.strong(h);
            }
            ui.end_row();
            for n in &t.nodes {
                let h = Handle::Joint(n.id.clone());
                row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    h.clone(),
                    &n.id,
                );
                let mut p = n.point.map(|v| v.si());
                let mut moved = false;
                for v in &mut p {
                    let r = coord(ui, v);
                    hold(f, &r, &format!("move joint {}", n.id));
                    moved |= r.changed();
                }
                if moved {
                    let _ = f.edit(&format!("move joint {}", n.id), |doc, t| {
                        edit::symmetry::move_to(doc, t, &h, p)
                    });
                }
                ui.label(
                    t.members
                        .iter()
                        .filter(|m| m.nodes.contains(&n.id))
                        .count()
                        .to_string(),
                );
                ui.end_row();
            }
        });
        if t.nodes.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No joint yet: Add joint, then Member between two joints picked.",
                )
                .weak(),
            );
        }
    }

    fn supports_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        egui::Grid::new("supports").striped(true).show(ui, |ui| {
            for h in ["joint", "z", "support"] {
                ui.strong(h);
            }
            ui.end_row();
            for n in &t.nodes {
                row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    Handle::Joint(n.id.clone()),
                    &n.id,
                );
                ui.label(format!("{:.3} m", n.point[2].si()));
                let support = n.support.as_ref().map_or("—".to_owned(), |s| {
                    s.as_str().map_or(s.to_string(), str::to_owned)
                });
                let mut chosen = support.clone();
                egui::ComboBox::from_id_salt(("support", &n.id))
                    .selected_text(&chosen)
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for s in ["—", "pinned", "fixed"] {
                            ui.selectable_value(&mut chosen, s.to_owned(), s);
                        }
                    });
                if chosen != support {
                    f.edit(&format!("support of {}", n.id), |doc, _| {
                        edit::set_support(doc, &n.id, (chosen != "—").then_some(chosen.as_str()))
                    });
                }
                ui.end_row();
            }
        });
        let lowest = t
            .nodes
            .iter()
            .map(|n| n.point[2].si())
            .fold(f64::INFINITY, f64::min);
        if lowest.is_finite()
            && ui
                .button("Support the feet (pinned)")
                .on_hover_text("Every joint at the lowest level pinned")
                .clicked()
        {
            f.edit("support the feet", |doc, t| {
                for n in t
                    .nodes
                    .iter()
                    .filter(|n| (n.point[2].si() - lowest).abs() < 1e-6)
                {
                    edit::set_support(doc, &n.id, Some("pinned"));
                }
            });
        }
    }

    fn design_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let sections: Vec<String> = edit::item_ids(f.doc(), "section");
        // The analysis settings ([analysis]) that go with the members.
        let analysis = t.analysis.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.strong("Analysis")
                .on_hover_text("The finite-element check's settings ([analysis] of the file)");
            let mut stabilise = analysis
                .get("stabilise")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            if ui
                .checkbox(&mut stabilise, "stabilise joints")
                .on_hover_text(
                    "Hold the joint directions no member stiffens (a joint of members in one \
                     plane, across it; a frame free to spin) when the model would otherwise be \
                     a mechanism; the results list them",
                )
                .changed()
                && let Err(e) = f.edit("stabilise joints", |doc, _| {
                    edit::set_analysis_field(doc, "stabilise", if stabilise { "" } else { "false" })
                })
            {
                self.message = Some((e, true));
            }
        });
        egui::Grid::new("design").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("member", ""),
                (
                    "section",
                    "A [[section]] of the file (Sections); none: a stick",
                ),
                (
                    "kind",
                    "truss (axial only), frame (bending too) or cable (tension only)",
                ),
                ("k", "Effective length factor for buckling (none: 1)"),
            ] {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for m in &t.members {
                row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    Handle::Member(m.id.clone()),
                    &m.id,
                );
                let current = m.section.clone().unwrap_or_else(|| "—".into());
                let mut chosen = current.clone();
                egui::ComboBox::from_id_salt(("section", &m.id))
                    .selected_text(&chosen)
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, "—".to_owned(), "— (a stick)");
                        for s in &sections {
                            ui.selectable_value(&mut chosen, s.clone(), s);
                        }
                    });
                if chosen != current {
                    f.edit(&format!("section of {}", m.id), |doc, _| {
                        edit::set_member_section(
                            doc,
                            &m.id,
                            (chosen != "—").then_some(chosen.as_str()),
                        )
                    });
                }
                let kind = m.kind.clone().unwrap_or_else(|| "truss".into());
                let mut new_kind = kind.clone();
                egui::ComboBox::from_id_salt(("kind", &m.id))
                    .selected_text(&new_kind)
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for k in ["truss", "frame", "cable"] {
                            ui.selectable_value(&mut new_kind, k.to_owned(), k);
                        }
                    });
                if new_kind != kind {
                    f.edit(&format!("kind of {}", m.id), |doc, _| {
                        edit::set_member_kind(doc, &m.id, &new_kind)
                    });
                }
                ui.horizontal(|ui| {
                    let mut on = m.k.is_some();
                    let mut k = m.k.unwrap_or(1.0);
                    let a = ui.checkbox(&mut on, "");
                    let b = ui.add_enabled(
                        on,
                        egui::DragValue::new(&mut k)
                            .range(0.1..=5.0)
                            .speed(0.01)
                            .max_decimals(3),
                    );
                    hold(f, &b, "k");
                    if a.changed() || b.changed() {
                        f.edit(&format!("k of {}", m.id), |doc, _| {
                            edit::set_member_k(doc, &m.id, on.then_some(k))
                        });
                    }
                });
                ui.end_row();
            }
        });
        if t.members.is_empty() {
            ui.label(egui::RichText::new("No member yet: build the body first (Body).").weak());
        } else if sections.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No [[section]] yet: add sections and their materials (Sections, Materials).",
                )
                .weak(),
            );
        }
    }

    /// The `[[section]]` or `[[material]]` tables: their fields typed as
    /// text (values with their units), added, removed.
    fn items_sheet(&mut self, ui: &mut egui::Ui, key: &str) {
        let Some(f) = &mut self.file else {
            return;
        };
        let ids = edit::item_ids(f.doc(), key);
        let mut drop: Option<String> = None;
        for id in &ids {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("{key} {id}"));
                    if ui
                        .small_button("🗑")
                        .on_hover_text(format!("Delete {key} {id}"))
                        .clicked()
                    {
                        drop = Some(id.clone());
                    }
                });
                let mut fields = edit::item_fields(f.doc(), key, id);
                fields.push((String::new(), String::new()));
                egui::Grid::new((key, id.as_str())).show(ui, |ui| {
                    for (k, (field, value)) in fields.into_iter().enumerate() {
                        let new = field.is_empty();
                        let fkey = format!("{key}/{id}/{k}/field");
                        let vkey = format!("{key}/{id}/{k}/value");
                        let fb = self
                            .buffers
                            .entry(fkey.clone())
                            .or_insert_with(|| field.clone());
                        let fr = ui.add_enabled(
                            new,
                            egui::TextEdit::singleline(fb)
                                .desired_width(110.0)
                                .hint_text("new field"),
                        );
                        let vb = self
                            .buffers
                            .entry(vkey.clone())
                            .or_insert_with(|| value.clone());
                        let vr = ui.add(
                            egui::TextEdit::singleline(vb)
                                .desired_width(160.0)
                                .hint_text("value"),
                        );
                        // Written when the value is left (Enter, Tab or a click
                        // elsewhere): one undo step per field.
                        if vr.lost_focus() || (new && fr.lost_focus()) {
                            let name = self.buffers.get(&fkey).cloned().unwrap_or_default();
                            let typed = self.buffers.get(&vkey).cloned().unwrap_or_default();
                            if (!name.trim().is_empty() && typed != value)
                                || (new && !typed.trim().is_empty())
                            {
                                match f.edit(&format!("{key} {id}: {name}"), |doc, _| {
                                    edit::set_item_field(doc, key, id, &name, &typed)
                                }) {
                                    Ok(()) => {}
                                    Err(e) => self.message = Some((e, true)),
                                }
                                self.buffers
                                    .retain(|k, _| !k.starts_with(&format!("{key}/{id}/")));
                            }
                        }
                        ui.end_row();
                    }
                });
            });
        }
        if let Some(id) = drop {
            f.edit(&format!("delete {key} {id}"), |doc, _| {
                edit::remove_item(doc, key, "id", &id)
            });
            self.buffers.clear();
        }
        let label = format!(
            "+ {}",
            if key == "section" {
                "Section"
            } else {
                "Material"
            }
        );
        if ui.button(label).clicked() {
            let id = if key == "section" {
                let material = edit::item_ids(f.doc(), "material").into_iter().next();
                let mut fields = vec![("area", "1000 mm2"), ("i_min", "40 cm4")];
                let m = material.unwrap_or_default();
                if !m.is_empty() {
                    fields.insert(0, ("material", m.as_str()));
                }
                f.edit("add section", |doc, _| {
                    edit::add_item(doc, key, "S1", &fields)
                })
            } else {
                f.edit("add material", |doc, _| {
                    edit::add_item(
                        doc,
                        key,
                        "steel",
                        &[
                            ("elasticity", "210 GPa"),
                            ("shear_modulus", "81 GPa"),
                            ("density", "7850 kg/m3"),
                            ("yield", "355 MPa"),
                        ],
                    )
                })
            };
            self.message = Some((format!("added {key} {id}: type its values"), false));
            self.buffers.clear();
        }
        if let Some(e) = f.error() {
            error_label(ui, e);
        }
    }
}
