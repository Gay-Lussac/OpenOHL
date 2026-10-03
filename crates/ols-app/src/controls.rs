// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
            },
            Controls {
                name: "Minecraft creative",
                keys: wasd(),
                left: Drag::Look,
                right: Drag::Pan,
                middle: Drag::Pan,
                speed: 30.0,
                sprint: 4.0,
            },
        ]
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
        out.push(("left drag".into(), self.left.name().into()));
        out.push(("right drag".into(), self.right.name().into()));
        out.push(("middle drag".into(), self.middle.name().into()));
        out.push(("scroll".into(), "zoom".into()));
        out.push(("double-click".into(), "fly to the object".into()));
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
    }
}
