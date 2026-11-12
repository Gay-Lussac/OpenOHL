// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The mouse part of the navigation schemes both applications offer
//! (owner, 2026-11-12: "a way to control the view similarly as in [a
//! common CAD program] so that people used to it can also switch the
//! controls to them"): what a drag of each button does (Shift + middle
//! apart), whether turning and zooming go about the cursor, whether a
//! double-click of the middle button fits the view. OHL-Design adds its
//! keys and first person ([`Mouse`] inside its `Controls`); OHL-Structure
//! offers the schemes of [`Scheme`].
//!
//! The scheme in force is held in the [`egui::Context`]'s memory (as the
//! snap marker's settings): the 2D views and the Detail view read it there
//! each frame; each application keeps the choice under its own key.
//!
//! The hotbar, its keys 1–9, E (the inventory) and Escape are the same in
//! every scheme; the wheel never steps the hotbar outside first person.

use eframe::egui;

/// What a mouse drag does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    /// Turn around the point looked at (or the cursor's).
    Orbit,
    /// Turn the view around the eye (first person).
    Look,
    /// Move in the screen plane.
    Pan,
    /// Nothing.
    None,
}

impl Drag {
    /// Its name in a controls list.
    pub fn name(self) -> &'static str {
        match self {
            Drag::Orbit => "turn around the target",
            Drag::Look => "look around",
            Drag::Pan => "pan",
            Drag::None => "—",
        }
    }
}

/// What the mouse does in the views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    /// Left drag.
    pub left: Drag,
    /// Right drag.
    pub right: Drag,
    /// Middle drag.
    pub middle: Drag,
    /// Shift + middle drag (none: as the middle drag).
    pub shift_middle: Option<Drag>,
    /// Turning goes about the point under the cursor (else the target).
    pub orbit_about_cursor: bool,
    /// The wheel zooms towards the cursor (else towards the target).
    pub zoom_to_cursor: bool,
    /// A double-click of the middle button shows the whole extent.
    pub middle_double_fits: bool,
}

impl Mouse {
    /// The "Orbit" scheme: left drag turns, right and middle pan, the
    /// wheel zooms (OHL-Structure's and OHL-Design's default).
    pub const ORBIT: Mouse = Mouse {
        left: Drag::Orbit,
        right: Drag::Pan,
        middle: Drag::Pan,
        shift_middle: None,
        orbit_about_cursor: false,
        zoom_to_cursor: false,
        middle_double_fits: false,
    };

    /// The "CAD" scheme: middle drag pans, Shift + middle drag turns about
    /// the point under the cursor, the wheel zooms towards the cursor, a
    /// double-click of the middle button fits; left and right drags leave
    /// the view (left stays for clicks and tools, right for the menu).
    pub const CAD: Mouse = Mouse {
        left: Drag::None,
        right: Drag::None,
        middle: Drag::Pan,
        shift_middle: Some(Drag::Orbit),
        orbit_about_cursor: true,
        zoom_to_cursor: true,
        middle_double_fits: true,
    };

    /// What a drag of `button` does with `modifiers` held.
    pub fn drag(&self, button: egui::PointerButton, modifiers: egui::Modifiers) -> Drag {
        match button {
            egui::PointerButton::Primary => self.left,
            egui::PointerButton::Secondary => self.right,
            egui::PointerButton::Middle => match self.shift_middle {
                Some(d) if modifiers.shift => d,
                _ => self.middle,
            },
            _ => Drag::None,
        }
    }

    /// In a 2D view (no turning): whether a drag of `button` pans — any
    /// drag the scheme gives an action to.
    pub fn pans_2d(&self, button: egui::PointerButton, modifiers: egui::Modifiers) -> bool {
        self.drag(button, modifiers) != Drag::None
    }

    /// The scheme in force (Orbit before any was set).
    pub fn get(ctx: &egui::Context) -> Mouse {
        ctx.data(|d| d.get_temp::<Mouse>(id()))
            .unwrap_or(Mouse::ORBIT)
    }

    /// Puts `self` in force for the views that read it.
    pub fn set(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(id(), self));
    }

    /// The 2D views' pan this frame, points: the drag of `response` by a
    /// button that pans (none: no drag that pans).
    pub fn pan_2d(&self, ui: &egui::Ui, response: &egui::Response) -> Option<egui::Vec2> {
        if !response.dragged() {
            return None;
        }
        let modifiers = ui.input(|i| i.modifiers);
        [
            egui::PointerButton::Primary,
            egui::PointerButton::Secondary,
            egui::PointerButton::Middle,
        ]
        .into_iter()
        .any(|b| response.dragged_by(b) && self.pans_2d(b, modifiers))
        .then(|| response.drag_delta())
    }

    /// Whether the middle button was double-clicked on `response` and the
    /// scheme fits the view on it.
    pub fn fit_asked(&self, response: &egui::Response) -> bool {
        self.middle_double_fits
            && response.hovered()
            && response
                .ctx
                .input(|i| i.pointer.button_double_clicked(egui::PointerButton::Middle))
    }
}

fn id() -> egui::Id {
    egui::Id::new("ohl-ui-navigation-mouse")
}

/// The schemes OHL-Structure offers (OHL-Design's add keys and first
/// person in its own `Controls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    /// Left drag turns, right drag pans, the wheel zooms (the default).
    #[default]
    Orbit,
    /// Middle drag pans, Shift + middle turns, double middle fits.
    Cad,
}

impl Scheme {
    /// Every scheme, in the menu's order.
    pub const ALL: [Scheme; 2] = [Scheme::Orbit, Scheme::Cad];

    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Scheme::Orbit => "Orbit",
            Scheme::Cad => "CAD",
        }
    }

    /// What it does, in a sentence (its hover text).
    pub fn help(self) -> &'static str {
        match self {
            Scheme::Orbit => {
                "Left drag turns the view, right or middle drag pans, the wheel zooms \
                 (as before)"
            }
            Scheme::Cad => {
                "As in common CAD programs: middle drag pans, Shift + middle drag turns about \
                 the point under the cursor, the wheel zooms towards the cursor, a double-click \
                 of the middle button shows everything; left drag leaves the view (left is for \
                 clicks and tools), right-click opens the menu. The hotbar stays: click a slot, \
                 keys 1-9 or E (the inventory) to change the tool"
            }
        }
    }

    /// Its mouse.
    pub fn mouse(self) -> Mouse {
        match self {
            Scheme::Orbit => Mouse::ORBIT,
            Scheme::Cad => Mouse::CAD,
        }
    }

    /// The scheme kept in `storage` under `key` (none or unknown: Orbit).
    pub fn load(storage: Option<&dyn eframe::Storage>, key: &str) -> Scheme {
        let name = storage.and_then(|s| s.get_string(key)).unwrap_or_default();
        Scheme::ALL
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(name.trim()))
            .unwrap_or_default()
    }

    /// Keeps the scheme in `storage` under `key`.
    pub fn save(self, storage: &mut dyn eframe::Storage, key: &str) {
        storage.set_string(key, self.name().to_owned());
    }

    /// The radio buttons to choose one, each explaining its effect;
    /// changed: true.
    pub fn menu(ui: &mut egui::Ui, scheme: &mut Scheme) -> bool {
        let mut changed = false;
        for s in Scheme::ALL {
            changed |= ui
                .radio_value(scheme, s, s.name())
                .on_hover_text(s.help())
                .changed();
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cad_pans_with_the_middle_button_and_turns_with_shift() {
        use egui::PointerButton::{Middle, Primary, Secondary};
        let none = egui::Modifiers::NONE;
        let shift = egui::Modifiers::SHIFT;
        let c = Mouse::CAD;
        assert_eq!(c.drag(Middle, none), Drag::Pan);
        assert_eq!(c.drag(Middle, shift), Drag::Orbit);
        assert_eq!(c.drag(Primary, none), Drag::None);
        assert_eq!(c.drag(Secondary, none), Drag::None);
        assert!(c.pans_2d(Middle, shift) && !c.pans_2d(Primary, none));
        let o = Mouse::ORBIT;
        assert_eq!(o.drag(Middle, shift), Drag::Pan);
        assert_eq!(o.drag(Primary, none), Drag::Orbit);
        assert!(o.pans_2d(Primary, none));
        let ctx = egui::Context::default();
        assert_eq!(Mouse::get(&ctx), Mouse::ORBIT);
        Mouse::CAD.set(&ctx);
        assert_eq!(Mouse::get(&ctx), Mouse::CAD);
        assert_eq!(Scheme::load(None, "k"), Scheme::Orbit);
    }
}
