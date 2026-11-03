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
    // A tower's member: also Member details and the Detail view.
    assert!(h.state().is_open(Panel::MemberDetails) && h.state().is_open(Panel::JointView));
    assert!(h.get_all_by_label_contains("Member m155").count() >= 2);
    h.state_mut()
        .tool_click(Some(Handle::Joint("L9A".into())), None);
    h.run();
    assert!(h.state().is_open(Panel::JointDetails) && h.state().is_open(Panel::JointView));
    assert_eq!(h.state().joints.joint.as_deref(), Some("L9A"));
    assert_eq!(h.state_mut().focused(), Some(Panel::Inspector));
    // The Inspector's heading and the Detail view's.
    assert_eq!(h.get_all_by_label("Joint L9A").count(), 2);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// Member details (owner 2026-11-02: "the same view of the members as for
/// the joints … see live the impacts on the bar's resistance"): the Edit
/// tool on a member opens it and the Detail view on the member laid flat;
/// its two ends are the joints' rows; its capacities follow a change of
/// its bolts at once.
#[test]
fn member_details_follow_the_bolts_live() {
    use crate::member_panel::Subject;
    use crate::tools::Tool;
    let path = copy_of("type-02.ohls", "member-details");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("Bolts")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/bolts/example-bolts.ohlb"),
        dir.join("Bolts/example-bolts.ohlb"),
    )
    .unwrap();
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, ANGLES).unwrap();
    let shots = ohl_core::env::var_os("OHL_JOINT_SCREENSHOTS").map(std::path::PathBuf::from);
    let mut h = harness(&tower);
    h.state_mut().hold_tool(Tool::Edit);
    h.state_mut()
        .tool_click(Some(Handle::Member("D1".into())), None);
    h.run();
    h.run();
    assert!(h.state().is_open(Panel::MemberDetails) && h.state().is_open(Panel::JointView));
    assert_eq!(h.state().joints.subject, Subject::Member);
    assert_eq!(h.state().joints.member.as_deref(), Some("D1"));
    menu(&mut h, "Design", "Member details…");
    h.run();
    h.get_by_label_contains("Resistance (ASCE 10-97");
    h.get_by_label("Tension");
    // The Inspector's heading and the Detail view's.
    assert_eq!(h.get_all_by_label("Member D1").count(), 2);
    let tension = |h: &mut egui_kittest::Harness<'static, crate::app::StructureApp>| {
        h.state_mut()
            .member_capacity("D1")
            .as_ref()
            .as_ref()
            .unwrap()
            .tension
            .clone()
            .unwrap()
            .0
    };
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("member-details.png"))
            .unwrap();
    }
    // D1's end at N4 given one library bolt, then three: each one undo
    // step, the capacity worked out again at once (the bolt's shear
    // governing with one).
    let bolts_at_n4 = |h: &mut egui_kittest::Harness<'static, crate::app::StructureApp>, n: u32| {
        h.state_mut()
            .file
            .as_mut()
            .unwrap()
            .edit("bolts", |doc, _| {
                ohl_structure_edit::connection_edits::set_bolt_layout(
                    doc,
                    "D1",
                    "N4",
                    &ohl_structure_edit::connection_edits::BoltLayout {
                        count: n,
                        bolt: Some("X17-example".into()),
                        diameter: 0.017,
                        hole: None,
                        grade: None,
                        gauges: vec![0.03],
                        pitch: 0.055,
                        end: 0.03,
                        edge: None,
                    },
                )
            });
        h.run();
    };
    bolts_at_n4(&mut h, 1);
    let one = tension(&mut h);
    bolts_at_n4(&mut h, 3);
    let three = tension(&mut h);
    assert!(three > one, "{one} {three}");
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("member-details-3-bolts.png"))
            .unwrap();
    }
    // A joint selected: the Detail view shows the joint again.
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run();
    assert_eq!(h.state().joints.subject, Subject::Joint);
    std::fs::remove_dir_all(dir).ok();
}

/// The Detail view's tools (owner 2026-11-02): the strip shows in the
/// face view and the member view; a bolt placed by hand, a member end's
/// cut and shift, a plate's outline written — each one undo step.
#[test]
fn the_detail_view_tools_act() {
    use crate::joint_panel::Drawn;
    use crate::joint_tools::Act;
    let path = copy_of("type-02.ohls", "detail-tools");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("angles.ohls");
    std::fs::write(
        &tower,
        // D1 and H1 bolted onto a gusset plate at N2 (its outline drawn
        // around their bolts).
        format!(
            "{}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
             [[connection]]\nmember = \"H1\"\njoint = \"N2\"\nonto = \"G1\"\n\
             bolts = {{ count = 2, diameter = \"16 mm\" }}\ngauges = [\"30 mm\"]\n\
             gauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n",
            ANGLES.replace("onto = \"L1\"", "onto = \"G1\"")
        ),
    )
    .unwrap();
    let shots = ohl_core::env::var_os("OHL_JOINT_SCREENSHOTS").map(std::path::PathBuf::from);
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details…");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.state_mut().joints.drawn = Drawn::Face;
    h.run();
    h.run();
    h.get_by_label("Add bolt");
    h.get_by_label("Member end");
    let details = h.state_mut().joint_details().unwrap();
    let details = details.as_ref().as_ref().unwrap().clone();
    let file = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>| {
        h.state().file.as_ref().unwrap().structure().clone()
    };
    // A bolt placed by hand on H1's end at N2.
    let kind = ohl_structure_edit::connection_edits::FieldValue::Bolts {
        count: 1,
        bolt: None,
        diameter: Some(0.016),
        hole: None,
        grade: None,
    };
    h.state_mut().apply_joint_acts(
        vec![Act::BoltPoints(
            "H1".into(),
            "N2".into(),
            vec![[0.06, 0.03]],
            Some(kind),
        )],
        &details,
    );
    h.run();
    assert_eq!(
        file(&h).connection("H1", "N2").unwrap().bolt_points.len(),
        1
    );
    // D1 stopping 60 mm from the work point, shifted 15 mm.
    h.state_mut().apply_joint_acts(
        vec![Act::CutShift("D1".into(), "N2".into(), 0.06, 0.015)],
        &details,
    );
    h.run();
    let c = file(&h).connection("D1", "N2").unwrap().clone();
    assert!((c.cut.unwrap().si() - 0.06).abs() < 1e-12);
    assert!((c.shift.unwrap().si() - 0.015).abs() < 1e-12);
    // The plate drawn around D1's and H1's bolts (no outline in the file):
    // every bolt onto it inside it.
    let now = h.state_mut().joint_details().unwrap();
    let now = now.as_ref().as_ref().unwrap().clone();
    let n2 = now.joint("N2").unwrap();
    let g1 = &n2.plates[0];
    assert!(!g1.outline_given && g1.outline.len() >= 3);
    assert_eq!(g1.members, ["D1", "H1"]);
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("detail-tools-face.png"))
            .unwrap();
    }
    // Plate around the bolts: that outline written into the file.
    h.state_mut().apply_joint_acts(
        vec![Act::PlateOutline("G1".into(), g1.outline.clone())],
        &now,
    );
    h.run();
    assert_eq!(file(&h).plates[0].outline.len(), g1.outline.len());
    // The member view: the strip there too.
    h.state_mut().selected = vec![Handle::Member("D1".into())];
    h.run();
    h.run();
    h.get_by_label("Remove bolt");
    if let Some(dir) = &shots {
        h.run_steps(4);
        h.render()
            .unwrap()
            .save(dir.join("detail-tools-member.png"))
            .unwrap();
    }
    // Undo: the plate's outline goes again.
    h.get_by_label("Undo").click();
    h.run();
    assert!(file(&h).plates[0].outline.is_empty() || file(&h).plates[0].outline.len() == 3);
    std::fs::remove_dir_all(dir).ok();
}

/// The bolt tools keep the design code (owner 2026-11-03): H1's end at N2
/// (two M16 bolts on one line at 30 mm, pitch 50 mm) gets ASCE 10-97's
/// minimums (no analysis: P its bolts' shear capacity, or not known); a
/// bolt asked for too near the steel's end is moved to the end distance
/// on the line; one asked for between the two with no room there is
/// offered a new gauge line, written only once confirmed; Move bolt is in
/// the strip.
#[test]
fn bolt_tools_keep_the_code() {
    use crate::joint_panel::Drawn;
    use crate::joint_tools::Act;
    let path = copy_of("type-02.ohls", "bolt-rules");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("angles.ohls");
    std::fs::write(
        &tower,
        format!(
            "{ANGLES}\n[[connection]]\nmember = \"H1\"\njoint = \"N2\"\n\
             bolts = {{ count = 2, diameter = \"16 mm\" }}\ngauges = [\"30 mm\"]\n\
             gauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n"
        ),
    )
    .unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details…");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.state_mut().joints.drawn = Drawn::Face;
    h.run();
    h.run();
    h.get_by_label("Move bolt");
    let details = h.state_mut().joint_details().unwrap();
    let details = details.as_ref().as_ref().unwrap().clone();
    let ends = [("H1".to_owned(), "N2".to_owned())];
    let rules = h.state_mut().bolt_rules_of(&ends);
    let r = rules.get(&ends[0]).expect("ASCE 10-97 rules").clone();
    assert_eq!(r.code, "ASCE 10-97");
    assert!(r.end >= 1.3 * 0.016 - 1e-9 && r.spacing >= 0.6 * 0.016);
    let a = details
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "H1")
        .unwrap()
        .clone();
    let cut = a.cut.value.unwrap_or(0.0);
    let gauge = a.bolt_flat[0][1];
    // Too near the steel's end: on the line at the end distance (or
    // beyond, past the other bolts).
    match crate::detail_tools::place(&a, "N2", Some(&r), vec![], [cut, gauge], true) {
        Act::BoltPoints(_, _, pts, _) => {
            assert!((pts[0][0] - (cut + r.end)).abs() < 1e-3, "{pts:?}");
            assert!((pts[0][1] - gauge).abs() < 1e-9);
        }
        other => panic!("{other:?}"),
    }
    // Between the two bolts (50 mm apart), the spacing 30 mm (a force
    // per bolt larger than here): no room on the line — a new line
    // offered.
    let wide = ohl_structure_link::BoltRules {
        spacing: 0.03,
        ..r.clone()
    };
    let mid = [(a.bolt_flat[0][0] + a.bolt_flat[1][0]) / 2.0, gauge];
    let act = crate::detail_tools::place(&a, "N2", Some(&wide), a.bolt_flat.clone(), mid, true);
    if let Act::OfferLine { points, .. } = &act {
        assert_eq!(points.len(), 3);
        h.state_mut().apply_joint_acts(vec![act.clone()], &details);
        h.run();
        assert!(h.state().joint_tools.offer.is_some());
        // Not written before it is confirmed.
        let c = h
            .state()
            .file
            .as_ref()
            .unwrap()
            .structure()
            .connection("H1", "N2")
            .cloned();
        assert!(c.is_some_and(|c| c.bolt_points.is_empty()));
        h.get_by_label("Add the line and the bolt").click();
        h.run();
        h.run();
        assert!(h.state().joint_tools.offer.is_none());
        let c = h
            .state()
            .file
            .as_ref()
            .unwrap()
            .structure()
            .connection("H1", "N2")
            .cloned();
        assert_eq!(c.unwrap().bolt_points.len(), 3);
    } else {
        // A leg too narrow for a second line: said why, nothing written.
        assert!(matches!(act, Act::Say(_, true)), "{act:?}");
    }
    std::fs::remove_dir_all(dir).ok();
}
