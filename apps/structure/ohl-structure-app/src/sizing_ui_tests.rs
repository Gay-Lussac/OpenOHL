// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Choose the members (O49 stage 1): Design › Choose the members… opens
//! the window, the sizer runs on a thread of its own, the proposal is a
//! table, Apply takes the ticked groups as one undo step, Escape changes
//! nothing. On an invented plane truss checked by the bundled ASCE 10-97
//! plugin (the default code), its library of invented angles beside it.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::StructureApp;
use crate::tests::harness;

/// Invented equal angles, one steel.
const LIBRARY: &str = r#"format = "ohl-sections"
version = 1
name = "Sizing angles (invented)"
default_material = "s355"

[[material]]
id = "s355"
name = "Steel (invented)"
elasticity = "210 GPa"
shear_modulus = "81 GPa"
density = "7850 kg/m3"
yield = "355 MPa"
ultimate = "510 MPa"

[[section]]
id = "L40x4"
shape = "equal_angle"
h = "40 mm"
t = "4 mm"
r = "5 mm"

[[section]]
id = "L45x4"
shape = "equal_angle"
h = "45 mm"
t = "4 mm"
r = "6 mm"

[[section]]
id = "L50x5"
shape = "equal_angle"
h = "50 mm"
t = "5 mm"
r = "6 mm"

[[section]]
id = "L60x6"
shape = "equal_angle"
h = "60 mm"
t = "6 mm"
r = "7 mm"

[[section]]
id = "L70x7"
shape = "equal_angle"
h = "70 mm"
t = "7 mm"
r = "8 mm"

[[section]]
id = "L80x8"
shape = "equal_angle"
h = "80 mm"
t = "8 mm"
r = "9 mm"
"#;

/// The truss: two panels, 30 kN down at its tip (the case "model").
pub(crate) const TRUSS: &str = r#"format = "ohl-structure"
version = 21
id = "sizing-truss"
name = "Sizing truss (invented)"
kind = "lattice"

[[node]]
id = "A"
point = ["0 m", "0 m", "0 m"]
support = "pinned"

[[node]]
id = "B"
point = ["0 m", "0 m", "2 m"]
support = "pinned"

[[node]]
id = "C"
point = ["2 m", "0 m", "0 m"]
support = ["y"]

[[node]]
id = "D"
point = ["2 m", "0 m", "2 m"]
support = ["y"]

[[node]]
id = "E"
point = ["4 m", "0 m", "0 m"]
support = ["y"]

[[member]]
id = "AC"
nodes = ["A", "C"]
section = "L80x8"
group = "bottom"
design = { class = "leg", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "CE"
nodes = ["C", "E"]
section = "L80x8"
group = "bottom"
design = { class = "leg", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "BD"
nodes = ["B", "D"]
section = "L80x8"
group = "top"
design = { class = "leg", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "CD"
nodes = ["C", "D"]
section = "L80x8"
group = "web"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "BC"
nodes = ["B", "C"]
section = "L80x8"
group = "web"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "DE"
nodes = ["D", "E"]
section = "L80x8"
group = "web"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[load]]
node = "E"
force = ["0 kN", "0 kN", "-30 kN"]
"#;

/// The truss in a folder of its own with its section library beside it.
fn open(tag: &str) -> (Harness<'static, StructureApp>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ohl-structure-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::write(dir.join("Sections/angles.ohlsec"), LIBRARY).unwrap();
    let path = dir.join("sizing-truss.ohls");
    std::fs::write(&path, TRUSS).unwrap();
    (harness(&path), dir)
}

/// Runs frames until the proposal is there (at most ~60 s).
fn wait_result(h: &mut Harness<'static, StructureApp>) {
    for _ in 0..1200 {
        h.step();
        let w = h.state().sizing_window.as_ref().unwrap();
        assert!(w.error.is_none(), "{:?}", w.error);
        if w.result.is_some() {
            h.run_steps(2);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("no proposal");
}

#[test]
fn the_members_are_chosen_in_one_undo_step() {
    let (mut h, dir) = open("sizing-window");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_sizing_window();
    h.run_steps(2);
    crate::tool_window_tests::window_shot(&mut h, "Choose the members", "member-sizing-setup");
    h.get_all_by_label("Choose the members")
        .last()
        .unwrap()
        .click();
    h.run_steps(2);
    wait_result(&mut h);
    let changed = {
        let w = h.state().sizing_window.as_ref().unwrap();
        let s = &w.result.as_ref().unwrap().sizing;
        assert!(s.over_target.is_empty(), "{:?}", s.over_target);
        // The bundled code plugin: no simple-check warning, its limits
        // checked.
        assert!(
            !s.notes
                .iter()
                .any(|n| n.starts_with("NOT ON THE SAFE SIDE")),
            "{:?}",
            s.notes
        );
        assert!(s.mass[1] < s.mass[0]);
        s.groups.iter().filter(|g| g.changed()).count()
    };
    assert!(changed > 0);
    crate::tool_window_tests::window_shot(&mut h, "Choose the members", "member-sizing");
    // Nothing written before Apply.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.state().sizing_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert_ne!(f.text(), before);
    assert!(f.undo_name().unwrap().starts_with("Choose the members"));
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn escape_closes_the_sizing_window_and_changes_nothing() {
    let (mut h, dir) = open("sizing-escape");
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_sizing_window();
    h.step();
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().sizing_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(dir).ok();
}
