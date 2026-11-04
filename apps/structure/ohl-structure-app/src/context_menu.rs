// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The 3D view's right-click menu (owner 2026-11-03, the menu cleanup):
//! on a joint, member or point (selected with the click) what can be done
//! to it — its Inspector, a tower's joint or member in the Detail view,
//! a point on the joint, a joint inserted on the member, the point linked
//! or unlinked, deleted (asked first) —; elsewhere, or on an object of a
//! selection of several, the selection's: the Inspector, delete, clear.
//! The entries are named as the menus' and the tools' that do the same.

use eframe::egui;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::icons::Icon;
use crate::layout::Panel;
use crate::link_tool::LinkAction;
use crate::view::MenuOn;

/// An entry of the menu: true when clicked (the menu closes).
fn entry(
    ui: &mut egui::Ui,
    icon: Option<Icon>,
    label: &str,
    enabled: bool,
    tip: &str,
    why_not: &str,
) -> bool {
    let button = match icon {
        Some(i) => i.button(ui, label),
        None => egui::Button::new(label),
    };
    let r = ui
        .add_enabled(enabled, button)
        .on_hover_text(tip)
        .on_disabled_hover_text(why_not);
    if r.clicked() {
        ui.close();
        true
    } else {
        false
    }
}

impl StructureApp {
    /// The right-click menu of `on`.
    pub(crate) fn view_menu_ui(&mut self, ui: &mut egui::Ui, on: &MenuOn) {
        match on {
            MenuOn::Object(h) => self.object_menu(ui, h),
            MenuOn::Selection => self.selection_menu(ui),
        }
    }

    fn object_menu(&mut self, ui: &mut egui::Ui, h: &Handle) {
        ui.label(egui::RichText::new(h.describe()).strong());
        ui.separator();
        if entry(
            ui,
            None,
            "Inspector",
            true,
            "Its fields in the Inspector",
            "",
        ) {
            self.selected = vec![h.clone()];
            self.scroll_to = Some(h.clone());
            self.focus(Panel::Inspector);
        }
        if matches!(h, Handle::Joint(_) | Handle::Member(_)) {
            let tower = self
                .file
                .as_ref()
                .is_some_and(|f| ohl_structure_edit::is_tower(f.structure().kind));
            if entry(
                ui,
                Some(Icon::Edit),
                "Open in the Detail view",
                tower,
                "In the Detail view (a tab on top of the 3D view), its details in front on the left",
                "A tower's joint or member (Geometry › Structure kind: lattice tower or portal)",
            ) {
                self.open_in_detail(h.clone());
            }
        }
        match h {
            // Selected with the right-click: the point goes on it.
            Handle::Joint(_) => {
                if entry(
                    ui,
                    Some(Icon::PointOnJoint),
                    "Point on the joint",
                    self.picked_attachable_joint(),
                    "A point where strings hang, on this joint (it moves with it)",
                    "Not on a support joint (strings hang from attach joints)",
                ) {
                    self.point_here();
                }
            }
            Handle::Member(m) => {
                if entry(
                    ui,
                    Some(Icon::InsertJoint),
                    "Insert a joint",
                    true,
                    &format!(
                        "A joint inserted at {:.3} of its length (the Insert a joint tool's \
                         fraction): the member split there, continuous",
                        self.insert_at
                    ),
                    "",
                ) {
                    self.insert_joint(m);
                }
            }
            Handle::Point(l) => {
                if entry(
                    ui,
                    Some(Icon::Link),
                    "Link to a joint…",
                    true,
                    "The Link tool on this point: click the joint to link it to",
                    "",
                ) {
                    self.link_action(LinkAction::Start(l.clone()));
                }
                if entry(
                    ui,
                    None,
                    "Unlink",
                    true,
                    "It keeps its place, a joint of its own on no member (its load then reaches \
                     no member: link it again for the analysis)",
                    "",
                ) {
                    self.link_action(LinkAction::Unlink(l.clone()));
                }
            }
            _ => {}
        }
        ui.separator();
        if entry(
            ui,
            Some(Icon::Delete),
            "Delete…",
            true,
            "Delete it, with what goes with it (asks first; one undo step)",
            "",
        ) {
            self.selected = vec![h.clone()];
            self.ask_delete();
        }
    }

    fn selection_menu(&mut self, ui: &mut egui::Ui) {
        let n = self.selected.len();
        let what = match self.selected.as_slice() {
            [one] => one.describe(),
            _ => format!("{n} selected"),
        };
        ui.label(egui::RichText::new(what).strong());
        ui.separator();
        if entry(
            ui,
            None,
            "Inspector",
            n > 0,
            "What is selected, its fields in the Inspector",
            "Nothing is selected",
        ) {
            self.focus(Panel::Inspector);
        }
        if entry(
            ui,
            Some(Icon::Delete),
            "Delete selected…",
            n > 0,
            "Delete it all, with what goes with each (asks first; one undo step)",
            "Nothing is selected",
        ) {
            self.ask_delete();
        }
        if entry(
            ui,
            None,
            "Clear selection",
            n > 0,
            "Select nothing",
            "Nothing is selected",
        ) {
            self.selected.clear();
        }
    }
}
