// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Joint details panel and the Joint view pane (owner 2026-11-02,
//! stage 2 of the joint details; the view a pane of its own beside the 3D
//! view: "make it so that the joint view opens on the tower window as a
//! new pane … it's easier to not fill the view with screens"): a tower's
//! joint — the one selected — with
//!
//! - a **close-up**: the members as their profiles, cut a little beyond
//!   the joint, the bolts on the legs bolted, the plates; turned by
//!   dragging, zoomed by scrolling;
//! - a **face view**: the joint square to a face, as a shop drawing (the
//!   gauge lines, the bolts, the end distance and pitch), saved as SVG or
//!   PDF;
//! - the **connections table**: each member end, where it comes from and
//!   its condition, each value marked given, the member's, worked out,
//!   assumed or not given ([`ohl_structure_link::Source`]).
//!
//! The details are worked out by the link crate
//! ([`ohl_structure_link::joint_details`]) and kept until the file or the
//! libraries change. The end conditions shown are the check's
//! (`ohl_structures_fe::member_ends`, stage 4.1 and 4.2).

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use ohl_structure_link::{
    Completeness, FaceView, JointDetail, JointDetails, LineKind, Mark, Source,
};

use crate::app::StructureApp;
use ohl_structure_edit::Handle;

/// The joint details worked out, or why not.
type Details = Arc<Result<JointDetails, String>>;

/// What the drawing shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Drawn {
    /// The close-up in 3D.
    #[default]
    CloseUp,
    /// The face view.
    Face,
}

/// The panel's state.
#[derive(Default)]
pub struct JointPanel {
    /// The details, for (file revision, libraries stamp).
    details: Option<((u64, u64), Details)>,
    /// The fixes of the joint shown's problems, found in the background.
    pub fixes: crate::joint_fixes::Finder,
    /// The joint shown (follows the selection).
    pub joint: Option<String>,
    /// The face of the face view (none: the joint's first).
    pub face: Option<String>,
    /// What is drawn.
    pub drawn: Drawn,
    /// The close-up's turn about z and tilt, radians.
    pub yaw: f32,
    /// Its tilt.
    pub pitch: f32,
    /// Its zoom.
    pub zoom: f32,
    /// What the Detail view shows: the joint or the member
    /// ([`crate::member_panel`]).
    pub subject: crate::member_panel::Subject,
    /// The member of Member details.
    pub member: Option<String>,
    /// Its capacities worked out.
    pub capacity: crate::member_panel::CapacityCache,
    /// The Detail view's tool held ([`crate::detail_tools`]).
    pub tool: crate::detail_tools::DetailTool,
    /// A member end being dragged with Member end.
    pub drag: Option<crate::detail_tools::EndDrag>,
    /// What Member end or Move bolt has picked (a click picks, a drag
    /// moves only what is picked).
    pub tool_pick: Option<crate::detail_tools::ToolPick>,
    /// The steps Member end and Rotate member snap to (kept between runs).
    pub steps: crate::detail_tools::ToolSteps,
    /// The member Rotate member has picked.
    pub roll_pick: Option<String>,
    /// How far the picked member has been dragged sideways, points (a roll
    /// in the making, [`crate::detail_tools::roll_steps`]).
    pub roll_drag: Option<f32>,
    /// The joint the Detail view goes back to from a member (‹ Joint,
    /// Escape): the one shown before.
    pub back: Option<String>,
    /// What the Edit tool picked in the Detail view: its properties at
    /// the top of the details below it.
    pub picked: Option<Picked>,
    /// Where the Detail view drew last (Escape over it goes back).
    pub view_rect: Option<egui::Rect>,
    /// The Edit tool's question: open this joint or member in the Detail
    /// view?
    pub ask_open: Option<Handle>,
    /// The close-up's pieces and mesh kept between frames
    /// ([`crate::close_up::CloseUpCache`]).
    pub close_up: crate::close_up::CloseUpCache,
    /// A plate's corner being dragged in the face view
    /// ([`crate::plate_outline_ui::corner_handles`]).
    pub corner: Option<crate::plate_outline_ui::CornerDrag>,
}

/// What the Detail view's Edit tool picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picked {
    /// A bolt: its member end and its place among the end's bolts.
    Bolt {
        /// Member.
        member: String,
        /// Joint.
        joint: String,
        /// Its place among the end's bolts (0: the first).
        index: usize,
    },
    /// A plate, by id.
    Plate(String),
}

/// A colour per completeness, for the 3D view (View › Joints by their
/// details) and the panel.
pub fn completeness_color(c: Completeness) -> [u8; 4] {
    match c {
        Completeness::Complete => [80, 190, 110, 255],
        Completeness::PartlyAssumed => [235, 170, 60, 255],
        Completeness::Nothing => [200, 80, 80, 255],
    }
}

impl JointPanel {
    /// What the joint details kept were worked out for: (file revision,
    /// libraries stamp).
    pub(crate) fn details_key(&self) -> Option<(u64, u64)> {
        self.details.as_ref().map(|d| d.0)
    }
}

impl StructureApp {
    /// The joint details of the file open, worked out once per revision
    /// and libraries.
    pub(crate) fn joint_details(&mut self) -> Option<Arc<Result<JointDetails, String>>> {
        let f = self.file.as_ref()?;
        let key = (f.revision(), self.libs_stamp);
        if let Some((k, d)) = &self.joints.details
            && *k == key
        {
            return Some(d.clone());
        }
        let bolts = self.bolt_library();
        let f = self.file.as_ref()?;
        let d = Arc::new(ohl_structure_link::joint_details(
            f.text_ref(),
            &self.loads.sections,
            &bolts.0,
        ));
        self.joints.details = Some((key, d.clone()));
        Some(d)
    }

    /// The colours of the joints by their details (View › Joints by their
    /// details): a tower's only.
    pub(crate) fn joint_color_list(&mut self) -> Vec<(String, [u8; 4])> {
        let tower = self
            .file
            .as_ref()
            .is_some_and(|f| ohl_structure_edit::is_tower(f.structure().kind));
        if !self.joint_colors || !tower {
            return Vec::new();
        }
        match self.joint_details().as_deref() {
            Some(Ok(d)) => d
                .joints
                .iter()
                .map(|j| (j.id.clone(), completeness_color(j.completeness)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The Joint details panel.
    pub(crate) fn joint_panel_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("joint-details-panel")
            .auto_shrink([false, false])
            .show(ui, |ui| self.joint_panel_body(ui));
    }

    fn joint_panel_body(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &self.file else {
            ui.label("No structure open.");
            return;
        };
        let t = f.structure_arc();
        if !ohl_structure_edit::is_tower(t.kind) {
            ui.label(format!(
                "Joint details are a tower's: this structure is a {}. Geometry › Structure kind \
                 makes it a lattice tower or a portal.",
                kind_words(t.kind)
            ));
            return;
        }
        self.follow_selection();
        let Some(details) = self.joint_details() else {
            return;
        };
        let details = match details.as_ref() {
            Ok(d) => d,
            Err(e) => {
                crate::app::error_label(ui, format!("The joints could not be read: {e}"));
                return;
            }
        };
        if details.joints.is_empty() {
            ui.label("No joints yet: build the body first (Body › Generators).");
            return;
        }
        let id = self
            .joints
            .joint
            .clone()
            .filter(|j| details.joint(j).is_some())
            .unwrap_or_else(|| details.joints[0].id.clone());
        let mut chosen = id.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label("Joint");
            egui::ComboBox::from_id_salt("joint-detail-joint")
                .selected_text(&chosen)
                .height(320.0)
                .show_ui(ui, |ui| {
                    for j in &details.joints {
                        let [r, g, b, _] = completeness_color(j.completeness);
                        let text =
                            egui::RichText::new(format!("{} — {}", j.id, j.completeness.words()))
                                .color(egui::Color32::from_rgb(r, g, b));
                        ui.selectable_value(&mut chosen, j.id.clone(), text);
                    }
                });
            if let Some(j) = details.joint(&chosen) {
                let [r, g, b, _] = completeness_color(j.completeness);
                ui.colored_label(
                    egui::Color32::from_rgb(r, g, b),
                    format!(
                        "{} member end(s) · {}",
                        j.arrivals.len(),
                        j.completeness.words()
                    ),
                )
                .on_hover_text(
                    "complete: every member end's condition given, the member's or worked out; \
                     partly assumed: some of it assumed; no connection data: nothing given at \
                     any end",
                );
            }
        });
        if chosen != id {
            self.selected = vec![Handle::Joint(chosen.clone())];
            self.scroll_to = Some(Handle::Joint(chosen.clone()));
            self.joints.face = None;
        }
        self.joints.joint = Some(chosen.clone());
        let Some(j) = details.joint(&chosen) else {
            return;
        };
        // Which form the members' check takes (owner 2026-11-11).
        let (taken, bare) = self.file.as_ref().map_or((false, 0), |f| {
            let t = f.structure();
            (
                t.joint_details_taken(),
                t.ends_without_joint_details().len(),
            )
        });
        let short = if taken {
            "Joint details taken in the calculation."
        } else {
            "Members checked in the simple form: joint details not taken."
        };
        let how = if taken {
            "Joint details taken (Results \u{203a} Joint details in the calculation is on, level \
             4): the ASCE 10-97 check takes from this table each end's eccentric and restrained \
             (as shown) and its bolts, holes and leg bolted for the bolt shear, bearing and net \
             section, the weaker end governing; a shift is a rigid end offset in the model."
                .to_owned()
        } else if bare == 0 {
            "Level 4: every member end has its joint detail. The members are still checked in \
             the simple form \u{2014} joint details not taken \u{2014} until Results \u{203a} \
             Joint details in the calculation is ticked."
                .to_owned()
        } else {
            format!(
                "The members are checked in the simple form: joint details not taken. This \
                 table is the joint's design aid until every member end has its joint detail \
                 (level 4; {bare} end(s) still without, the To do list names the joints) and \
                 Results \u{203a} Joint details in the calculation is ticked."
            )
        };
        // One line; the rest on hover (UI review 2026-11-11).
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(short).small().weak());
            ui.label(egui::RichText::new("(why?)").small().weak().underline());
        })
        .response
        .on_hover_text(format!(
            "{how} The plates and their welds are checked per case with the analysis (Results \
             \u{203a} Run) whatever that option: below, under Plate checks. Values in a \
             connection override the member's design data at that end."
        ));
        if !details.level3 {
            ui.label(
                "The structure is not a full design yet (level 3: every member with a \
                 cross-section, supports): the members meeting here only. Connections belong to \
                 level 3.",
            );
            arrivals_only(ui, j);
            return;
        }
        ui.add_space(4.0);
        // What does not fit here, with the fixes that work (2026-11-08).
        self.problems_box(ui, details, &chosen);
        // What the Detail view's Edit tool picked (a bolt, a plate).
        self.picked_box(ui, details);
        if j.arrivals.iter().any(|a| {
            a.bolts.value.is_some() && a.profile.as_ref().is_some_and(|p| p.angle.is_none())
        }) {
            ui.label(
                egui::RichText::new(
                    "Bolts are drawn on angles only: a member of another cross-section (or a \
                     [[section]] table of values) shows its bolts in the table.",
                )
                .small()
                .weak(),
            );
        }
        self.joint_forms(ui);
        legend(ui);
        let t = self.file.as_ref().map(|f| f.structure_arc());
        let Some(t) = t else {
            return;
        };
        let mut acts = flat_notes(ui, &t, j);
        acts.extend(crate::joint_tools::joint_bar(
            ui,
            &t,
            details,
            j,
            &mut self.buffers,
        ));
        let presets = self.presets();
        // What the table's values mean, drawn (owner 2026-11-04); with
        // the values of the member end whose bolt is picked.
        let shown = match &self.joints.picked {
            Some(Picked::Bolt { member, joint, .. }) if *joint == j.id => j
                .arrivals
                .iter()
                .find(|a| &a.member == member)
                .map(|a| crate::joint_sketch::Shown::of(a, &j.id)),
            _ => None,
        };
        // Closed until asked for (it pushed the table down); egui keeps
        // it open once the user opened it.
        egui::CollapsingHeader::new("What the values mean")
            .id_salt("joint-values-sketch")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(match &shown {
                        Some(s) => format!(
                            "The leg bolted of an angle seen flat, with the values of {} (the \
                             bolt picked); hover a name: what it is.",
                            s.end.as_deref().unwrap_or("")
                        ),
                        None => "The leg bolted of an angle seen flat; hover a name: what it is. \
                                 Pick a bolt with Edit in the Detail view: its member end's \
                                 values written here."
                            .to_owned(),
                    })
                    .small()
                    .weak(),
                );
                // As wide as the panel; narrower than the drawing's
                // smallest: scrolled.
                let width = ui.available_width();
                egui::ScrollArea::horizontal()
                    .id_salt("joint-values-sketch-scroll")
                    .show(ui, |ui| {
                        crate::joint_sketch::show(ui, &shown.clone().unwrap_or_default(), width);
                    });
            });
        egui::CollapsingHeader::new("How member ends meet")
            .id_salt("joint-plies-sketch")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "In section: nested (Face), a double cover splice (+ Splice), continuous \
                         (Held); hover a name: what it is.",
                    )
                    .small()
                    .weak(),
                );
                let width = ui.available_width();
                egui::ScrollArea::horizontal()
                    .id_salt("joint-plies-sketch-scroll")
                    .show(ui, |ui| crate::joint_sketch::show_plies(ui, width));
            });
        acts.extend(crate::joint_tools::table(
            ui,
            &t,
            details,
            j,
            &mut self.buffers,
            &presets,
        ));
        if let Some(e) = self.presets_error() {
            crate::app::error_label(ui, format!("Connection presets: {e}"));
        }
        ui.label(
            egui::RichText::new(
                "Type or pick a value to give it at this member end; right-click a value (or \
                 pick \"not given here\") to go back to the member's or what is worked out. \
                 Each change is one undo step.",
            )
            .small()
            .weak(),
        );
        self.apply_joint_acts(acts, details);
        self.plate_results_ui(ui, &chosen);
        for n in &details.notes {
            ui.label(egui::RichText::new(n).small().weak());
        }
    }

    /// The plates' and welds' checks at joint `joint` from the last run
    /// (2026-11-04): per plate part its usage in the case shown, its
    /// largest over the cases with that case and what governs (with its
    /// clause); each check and what was assumed in a scroll box.
    pub(crate) fn plate_results_ui(&self, ui: &mut egui::Ui, joint: &str) {
        let Some(run) = self.results.run() else {
            return;
        };
        let solved: Vec<(&str, &ohl_structure_link::CaseResult)> = run
            .cases
            .iter()
            .filter_map(|c| Some((c.name.as_str(), c.fe.as_ref()?.as_ref().ok()?)))
            .collect();
        let Some((_, first)) = solved.first() else {
            return;
        };
        // The same parts in every case, in the same order.
        let here: Vec<usize> = first
            .plates
            .iter()
            .enumerate()
            .filter(|(_, p)| p.joint == joint)
            .map(|(k, _)| k)
            .collect();
        if here.is_empty() {
            return;
        }
        let shown = self.results.shown();
        let shown_name = run
            .cases
            .get(self.results.shown)
            .map_or("", |c| c.name.as_str());
        ui.add_space(6.0);
        ui.strong("Plate checks (the last run)");
        let colour = |u: Option<f64>| {
            let [r, g, b, _] = crate::results_tab::usage_colour(u, "");
            egui::Color32::from_rgb(r, g, b)
        };
        let pct = |u: Option<f64>| u.map_or("—".to_owned(), |u| format!("{:.0} %", u * 100.0));
        egui::Grid::new("joint-plate-checks")
            .striped(true)
            .show(ui, |ui| {
                for h in [
                    "Plate",
                    "Part",
                    &format!("Case {shown_name}"),
                    "Largest",
                    "In case",
                    "What governs",
                ] {
                    ui.strong(h);
                }
                ui.end_row();
                for &k in &here {
                    let p = &first.plates[k];
                    let now = shown.and_then(|r| r.plates.get(k));
                    let mut best: Option<(f64, &str, &str)> = None;
                    for (case, r) in &solved {
                        if let Some(x) = r.plates.get(k)
                            && let Some(u) = x.usage
                            && best.is_none_or(|b| ohl_structure_link::usage_order(u, b.0).is_gt())
                        {
                            best = Some((u, case, x.governs.as_str()));
                        }
                    }
                    ui.label(&p.plate);
                    ui.label(p.part.words());
                    let u = now.and_then(|x| x.usage);
                    ui.colored_label(colour(u), pct(u));
                    let b = best.map(|b| b.0);
                    ui.colored_label(colour(b), pct(b));
                    ui.label(best.map_or("", |b| b.1));
                    ui.label(
                        egui::RichText::new(best.map_or(
                            now.map_or(p.governs.as_str(), |x| x.governs.as_str()),
                            |b| b.2,
                        ))
                        .small(),
                    );
                    ui.end_row();
                }
            });
        if let Some(r) = shown {
            egui::CollapsingHeader::new(format!("Each check in case {shown_name}"))
                .id_salt("joint-plate-checks-each")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("joint-plate-checks-scroll")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for &k in &here {
                                let Some(p) = r.plates.get(k) else {
                                    continue;
                                };
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} {} — {:.1} kN",
                                        p.plate,
                                        p.part.words(),
                                        p.force / 1000.0
                                    ))
                                    .strong(),
                                );
                                for (what, u) in &p.checks {
                                    ui.colored_label(
                                        colour(Some(*u)),
                                        format!("{} {what}", pct(Some(*u))),
                                    );
                                }
                                for n in &p.notes {
                                    ui.label(egui::RichText::new(n).small().weak());
                                }
                            }
                        });
                });
        }
    }

    /// The Joint view pane (beside the 3D view): the joint shown in the
    /// Joint details panel as a close-up or a face view.
    pub(crate) fn joint_view_ui(&mut self, ui: &mut egui::Ui) {
        // Also when the Joint details panel is not the tab in front.
        self.follow_selection();
        let Some(f) = &self.file else {
            return;
        };
        if !ohl_structure_edit::is_tower(f.structure().kind) {
            ui.label("A tower's joints are drawn here (Geometry › Structure kind).");
            return;
        }
        let Some(details) = self.joint_details() else {
            return;
        };
        let Ok(details) = details.as_ref() else {
            ui.label("The joints could not be read (Design › Joint details says why).");
            return;
        };
        if self.joints.subject == crate::member_panel::Subject::Member
            && let Some(m) = self
                .joints
                .member
                .as_deref()
                .and_then(|id| ohl_structure_link::member_ends(details, id))
        {
            // The way back to the joint (owner 2026-11-03): the one it was
            // opened from first, then its other end; Escape too.
            let backs = self.back_joints();
            let mut back = None;
            ui.horizontal_wrapped(|ui| {
                for (k, j) in backs.iter().enumerate() {
                    let r = ui.button(format!("‹ Joint {j}"));
                    let r = if k == 0 {
                        r.on_hover_text(format!("Back to joint {j} in detail (Escape)"))
                    } else {
                        r.on_hover_text(format!("Joint {j}, the member's other end, in detail"))
                    };
                    if r.clicked() {
                        back = Some(j.clone());
                    }
                }
                ui.strong(format!("Member {}", m.member));
            });
            // Its roll, turned here too (2026-11-04).
            let t = self.file.as_ref().map(|f| f.structure_arc());
            if let Some(t) = t {
                let ends = crate::roll_ui::flat_ends(details, &m.member);
                let roll = crate::roll_ui::roll_of(&t, &m.member);
                if let Some(r) = crate::roll_ui::roll_control(ui, &m.member, roll, &ends) {
                    self.apply_joint_acts(
                        vec![crate::joint_tools::Act::Roll(m.member.clone(), r)],
                        details,
                    );
                }
            }
            let leg = ohl_structure_link::flat_leg(&m);
            crate::detail_tools::strip(
                ui,
                self.joints.tool,
                leg.is_some() && !self.joints.tool.in_close_up(),
                &mut self.joints.steps,
            );
            self.other_leg_button(ui, details);
            let ends: Vec<(String, String)> = m
                .ends
                .iter()
                .flatten()
                .map(|(j, a)| (a.member.clone(), j.clone()))
                .collect();
            let rules = self.bolt_rules_of(&ends);
            rules_caption(ui, &rules);
            let size = ui.available_size().max(egui::vec2(120.0, 120.0));
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
            self.joints.view_rect = Some(rect);
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            crate::member_panel::paint_member(
                &painter,
                rect,
                &m,
                leg.as_ref(),
                ui.visuals().dark_mode,
            );
            if let Some(leg) = &leg {
                let fit = crate::detail_tools::MemberFit::new(rect, leg);
                let acts = crate::detail_tools::member_act(
                    &m,
                    leg,
                    &fit,
                    &response,
                    self.joints.tool,
                    (
                        &mut self.joints.drag,
                        &mut self.joints.tool_pick,
                        self.joints.steps,
                    ),
                    &painter,
                    &rules,
                );
                self.apply_joint_acts(acts, details);
            }
            let has_leg = leg.is_some();
            crate::detail_tools::hotbar(ui, rect, &mut self.joints.tool, &|t| {
                has_leg && !t.in_close_up()
            });
            self.offer_window(ui.ctx(), details);
            if back.is_some() {
                self.detail_back(back);
            }
            return;
        }
        let id = self
            .joints
            .joint
            .clone()
            .filter(|j| details.joint(j).is_some())
            .or_else(|| details.joints.first().map(|j| j.id.clone()));
        let Some(j) = id.as_deref().and_then(|id| details.joint(id)) else {
            ui.label("No joints yet: build the body first (Body › Generators).");
            return;
        };
        if !details.level3 {
            ui.label(format!(
                "Joint {}: drawn at level 3 (every member with a cross-section, supports).",
                j.id
            ));
            return;
        }
        let faces = ohl_structure_link::faces_of(j);
        let face = self
            .joints
            .face
            .clone()
            .filter(|f| faces.contains(f))
            .unwrap_or_else(|| faces[0].clone());
        let mut plate_acts = Vec::new();
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("Joint {}", j.id));
            ui.selectable_value(&mut self.joints.drawn, Drawn::CloseUp, "Close-up")
                .on_hover_text("The joint in 3D: drag to turn it, scroll to zoom");
            ui.selectable_value(&mut self.joints.drawn, Drawn::Face, "Face view")
                .on_hover_text("The joint square to a face, as a shop drawing");
            if self.joints.drawn == Drawn::Face {
                let mut f = face.clone();
                egui::ComboBox::from_id_salt("joint-detail-face")
                    .selected_text(face_words(&f))
                    .show_ui(ui, |ui| {
                        for x in &faces {
                            ui.selectable_value(&mut f, x.clone(), face_words(x));
                        }
                    });
                self.joints.face = Some(f);
                let view = ohl_structure_link::face_view(j, &face);
                if ui
                    .button("Save as SVG…")
                    .on_hover_text("The face view on an A4 sheet, scaled to fit")
                    .clicked()
                {
                    self.save_face(&view, "svg");
                }
                if ui
                    .button("Save as PDF…")
                    .on_hover_text("The face view on an A4 sheet, scaled to fit")
                    .clicked()
                {
                    self.save_face(&view, "pdf");
                }
            } else {
                if ui.button("Reset view").clicked() {
                    self.joints.yaw = 0.0;
                    self.joints.pitch = 0.0;
                    self.joints.zoom = 0.0;
                }
            }
            let plates: Vec<(String, Vec<[f64; 3]>)> = j
                .plates
                .iter()
                .filter(|p| !p.outline_given && p.outline.len() >= 3)
                .map(|p| (p.id.clone(), p.outline.clone()))
                .collect();
            if !plates.is_empty()
                && ui
                    .button("Plate around the bolts")
                    .on_hover_text(
                        "Write the outline each plate without one is drawn with into the file, \
                         to be edited: round its bolts, each kept the design code's end and edge \
                         distances from its edge (Joint details says which)",
                    )
                    .clicked()
            {
                let acts = plates
                    .into_iter()
                    .map(|(id, o)| crate::joint_tools::Act::PlateOutline(id, o))
                    .collect();
                plate_acts = acts;
            }
        });
        if !plate_acts.is_empty() {
            self.apply_joint_acts(std::mem::take(&mut plate_acts), details);
        }
        // The member Rotate member picked: its roll row too (typed, quarter
        // turns, Lay flat).
        if self.joints.drawn == Drawn::CloseUp
            && self.joints.tool == crate::detail_tools::DetailTool::RotateMember
            && let Some(m) = self
                .joints
                .roll_pick
                .clone()
                .filter(|m| j.arrivals.iter().any(|a| &a.member == m))
            && let Some(t) = self.file.as_ref().map(|f| f.structure_arc())
        {
            let ends = crate::roll_ui::flat_ends(details, &m);
            let roll = crate::roll_ui::roll_of(&t, &m);
            let mut out = None;
            ui.horizontal(|ui| {
                ui.strong(format!("Member {m}"));
                out = crate::roll_ui::roll_control(ui, &m, roll, &ends);
            });
            if let Some(r) = out {
                self.apply_joint_acts(vec![crate::joint_tools::Act::Roll(m, r)], details);
            }
        }
        // The face view's tools there, Rotate member in the close-up.
        if self.joints.drawn == Drawn::Face && self.joints.tool.in_close_up() {
            self.joints.tool = crate::detail_tools::DetailTool::Edit;
        }
        let active = (self.joints.drawn == Drawn::Face) != self.joints.tool.in_close_up();
        crate::detail_tools::strip(ui, self.joints.tool, active, &mut self.joints.steps);
        self.other_leg_button(ui, details);
        let size = ui.available_size().max(egui::vec2(120.0, 120.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        self.joints.view_rect = Some(rect);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        match self.joints.drawn {
            Drawn::CloseUp => {
                let mut roll_set = None;
                let rotating = self.joints.tool == crate::detail_tools::DetailTool::RotateMember;
                if rotating {
                    // A click picks a member, a drag on the picked one rolls it.
                    if (response.clicked() || response.drag_started())
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        let hit = crate::close_up::member_at(j, &self.joints, rect, pos)
                            .map(|ia| j.arrivals[ia].member.clone());
                        if crate::detail_tools::press(
                            &mut self.joints.roll_pick,
                            hit,
                            response.clicked(),
                            response.drag_started(),
                        ) {
                            self.joints.roll_drag = Some(0.0);
                        }
                    }
                } else {
                    self.joints.roll_drag = None;
                }
                if let Some(dx) = self.joints.roll_drag.as_mut() {
                    *dx += response.drag_delta().x;
                    if response.drag_stopped() || !response.dragged() {
                        let turn = crate::detail_tools::roll_steps(*dx, self.joints.steps.roll);
                        if turn != 0.0
                            && let Some(m) = self.joints.roll_pick.clone()
                            && let Some(t) = self.file.as_ref().map(|f| f.structure_arc())
                        {
                            roll_set = Some((m.clone(), crate::roll_ui::roll_of(&t, &m) + turn));
                        }
                        self.joints.roll_drag = None;
                    }
                } else if response.dragged() {
                    let d = response.drag_delta();
                    self.joints.yaw += d.x * 0.01;
                    self.joints.pitch = (self.joints.pitch + d.y * 0.01).clamp(-1.5, 1.5);
                }
                if response.hovered() {
                    let s = ui.input(|i| i.smooth_scroll_delta.y);
                    self.joints.zoom = (self.joints.zoom + s * 0.002).clamp(-1.5, 2.5);
                }
                let key = self.joints.details.as_ref().map_or((0, 0), |d| d.0);
                let dark = ui.visuals().dark_mode;
                // While a roll is dragged: the joint with the member turned
                // (its own pieces, a variant of its own).
                let turn = self.joints.roll_drag.map_or(0.0, |dx| {
                    crate::detail_tools::roll_steps(dx, self.joints.steps.roll)
                });
                let preview = self
                    .joints
                    .roll_pick
                    .as_deref()
                    .filter(|_| turn != 0.0)
                    .map(|m| crate::close_up::rolled(j, m, turn));
                let variant = preview.as_ref().map_or(0, |_| turn.to_bits());
                let shown = preview.as_ref().unwrap_or(j);
                crate::close_up::paint(&painter, rect, shown, &mut self.joints, key, dark, variant);
                if rotating {
                    let pick = self
                        .joints
                        .roll_pick
                        .as_deref()
                        .and_then(|m| shown.arrivals.iter().position(|a| a.member == m));
                    let blue = crate::detail_tools::PICKED;
                    if let Some(ia) = pick {
                        let ends = crate::close_up::axis_on_screen(shown, &self.joints, rect, ia);
                        painter.line_segment(ends, egui::Stroke::new(3.0, blue));
                        let words = if turn != 0.0 {
                            format!("{} {:+.0}°", shown.arrivals[ia].member, turn.to_degrees())
                        } else {
                            format!(
                                "{} picked: drag it sideways to roll it",
                                shown.arrivals[ia].member
                            )
                        };
                        // Under the view's own line, where it is never cut.
                        painter.text(
                            rect.left_top() + egui::vec2(6.0, 24.0),
                            egui::Align2::LEFT_TOP,
                            words,
                            egui::FontId::proportional(12.0),
                            blue,
                        );
                    }
                    if self.joints.roll_drag.is_none()
                        && let Some(pos) = response.hover_pos()
                        && let Some(ia) = crate::close_up::member_at(j, &self.joints, rect, pos)
                        && Some(ia) != pick
                    {
                        let ends = crate::close_up::axis_on_screen(j, &self.joints, rect, ia);
                        painter
                            .line_segment(ends, egui::Stroke::new(1.5, blue.gamma_multiply(0.8)));
                    }
                }
                if let Some((m, r)) = roll_set {
                    self.apply_joint_acts(vec![crate::joint_tools::Act::Roll(m, r)], details);
                }
                // A plate under the pointer: its thickness and more.
                if let Some(pos) = response.hover_pos()
                    && !response.dragged()
                    && let Some(p) = crate::close_up::plate_at(j, &self.joints, rect, pos)
                {
                    response
                        .clone()
                        .on_hover_text_at_pointer(crate::close_up::plate_hover(p));
                }
            }
            Drawn::Face => {
                let view = ohl_structure_link::face_view(j, &face);
                paint_face(&painter, rect, &view, ui.visuals().dark_mode);
                let ends: Vec<(String, String)> = j
                    .arrivals
                    .iter()
                    .map(|a| (a.member.clone(), j.id.clone()))
                    .collect();
                let rules = self.bolt_rules_of(&ends);
                let fit = crate::detail_tools::FaceFit::new(rect, view.bounds);
                // A plate under the pointer: its thickness and more (the
                // nearest to the viewer first).
                if let Some(pos) = response.hover_pos() {
                    let [r, u, toward] = ohl_structure_link::face_axes(&face);
                    let rel = |q: &[f64; 3]| -> [f64; 3] {
                        std::array::from_fn(|k| q[k] - j.position[k])
                    };
                    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                    let hit = j
                        .plates
                        .iter()
                        .filter(|p| {
                            let pts: Vec<egui::Pos2> = p
                                .outline
                                .iter()
                                .map(|q| fit.at([dot(rel(q), r), dot(rel(q), u)]))
                                .collect();
                            pts.len() >= 3 && crate::close_up::inside_2d(&pts, pos)
                        })
                        .max_by(|a, b| {
                            let d = |p: &ohl_structure_link::PlateDetail| {
                                dot(rel(&p.outline[0]), toward)
                            };
                            d(a).total_cmp(&d(b))
                        });
                    if let Some(p) = hit {
                        response
                            .clone()
                            .on_hover_text_at_pointer(crate::close_up::plate_hover(p));
                    }
                }
                // A picked plate's corners, dragged in its plane.
                let picked = match &self.joints.picked {
                    Some(Picked::Plate(id)) => Some(id.as_str()),
                    _ => None,
                };
                let corner_acts = crate::plate_outline_ui::corner_handles(
                    j,
                    picked,
                    &face,
                    &fit,
                    &response,
                    self.joints.tool,
                    &mut self.joints.corner,
                    &painter,
                );
                let mut acts = crate::detail_tools::face_act(
                    j,
                    &face,
                    &fit,
                    &response,
                    self.joints.tool,
                    (
                        &mut self.joints.drag,
                        &mut self.joints.tool_pick,
                        self.joints.steps,
                    ),
                    &painter,
                    &rules,
                );
                acts.extend(corner_acts);
                // The bolt picked with Edit, ringed.
                if let Some(Picked::Bolt {
                    member,
                    joint,
                    index,
                }) = &self.joints.picked
                    && joint == &j.id
                    && let Some(a) = j.arrivals.iter().find(|a| &a.member == member)
                    && let Some(p) = a.bolt_points.get(*index)
                {
                    let [r, u, _] = ohl_structure_link::face_axes(&face);
                    let q = [
                        p[0] - j.position[0],
                        p[1] - j.position[1],
                        p[2] - j.position[2],
                    ];
                    let v = [
                        q[0] * r[0] + q[1] * r[1] + q[2] * r[2],
                        q[0] * u[0] + q[1] * u[1] + q[2] * u[2],
                    ];
                    let rad = a.bolts.value.as_ref().map_or(0.008, |b| b.diameter) * fit.scale();
                    painter.circle_stroke(
                        fit.at(v),
                        rad as f32 + 5.0,
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(90, 150, 230)),
                    );
                }
                self.apply_joint_acts(acts, details);
            }
        }
        // Every tool offered: one of the face view's taken in the close-up
        // shows the face view, Rotate member taken in the face view the
        // close-up (Edit stays where it is).
        let before = self.joints.tool;
        crate::detail_tools::hotbar(ui, rect, &mut self.joints.tool, &|_| true);
        let now = self.joints.tool;
        if now != before {
            if now.in_close_up() {
                self.joints.drawn = Drawn::CloseUp;
            } else if now != crate::detail_tools::DetailTool::Edit {
                self.joints.drawn = Drawn::Face;
            }
        }
        self.offer_window(ui.ctx(), details);
    }

    /// Asks where, then writes the face view as `ext` (svg, pdf).
    fn save_face(&mut self, view: &FaceView, ext: &str) {
        let stem = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(|p| p.file_stem())
            .map_or("structure".to_owned(), |s| s.to_string_lossy().into_owned());
        let name = format!(
            "{stem}-joint-{}-{}.{ext}",
            view.joint,
            view.face.replace('+', "p").replace('-', "m")
        );
        let dir: Option<PathBuf> = self
            .file
            .as_ref()
            .and_then(|f| f.path())
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        let mut dialog = rfd::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(&name);
        if let Some(d) = dir {
            dialog = dialog.set_directory(d);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        let bytes = if ext == "pdf" {
            ohl_structure_link::face_pdf(view)
        } else {
            ohl_structure_link::face_svg(view).into_bytes()
        };
        self.message = Some(match std::fs::write(&path, bytes) {
            Ok(()) => (format!("face view written: {}", path.display()), false),
            Err(e) => (format!("{}: {e}", path.display()), true),
        });
    }
}

/// The members whose leg bolted does not lie flat on what it bolts onto,
/// each with a one-click fix (its roll turned so it does; owner
/// 2026-11-04), and the notes on the member ends.
fn flat_notes(
    ui: &mut egui::Ui,
    t: &ohl_structures::StructureType,
    j: &JointDetail,
) -> Vec<crate::joint_tools::Act> {
    let mut acts = Vec::new();
    for a in &j.arrivals {
        if let Some(f) = a.flat
            && f.off > ohl_structure_link::FLAT_OFF
            && let Some(o) = a.onto.value.as_ref()
        {
            let what = if j.plates.iter().any(|p| &p.id == o) {
                "plate"
            } else {
                "leg of"
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "{}: {}{} bolted face {:.0}° off the {what} {o} — turn it",
                        j.id,
                        a.member,
                        a.profile
                            .as_ref()
                            .map_or(String::new(), |p| format!(" ({})", p.section)),
                        f.off
                    ),
                );
                if ui
                    .small_button("Turn it flat")
                    .on_hover_text(format!(
                        "Its roll turned by {:+.1}° so its leg bolted lies flat on {o} (one undo \
                         step; its other end follows, and its axes for the analysis)",
                        f.turn.to_degrees()
                    ))
                    .clicked()
                {
                    let roll = crate::roll_ui::roll_of(t, &a.member) + f.turn;
                    acts.push(crate::joint_tools::Act::Roll(a.member.clone(), roll));
                }
            });
        }
        // A member end that does not fit as given (2026-11-05: a nested
        // angle too large, its leg through another's): its notes red.
        for n in a.notes.iter().filter(|n| !n.ends_with("— turn it")) {
            let text = egui::RichText::new(format!("{}: {n}", a.member)).small();
            if a.misfit {
                ui.colored_label(ui.visuals().error_fg_color, text);
            } else {
                ui.label(text.weak());
            }
        }
    }
    acts
}

fn kind_words(k: ohl_structures::structures::StructureKind) -> &'static str {
    use ohl_structures::structures::StructureKind as K;
    match k {
        K::Lattice => "lattice tower",
        K::Pole => "pole",
        K::Portal => "portal",
        K::Other => "structure of another kind",
    }
}

fn face_words(f: &str) -> String {
    match f {
        "plan" => "plan (from above)".into(),
        f => format!("face {f}"),
    }
}

use crate::joint_tools::source_color;

fn legend(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for (s, what) in [
            (Source::Given, "given at the member end"),
            (
                Source::Member,
                "the member's design data (both ends alike), in italics",
            ),
            (
                Source::WorkedOut,
                "worked out from the geometry, the bolts or the leg",
            ),
            (
                Source::Assumed,
                "assumed: as the checks assume, or for the drawing only",
            ),
            (Source::Missing, "not given"),
        ] {
            let mut t = egui::RichText::new(s.words())
                .small()
                .color(source_color(ui, s));
            if s == Source::Member {
                t = t.italics();
            }
            ui.label(t).on_hover_text(what);
        }
    });
}

/// The members meeting at `j` (below level 3).
fn arrivals_only(ui: &mut egui::Ui, j: &JointDetail) {
    egui::Grid::new("joint-arrivals")
        .striped(true)
        .show(ui, |ui| {
            for h in ["Member", "To joint", "Face", "Length"] {
                ui.strong(h);
            }
            ui.end_row();
            for a in &j.arrivals {
                ui.label(&a.member);
                ui.label(&a.other);
                ui.label(&a.face);
                ui.label(format!("{:.2} m", a.length));
                ui.end_row();
            }
        });
}

/// The face view painted into `rect`, fitted.
fn paint_face(painter: &egui::Painter, rect: egui::Rect, v: &FaceView, dark: bool) {
    // The same fit as the tools ([`crate::detail_tools::FaceFit`]).
    let fit = crate::detail_tools::FaceFit::new(rect, v.bounds);
    let k = fit.scale();
    let at = |p: [f64; 2]| fit.at(p);
    let _ = k;
    let ink = if dark {
        egui::Color32::from_gray(225)
    } else {
        egui::Color32::from_gray(20)
    };
    let grey = egui::Color32::from_gray(140);
    for m in &v.marks {
        match m {
            Mark::Line {
                points,
                closed,
                kind,
            } => {
                let stroke = match kind {
                    LineKind::Member => egui::Stroke::new(1.6, ink),
                    LineKind::Edge => egui::Stroke::new(0.8, ink),
                    LineKind::Gauge | LineKind::Dimension => egui::Stroke::new(0.7, grey),
                    LineKind::Axis => egui::Stroke::new(0.9, egui::Color32::from_rgb(90, 150, 230)),
                    LineKind::Weld => egui::Stroke::new(3.0, egui::Color32::from_rgb(215, 70, 50)),
                    LineKind::Plate => {
                        egui::Stroke::new(1.4, egui::Color32::from_rgb(70, 140, 210))
                    }
                };
                let pts: Vec<egui::Pos2> = points.iter().map(|p| at(*p)).collect();
                if matches!(kind, LineKind::Gauge | LineKind::Axis) && pts.len() == 2 {
                    painter.add(egui::Shape::dashed_line(&pts, stroke, 6.0, 3.0));
                } else if *kind == LineKind::Plate && pts.len() >= 3 {
                    // A plate filled (translucent) under the members.
                    painter.add(egui::Shape::convex_polygon(
                        pts,
                        egui::Color32::from_rgba_unmultiplied(70, 140, 210, 70),
                        stroke,
                    ));
                } else if *closed {
                    painter.add(egui::Shape::closed_line(pts, stroke));
                } else {
                    painter.add(egui::Shape::line(pts, stroke));
                }
            }
            Mark::Bolt { parts, assumed, .. } => {
                let color = if *assumed {
                    egui::Color32::from_rgb(235, 170, 60)
                } else {
                    egui::Color32::from_rgb(210, 60, 60)
                };
                let fill = if dark {
                    egui::Color32::from_gray(45)
                } else {
                    egui::Color32::from_gray(225)
                };
                for part in parts {
                    let pts: Vec<egui::Pos2> = part.iter().map(|p| at(*p)).collect();
                    if pts.len() >= 3 {
                        painter.add(egui::Shape::convex_polygon(
                            pts,
                            fill,
                            egui::Stroke::new(1.0, color),
                        ));
                    }
                }
            }
            Mark::Text { at: p, text, label } => {
                painter.text(
                    at(*p),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::proportional(if *label { 12.0 } else { 10.5 }),
                    if *label { ink } else { grey },
                );
            }
        }
    }
    // At the top: the tool strip is at the bottom.
    painter.text(
        rect.left_top() + egui::vec2(6.0, 6.0),
        egui::Align2::LEFT_TOP,
        &v.title,
        egui::FontId::proportional(11.0),
        ink,
    );
}

/// The design code's bolt minimums of the ends drawn, one line each (the
/// bolt tools keep them).
fn rules_caption(ui: &mut egui::Ui, rules: &crate::detail_tools::Rules) {
    let mut ends: Vec<_> = rules.iter().collect();
    ends.sort_by(|x, y| x.0.cmp(y.0));
    for ((m, j), r) in ends {
        ui.label(
            egui::RichText::new(format!("{m} at {j}: {}", r.words()))
                .small()
                .weak(),
        );
    }
}

impl crate::app::StructureApp {
    /// **Problems** of joint `joint` (owner, 2026-11-08: "a tool to help
    /// resolve errors"): each member end's in words, red, with the fixes
    /// that worked when tried on a copy of the structure (each a button:
    /// one undo step; its hover: the joint's problems before and after).
    fn problems_box(&mut self, ui: &mut egui::Ui, details: &JointDetails, joint: &str) {
        let n = crate::joint_fixes::problems(details, joint).len();
        if n == 0 {
            return;
        }
        let Some(f) = self.file.as_ref() else {
            return;
        };
        let key = (f.revision(), self.libs_stamp, joint.to_owned());
        let input = if self.joints.fixes.needs(&key) {
            let text = f.text_ref().to_owned();
            let sections = self.loads.sections.clone();
            let bolts = self.bolt_library().0.clone();
            Some((text, sections, bolts))
        } else {
            None
        };
        let ctx = ui.ctx().clone();
        let found = self.joints.fixes.get(key, input, &ctx).map(<[_]>::to_vec);
        let mut apply = None;
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("Problems ({n})")).color(ui.visuals().error_fg_color),
        )
        .id_salt("joint-problems")
        .default_open(true)
        .show(ui, |ui| {
            let Some(found) = found else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("trying the fixes on a copy…");
                });
                return;
            };
            for p in &found {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, format!("{}:", p.member));
                    ui.label(egui::RichText::new(&p.text).small());
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(12.0);
                    if p.fixes.is_empty() {
                        ui.label(
                            egui::RichText::new("no fix found that works: change it by hand")
                                .small()
                                .weak(),
                        );
                    }
                    for (fix, before, after) in &p.fixes {
                        if ui
                            .small_button(&fix.label)
                            .on_hover_text(format!(
                                "Tried on a copy: the joint's problems {before}, after it {after}; one \
                                 undo step"
                            ))
                            .clicked()
                        {
                            apply = Some(fix.clone());
                        }
                    }
                });
            }
        });
        if let Some(fix) = apply
            && let Some(f) = self.file.as_mut()
        {
            f.edit(&fix.label, |doc, _| {
                for c in &fix.changes {
                    c.apply(doc);
                }
            });
            self.message = Some((format!("{joint}: {}", fix.label), false));
        }
    }
}

impl crate::app::StructureApp {
    /// With Move bolt and a bolt picked on an angle: **To the other leg**
    /// (key L; owner, 2026-11-08: "moving single bolts to the other leg"),
    /// the bolt where it was along and across, the end then bolted by both
    /// legs (or by the one left); the bolt stays picked, so L takes it
    /// back.
    fn other_leg_button(&mut self, ui: &mut egui::Ui, details: &ohl_structure_link::JointDetails) {
        use crate::detail_tools::{DetailTool, ToolPick};
        if self.joints.tool != DetailTool::MoveBolt {
            return;
        }
        let Some(ToolPick::Bolt {
            member,
            joint,
            index,
        }) = self.joints.tool_pick.clone()
        else {
            return;
        };
        let Some(a) = details
            .joint(&joint)
            .and_then(|j| j.arrivals.iter().find(|a| a.member == member))
            .cloned()
        else {
            return;
        };
        if a.profile.as_ref().and_then(|p| p.angle).is_none() {
            return;
        }
        let pressed = ui
            .button("To the other leg (L)")
            .on_hover_text(
                "The picked bolt onto the angle's other leg, where it is along the member and \
                 across from the heel (both legs bolted then)",
            )
            .clicked()
            || (!ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::L)));
        if !pressed {
            return;
        }
        let n = a.bolt_flat.len();
        let moved = if index < n {
            a.bolt_flat.get(index)
        } else {
            a.other_flat.get(index - n)
        }
        .copied();
        let Some(act) = crate::detail_tools::to_other_leg(&a, &joint, index) else {
            return;
        };
        // Still picked where it went.
        if let (crate::joint_tools::Act::LegBolts { bolted, other, .. }, Some(q)) = (&act, moved) {
            let same = |p: &[f64; 2]| (p[0] - q[0]).abs() < 1e-9;
            let in_bolted = || bolted.iter().rposition(same);
            let in_other = || other.iter().rposition(same).map(|k| bolted.len() + k);
            // Looked for where it went first (a leg left empty swaps).
            let at = if index < n {
                in_other().or_else(in_bolted)
            } else {
                in_bolted().or_else(in_other)
            };
            if let Some(k) = at {
                self.joints.tool_pick = Some(ToolPick::Bolt {
                    member: member.clone(),
                    joint: joint.clone(),
                    index: k,
                });
            }
        }
        self.apply_joint_acts(vec![act], details);
    }
}
