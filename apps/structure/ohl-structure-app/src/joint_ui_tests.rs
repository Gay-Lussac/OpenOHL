// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Stages 0 and 2 of the joint details driven as a user would
//! (egui_kittest): the structure's kind toggled (Geometry › Structure
//! kind), one undo step, nothing deleted; Design › Joint details on the
//! joint selected, its close-up and face view, its connections table; the
//! joints coloured in the 3D view.

use egui_kittest::kittest::{NodeT, Queryable};
use ohl_structure_edit::Handle;
use ohl_structures::LibraryItem;
use ohl_structures::structures::StructureKind;

use crate::layout::Panel;
use crate::tests::{copy_of, harness, menu};

/// Geometry, then its Structure kind submenu (opened by hovering).
fn open_kind_menu(h: &mut egui_kittest::Harness<'static, crate::app::StructureApp>) {
    h.get_by_label("Geometry").click();
    h.run();
    // The menu's entry (the Joint details panel also names it).
    h.get_all_by_label_contains("Structure kind")
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Structure kind"))
        })
        .expect("the submenu")
        .hover();
    h.run();
    h.run();
}

fn kind(h: &egui_kittest::Harness<'static, crate::app::StructureApp>) -> StructureKind {
    h.state().file.as_ref().unwrap().structure().kind
}

#[test]
fn the_kind_is_toggled_and_a_joint_shown_in_detail() {
    // The example tower is of kind "other": no joint details yet. A
    // connection with bolts given at its fifth joint.
    let path = copy_of("type-02.ohls", "joint-ui");
    let text = std::fs::read_to_string(&path).unwrap();
    let t = ohl_structures::StructureType::parse(&path, &text).unwrap();
    let joint = t.nodes[4].id.clone();
    let member = t
        .members
        .iter()
        .find(|m| m.nodes.contains(&joint))
        .unwrap()
        .id
        .clone();
    std::fs::write(
        &path,
        format!(
            "{text}\n[[connection]]\nmember = \"{member}\"\njoint = \"{joint}\"\nleg = \"long\"\n\
             bolts = {{ count = 3, diameter = \"16 mm\", grade = \"8.8\" }}\n\
             gauges = [\"30 mm\"]\ngauge_lines = 1\npitch = \"55 mm\"\nend_distance = \"30 mm\"\n"
        ),
    )
    .unwrap();
    let shots = ohl_core::env::var_os("OHL_JOINT_SCREENSHOTS").map(std::path::PathBuf::from);
    let mut h = harness(&path);
    assert_eq!(kind(&h), StructureKind::Other);
    menu(&mut h, "Design", "Joint details…");
    assert_eq!(h.state_mut().focused(), Some(Panel::JointDetails));
    // Its drawing in a pane of its own beside the 3D view.
    assert!(h.state().is_open(Panel::JointView));
    h.get_by_label_contains("Joint details are a tower's");
    // Geometry › Structure kind › lattice tower: one undo step.
    open_kind_menu(&mut h);
    h.get_by_label("lattice tower").click();
    h.run();
    assert_eq!(kind(&h), StructureKind::Lattice);
    assert!(
        h.state()
            .message
            .as_ref()
            .is_some_and(|m| m.0.contains("structure kind: lattice"))
    );
    h.key_press(eframe::egui::Key::Escape);
    h.run();
    // A joint selected: shown in detail.
    h.state_mut().selected = vec![Handle::Joint(joint.clone())];
    h.run();
    h.run();
    assert_eq!(h.state().joints.joint.as_deref(), Some(joint.as_str()));
    h.get_by_label("Close-up");
    h.get_by_label("Bolts");
    h.get_by_label("Restrained");
    h.get_by_label_contains("member end(s)");
    h.get_by_label("3 × M16 8.8");
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("joint-close-up.png"))
            .unwrap();
    }
    // The face view, saved as SVG or PDF.
    h.get_by_label("Face view").click();
    h.run();
    h.get_by_label("Save as SVG…");
    h.get_by_label("Save as PDF…");
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("joint-face.png"))
            .unwrap();
    }
    // The joints coloured in the 3D view (a tower's): one per joint.
    h.state_mut().joint_colors = true;
    let n = h.state().file.as_ref().unwrap().structure().nodes.len();
    assert_eq!(h.state_mut().joint_color_list().len(), n);
    h.run();
    // Back to "other": undone in one step; the colours gone.
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(kind(&h), StructureKind::Other);
    assert!(h.state_mut().joint_color_list().is_empty());
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A pole: the panel says joint details are a tower's; a tower turned
/// into a pole keeps its connections in the file.
#[test]
fn a_pole_keeps_the_connections_it_does_not_show() {
    let path = copy_of("type-02.ohls", "joint-pole");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("kind = \"other\"", "kind = \"lattice\"");
    let first = text
        .lines()
        .skip_while(|l| !l.starts_with("[[member]]"))
        .find_map(|l| {
            l.strip_prefix("id = \"")
                .map(|x| x.trim_end_matches('"').to_owned())
        })
        .unwrap();
    let nodes_line = text
        .lines()
        .skip_while(|l| !l.starts_with("[[member]]"))
        .find(|l| l.starts_with("nodes = "))
        .unwrap()
        .to_owned();
    let joint = nodes_line.split('"').nth(1).unwrap().to_owned();
    std::fs::write(
        &path,
        format!(
            "{text}\n[[connection]]\nmember = \"{first}\"\njoint = \"{joint}\"\nkind = \"pinned\"\n"
        ),
    )
    .unwrap();
    let mut h = harness(&path);
    assert_eq!(kind(&h), StructureKind::Lattice);
    open_kind_menu(&mut h);
    h.get_by_label("pole").click();
    h.run();
    assert_eq!(kind(&h), StructureKind::Pole);
    let m = h.state().message.clone().unwrap().0;
    assert!(m.contains("1 connection(s) and 0 plate(s) kept"), "{m}");
    assert_eq!(
        h.state()
            .file
            .as_ref()
            .unwrap()
            .structure()
            .connections
            .len(),
        1
    );
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A tower of library angles (`Sections/` next to it): the joint's
/// members as angles, the diagonal's three bolts drawn on its long leg.
#[test]
fn an_angle_tower_shows_its_bolts() {
    let path = copy_of("type-02.ohls", "joint-angles");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, ANGLES).unwrap();
    let shots = ohl_core::env::var_os("OHL_JOINT_SCREENSHOTS").map(std::path::PathBuf::from);
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details…");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run();
    h.run();
    let d = h.state_mut().joint_details().unwrap();
    let d = d.as_ref().as_ref().unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let a = d
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    assert!(a.profile.as_ref().unwrap().angle.is_some());
    assert_eq!(a.bolt_points.len(), 3);
    h.get_by_label("3 × M16 8.8");
    if let Some(dir) = &shots {
        h.state_mut().joints.yaw = 0.6;
        h.state_mut().joints.zoom = 0.9;
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("angles-close-up.png"))
            .unwrap();
        h.get_by_label("Face view").click();
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("angles-face.png"))
            .unwrap();
        h.get_by_label("Close-up").click();
        h.run();
    }
    // Stage 3: H1's bolts (the member's) clicked: the bolt layout, its
    // bolts applied at this end — one undo step.
    h.get_by_label("2 × M16").click();
    h.run();
    h.get_by_label_contains("Bolt layout — H1 at N2");
    if let Some(dir) = &shots {
        h.render()
            .unwrap()
            .save(dir.join("angles-layout.png"))
            .unwrap();
    }
    h.get_by_label("Apply").click();
    h.run();
    let c = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>| {
        h.state()
            .file
            .as_ref()
            .unwrap()
            .structure()
            .connection("H1", "N2")
            .cloned()
    };
    let given = c(&h).expect("written");
    assert_eq!(given.bolts.as_ref().unwrap().count, 2);
    assert_eq!(given.gauge_lines, Some(1));
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("angles-defined.png"))
            .unwrap();
    }
    h.get_by_label("Undo").click();
    h.run();
    assert!(c(&h).is_none());
    // A plate at the joint: one undo step, listed.
    h.get_by_label("+ Plate").click();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().structure().plates.len(), 1);
    h.get_by_label("Members onto it");
    std::fs::remove_dir_all(dir).ok();
}

/// Two legs in the face y = 1 m, a diagonal and a horizontal to the
/// joint between them; the diagonal's end there given in full.
const ANGLES: &str = r#"id = "A"
name = "angles"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "1 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "1 m", "5 m"]

[[node]]
id = "N3"
point = ["0 m", "1 m", "10 m"]

[[node]]
id = "N4"
point = ["4 m", "1 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["4 m", "1 m", "5 m"]

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "UA-83x52x6"
orientation = "y-z+"
design = { class = "bracing" }

[[member]]
id = "H1"
nodes = ["N2", "N5"]
section = "UA-83x52x6"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "R1"
nodes = ["N4", "N5"]
section = "UA-83x52x6"
orientation = "x-y-"

[[connection]]
member = "D1"
joint = "N2"
onto = "L1"
leg = "long"
bolts = { count = 3, diameter = "16 mm", grade = "8.8" }
gauge_lines = 1
gauges = ["30 mm"]
pitch = "55 mm"
end_distance = "30 mm"
edge_distance = "25 mm"
"#;

/// The Joint view follows the joint selected even when the Joint details
/// panel is behind another tab (owner 2026-11-02: "clicking on them but
/// still have only one showing").
#[test]
fn the_joint_view_follows_the_selection_with_the_panel_hidden() {
    let path = copy_of("type-02.ohls", "joint-follow");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("kind = \"other\"", "kind = \"lattice\"");
    std::fs::write(&path, text).unwrap();
    let mut h = harness(&path);
    menu(&mut h, "Design", "Joint details…");
    assert!(h.state().is_open(Panel::JointView));
    // Another tab in front of Joint details.
    menu(&mut h, "Body", "Joints…");
    let nodes: Vec<String> = h
        .state()
        .file
        .as_ref()
        .unwrap()
        .structure()
        .nodes
        .iter()
        .map(|n| n.id.clone())
        .collect();
    for id in [&nodes[3], &nodes[7]] {
        h.state_mut().selected = vec![Handle::Joint(id.clone())];
        h.run();
        assert_eq!(h.state().joints.joint.as_deref(), Some(id.as_str()));
    }
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The Edit tool (owner 2026-11-02: "an edit tool … a click on members
/// or joints and open the necessary window to edit them"): a member
/// clicked is selected, the Inspector in front with its fields; a
/// tower's joint also opens Joint details and the Joint view on it.
#[test]
fn the_edit_tool_opens_where_it_is_edited() {
    use crate::tools::Tool;
    let path = copy_of("type-02.ohls", "edit-tool");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("kind = \"other\"", "kind = \"lattice\"");
    std::fs::write(&path, text).unwrap();
    let mut h = harness(&path);
    h.state_mut().hold_tool(Tool::Edit);
    assert_eq!(h.state().tool(), Tool::Edit);
    h.state_mut()
        .tool_click(Some(Handle::Member("m155".into())), None);
    h.run();
    assert_eq!(h.state().selected, vec![Handle::Member("m155".into())]);
    assert_eq!(h.state_mut().focused(), Some(Panel::Inspector));
    h.get_by_label_contains("Member m155");
    assert!(!h.state().is_open(Panel::JointView));
    h.state_mut()
        .tool_click(Some(Handle::Joint("L9A".into())), None);
    h.run();
    assert!(h.state().is_open(Panel::JointDetails) && h.state().is_open(Panel::JointView));
    assert_eq!(h.state().joints.joint.as_deref(), Some("L9A"));
    assert_eq!(h.state_mut().focused(), Some(Panel::Inspector));
    // The Inspector's heading and the Joint view's.
    assert_eq!(h.get_all_by_label("Joint L9A").count(), 2);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
