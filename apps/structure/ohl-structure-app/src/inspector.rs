// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Inspector panel (owner, 2026-10-26): what is selected with its
//! fields, edited in place — a joint (where, its kind, its support), a
//! member (its joints, length, cross-section, kind, k), an attachment
//! point (where, its joint and offset: the rigid link, link or unlink), a
//! wire point (where when fixed, its strings and their lengths, its set,
//! its counterweight). Several of one kind: the fields they share, a
//! change written to each (one undo step). Every change goes through the
//! same edits as the tables (one undo step each; a value dragged is one
//! step).

use eframe::egui;
use ohl_structure_edit::{self as edit, Handle, StringChange, StructureFile};
use ohl_structures::StructureType;

use crate::app::StructureApp;

/// Same within a micrometre (a shared coordinate).
const SAME: f64 = 1e-6;

/// A member's `[member.design]` table in one line: `key value, …`.
fn design_text(d: &toml::Value) -> String {
    match d.as_table() {
        Some(t) => t
            .iter()
            .map(|(k, v)| match v {
                toml::Value::String(s) => format!("{k} {s}"),
                other => format!("{k} {other}"),
            })
            .collect::<Vec<_>>()
            .join(", "),
        None => d.to_string(),
    }
}

/// While the pointer is pressed on `r` its changes are one undo step.
fn hold(file: &mut StructureFile, r: &egui::Response, what: &str) {
    if r.is_pointer_button_down_on() {
        file.begin(what);
    }
}

/// A label and its value field (m), in a grid row; the value when changed.
fn length_row(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f64,
    f: &mut StructureFile,
    what: &str,
) -> bool {
    ui.label(label);
    let r = crate::style::field(ui, egui::DragValue::new(v).speed(0.01).max_decimals(3));
    hold(f, &r, what);
    ui.end_row();
    r.changed()
}

/// The kind of the handles when they are all of one kind: its words.
fn kind_of(selected: &[Handle]) -> Option<&'static str> {
    let word = |h: &Handle| match h {
        Handle::Joint(_) => "joints",
        Handle::Member(_) => "members",
        Handle::Point(_) => "points",
        Handle::Wire(_) => "wire points",
    };
    let first = word(selected.first()?);
    selected.iter().all(|h| word(h) == first).then_some(first)
}

impl StructureApp {
    /// The Inspector panel.
    pub fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            return;
        };
        let t = f.structure_arc();
        match self.selected.clone().as_slice() {
            [] => {
                ui.label(
                    egui::RichText::new(
                        "Nothing selected: click a joint, member, point or wire point in the 3D \
                         view or a table.",
                    )
                    .weak(),
                );
            }
            [Handle::Joint(id)] => self.joint_inspector(ui, &t, id),
            [Handle::Member(id)] => self.member_inspector(ui, &t, id),
            [Handle::Point(l)] => self.point_inspector(ui, &t, l),
            [Handle::Wire(l)] => self.wire_inspector(ui, &t, l),
            many => self.many_inspector(ui, &t, many),
        }
    }

    fn joint_inspector(&mut self, ui: &mut egui::Ui, t: &StructureType, id: &str) {
        let Some(n) = t.nodes.iter().find(|n| n.id == id) else {
            ui.label(format!(
                "joint {id}: not in the body (a joint of a point or an extension)"
            ));
            return;
        };
        ui.heading(format!("Joint {id}"));
        let Some(f) = &mut self.file else {
            return;
        };
        let h = Handle::Joint(id.to_owned());
        let mut p = n.point.map(|v| v.si());
        let mut moved = false;
        let what = format!("move joint {id}");
        egui::Grid::new("inspector-joint")
            .num_columns(2)
            .show(ui, |ui| {
                for (k, name) in ["x [m]", "y [m]", "z [m]"].into_iter().enumerate() {
                    moved |= length_row(ui, name, &mut p[k], f, &what);
                }
            });
        if moved && let Err(e) = f.edit(&what, |doc, t| edit::symmetry::move_to(doc, t, &h, p)) {
            self.message = Some((e, true));
        }
        let worked = t
            .joints()
            .into_iter()
            .find(|j| j.id == id)
            .and_then(|j| j.kind);
        let mut kind_change = None;
        let mut support_change: Option<String> = None;
        egui::Grid::new("inspector-joint-kind")
            .num_columns(2)
            .show(ui, |ui| {
                use ohl_structures::JointKind;
                ui.label("kind");
                let text = |k: Option<JointKind>| match k {
                    Some(k) => k.as_str().to_owned(),
                    None => format!("auto ({})", worked.map_or("none", JointKind::as_str)),
                };
                let mut chosen = n.kind;
                egui::ComboBox::from_id_salt("inspector-joint-kind-choice")
                    .selected_text(text(n.kind))
                    .show_ui(ui, |ui| {
                        for k in [None, Some(JointKind::Attach), Some(JointKind::Support)] {
                            ui.selectable_value(&mut chosen, k, text(k));
                        }
                    });
                if chosen != n.kind {
                    kind_change = Some(chosen);
                }
                ui.end_row();
                ui.label("support");
                let support = n.support.as_ref().map_or("—".to_owned(), |s| {
                    s.as_str().map_or(s.to_string(), str::to_owned)
                });
                let mut chosen = support.clone();
                egui::ComboBox::from_id_salt("inspector-support")
                    .selected_text(&chosen)
                    .show_ui(ui, |ui| {
                        for s in ["—", "pinned", "fixed"] {
                            ui.selectable_value(&mut chosen, s.to_owned(), s);
                        }
                    });
                if chosen != support {
                    support_change = Some(chosen);
                }
                ui.end_row();
                let meeting: Vec<&str> = t
                    .members
                    .iter()
                    .filter(|m| m.nodes.iter().any(|x| x == id))
                    .map(|m| m.id.as_str())
                    .collect();
                ui.label("members");
                ui.add(
                    egui::Label::new(if meeting.is_empty() {
                        "none".to_owned()
                    } else {
                        meeting.join(", ")
                    })
                    .wrap(),
                );
                ui.end_row();
                let points: Vec<&str> = t
                    .points
                    .iter()
                    .filter(|p| p.joint == id)
                    .map(|p| p.label.as_str())
                    .collect();
                if !points.is_empty() {
                    ui.label("points linked");
                    ui.add(egui::Label::new(points.join(", ")).wrap());
                    ui.end_row();
                }
            });
        if let Some(s) = support_change {
            f.edit(&format!("support of {id}"), |doc, _| {
                edit::set_support(doc, id, (s != "—").then_some(s.as_str()))
            });
        }
        if let Some(k) = kind_change {
            self.set_joint_kind(id, k);
        }
    }

    fn member_inspector(&mut self, ui: &mut egui::Ui, t: &StructureType, id: &str) {
        let Some(m) = t.members.iter().find(|m| m.id == id) else {
            ui.label(format!("member {id}: not in the body"));
            return;
        };
        ui.heading(format!("Member {id}"));
        let Some(f) = &mut self.file else {
            return;
        };
        let sections = crate::section_editor::section_choices(&self.loads.sections, f.doc());
        let length = t
            .joint(&m.nodes[0])
            .zip(t.joint(&m.nodes[1]))
            .map(|(a, b)| {
                (0..3)
                    .map(|k| (b.position[k] - a.position[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            });
        egui::Grid::new("inspector-member")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("joints");
                ui.label(format!("{} – {}", m.nodes[0], m.nodes[1]));
                ui.end_row();
                if let Some(l) = length {
                    ui.label("length [m]");
                    ui.label(format!("{l:.3}"));
                    ui.end_row();
                }
                ui.label("group");
                ui.label(m.group.as_deref().unwrap_or("—"));
                ui.end_row();
                ui.label("made by");
                ui.label(m.made_by.as_deref().unwrap_or("by hand"));
                ui.end_row();
                ui.label("cross-section");
                let current = m.section.clone().unwrap_or_else(|| "—".into());
                let mut chosen = current.clone();
                egui::ComboBox::from_id_salt("inspector-section")
                    .selected_text(&chosen)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, "—".to_owned(), "— (a stick)");
                        for s in &sections {
                            ui.selectable_value(&mut chosen, s.clone(), s);
                        }
                    });
                if chosen != current {
                    f.edit(&format!("cross-section of {id}"), |doc, _| {
                        edit::set_member_section(
                            doc,
                            id,
                            (chosen != "—").then_some(chosen.as_str()),
                        )
                    });
                }
                ui.end_row();
                ui.label("kind");
                let kind = m.kind.clone().unwrap_or_else(|| "truss".into());
                let mut new_kind = kind.clone();
                egui::ComboBox::from_id_salt("inspector-kind")
                    .selected_text(&new_kind)
                    .show_ui(ui, |ui| {
                        for (k, what) in edit::MEMBER_KINDS {
                            ui.selectable_value(&mut new_kind, k.to_owned(), k)
                                .on_hover_text(what);
                        }
                    });
                if new_kind != kind {
                    f.edit(&format!("kind of {id}"), |doc, _| {
                        edit::set_member_kind(doc, id, &new_kind)
                    });
                }
                ui.end_row();
                ui.label("k")
                    .on_hover_text("Effective length factor for buckling (unticked: 1)");
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
                        f.edit(&format!("k of {id}"), |doc, _| {
                            edit::set_member_k(doc, id, on.then_some(k))
                        });
                    }
                });
                ui.end_row();
                // Design data (2026-10-27): shown, edited in the file
                // (editing here: not yet).
                ui.label("design data").on_hover_text(
                    "[member.design]: effective length ratios, end eccentricity and restraint, \
                     leg slenderness, bolts, class, crossing partner — read by the capacity \
                     method. Edited in the file (not yet here).",
                );
                match &m.design {
                    None => ui.label("— (the method's assumptions)"),
                    Some(d) => {
                        let text = design_text(d);
                        let short: String = text.chars().take(60).collect();
                        let more = if short.len() < text.len() { "…" } else { "" };
                        ui.label(format!("{short}{more}")).on_hover_text(text)
                    }
                };
                ui.end_row();
            });
    }

    fn point_inspector(&mut self, ui: &mut egui::Ui, t: &StructureType, label: &str) {
        let Some(p) = t.points.iter().find(|p| p.label == label) else {
            return;
        };
        ui.heading(format!("Point {label}"));
        let Some(f) = &mut self.file else {
            return;
        };
        let h = Handle::Point(label.to_owned());
        let mut at = p.position().map(|v| v.si());
        let what = format!("move point {label}");
        let mut moved = false;
        let free = p.node.is_none();
        let mut o = t.point_offset(p);
        let mut shifted = false;
        let offset_what = format!("offset of point {label}");
        egui::Grid::new("inspector-point")
            .num_columns(2)
            .show(ui, |ui| {
                for (k, name) in ["x [m]", "y [m]", "z [m]"].into_iter().enumerate() {
                    moved |= length_row(ui, name, &mut at[k], f, &what);
                }
                ui.label("joint");
                ui.label(if free {
                    format!("{} (its own: not linked)", p.joint)
                } else {
                    p.joint.clone()
                });
                ui.end_row();
                if !free {
                    for (k, name) in ["offset x [m]", "offset y [m]", "offset z [m]"]
                        .into_iter()
                        .enumerate()
                    {
                        shifted |= length_row(ui, name, &mut o[k], f, &offset_what);
                    }
                }
                let hanging: Vec<&str> = t
                    .strings
                    .iter()
                    .filter(|s| s.top == label)
                    .map(|s| s.label.as_str())
                    .collect();
                ui.label("strings");
                ui.label(if hanging.is_empty() {
                    "none".to_owned()
                } else {
                    hanging.join(", ")
                });
                ui.end_row();
            });
        if moved && let Err(e) = f.edit(&what, |doc, t| edit::move_to(doc, t, &h, at)) {
            self.message = Some((e, true));
        }
        if shifted
            && let Err(e) = f.edit(&offset_what, |doc, t| {
                edit::links::link_point(doc, t, label, &p.joint, o)
            })
        {
            self.message = Some((e, true));
        }
        if free {
            ui.label(
                egui::RichText::new(
                    "Not linked: its load reaches no member. Link it to an attach joint (the \
                     Link tool, or below).",
                )
                .weak(),
            );
        } else {
            ui.label(format!(
                "Rigid link to joint {}: {}",
                p.joint,
                crate::link_tool::offset_words(o)
            ));
        }
        let mut action = None;
        ui.horizontal(|ui| {
            if ui
                .button("Link to a joint…")
                .on_hover_text("The Link tool on this point: click the joint to link it to")
                .clicked()
            {
                action = Some(crate::link_tool::LinkAction::Start(label.to_owned()));
            }
            if !free
                && ui
                    .button("Unlink")
                    .on_hover_text("It keeps its place, a joint of its own on no member")
                    .clicked()
            {
                action = Some(crate::link_tool::LinkAction::Unlink(label.to_owned()));
            }
        });
        if let Some(a) = action {
            self.link_action(a);
        }
    }

    fn wire_inspector(&mut self, ui: &mut egui::Ui, t: &StructureType, label: &str) {
        let Some(w) = t.wires.iter().find(|w| w.label == label) else {
            return;
        };
        ui.heading(format!("Wire point {label}"));
        let revision = self.file.as_ref().map_or(0, StructureFile::revision);
        let g = self.geometry_of(t, revision, None);
        let Some(f) = &mut self.file else {
            return;
        };
        let h = Handle::Wire(label.to_owned());
        let held: Vec<&ohl_structures::TypeString> =
            t.strings.iter().filter(|s| s.wire == label).collect();
        let mut at = g.at(label).unwrap_or([0.0; 3]);
        let what = format!("move wire point {label}");
        let mut moved = false;
        let mut cw = w.weight.map_or(0.0, |x| x.si());
        let mut weighed = false;
        let mut lengths: Vec<(String, f64, bool)> = Vec::new();
        egui::Grid::new("inspector-wire")
            .num_columns(2)
            .show(ui, |ui| {
                if held.is_empty() {
                    for (k, name) in ["x [m]", "y [m]", "z [m]"].into_iter().enumerate() {
                        moved |= length_row(ui, name, &mut at[k], f, &what);
                    }
                } else {
                    for (k, name) in ["x [m]", "y [m]", "z [m]"].into_iter().enumerate() {
                        ui.label(name);
                        ui.label(format!("{:.3}", at[k]))
                            .on_hover_text("Placed by its string(s): change their length");
                        ui.end_row();
                    }
                }
                for s in &held {
                    let assembly = s.insulator.is_some();
                    let ends = g.at(&s.top).zip(g.at(&s.wire));
                    let now = ends.map_or(0.0, |(a, b)| ohl_structure_edit::polar(a, b).2);
                    let mut l = s.length.map_or(now, |l| l.si());
                    ui.label(format!("string {} [m]", s.label));
                    if assembly {
                        ui.label(format!("{now:.3}"))
                            .on_hover_text("Its assembly's length");
                    } else {
                        let r = crate::style::field(
                            ui,
                            egui::DragValue::new(&mut l)
                                .range(0.01..=50.0)
                                .speed(0.01)
                                .max_decimals(3),
                        );
                        if r.changed() {
                            lengths.push((s.label.clone(), l, r.is_pointer_button_down_on()));
                        }
                    }
                    ui.end_row();
                }
                ui.label("set");
                ui.label(w.set.map_or("—".to_owned(), |s| s.to_string()));
                ui.end_row();
                ui.label("counterweight [N]");
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut cw)
                        .range(0.0..=1e6)
                        .speed(10.0)
                        .max_decimals(0),
                );
                hold(f, &r, "counterweight");
                weighed = r.changed();
                ui.end_row();
            });
        if moved {
            let _ = f.edit(&what, |doc, t| edit::move_to(doc, t, &h, at));
        }
        if weighed {
            f.edit("counterweight", |doc, _| {
                edit::set_counterweight(doc, label, cw)
            });
        }
        for (s, l, dragged) in lengths {
            if dragged {
                f.begin("string length");
            }
            if let Err(e) = f.edit(&format!("string {s}"), |doc, t| {
                edit::set_string(doc, t, &g, &self.insulators, &s, StringChange::Length(l))
            }) {
                self.message = Some((e, true));
            }
        }
    }

    /// Several selected: of one kind, the fields they share (a change
    /// written to each, one undo step); of several kinds, the list.
    fn many_inspector(&mut self, ui: &mut egui::Ui, t: &StructureType, many: &[Handle]) {
        let names: Vec<String> = many.iter().map(Handle::describe).collect();
        let Some(kind) = kind_of(many) else {
            ui.heading(format!("{} selected", many.len()));
            ui.label(names.join(", "));
            ui.label(egui::RichText::new("Of several kinds: nothing they all share.").weak());
            return;
        };
        ui.heading(format!("{} {kind}", many.len()));
        let ids: Vec<String> = many
            .iter()
            .map(|h| match h {
                Handle::Joint(s) | Handle::Member(s) | Handle::Point(s) | Handle::Wire(s) => {
                    s.clone()
                }
            })
            .collect();
        ui.label(egui::RichText::new(ids.join(", ")).weak());
        let Some(f) = &mut self.file else {
            return;
        };
        match kind {
            "joints" | "points" => {
                let at: Vec<[f64; 3]> = many
                    .iter()
                    .filter_map(|h| match h {
                        Handle::Joint(id) => t
                            .nodes
                            .iter()
                            .find(|n| &n.id == id)
                            .map(|n| n.point.map(|v| v.si())),
                        Handle::Point(l) => t
                            .points
                            .iter()
                            .find(|p| &p.label == l)
                            .map(|p| p.position().map(|v| v.si())),
                        _ => None,
                    })
                    .collect();
                let what = format!("move {} {kind}", many.len());
                let mut change: Option<(usize, f64)> = None;
                egui::Grid::new("inspector-many")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (k, name) in ["x [m]", "y [m]", "z [m]"].into_iter().enumerate() {
                            let first = at.first().map_or(0.0, |p| p[k]);
                            if at.iter().all(|p| (p[k] - first).abs() < SAME) {
                                let mut v = first;
                                if length_row(ui, name, &mut v, f, &what) {
                                    change = Some((k, v));
                                }
                            } else {
                                ui.label(name);
                                ui.label(egui::RichText::new("differ").weak());
                                ui.end_row();
                            }
                        }
                    });
                if let Some((k, v)) = change {
                    // One undo step for all of them.
                    f.begin(&what);
                    for (h, p) in many.iter().zip(&at) {
                        let mut p = *p;
                        p[k] = v;
                        let r = f.edit(&what, |doc, t| match h {
                            Handle::Joint(_) => edit::symmetry::move_to(doc, t, h, p),
                            _ => edit::move_to(doc, t, h, p),
                        });
                        if let Err(e) = r {
                            self.message = Some((e, true));
                        }
                    }
                }
            }
            "members" => {
                let members: Vec<&ohl_structures::structures::TypeMember> = ids
                    .iter()
                    .filter_map(|id| t.members.iter().find(|m| &m.id == id))
                    .collect();
                let sections =
                    crate::section_editor::section_choices(&self.loads.sections, f.doc());
                let shared = |get: &dyn Fn(&ohl_structures::structures::TypeMember) -> String| {
                    let first = members.first().map(|m| get(m));
                    if members.iter().all(|m| Some(get(m)) == first) {
                        first
                    } else {
                        None
                    }
                };
                let section = shared(&|m| m.section.clone().unwrap_or_else(|| "—".into()));
                let kind_now = shared(&|m| m.kind.clone().unwrap_or_else(|| "truss".into()));
                let mut new_section = None;
                let mut new_kind = None;
                egui::Grid::new("inspector-many-members")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("cross-section");
                        let mut chosen = section.clone().unwrap_or_else(|| "mixed".into());
                        let before = chosen.clone();
                        egui::ComboBox::from_id_salt("inspector-many-section")
                            .selected_text(&chosen)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut chosen, "—".to_owned(), "— (sticks)");
                                for s in &sections {
                                    ui.selectable_value(&mut chosen, s.clone(), s);
                                }
                            });
                        if chosen != before {
                            new_section = Some(chosen);
                        }
                        ui.end_row();
                        ui.label("kind");
                        let mut chosen = kind_now.clone().unwrap_or_else(|| "mixed".into());
                        let before = chosen.clone();
                        egui::ComboBox::from_id_salt("inspector-many-kind")
                            .selected_text(&chosen)
                            .show_ui(ui, |ui| {
                                for (k, what) in edit::MEMBER_KINDS {
                                    ui.selectable_value(&mut chosen, k.to_owned(), k)
                                        .on_hover_text(what);
                                }
                            });
                        if chosen != before {
                            new_kind = Some(chosen);
                        }
                        ui.end_row();
                    });
                if let Some(s) = new_section {
                    f.edit(
                        &format!("cross-section of {} members", ids.len()),
                        |doc, _| {
                            for id in &ids {
                                edit::set_member_section(doc, id, (s != "—").then_some(s.as_str()));
                            }
                        },
                    );
                }
                if let Some(k) = new_kind {
                    f.edit(&format!("kind of {} members", ids.len()), |doc, _| {
                        for id in &ids {
                            edit::set_member_kind(doc, id, &k);
                        }
                    });
                }
            }
            _ => {
                ui.label(egui::RichText::new("Nothing they all share to edit here.").weak());
            }
        }
    }
}
