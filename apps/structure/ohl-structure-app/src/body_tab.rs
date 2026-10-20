// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Joints and Members panels' own tools: the symmetry option and
//! Make symmetric, the member group of the members picked, the Members
//! table with its group column and filter; and the Groups panel (Design
//! menu: a section and kind per group). The generators are in [`crate::generators_ui`].

use eframe::egui;
use ohl_structure_edit::body::{set_symmetry, symmetry_option};
use ohl_structure_edit::{
    self as edit, Geometry, Handle, Mirror, groups, set_group, set_group_field, set_group_kind,
    set_group_section,
};
use ohl_structures::StructureType;

use crate::app::StructureApp;
use crate::generators_ui::Form;
use crate::tables::{MEMBER_FIELDS, field_text, member_field, row, text_cell};

/// The Body panels' state between frames.
#[derive(Default)]
pub struct BodyState {
    /// What the generator being edited would build: lines in the view.
    pub preview: Vec<[[f64; 3]; 2]>,
    /// The generator being edited (none: the list only).
    pub form: Option<Form>,
    /// What the last apply or rebuild did: a summary and what it kept.
    pub report: Option<(String, Vec<String>)>,
    /// A generator to remove, waiting for its confirmation.
    pub confirm_remove: Option<String>,
    /// The Members table shows one group only (`Some("")`: members
    /// without a group).
    pub group_filter: Option<String>,
    /// The group typed for the members picked.
    pub group_text: String,
}

impl BodyState {
    /// Changes when the preview does (the view is drawn again).
    pub fn preview_stamp(&self) -> usize {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for l in &self.preview {
            for p in l {
                for v in p {
                    v.to_bits().hash(&mut h);
                }
            }
        }
        h.finish() as usize
    }
}

/// The members picked.
fn picked_members(selected: &[Handle]) -> Vec<String> {
    selected
        .iter()
        .filter_map(|h| match h {
            Handle::Member(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

/// The symmetry option set to `chosen` (one edit).
fn set_symmetry_of(
    f: &mut ohl_structure_edit::StructureFile,
    chosen: Option<Mirror>,
    message: &mut Option<(String, bool)>,
) {
    f.edit("symmetry", |doc, _| set_symmetry(doc, chosen));
    *message = Some((
        format!("symmetry: {}", chosen.map_or("none", Mirror::name)),
        false,
    ));
}

/// Every joint and member given the mirror images it lacks (one edit).
fn make_symmetric_of(
    f: &mut ohl_structure_edit::StructureFile,
    message: &mut Option<(String, bool)>,
) {
    let (j, m) = f.edit("make symmetric", edit::symmetry::make_symmetric);
    *message = Some((
        format!("made symmetric: {j} joint(s) and {m} member(s) added"),
        false,
    ));
}

impl StructureApp {
    /// Body › Symmetry: the option set.
    pub fn set_symmetry(&mut self, chosen: Option<Mirror>) {
        if let Some(f) = &mut self.file {
            set_symmetry_of(f, chosen, &mut self.message);
        }
    }

    /// Body › Make symmetric.
    pub fn make_symmetric(&mut self) {
        if let Some(f) = &mut self.file {
            make_symmetric_of(f, &mut self.message);
        }
    }

    /// The second row of the Joints and Members panels' tools: symmetry, Make symmetric,
    /// the group of the members picked.
    pub(crate) fn body_tools(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            let now = symmetry_option(t);
            let mut chosen = now;
            ui.add(
                crate::icons::Icon::Symmetry
                    .image(ui.ctx(), 16.0)
                    .tint(ui.visuals().text_color()),
            );
            ui.label("symmetry").on_hover_text(
                "Joints and members added, moved or deleted take their mirror images along \
                 (about x = 0: left and right of the line; about y = 0: ahead and back)",
            );
            egui::ComboBox::from_id_salt("symmetry")
                .selected_text(chosen.map_or("none", Mirror::name))
                .width(130.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "none");
                    for m in Mirror::ALL {
                        ui.selectable_value(&mut chosen, Some(m), m.name());
                    }
                });
            if chosen != now {
                set_symmetry_of(f, chosen, &mut self.message);
            }
            if ui
                .add_enabled(now.is_some(), egui::Button::new("Make symmetric"))
                .on_hover_text("Every joint and member gets the mirror images it lacks")
                .on_disabled_hover_text("Choose a symmetry first")
                .clicked()
            {
                make_symmetric_of(f, &mut self.message);
            }
            ui.separator();
            let members = picked_members(&self.selected);
            ui.label("group");
            ui.add(
                egui::TextEdit::singleline(&mut self.body.group_text)
                    .desired_width(110.0)
                    .hint_text("e.g. body-leg"),
            )
            .on_hover_text("A member group: sections are set per group (Design › Groups)");
            let n = members.len();
            if ui
                .add_enabled(
                    n > 0 && !self.body.group_text.trim().is_empty(),
                    egui::Button::new(format!("Set on {n} member(s)")),
                )
                .on_hover_text("The group on the members picked; a member joining a group whose members share a section takes it")
                .on_disabled_hover_text("Pick members (Shift + click for more) and type a group")
                .clicked()
            {
                let g = self.body.group_text.trim().to_owned();
                f.edit(&format!("group {g}"), |doc, t| {
                    set_group(doc, t, &members, Some(&g))
                });
                self.message = Some((format!("group {g} on {n} member(s)"), false));
            }
            if ui
                .add_enabled(n > 0, egui::Button::new("No group"))
                .on_hover_text("Take the members picked out of their group")
                .clicked()
            {
                f.edit("no group", |doc, t| set_group(doc, t, &members, None));
            }
        });
    }

    /// The members: ends, length, group (filtered by group), section,
    /// the generator that made it.
    pub(crate) fn members_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType, g: &Geometry) {
        let names: Vec<String> = groups(t).into_iter().map(|g| g.name).collect();
        ui.horizontal(|ui| {
            ui.label("show");
            let text = match &self.body.group_filter {
                None => "all members".to_owned(),
                Some(x) if x.is_empty() => "no group".to_owned(),
                Some(x) => x.clone(),
            };
            egui::ComboBox::from_id_salt("group-filter")
                .selected_text(text)
                .width(150.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.body.group_filter, None, "all members");
                    ui.selectable_value(
                        &mut self.body.group_filter,
                        Some(String::new()),
                        "no group",
                    );
                    for n in &names {
                        ui.selectable_value(&mut self.body.group_filter, Some(n.clone()), n);
                    }
                });
            if ui
                .button("Pick them")
                .on_hover_text("Pick the members shown (then set their group)")
                .clicked()
            {
                self.selected = t
                    .members
                    .iter()
                    .filter(|m| shown(&self.body.group_filter, m.group.as_deref()))
                    .map(|m| Handle::Member(m.id.clone()))
                    .collect();
            }
        });
        egui::Grid::new("members").striped(true).show(ui, |ui| {
            for h in [
                "member", "from", "to", "length", "group", "section", "made by",
            ] {
                ui.strong(h);
            }
            ui.end_row();
            for m in t
                .members
                .iter()
                .filter(|m| shown(&self.body.group_filter, m.group.as_deref()))
            {
                let h = Handle::Member(m.id.clone());
                row(ui, &mut self.selected, &mut self.scroll_to, h, &m.id);
                ui.label(&m.nodes[0]);
                ui.label(&m.nodes[1]);
                let l = g.members.iter().find(|x| x.0 == m.id).map_or(0.0, |x| {
                    (0..3)
                        .map(|k| (x.2[k] - x.1[k]).powi(2))
                        .sum::<f64>()
                        .sqrt()
                });
                ui.label(format!("{l:.3} m"));
                ui.label(m.group.as_deref().unwrap_or("—"));
                ui.label(m.section.as_deref().unwrap_or("—"));
                ui.label(m.made_by.as_deref().unwrap_or("by hand"));
                ui.end_row();
            }
        });
    }

    /// Design › Groups: per group its members, one section and one kind
    /// for all of them.
    pub(crate) fn groups_sheet(&mut self, ui: &mut egui::Ui, t: &StructureType) {
        let Some(f) = &mut self.file else {
            return;
        };
        // The section library's first (looked up first), then the file's.
        let sections = crate::section_editor::section_choices(&self.loads.sections, f.doc());
        let all = groups(t);
        if all.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No member group yet: the generators give their members groups, or set one on \
                     members picked (Body).",
                )
                .weak(),
            );
            return;
        }
        egui::Grid::new("groups").striped(true).show(ui, |ui| {
            for (h, tip) in [
                ("group", "Click: pick its members"),
                ("members", ""),
                ("section", "Written on every member of the group"),
                ("kind", "Written on every member of the group"),
            ]
            .into_iter()
            .chain(
                GROUP_FIELDS
                    .iter()
                    .filter_map(|f| MEMBER_FIELDS.iter().find(|m| m.0 == *f))
                    .map(|(_, h, tip, _)| (*h, *tip)),
            ) {
                ui.strong(h).on_hover_text(tip);
            }
            ui.end_row();
            for gr in &all {
                let handles: Vec<Handle> = gr.members.iter().cloned().map(Handle::Member).collect();
                let on = !handles.is_empty() && handles.iter().all(|h| self.selected.contains(h));
                if ui.selectable_label(on, &gr.name).clicked() {
                    self.selected = handles;
                }
                ui.label(gr.members.len().to_string());
                let current = if gr.mixed_sections {
                    "mixed".to_owned()
                } else {
                    gr.section.clone().unwrap_or_else(|| "—".into())
                };
                let mut chosen = current.clone();
                egui::ComboBox::from_id_salt(("group-section", &gr.name))
                    .selected_text(&chosen)
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, "—".to_owned(), "— (sticks)");
                        for s in &sections {
                            ui.selectable_value(&mut chosen, s.clone(), s);
                        }
                    });
                if chosen != current {
                    f.edit(&format!("section of group {}", gr.name), |doc, t| {
                        set_group_section(
                            doc,
                            t,
                            &gr.name,
                            (chosen != "—").then_some(chosen.as_str()),
                        )
                    });
                }
                let kind = gr.kind.clone().unwrap_or_else(|| "mixed".into());
                let mut new_kind = kind.clone();
                egui::ComboBox::from_id_salt(("group-kind", &gr.name))
                    .selected_text(&new_kind)
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for (k, what) in ohl_structure_edit::MEMBER_KINDS {
                            ui.selectable_value(&mut new_kind, k.to_owned(), k)
                                .on_hover_text(what);
                        }
                    });
                if new_kind != kind {
                    f.edit(&format!("kind of group {}", gr.name), |doc, t| {
                        set_group_kind(doc, t, &gr.name, &new_kind)
                    });
                }
                let members: Vec<&ohl_structures::structures::TypeMember> = t
                    .members
                    .iter()
                    .filter(|m| m.group.as_deref() == Some(gr.name.as_str()))
                    .collect();
                for (field, _, tip, hint) in
                    MEMBER_FIELDS.iter().filter(|m| GROUP_FIELDS.contains(&m.0))
                {
                    // The value all its members have; mixed: none shown.
                    let values: Vec<String> = members
                        .iter()
                        .map(|m| field_text(member_field(m, field)))
                        .collect();
                    let same = values.iter().all(|v| *v == values[0]);
                    let current = if same {
                        values[0].clone()
                    } else {
                        String::new()
                    };
                    if let Some(typed) = text_cell(
                        ui,
                        &mut self.buffers,
                        format!("group/{}/{field}", gr.name),
                        &current,
                        if same { hint } else { "mixed" },
                        &format!("{tip}; written on every member of the group"),
                    ) && let Err(e) = f
                        .edit(&format!("{field} of group {}", gr.name), |doc, t| {
                            set_group_field(doc, t, &gr.name, field, &typed)
                        })
                    {
                        self.message = Some((e, true));
                    }
                }
                ui.end_row();
            }
        });
        if sections.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No section yet: type them in the section library (Sections & materials › \
                     Section library), or add [[section]] tables and their materials (Sections, \
                     Materials).",
                )
                .weak(),
            );
        }
    }
}

/// The member fields a group sets on all its members (Design › Groups).
const GROUP_FIELDS: [&str; 4] = [
    "lk",
    "capacity_tension",
    "capacity_compression",
    "dead_load_factor",
];

/// Whether a member of `group` is shown with `filter`.
fn shown(filter: &Option<String>, group: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(f) if f.is_empty() => group.is_none(),
        Some(f) => group == Some(f.as_str()),
    }
}
