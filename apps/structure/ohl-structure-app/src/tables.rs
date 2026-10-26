// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The panels of tables (Geometry, Body, Design): their tools and tables,
//! and the actions their menus share with the tools. Every change is one
//! edit of the file (one undo step; a value dragged is one step).

use eframe::egui;
use ohl_structure_edit::{self as edit, Geometry, Handle, StringChange, StructureFile, polar};
use ohl_structures::{AssemblyKind, InsulatorLibrary, StructureType};

use crate::app::{Sheet, StructureApp, error_label};
use crate::layout::{Area, Panel};

/// A new material's values: structural steel (S355).
const STEEL: [(&str, &str); 4] = [
    ("elasticity", "210 GPa"),
    ("shear_modulus", "81 GPa"),
    ("density", "7850 kg/m3"),
    ("yield", "355 MPa"),
];

/// A length typed or dragged in a table.
/// (Its unit, m, is in the column's header.)
fn coord(ui: &mut egui::Ui, v: &mut f64) -> egui::Response {
    crate::style::field(ui, egui::DragValue::new(v).speed(0.01).max_decimals(3))
}

/// A text cell of a table (a value with its unit): kept in `buffers`
/// under `key` while it is typed in, the text typed when it is left
/// (Enter, Tab or a click elsewhere) and differs from `current` — to be
/// written as one edit (one undo step).
pub(crate) fn text_cell(
    ui: &mut egui::Ui,
    buffers: &mut std::collections::HashMap<String, String>,
    key: String,
    current: &str,
    hint: &str,
    tip: &str,
) -> Option<String> {
    let mut text = buffers
        .get(&key)
        .cloned()
        .unwrap_or_else(|| current.to_owned());
    let r = ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(70.0)
                .hint_text(hint),
        )
        .on_hover_text(tip);
    if r.lost_focus() {
        buffers.remove(&key);
        return (text.trim() != current.trim()).then_some(text);
    }
    if r.has_focus() {
        buffers.insert(key, text);
    } else {
        buffers.remove(&key);
    }
    None
}

/// The member fields of the finite-element check typed as text in the
/// Member sections and Groups tables: (field, heading, what it is, hint).
pub(crate) const MEMBER_FIELDS: [(&str, &str, &str, &str); 5] = [
    (
        "lk",
        "lk",
        "Effective length for buckling, instead of k × length (empty: k × length)",
        "k × L",
    ),
    (
        "orientation",
        "legs",
        "An angle's legs (local y, z) towards two structure directions, e.g. x+y- (empty: \
         reference and roll)",
        "x+y-",
    ),
    (
        "capacity_tension",
        "N tension",
        "Tension capacity, instead of the cross-section's (empty: the cross-section's)",
        "section",
    ),
    (
        "capacity_compression",
        "N compression",
        "Compression capacity, instead of the cross-section's and the buckling load (empty: computed)",
        "computed",
    ),
    (
        "dead_load_factor",
        "DLF",
        "Factor on its self-weight (bolts, plates), instead of the analysis's dead-load factor",
        "analysis",
    ),
];

/// A member's field of [`MEMBER_FIELDS`] as written.
pub(crate) fn member_field<'a>(
    m: &'a ohl_structures::structures::TypeMember,
    field: &str,
) -> Option<&'a toml::Value> {
    match field {
        "lk" => m.lk.as_ref(),
        "orientation" => m.orientation.as_ref(),
        "capacity_tension" => m.capacity_tension.as_ref(),
        "capacity_compression" => m.capacity_compression.as_ref(),
        "dead_load_factor" => m.dead_load_factor.as_ref(),
        _ => None,
    }
}

/// A member field as text (`"180 kN"`, `1.1`; empty: none).
pub(crate) fn field_text(v: Option<&toml::Value>) -> String {
    match v {
        None => String::new(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
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
) -> egui::Response {
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
    r
}

/// The kind of a joint, chosen in the Joints table: `given` as written,
/// `worked_out` as read (attach when a point is linked to it, support
/// with a support). The kind chosen (none: auto) when it changed.
fn kind_cell(
    ui: &mut egui::Ui,
    id: &str,
    given: Option<ohl_structures::JointKind>,
    worked_out: Option<ohl_structures::JointKind>,
) -> Option<Option<ohl_structures::JointKind>> {
    use ohl_structures::JointKind;
    let name = |k: Option<JointKind>| k.map_or("none", JointKind::as_str);
    let text = |k: Option<JointKind>| match k {
        Some(k) => k.as_str().to_owned(),
        None => format!("auto ({})", name(worked_out)),
    };
    let mut chosen = given;
    egui::ComboBox::from_id_salt(("joint-kind", id))
        .selected_text(text(given))
        .width(110.0)
        .show_ui(ui, |ui| {
            for k in [None, Some(JointKind::Attach), Some(JointKind::Support)] {
                ui.selectable_value(&mut chosen, k, text(k));
            }
        })
        .response
        .on_hover_text(match worked_out {
            Some(JointKind::Attach) => "An attach joint: strings hang here",
            Some(JointKind::Support) => "A support joint: the structure stands on it",
            None => "An ordinary joint (link a point to it to make it an attach joint)",
        });
    (chosen != given).then_some(chosen)
}

impl StructureApp {
    /// A panel of tables: its tools, the table chosen (when it has
    /// several), the table.
    pub fn sheet_panel(&mut self, ui: &mut egui::Ui, panel: Panel) {
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure_arc();
        let revision = f.revision();
        let g = self.geometry_of(&t, revision, None);
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
                Sheet::Library => self.section_library_sheet(ui),
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
            1 => "one string",
            2 => "two strings to one wire point",
            _ => return,
        };
        let Some(f) = &mut self.file else {
            return;
        };
        let length = self.string_length;
        match f.edit(name, |doc, t| edit::add_strings(doc, t, &tops, length)) {
            Ok(w) => {
                self.message = Some((
                    format!("added the strings to wire point {w}: put its set in Sets"),
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

    /// Joint `id` given kind `kind` (none: worked out): one edit (one undo
    /// step); refused for a joint carrying points made a support joint.
    pub fn set_joint_kind(&mut self, id: &str, kind: Option<ohl_structures::JointKind>) {
        let Some(f) = &mut self.file else {
            return;
        };
        let what = format!(
            "kind of joint {id}: {}",
            kind.map_or("auto", ohl_structures::JointKind::as_str)
        );
        self.message = Some(
            match f.edit(&what, |doc, t| {
                edit::links::set_joint_kind(doc, t, id, kind)
            }) {
                Ok(()) => (what, false),
                Err(e) => (e, true),
            },
        );
    }

    /// One joint picked that strings may hang from (not a support joint).
    pub fn picked_attachable_joint(&self) -> bool {
        match (&self.file, self.selected.as_slice()) {
            (Some(f), [Handle::Joint(j)]) => {
                f.structure().linkable_joints().iter().any(|x| &x.id == j)
            }
            _ => false,
        }
    }

    /// Body › Point on the joint: a point on the joint picked (not on a
    /// support joint: strings hang from attach joints).
    pub fn point_here(&mut self) {
        if !self.picked_attachable_joint() {
            self.message = Some((
                "Point here: pick one joint that is not a support joint (strings hang from \
                 attach joints)"
                    .into(),
                true,
            ));
            return;
        }
        let (Some(f), [Handle::Joint(j)]) = (&mut self.file, self.selected.as_slice()) else {
            return;
        };
        let j = j.clone();
        let l = f.edit("add point", |doc, t| edit::add_point_at_joint(doc, t, &j));
        self.selected = vec![Handle::Point(l.clone())];
        self.message = Some((format!("added point {l} on joint {j}"), false));
    }

    /// Delete…: what is picked, asked first.
    /// Asks before deleting what is picked (all of it, with what goes
    /// with each: one confirmation, one undo step).
    pub fn ask_delete(&mut self) {
        let Some(f) = &self.file else {
            return;
        };
        let mut picked = self.selected.iter();
        if let Some(first) = picked.next() {
            let mut r = edit::symmetry::removal(f.structure(), first);
            for h in picked {
                r.merge(edit::symmetry::removal(f.structure(), h));
            }
            self.confirm = Some(r);
        }
    }

    /// The tools of a Geometry or Body panel: snap, add, presets, delete,
    /// fit, what is picked.
    fn tools(&mut self, ui: &mut egui::Ui, _t: &StructureType, area: Area) {
        ui.horizontal_wrapped(|ui| {
            if area == Area::Geometry {
                if ui
                    .button("+ Point")
                    .on_hover_text("A point where strings hang, at the top of the stick figure (move it in the view or the table)")
                    .clicked()
                {
                    self.add_point();
                }
                let tops = self.picked_tops();
                for (name, n, help) in [
                    ("One string", 1, "A string from the point picked down to a new wire point (its set: Sets)"),
                    ("Two strings to one wire point", 2, "Two strings from the two points picked to one wire point below their middle"),
                ] {
                    if ui
                        .add_enabled(tops.len() == n, egui::Button::new(name))
                        .on_hover_text(help)
                        .on_disabled_hover_text(if n == 1 {
                            "Pick one point (or a wire point: a string below it)"
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
                    .add(crate::icons::Icon::AddJoint.button(ui, "Add joint"))
                    .on_hover_text("A joint above the joint picked (or at the centre)")
                    .clicked()
                {
                    self.add_joint();
                }
                let joints = self.picked_joints();
                if ui
                    .add_enabled(
                        joints.len() == 2,
                        crate::icons::Icon::AddMember.button(ui, "Member"),
                    )
                    .on_hover_text("A member between the two joints picked")
                    .on_disabled_hover_text("Pick two joints (Shift + click the second)")
                    .clicked()
                {
                    self.add_member();
                }
                if self.picked_attachable_joint()
                    && ui
                        .add(crate::icons::Icon::PointOnJoint.button(ui, "Point here"))
                        .on_hover_text("A point where strings hang, on this joint (it moves with it)")
                        .clicked()
                {
                    self.point_here();
                }
            }
            ui.separator();
            let mut held = self.link.is_some();
            if ui
                .toggle_value(&mut held, "Link")
                .on_hover_text(
                    "The Link tool: click an attachment point, then the joint to link it to \
                     (right-click or Escape: cancel)",
                )
                .changed()
            {
                self.toggle_link_tool();
            }
            if ui
                .add_enabled(!self.selected.is_empty(), egui::Button::new("Delete…"))
                .on_hover_text(
                    "Delete what is picked, with what goes with each (asks first; one undo step)",
                )
                .on_disabled_hover_text("Pick something first")
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
                        "Top of the shaft in the stick drawing (none: the highest point)",
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
        ui.strong("Extensions").on_hover_text(
            "Named heights added below the reference level: every point above the feet rises by dz",
        );
        let mut drop: Option<String> = None;
        egui::Grid::new("extensions").striped(true).show(ui, |ui| {
            for h in ["name", "dz [m]", ""] {
                crate::style::header(ui, h, "");
            }
            ui.end_row();
            for e in &t.extensions {
                ui.label(&e.name);
                let mut dz = e.dz.si();
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut dz).speed(0.05).max_decimals(3),
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

    /// The attachment points: label, where (typed: a point on a joint
    /// stays on it, its offset follows — owner decision 2026-10-21), its
    /// joint, its offset from it (typed for one on a joint), the offset's
    /// length, the strings hanging from it; a row's menu links or unlinks
    /// it.
    fn points_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let joints = t.joints();
        let mut action = None;
        egui::Grid::new("points").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("attachment point", "Where strings hang; right-click: link, unlink"),
                ("x [m]", "Where it is: typed, a point on a joint stays on it and its offset follows"),
                ("y [m]", ""),
                ("z [m]", ""),
                ("joint", "The joint it is linked to (Link tool, or its menu)"),
                ("offset x [m]", "From its joint to it: a rigid link, whose moment the analysis applies at the joint"),
                ("offset y [m]", ""),
                ("offset z [m]", ""),
                ("length [m]", "The offset's length"),
                ("strings", "Strings hanging from it"),
            ] {
                crate::style::header(ui, h, tip);
            }
            ui.end_row();
            for p in &t.points {
                let hanging = t.strings.iter().filter(|x| x.top == p.label).count();
                let h = Handle::Point(p.label.clone());
                let r = row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    h.clone(),
                    &p.label,
                );
                r.context_menu(|ui| {
                    if ui.button("Link to a joint…").clicked() {
                        action = Some(crate::link_tool::LinkAction::Start(p.label.clone()));
                        ui.close();
                    }
                    if ui.button("Unlink").clicked() {
                        action = Some(crate::link_tool::LinkAction::Unlink(p.label.clone()));
                        ui.close();
                    }
                });
                let mut at = p.position().map(|v| v.si());
                let free = p.node.is_none();
                let mut moved = false;
                for v in &mut at {
                    let r = coord(ui, v);
                    hold(f, &r, &format!("move point {}", p.label));
                    moved |= r.changed();
                }
                if moved
                    && let Err(e) = f.edit(&format!("move point {}", p.label), |doc, t| {
                        edit::move_to(doc, t, &h, at)
                    })
                {
                    self.message = Some((e, true));
                }
                let own = joints
                    .iter()
                    .any(|j| j.id == p.joint && j.source == ohl_structures::JointSource::Point);
                ui.label(if own {
                    format!("{} (its own)", p.joint)
                } else {
                    p.joint.clone()
                });
                let mut o = t.point_offset(p);
                let mut shifted = false;
                for v in &mut o {
                    if free {
                        ui.label("—").on_hover_text("Its own joint: no offset");
                    } else {
                        let r = coord(ui, v);
                        hold(f, &r, &format!("offset of point {}", p.label));
                        shifted |= r.changed();
                    }
                }
                if shifted
                    && let Err(e) = f.edit(&format!("offset of point {}", p.label), |doc, t| {
                        edit::links::link_point(doc, t, &p.label, &p.joint, o)
                    })
                {
                    self.message = Some((e, true));
                }
                crate::style::num_label(
                    ui,
                    format!("{:.3}", (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt()),
                );
                crate::style::num_label(ui, hanging.to_string());
                ui.end_row();
            }
        });
        if let Some(a) = action {
            self.link_action(a);
        }
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
                    "x [m]",
                    "Placed by its string(s); typed only for one no string reaches (fixed)",
                ),
                ("y [m]", ""),
                ("z [m]", ""),
                (
                    "held by",
                    "The strings ending here (one, or two to one wire point)",
                ),
                ("below it", "Strings hanging from it"),
                ("set", "A set on it: a wire is strung here (Sets)"),
                ("counterweight [N]", "A weight hanging at it"),
            ] {
                crate::style::header(ui, h, tip);
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
                        crate::style::num_label(ui, format!("{v:.3}")).on_hover_text(
                            "Placed by its string(s): change their length or direction",
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
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut cw)
                        .range(0.0..=1e6)
                        .speed(10.0)
                        .max_decimals(0),
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
            let on: Vec<&ohl_structures::TypeString> =
                t.strings.iter().filter(|x| x.wire == w).collect();
            if on.is_empty() {
                "fixed (no string)"
            } else if on.iter().any(|x| x.strain) {
                "strain"
            } else {
                "suspension"
            }
        };
        let mut drop_set: Option<String> = None;
        egui::Grid::new("sets").striped(true).show(ui, |ui| {
            for (h, tip) in [
                (
                    "set",
                    "Its number: sections and jumpers end here (unique on the structure)",
                ),
                ("on wire point", "The wire point it is on"),
                (
                    "kind",
                    "From its string: suspension (the wire runs through) or strain (the wire ends)",
                ),
                ("", ""),
            ] {
                crate::style::header(ui, h, tip);
            }
            ui.end_row();
            let mut sets: Vec<&ohl_structures::TypeWire> =
                t.wires.iter().filter(|w| w.set.is_some()).collect();
            sets.sort_by_key(|w| w.set);
            for w in sets {
                let set = w.set.expect("filtered");
                let mut n = i64::from(set);
                let r = ui.add(egui::DragValue::new(&mut n).range(1..=99_999));
                if r.changed() {
                    let n = u32::try_from(n).unwrap_or(set);
                    if let Err(e) =
                        f.edit("set number", |doc, t| edit::set_set(doc, t, &w.label, n))
                    {
                        self.message = Some((e, true));
                    }
                }
                let mut on = w.label.clone();
                egui::ComboBox::from_id_salt(("set-on", set))
                    .selected_text(&on)
                    .show_ui(ui, |ui| {
                        for x in t
                            .wires
                            .iter()
                            .filter(|x| x.set.is_none() || x.label == w.label)
                        {
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
        let assemblies: Vec<&ohl_structures::InsulatorAssembly> = insulators.iter().collect();
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
                .add_enabled(can, egui::Button::new("+ New string"))
                .on_hover_text(format!(
                    "From {from} (the point picked), {length:.2} m, hanging, to a new wire point"
                ))
                .on_disabled_hover_text("Add an attachment point first")
                .clicked()
            {
                match f.edit("new string", |doc, t| {
                    edit::new_string(doc, t, &from, None, length, None)
                }) {
                    Ok((l, w)) => {
                        self.message = Some((format!("string {l} from {from} to {w}"), false));
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
                    .on_hover_text(format!("An assembly of the library from {from}"))
                    .on_disabled_hover_text("No insulator assemblies next to the file");
            });
            if let Some(id) = chosen
                && let Some(a) = assemblies.iter().find(|a| a.id == id)
            {
                match f.edit("new string", |doc, t| {
                    edit::new_string(doc, t, &from, Some(a), a.length.si(), None)
                }) {
                    Ok((l, w)) => {
                        self.message =
                            Some((format!("string {l} ({id}) from {from} to {w}"), false));
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
                    "An attachment point, or another string's wire point",
                ),
                ("wire point", "Where it ends: another string's makes two strings to one wire point"),
                (
                    "kind",
                    "Suspension: the wire runs through its set. Strain: the wire ends at its set",
                ),
                (
                    "assembly",
                    "An assembly of the insulator library, or none: a length",
                ),
                (
                    "length [m]",
                    "From its top to its wire point: it places the wire point",
                ),
                (
                    "angle [°]",
                    "From straight down: 0° hanging, 90° level, 180° up",
                ),
                (
                    "direction [°]",
                    "In plan, clockwise seen from above (ahead at the angle given)",
                ),
                ("weight [N]", "Its weight (default: its assembly's)"),
                (
                    "wind area [m²]",
                    "The area the wind blows on (default: its assembly's)",
                ),
                (
                    "compression [N]",
                    "The push it may take (two strings to one wire point); none: tension only",
                ),
                (
                    "limits",
                    "Kept with the string, not checked by OpenOHL (for a plugin or a report): \
                     the tension it is rated for, the swing from straight down it may take (any \
                     direction), across and along the line. Unticked: not given — its \
                     assembly's, if it gives one (shown dimmed)",
                ),
                ("description", "What it is, in words"),
                ("", ""),
            ] {
                crate::style::header(ui, h, tip);
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
                    crate::style::num_label(ui, format!("{l:.3}")).on_hover_text("Its assembly's");
                } else {
                    let r = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut l)
                            .range(0.01..=50.0)
                            .speed(0.01)
                            .max_decimals(3),
                    );
                    if r.changed() {
                        put(
                            StringChange::Length(l),
                            r.is_pointer_button_down_on()
                                .then(|| "string length".into()),
                        );
                    }
                }
                let legs = t.strings.iter().filter(|x| x.wire == s.wire).count();
                if legs == 1 {
                    let (angle, direction) = s.angles();
                    let (mut dn, mut dr) = (angle, (direction + ahead).rem_euclid(360.0));
                    let a1 = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut dn)
                            .range(0.0..=180.0)
                            .speed(0.5)
                            .max_decimals(2),
                    );
                    let a2 = crate::style::field(
                        ui,
                        egui::DragValue::new(&mut dr)
                            .range(0.0..=360.0)
                            .speed(0.5)
                            .max_decimals(2),
                    );
                    if a1.changed() || a2.changed() {
                        let held = (a1.is_pointer_button_down_on()
                            || a2.is_pointer_button_down_on())
                        .then(|| "string direction".into());
                        put(
                            StringChange::Direction(dn, (dr - ahead).rem_euclid(360.0)),
                            held,
                        );
                    }
                } else {
                    let (dn, dr, _) = ends.map_or((0.0, 0.0, 0.0), |(a, b)| polar(a, b));
                    let why = "One of two strings to one wire point: its direction follows where they meet";
                    crate::style::num_label(ui, format!("{dn:.2}")).on_hover_text(why);
                    crate::style::num_label(ui, format!("{:.2}", (dr + ahead).rem_euclid(360.0)))
                        .on_hover_text(why);
                }
                let mut w = s
                    .weight
                    .map(|w| w.si())
                    .or(a.map(|a| a.weight.si()))
                    .unwrap_or(0.0);
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut w)
                        .range(0.0..=1e6)
                        .speed(1.0)
                        .max_decimals(1),
                );
                if r.changed() {
                    put(
                        StringChange::Weight(w),
                        r.is_pointer_button_down_on()
                            .then(|| "string weight".into()),
                    );
                }
                let mut wa = s
                    .wind_area
                    .map(|w| w.si())
                    .or(a.map(|a| a.wind_area.si()))
                    .unwrap_or(0.0);
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut wa)
                        .range(0.0..=100.0)
                        .speed(0.005)
                        .max_decimals(3),
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
                    let b = ui
                        .add_enabled_ui(push, |ui| {
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut c)
                                    .range(0.0..=1e7)
                                    .speed(10.0)
                                    .max_decimals(0),
                            )
                        })
                        .inner;
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
                                    .then(|| format!("string {name} limit")),
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
                    .on_hover_text("Delete this string")
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
            if let Err(e) = f.edit(&format!("string {label}"), |doc, t| {
                edit::set_string(doc, t, g, insulators, &label, change)
            }) {
                self.message = Some((e, true));
            }
        }
        if let Some(label) = delete {
            f.edit("delete string", |doc, t| {
                edit::delete_string(doc, t, g, &label)
            });
            self.message = Some((format!("deleted string {label}"), false));
        }
        if t.strings.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No string yet: pick an attachment point, then + New string or From the library.",
                )
                .weak(),
            );
        }
    }

    fn joints_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        let joints = t.joints();
        // By joint id (the first of an id, as a search finds it): a search
        // per row through the joints and members was slow on a large
        // lattice.
        use std::collections::HashMap;
        let worked: HashMap<&str, Option<ohl_structures::JointKind>> = joints
            .iter()
            .rev()
            .map(|j| (j.id.as_str(), j.kind))
            .collect();
        let mut meeting: HashMap<&str, usize> = HashMap::new();
        for m in &t.members {
            for (k, id) in m.nodes.iter().enumerate() {
                if !m.nodes[..k].contains(id) {
                    *meeting.entry(id.as_str()).or_default() += 1;
                }
            }
        }
        let mut linked: HashMap<&str, Vec<&str>> = HashMap::new();
        for p in &t.points {
            linked
                .entry(p.joint.as_str())
                .or_default()
                .push(p.label.as_str());
        }
        let mut change = None;
        egui::Grid::new("joints").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("joint", "Its id"),
                ("x [m]", ""),
                ("y [m]", ""),
                ("z [m]", ""),
                (
                    "kind",
                    "attach: strings hang here (through the attachment points linked to it); \
                     support: where the structure stands on its foundation; none: an ordinary \
                     joint. \"auto\": not written, worked out (attach when a point is linked \
                     to it, support when it has a support)",
                ),
                ("members", "The members meeting it"),
                ("points", "The attachment points linked to it"),
            ] {
                crate::style::header(ui, h, tip);
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
                let worked_out = worked.get(n.id.as_str()).copied().flatten();
                if let Some(kind) = kind_cell(ui, &n.id, n.kind, worked_out) {
                    change = Some((n.id.clone(), kind));
                }
                crate::style::num_label(
                    ui,
                    meeting.get(n.id.as_str()).copied().unwrap_or(0).to_string(),
                );
                ui.label(
                    linked
                        .get(n.id.as_str())
                        .map_or(String::new(), |l| l.join(", ")),
                );
                ui.end_row();
            }
        });
        if let Some((id, kind)) = change {
            self.set_joint_kind(&id, kind);
        }
        if t.nodes.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No joint yet: Add joint, then Member between two joints picked.",
                )
                .weak(),
            );
        }
    }

    /// Every joint at the lowest level pinned (one undoable edit); whether
    /// there was a joint.
    pub fn support_feet(&mut self) -> bool {
        let Some(f) = &mut self.file else {
            return false;
        };
        let lowest = f
            .structure()
            .nodes
            .iter()
            .map(|n| n.point[2].si())
            .fold(f64::INFINITY, f64::min);
        if !lowest.is_finite() {
            return false;
        }
        f.edit("support the feet", |doc, t| {
            for n in t
                .nodes
                .iter()
                .filter(|n| (n.point[2].si() - lowest).abs() < 1e-6)
            {
                edit::set_support(doc, &n.id, Some("pinned"));
            }
        });
        true
    }

    fn supports_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        // Above the table (a tower has many joints).
        if !t.nodes.is_empty()
            && ui
                .button("Support the feet (pinned)")
                .on_hover_text("Every joint at the lowest level pinned")
                .clicked()
        {
            self.support_feet();
        }
        let Some(f) = &mut self.file else {
            return;
        };
        egui::Grid::new("supports").striped(true).show(ui, |ui| {
            for h in ["joint", "z [m]", "support"] {
                crate::style::header(ui, h, "");
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
                crate::style::num_label(ui, format!("{:.3}", n.point[2].si()));
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
    }

    fn design_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        // The section library's first (looked up first), then the file's.
        let sections = crate::section_editor::section_choices(&self.loads.sections, f.doc());
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
            ui.label("dead-load factor");
            let current = field_text(analysis.get("dead_load_factor"));
            if let Some(typed) = text_cell(
                ui,
                &mut self.buffers,
                "analysis/dead_load_factor".into(),
                &current,
                "1",
                "Factor on the members' self-weight for bolts, plates and the like (typically \
                 1.1–1.4; empty: 1); a member's own (DLF) replaces it",
            ) && let Err(e) = f.edit("dead-load factor", |doc, _| {
                edit::set_analysis_field(doc, "dead_load_factor", &typed)
            }) {
                self.message = Some((e, true));
            }
        });
        let mut insert: Option<String> = None;
        egui::Grid::new("design").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("member", ""),
                (
                    "cross-section",
                    "A cross-section of the cross-section library, or a [[section]] of the file; none: a stick",
                ),
                (
                    "kind",
                    "truss (axial only), frame (bending too), cable (tension only, a guy) or \
                     tension (a truss in tension only: in compression up to its own buckling \
                     load, bowed beyond it; slack without a second moment)",
                ),
                ("k", "Effective length factor for buckling (none: 1)"),
            ]
            .into_iter()
            .chain(MEMBER_FIELDS.iter().map(|(_, h, tip, _)| (*h, *tip)))
            {
                crate::style::header(ui, h, tip);
            }
            ui.end_row();
            for m in &t.members {
                let r = row(
                    ui,
                    &mut self.selected,
                    &mut self.scroll_to,
                    Handle::Member(m.id.clone()),
                    &m.id,
                );
                // A point cannot be put on a member: a joint is inserted
                // on it (the member split there, continuous), then linked.
                r.on_hover_text("Right-click: insert a joint on it")
                    .context_menu(|ui| {
                        ui.label("Insert a joint on this member (an attach joint)");
                        ui.horizontal(|ui| {
                            ui.label("at");
                            ui.add(
                                egui::DragValue::new(&mut self.insert_at)
                                    .range(0.001..=0.999)
                                    .speed(0.01)
                                    .max_decimals(3),
                            )
                            .on_hover_text("Fraction of its length from its first joint");
                        });
                        if ui
                            .button("Insert a joint")
                            .on_hover_text(
                                "The member is split there into two pieces of the same fields \
                                 (continuous: a rigid connection, each piece buckling over the \
                                 whole member's length); link a point to the joint with the Link \
                                 tool",
                            )
                            .clicked()
                        {
                            insert = Some(m.id.clone());
                            ui.close();
                        }
                    });
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
                    f.edit(&format!("cross-section of {}", m.id), |doc, _| {
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
                        for (k, what) in edit::MEMBER_KINDS {
                            ui.selectable_value(&mut new_kind, k.to_owned(), k)
                                .on_hover_text(what);
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
                    let b = ui
                        .add_enabled_ui(on, |ui| {
                            crate::style::field(
                                ui,
                                egui::DragValue::new(&mut k)
                                    .range(0.1..=5.0)
                                    .speed(0.01)
                                    .max_decimals(3),
                            )
                        })
                        .inner;
                    hold(f, &b, "k");
                    if a.changed() || b.changed() {
                        f.edit(&format!("k of {}", m.id), |doc, _| {
                            edit::set_member_k(doc, &m.id, on.then_some(k))
                        });
                    }
                });
                for (field, _, tip, hint) in MEMBER_FIELDS {
                    let current = field_text(member_field(m, field));
                    if let Some(typed) = text_cell(
                        ui,
                        &mut self.buffers,
                        format!("member/{}/{field}", m.id),
                        &current,
                        hint,
                        tip,
                    ) && let Err(e) = f.edit(&format!("{field} of {}", m.id), |doc, _| {
                        edit::set_item_field(doc, "member", &m.id, field, &typed)
                    }) {
                        self.message = Some((e, true));
                    }
                }
                ui.end_row();
            }
        });
        if let Some(m) = insert {
            self.insert_joint(&m);
        }
        if t.members.is_empty() {
            ui.label(egui::RichText::new("No member yet: build the body first (Body).").weak());
        } else if sections.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No cross-section yet: type them in the cross-section library (Cross-sections & materials › \
                     Cross-section library), or add [[section]] tables and their materials (Cross-sections, \
                     Materials).",
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
                "Cross-section"
            } else {
                "Material"
            }
        );
        if ui.button(label).clicked() {
            let id = if key == "section" {
                // Its material: the first there is; none: steel added
                // with it (a section without one cannot be analysed).
                let material = edit::item_ids(f.doc(), "material").into_iter().next();
                let add_steel = material.is_none();
                let m = material.unwrap_or_else(|| "steel".to_owned());
                let fields = vec![
                    ("material", m.as_str()),
                    ("area", "1000 mm2"),
                    ("i_min", "40 cm4"),
                ];
                f.edit("add section", |doc, _| {
                    if add_steel {
                        edit::add_item(doc, "material", "steel", &STEEL);
                    }
                    edit::add_item(doc, key, "S1", &fields)
                })
            } else {
                f.edit("add material", |doc, _| {
                    edit::add_item(doc, key, "steel", &STEEL)
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
