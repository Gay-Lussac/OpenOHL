// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Link and Move attachment point windows (owner 2026-11-15): the
//! chooser (several points at the place clicked: one to move) and the
//! window asking how the point goes to the joint — onto it (the default)
//! or its position kept at an offset, when it is not on it (farther than
//! [`ohl_structures::ON_JOINT`], 1 mm) — and, for a move, where: this
//! structure only (its relink, one command of the project) or the type
//! (its file, as OHL-Structure changes it: every structure of the type).
//! Escape cancels either.

use super::*;

use crate::link_tool::Scope;
use ohl_structure_edit::links::LinkHow;

/// Both windows, the chooser first.
pub(crate) fn link_windows(ctx: &egui::Context, r: &mut Ready) {
    if r.point_chooser.is_some() {
        chooser_window(ctx, r);
        return;
    }
    link_window(ctx, r);
}

fn chooser_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(c) = r.point_chooser.clone() else {
        return;
    };
    let mut chosen = None;
    let mut close = false;
    tool_window(
        r,
        "link",
        format!("Move attachment point — {}", c.structure),
    )
    .show(ctx, |ui| {
        crate::tool_ui::intro_of(
            ui,
            &format!(
                "{} attachment points of {} are at this place: choose the one to move.",
                c.labels.len(),
                c.structure
            ),
            "Each row is an attachment point at the place clicked, with the strings hanging from \
             it and the sets of their wire points. The point chosen is moved to the joint you \
             click next; its strings follow. Escape: nothing changes.",
            crate::tools::Tool::MovePoint,
        );
        egui::Grid::new("move-point-chooser")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("point");
                ui.strong("what hangs there");
                ui.end_row();
                for l in &c.labels {
                    if ui
                        .button(l.as_str())
                        .on_hover_text(format!("Move {l}: then click its new joint"))
                        .clicked()
                    {
                        chosen = Some(l.clone());
                    }
                    ui.label(crate::link_tool::hanging(&r.model, &c.structure, l));
                    ui.end_row();
                }
            });
        ui.separator();
        close |= ui
            .button("Cancel")
            .on_hover_text("Escape: nothing changes")
            .clicked();
    });
    close |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if let Some(l) = chosen {
        r.choose_point(&c.structure, &l);
    } else if close {
        r.point_chooser = None;
        r.status = Some(("Move attachment point: cancelled".into(), false));
    }
}

fn link_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut w) = r.link_window.clone() else {
        return;
    };
    let o = w.offset;
    let tool = if w.moving {
        crate::tools::Tool::MovePoint
    } else {
        crate::tools::Tool::Link
    };
    let title = format!(
        "{} {} of {} to joint {}",
        if w.moving { "Move" } else { "Link" },
        w.label,
        w.structure,
        w.joint
    );
    let type_id = r
        .model
        .line
        .structure(&w.structure)
        .and_then(|s| s.structure_type.clone())
        .unwrap_or_default();
    let mut pressed = None;
    tool_window(r, "link", title).show(ctx, |ui| {
        crate::tool_ui::intro_of(
            ui,
            &if w.ask_how {
                ohl_ui::link_question::asked(&w.label, &w.joint, o)
            } else {
                format!("Point {} goes onto joint {}.", w.label, w.joint)
            },
            "Onto the joint: the point moves there, the strings hanging from it with it (no \
             offset). Keep its position: the point stays where it is, its distance from the joint \
             kept as its offset — a rigid link whose moment the structure analysis applies at the \
             joint. Within 1 mm the point goes onto the joint without this question. This \
             structure only: its own link (a relink), one change of the project (Edit › Undo). \
             The type: its structure file, changed as OHL-Structure changes it — every structure \
             of the type follows; the file is saved into the project (Edit › Undo does not take \
             it back: move the point back the same way).",
            tool,
        );
        if w.ask_how {
            ohl_ui::link_question::choices(
                ui,
                &mut w.how,
                LinkHow::Onto,
                LinkHow::KeepPosition,
                &w.joint,
                o,
                &crate::link_tool::offset_words(o),
            );
        }
        if w.moving {
            ui.separator();
            ui.radio_value(
                &mut w.scope,
                Scope::ThisStructure,
                format!("{} only (relink)", w.structure),
            )
            .on_hover_text(
                "This placed structure's own link: one change of the project (the default)",
            );
            ui.radio_value(
                &mut w.scope,
                Scope::Type,
                format!("The type {type_id} (all its structures)"),
            )
            .on_hover_text(
                "The structure type's file, changed as OHL-Structure changes it: every structure \
                 of the type follows (a structure with its own link of the point keeps it)",
            );
        }
        if let Some(e) = &w.error {
            ohl_ui::messages::error_label(ui, e.as_str());
        }
        pressed = ohl_ui::tool_window::buttons(
            ui,
            if w.moving { "Move" } else { "Link" },
            true,
            None,
            ohl_ui::tool_window::Keys::Both,
        );
    });
    if pressed.is_none() && !ctx.text_edit_focused() {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            pressed = Some(ohl_ui::tool_window::Pressed::Action);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            pressed = Some(ohl_ui::tool_window::Pressed::Cancel);
        }
    }
    match pressed {
        Some(ohl_ui::tool_window::Pressed::Action) => {
            if let Err(e) = r.answer_link_window(&w) {
                w.error = Some(e);
                r.link_window = Some(w);
            }
        }
        Some(ohl_ui::tool_window::Pressed::Cancel) => {
            r.link_window = None;
            r.status = Some((
                format!("{}: cancelled, nothing changed", tool.name()),
                false,
            ));
        }
        None => r.link_window = Some(w),
    }
}

impl Ready {
    /// Move attachment point: point `label` of structure `id` chosen in
    /// the chooser; its joint is clicked next.
    pub fn choose_point(&mut self, id: &str, label: &str) {
        self.point_chooser = None;
        match crate::link_tool::overlay(&self.model, id, label) {
            Ok(_) => {
                self.status = Some((
                    format!(
                        "Move attachment point {label} of {id}: click a ringed joint of its type \
                         (Escape: cancel)"
                    ),
                    false,
                ));
                self.link_from = Some((id.to_owned(), label.to_owned()));
                self.selection = Selection::Point {
                    structure: id.to_owned(),
                    label: label.to_owned(),
                };
            }
            Err(e) => self.status = Some((format!("Move attachment point: {e}"), true)),
        }
    }

    /// The window's action: the point linked as chosen, this structure
    /// only or the type. Err: why not (the window stays open).
    pub fn answer_link_window(&mut self, w: &crate::link_tool::LinkWindow) -> Result<(), String> {
        let how = if w.ask_how { w.how } else { LinkHow::Onto };
        match w.scope {
            Scope::ThisStructure => {
                if !self.apply_link(&w.structure, &w.label, &w.joint, how) {
                    return Err(self
                        .status
                        .clone()
                        .map_or_else(|| "refused".to_owned(), |s| s.0));
                }
            }
            Scope::Type => self.apply_type_move(&w.structure, &w.label, &w.joint, how)?,
        }
        self.link_window = None;
        Ok(())
    }
}
