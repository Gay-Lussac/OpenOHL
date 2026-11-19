// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Bolt grade** (the owner's decision of 2026-11-19: "add a command to
//! set it"): Design › Bolt grade… opens one tool window (the tool-window
//! pattern) that sets the grade of the bolts of the members picked, a
//! member group's or the whole tower — its strengths those of SIA
//! 263:2013, 3.4.2, Table 2 ([`ohl_structures::bolts::SIA263_GRADES`]),
//! written into the structure file with their source and "set by
//! command" (`[[bolt_grade]]`, a member's `bolt_grade`; structure file
//! version 21) — or takes it away. **Apply** writes it as one undo step.
//! Nothing takes a grade by itself: a bolt without strengths stays without
//! them, its checks said NOT ON THE SAFE SIDE.

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structures::bolts::{SIA263_GRADES, SIA263_GRADES_SOURCE, StandardGrade};

use crate::app::StructureApp;

/// The manual's section of the window (page, anchor).
pub const BOLT_GRADE_PAGE: (&str, &str) = ("user-guide/bolt-grade", BOLT_GRADE_ANCHOR);
const BOLT_GRADE_ANCHOR: &str = "bolt-grade";

/// Which members' bolts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GradeScope {
    /// The members picked.
    Picked,
    /// A member group's.
    Group(String),
    /// Every member.
    Tower,
}

/// The Bolt grade window.
pub struct BoltGradeWindow {
    /// Which members' bolts.
    pub scope: GradeScope,
    /// The grade (none: take the grade away).
    pub grade: Option<&'static str>,
    /// Why the last Apply was refused.
    pub error: Option<String>,
}

impl StructureApp {
    /// Opens the Bolt grade window: the members picked when some are, else
    /// the whole tower; 8.8 offered first (the owner's choice for the
    /// current line, 2026-11-19 — offered, never set by itself).
    pub(crate) fn open_bolt_grade_window(&mut self) {
        let picked = self.selected.iter().any(|h| matches!(h, Handle::Member(_)));
        self.bolt_grade_window = Some(BoltGradeWindow {
            scope: if picked {
                GradeScope::Picked
            } else {
                GradeScope::Tower
            },
            grade: Some("8.8"),
            error: None,
        });
    }

    /// The members the window's scope names.
    fn grade_members(&self, w: &BoltGradeWindow) -> Vec<String> {
        let Some(f) = self.file.as_ref() else {
            return Vec::new();
        };
        let t = f.structure();
        match &w.scope {
            GradeScope::Tower => t.members.iter().map(|m| m.id.clone()).collect(),
            GradeScope::Picked => self
                .selected
                .iter()
                .filter_map(|h| match h {
                    Handle::Member(m) => Some(m.clone()),
                    _ => None,
                })
                .collect(),
            GradeScope::Group(g) => t
                .members
                .iter()
                .filter(|m| m.group.as_deref() == Some(g.as_str()))
                .map(|m| m.id.clone())
                .collect(),
        }
    }

    /// The Bolt grade window ([`Self::open_bolt_grade_window`]); its
    /// section of the manual: [`BOLT_GRADE_PAGE`].
    pub(crate) fn bolt_grade_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.bolt_grade_window.take() else {
            return;
        };
        let Some(f) = self.file.as_ref() else {
            return;
        };
        let t = f.structure();
        let groups: Vec<String> = ohl_structure_edit::groups(t)
            .into_iter()
            .map(|g| g.name)
            .collect();
        let picked = self
            .selected
            .iter()
            .filter(|h| matches!(h, Handle::Member(_)))
            .count();
        let all = t.members.len();
        // What is set now, per grade.
        let mut now: Vec<(String, usize, String)> = t
            .bolt_grades
            .iter()
            .map(|g| {
                let n = t
                    .members
                    .iter()
                    .filter(|m| m.bolt_grade.as_deref().map(str::trim) == Some(g.grade.trim()))
                    .count();
                (g.grade.clone(), n, g.words())
            })
            .collect();
        now.retain(|x| x.1 > 0);
        let none = t.members.iter().filter(|m| m.bolt_grade.is_none()).count();
        let members = self.grade_members(&w);
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Bolt grade")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ohl_ui::tool_window::intro(
                    ui,
                    "Sets the grade of the bolts of the members picked, a member group's or the \
                     whole tower — their strengths from SIA 263:2013 Table 2 — or takes it away.",
                    "The grade gives the bolts its ultimate strength f_ub and yield strength \
                     f_yb (SIA 263:2013, 3.4.2, Table 2, the strength classes of SN EN \
                     20898-2), over what their bolt library or the member's design data give: \
                     the design code then checks the bolts' shear, bearing and tension (with \
                     prying) on them. A capacity the library gives for another grade is \
                     dropped and the grade's worked out by the code instead (said). The grade \
                     is written into the structure file with its values, their source and \
                     'set by command', so every result says where the bolts' strengths come \
                     from. Nothing takes a grade by itself: a bolt without strengths stays \
                     without them, its checks said NOT ON THE SAFE SIDE. A member's bolts \
                     include its ends' and a gusset plate's own bolts onto it. One undo step.",
                    BOLT_GRADE_ANCHOR,
                    || crate::help::open_section(BOLT_GRADE_PAGE.0, BOLT_GRADE_PAGE.1),
                );
                ohl_ui::tool_window::heading(
                    ui,
                    "1. Which bolts",
                    "The bolts of these members: their ends' bolts, their design data's bolts, \
                     and a gusset plate's own bolts onto them.",
                );
                ui.radio_value(
                    &mut w.scope,
                    GradeScope::Picked,
                    format!("The members picked ({picked})"),
                );
                ui.horizontal(|ui| {
                    let in_group = matches!(w.scope, GradeScope::Group(_));
                    if ui
                        .add_enabled(
                            !groups.is_empty(),
                            egui::RadioButton::new(in_group, "A member group's"),
                        )
                        .on_disabled_hover_text("The tower has no member groups")
                        .clicked()
                        && !in_group
                        && let Some(g) = groups.first()
                    {
                        w.scope = GradeScope::Group(g.clone());
                    }
                    if let GradeScope::Group(g) = &mut w.scope {
                        egui::ComboBox::from_id_salt("bolt-grade-group")
                            .selected_text(g.clone())
                            .show_ui(ui, |ui| {
                                for x in &groups {
                                    ui.selectable_value(g, x.clone(), x);
                                }
                            });
                    }
                });
                ui.radio_value(
                    &mut w.scope,
                    GradeScope::Tower,
                    format!("The whole tower ({all} members)"),
                );
                ohl_ui::tool_window::heading(
                    ui,
                    "2. The grade",
                    "The strength classes of SIA 263:2013, 3.4.2, Table 2 (read in the \
                     standard's text). 4.8, 5.8 and 6.8 are not used in Switzerland (their \
                     elongation too low, the table's footnote). 'None' takes the grade away: \
                     the bolts' own strengths again, if they have any.",
                );
                let text = |g: Option<&str>| match g.and_then(StandardGrade::sia263) {
                    Some(s) => format!(
                        "{} — f_ub {:.0} MPa, f_yb {:.0} MPa{}",
                        s.grade,
                        s.ultimate_mpa,
                        s.yield_mpa,
                        if s.used_in_switzerland {
                            ""
                        } else {
                            " (not used in Switzerland)"
                        }
                    ),
                    None => "None: take the grade away".to_owned(),
                };
                egui::ComboBox::from_id_salt("bolt-grade-grade")
                    .selected_text(text(w.grade))
                    .width(360.0)
                    .show_ui(ui, |ui| {
                        for g in SIA263_GRADES {
                            ui.selectable_value(&mut w.grade, Some(g.grade), text(Some(g.grade)));
                        }
                        ui.selectable_value(&mut w.grade, None, text(None));
                    });
                ui.label(
                    egui::RichText::new(format!("Values from {SIA263_GRADES_SOURCE}."))
                        .small()
                        .weak(),
                );
                ui.separator();
                ohl_ui::tool_window::heading(
                    ui,
                    "Set now",
                    "The grades the file gives the members' bolts now, each with where its \
                     values come from and who set it.",
                );
                for (_, n, words) in &now {
                    ui.label(format!("{words}: {n} member(s)"));
                }
                // Always visible: what stays not on the safe side.
                if none > 0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "{none} member(s) without a grade: their bolts take only their own \
                             strengths — where they have none, their checks are said NOT ON \
                             THE SAFE SIDE."
                        ))
                        .color(ui.visuals().warn_fg_color),
                    );
                }
                if let Some(e) = &w.error {
                    crate::app::error_label(ui, e.as_str());
                }
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                let n = members.len();
                match buttons(
                    ui,
                    &format!("Apply ({n} members)"),
                    n > 0,
                    Some("No members: pick some, or choose a group or the whole tower"),
                    Keys::Escape,
                ) {
                    Some(Pressed::Action) => apply = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if apply {
            let record = w.grade.and_then(StandardGrade::sia263).map(|g| g.record());
            let what = match &record {
                Some(g) => format!("Bolt grade {} ({} members)", g.grade, members.len()),
                None => format!("Bolt grade taken away ({} members)", members.len()),
            };
            let r = self.file.as_mut().map(|f| {
                f.edit_alone(&what, |doc, _| {
                    ohl_structure_edit::set_bolt_grade(doc, &members, record.as_ref())
                })
            });
            match r {
                Some(Ok(s)) => {
                    self.message = Some((
                        format!(
                            "{what}: {} changed, {} already so (one undo step)",
                            s.changed, s.unchanged
                        ),
                        false,
                    ));
                    return;
                }
                Some(Err(e)) => w.error = Some(e),
                None => return,
            }
        }
        if open && !cancel {
            self.bolt_grade_window = Some(w);
        }
    }
}
