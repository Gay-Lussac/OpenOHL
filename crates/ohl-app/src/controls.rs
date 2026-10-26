// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Navigation controls of the 3D view as data: which keys move the camera
//! and what each mouse drag does. Schemes are presets ([`Controls::presets`]);
//! schemes matching other software can be added the same way, and later
//! edited and saved.

use eframe::egui;

/// A camera movement held down with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Forward along the ground.
    Forward,
    /// Backward along the ground.
    Back,
    /// To the left.
    Left,
    /// To the right.
    Right,
    /// Straight up.
    Up,
    /// Straight down.
    Down,
    /// Faster while held.
    Sprint,
}

impl Move {
    /// Name in the controls list.
    pub fn name(self) -> &'static str {
        match self {
            Move::Forward => "forward",
            Move::Back => "back",
            Move::Left => "left",
            Move::Right => "right",
            Move::Up => "up",
            Move::Down => "down",
            Move::Sprint => "sprint",
        }
    }
}

/// A key or modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// A key.
    Key(egui::Key),
    /// Shift.
    Shift,
    /// Ctrl (⌃).
    Ctrl,
}

impl Input {
    /// Whether it is held down.
    pub fn down(self, i: &egui::InputState) -> bool {
        match self {
            Input::Key(k) => i.key_down(k),
            Input::Shift => i.modifiers.shift,
            Input::Ctrl => i.modifiers.ctrl,
        }
    }

    /// Name in the controls list.
    pub fn name(self) -> String {
        match self {
            Input::Key(k) => k.name().to_owned(),
            Input::Shift => "Shift".into(),
            Input::Ctrl => "Ctrl".into(),
        }
    }
}

/// What a mouse drag does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    /// Turn around the point looked at.
    Orbit,
    /// Turn the view around the eye (first person).
    Look,
    /// Move in the screen plane.
    Pan,
    /// Nothing.
    None,
}

impl Drag {
    fn name(self) -> &'static str {
        match self {
            Drag::Orbit => "turn around the target",
            Drag::Look => "look around",
            Drag::Pan => "pan",
            Drag::None => "—",
        }
    }
}

/// A navigation scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct Controls {
    /// Name of the scheme.
    pub name: &'static str,
    /// Keys held to move.
    pub keys: Vec<(Move, Input)>,
    /// Left drag.
    pub left: Drag,
    /// Right drag.
    pub right: Drag,
    /// Middle drag.
    pub middle: Drag,
    /// Moving speed, m/s.
    pub speed: f64,
    /// Speed factor while sprinting.
    pub sprint: f64,
    /// Clicking the 3D view captures the mouse (first person, crosshair,
    /// Escape frees it).
    pub capture: bool,
    /// Mouse movement while captured, radians per pixel.
    pub mouse_sensitivity: f64,
}

/// The mouse buttons whose clicks select and use the tool held in a view
/// (owner: OHL-Design UI review, 2026-10-26, as in OHL-Structure): a
/// left-click acts with the tool held — with Select held it selects —
/// and a right-click selects what is under the pointer and opens its
/// menu (Properties, Edit…, Delete, Zoom to, the tools that apply). As
/// (select, use the tool): Select held, the left button selects and the
/// right one "uses" Select (selects too, then the menu); another tool,
/// the right button selects (then the menu) and the left one uses it.
/// Drags are not clicks (a press and release without moving).
pub fn click_buttons(select_held: bool) -> (egui::PointerButton, egui::PointerButton) {
    use egui::PointerButton::{Primary, Secondary};
    if select_held {
        (Primary, Secondary)
    } else {
        (Secondary, Primary)
    }
}

/// The click rows of the controls list: what a left-click and a
/// right-click do.
pub fn click_rows() -> Vec<(String, String)> {
    vec![
        (
            "left-click".into(),
            "use the tool held (Select held: select)".into(),
        ),
        (
            "right-click".into(),
            "the object's menu: Properties, Edit…, Delete, Zoom to, its tools".into(),
        ),
        ("Esc".into(), "back to Select".into()),
    ]
}

/// W A S D, Space up, Shift down, Ctrl sprint (Minecraft creative).
fn wasd() -> Vec<(Move, Input)> {
    vec![
        (Move::Forward, Input::Key(egui::Key::W)),
        (Move::Left, Input::Key(egui::Key::A)),
        (Move::Back, Input::Key(egui::Key::S)),
        (Move::Right, Input::Key(egui::Key::D)),
        (Move::Up, Input::Key(egui::Key::Space)),
        (Move::Down, Input::Shift),
        (Move::Sprint, Input::Ctrl),
    ]
}

impl Default for Controls {
    fn default() -> Self {
        Self::presets().remove(0)
    }
}

impl Controls {
    /// The schemes to choose from.
    pub fn presets() -> Vec<Controls> {
        vec![
            Controls {
                name: "Orbit",
                keys: wasd(),
                left: Drag::Orbit,
                right: Drag::Pan,
                middle: Drag::Pan,
                speed: 30.0,
                sprint: 4.0,
                capture: false,
                mouse_sensitivity: 0.003,
            },
            Controls {
                name: "Minecraft creative",
                keys: wasd(),
                left: Drag::Look,
                right: Drag::Pan,
                middle: Drag::Pan,
                speed: 30.0,
                sprint: 4.0,
                capture: true,
                mouse_sensitivity: 0.003,
            },
        ]
    }

    /// The preset whose name starts with `name` (case-insensitive).
    pub fn by_name(name: &str) -> Option<Controls> {
        let wanted = name.trim().to_lowercase();
        Self::presets()
            .into_iter()
            .find(|c| !wanted.is_empty() && c.name.to_lowercase().starts_with(&wanted))
    }

    /// Saves the scheme, speed and sensitivity.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string("controls", self.name.to_owned());
        storage.set_string("controls.speed", self.speed.to_string());
        storage.set_string("controls.sensitivity", self.mouse_sensitivity.to_string());
    }

    /// The scheme saved by [`Controls::save`], if any.
    pub fn load(storage: &dyn eframe::Storage) -> Option<Controls> {
        let mut c = Self::by_name(&storage.get_string("controls")?)?;
        let num = |key: &str| storage.get_string(key).and_then(|v| v.parse::<f64>().ok());
        if let Some(v) = num("controls.speed").filter(|v| v.is_finite() && *v > 0.0) {
            c.speed = v;
        }
        if let Some(v) = num("controls.sensitivity").filter(|v| v.is_finite() && *v > 0.0) {
            c.mouse_sensitivity = v;
        }
        Some(c)
    }

    /// The drag action for a pointer button.
    pub fn drag(&self, button: egui::PointerButton) -> Drag {
        match button {
            egui::PointerButton::Primary => self.left,
            egui::PointerButton::Secondary => self.right,
            egui::PointerButton::Middle => self.middle,
            _ => Drag::None,
        }
    }

    /// The movement held down this frame: (forward, right, up) in units of
    /// the speed, and whether sprinting.
    pub fn held(&self, i: &egui::InputState) -> ([f64; 3], bool) {
        let mut v = [0.0; 3];
        let mut sprint = false;
        for (m, input) in &self.keys {
            if !input.down(i) {
                continue;
            }
            match m {
                Move::Forward => v[0] += 1.0,
                Move::Back => v[0] -= 1.0,
                Move::Right => v[1] += 1.0,
                Move::Left => v[1] -= 1.0,
                Move::Up => v[2] += 1.0,
                Move::Down => v[2] -= 1.0,
                Move::Sprint => sprint = true,
            }
        }
        (v, sprint)
    }

    /// The scheme as text lines, for the controls list.
    pub fn describe(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .keys
            .iter()
            .map(|(m, i)| (i.name(), m.name().to_owned()))
            .collect();
        if self.capture {
            out.push(("left-click 3D view".into(), "capture the mouse".into()));
            out.push(("mouse (captured)".into(), "look around".into()));
            out.push((
                "left-click (captured)".into(),
                "use the tool held at the crosshair (Select held: select)".into(),
            ));
            out.push((
                "right-click (captured)".into(),
                "select at the crosshair".into(),
            ));
            out.push(("Ctrl + wheel (captured)".into(), "speed".into()));
            out.push(("Esc".into(), "free the mouse".into()));
        }
        if !self.capture {
            out.extend(click_rows());
        }
        out.push(("left drag".into(), self.left.name().into()));
        out.push(("right drag".into(), self.right.name().into()));
        out.push(("middle drag".into(), self.middle.name().into()));
        // Box and lasso selection of LiDAR points (the selecting button,
        // [`crate::area`]).
        out.push((
            "Alt + left drag (Select held)".into(),
            "box: the LiDAR points drawn in it, their class changed together (Edit window)".into(),
        ));
        out.push((
            "Alt + Shift + left drag (Select held)".into(),
            "lasso: the same in a free shape (Esc: give up)".into(),
        ));
        out.push(("scroll".into(), "zoom".into()));
        out.push((
            "double-click (Select held)".into(),
            "fly to the object".into(),
        ));
        // The drag handles of the structure being edited (its structure
        // screen open: [`crate::handles`]).
        out.push((
            "drag a handle (Move held)".into(),
            "move a point of the structure being edited".into(),
        ));
        out.push((
            "X / Y / Z (dragging)".into(),
            "hold it on that axis of the structure (again: free)".into(),
        ));
        out.push((
            "Shift + X / Y / Z (dragging)".into(),
            "hold it in the plane across that axis".into(),
        ));
        out.push(("Alt (dragging)".into(), "no snapping".into()));
        out.push(("Esc (dragging)".into(), "cancel the drag".into()));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_add_up() {
        let c = Controls::default();
        let mut i = egui::InputState::default();
        i.keys_down.insert(egui::Key::W);
        i.keys_down.insert(egui::Key::D);
        i.modifiers.shift = true;
        assert_eq!(c.held(&i), ([1.0, 1.0, -1.0], false));
        i.keys_down.insert(egui::Key::S);
        i.modifiers.ctrl = true;
        assert_eq!(c.held(&i), ([0.0, 1.0, -1.0], true));
        assert_eq!(c.drag(egui::PointerButton::Primary), Drag::Orbit);
        let mc = &Controls::presets()[1];
        assert_eq!(mc.drag(egui::PointerButton::Primary), Drag::Look);
        assert!(mc.describe().iter().any(|(k, v)| k == "Space" && v == "up"));
        // A left-click acts with the tool (Select: selects), a right-click
        // selects and opens the menu.
        use egui::PointerButton::{Primary, Secondary};
        assert_eq!(click_buttons(true), (Primary, Secondary));
        assert_eq!(click_buttons(false), (Secondary, Primary));
    }
}
