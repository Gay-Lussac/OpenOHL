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
    menu(&mut h, "Design", "Joint details");
    assert_eq!(h.state_mut().focused(), Some(Panel::JointDetails));
    // Its drawing, the Detail view, a tab on top of the 3D view.
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
    menu(&mut h, "Design", "Joint details");
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
    h.get_by_label("2 × M16").scroll_to_me();
    h.run();
    h.get_by_label("2 × M16").click();
    h.run();
    h.get_by_label_contains("Bolt layout — H1 at N2");
    if let Some(dir) = &shots {
        h.render()
            .unwrap()
            .save(dir.join("angles-layout.png"))
            .unwrap();
    }
    h.get_by_label("Apply").scroll_to_me();
    h.run();
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
    h.get_by_label("+ Plate").scroll_to_me();
    h.run();
    h.get_by_label("+ Plate").click();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().structure().plates.len(), 1);
    h.get_by_label("Members onto it");
    std::fs::remove_dir_all(dir).ok();
}

/// Two legs in the face y = 1 m, a diagonal and a horizontal to the
/// joint between them; the diagonal's end there given in full.
pub(crate) const ANGLES: &str = r#"id = "A"
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
    menu(&mut h, "Design", "Joint details");
    assert!(h.state().is_open(Panel::JointView));
    // Another tab in front of Joint details.
    menu(&mut h, "Body", "Joints");
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
/// clicked is selected, the Inspector in front with its fields; a tower's
/// joint or member is first asked about (owner 2026-11-03), then opened in
/// the Detail view with its details in front on the left — or not (Stay).
/// The Detail view open already: not asked again, the joint or member
/// shown at once, its details in front (owner 2026-11-04: "if i click with
/// edit and the detailed view is already open it should not ask to open
/// it again but just show the joint directly or member").
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
    // Asked first; nothing opened before the answer.
    h.get_by_label_contains("Open member m155 in the Detail view");
    assert!(!h.state().is_open(Panel::MemberDetails));
    h.get_by_label("Open the Detail view").click();
    h.run();
    h.run();
    // A tower's member: Member details in front on the left, the Detail
    // view on top of the 3D view.
    assert!(h.state().is_open(Panel::MemberDetails) && h.state().is_open(Panel::JointView));
    assert_eq!(h.state_mut().focused(), Some(Panel::MemberDetails));
    assert!(h.state().joints.ask_open.is_none());
    assert!(h.get_all_by_label_contains("Member m155").count() >= 1);
    // The details on the left with the text and edit panels; the Detail
    // view a tab on top of the 3D view, in front (owner 2026-11-04).
    let dock = &h.state().dock;
    assert!(crate::layout::same_leaf(
        dock,
        Panel::MemberDetails,
        Panel::Inspector
    ));
    assert!(crate::layout::same_leaf(
        dock,
        Panel::JointView,
        Panel::View3d
    ));
    assert!(crate::layout::is_shown(dock, Panel::JointView));
    // Back to the 3D view: the Detail view still open behind it.
    crate::layout::open_panel(&mut h.state_mut().dock, Panel::View3d);
    h.run();
    assert!(crate::layout::is_shown(&h.state().dock, Panel::View3d));
    assert!(h.state().is_open(Panel::JointView));
    // The Detail view open: a joint clicked is shown there at once.
    h.state_mut()
        .tool_click(Some(Handle::Joint("L9A".into())), None);
    h.run();
    assert!(h.state().joints.ask_open.is_none());
    assert_eq!(
        h.query_all_by_label("Open the Detail view").count(),
        0,
        "not asked again"
    );
    h.run();
    assert!(h.state().is_open(Panel::JointDetails) && h.state().is_open(Panel::JointView));
    assert_eq!(h.state().joints.joint.as_deref(), Some("L9A"));
    assert_eq!(h.state_mut().focused(), Some(Panel::JointDetails));
    assert!(crate::layout::same_leaf(
        &h.state().dock,
        Panel::JointDetails,
        Panel::Inspector
    ));
    assert!(crate::layout::is_shown(&h.state().dock, Panel::JointView));
    // The Detail view's heading (the Inspector a tab behind the details).
    assert_eq!(h.get_all_by_label("Joint L9A").count(), 1);
    // … and a member: Member details in front, not asked.
    h.state_mut()
        .tool_click(Some(Handle::Member("m155".into())), None);
    h.run();
    h.run();
    assert!(h.state().joints.ask_open.is_none());
    assert_eq!(h.state().joints.member.as_deref(), Some("m155"));
    assert_eq!(h.state_mut().focused(), Some(Panel::MemberDetails));
    // The Detail view closed: asked again. Stay: the Inspector only.
    crate::layout::close_panel(&mut h.state_mut().dock, Panel::JointView);
    h.run();
    h.state_mut()
        .tool_click(Some(Handle::Joint("L9B".into())), None);
    h.run();
    assert!(h.state().joints.ask_open.is_some());
    h.run();
    h.get_by_label("Stay").click();
    h.run();
    assert!(h.state().joints.ask_open.is_none());
    assert_eq!(h.state_mut().focused(), Some(Panel::Inspector));
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
    h.get_by_label("Open the Detail view").click();
    h.run();
    h.run();
    assert!(h.state().is_open(Panel::MemberDetails) && h.state().is_open(Panel::JointView));
    assert_eq!(h.state().joints.subject, Subject::Member);
    assert_eq!(h.state().joints.member.as_deref(), Some("D1"));
    menu(&mut h, "Design", "Member details");
    h.run();
    h.get_by_label_contains("Resistance (ASCE 10-97");
    h.get_by_label("Tension");
    // The Detail view's heading (the Inspector a tab behind the details).
    assert_eq!(h.get_all_by_label("Member D1").count(), 1);
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
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.state_mut().joints.drawn = Drawn::Face;
    h.run();
    h.run();
    h.get_by_label("slot: Add bolt");
    h.get_by_label("slot: Member end");
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
    h.get_by_label("slot: Remove bolt");
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
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.state_mut().joints.drawn = Drawn::Face;
    h.run();
    h.run();
    h.get_by_label("slot: Move bolt");
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

/// Getting around the Detail view (owner 2026-11-03): a member opened
/// from the joint goes back to it (‹ Joint, Escape); a bolt picked with
/// the Edit tool shows its properties below, its kind changed from the
/// bolt library (one undo step); several plates at the joint, welded
/// together (2026-11-04: a welded plate is several plates welded
/// together), a plate picked shows what it is welded to and its edges.
#[test]
fn the_detail_view_goes_back_and_shows_what_is_picked() {
    use crate::joint_panel::{Drawn, Picked};
    use crate::joint_tools::Act;
    use crate::member_panel::Subject;
    let path = copy_of("type-02.ohls", "detail-picked");
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
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.state_mut().joints.drawn = Drawn::Face;
    h.run();
    h.run();
    let file = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>| {
        h.state().file.as_ref().unwrap().structure().clone()
    };
    // The details on the left, the Detail view on top of the 3D view.
    assert!(crate::layout::same_leaf(
        &h.state().dock,
        Panel::JointDetails,
        Panel::Inspector
    ));
    assert!(crate::layout::same_leaf(
        &h.state().dock,
        Panel::JointView,
        Panel::View3d
    ));
    // The member in detail from the joint, then back: the button.
    h.state_mut().selected = vec![Handle::Member("D1".into())];
    h.run();
    h.run();
    assert_eq!(h.state().joints.subject, Subject::Member);
    assert_eq!(h.state().joints.back.as_deref(), Some("N2"));
    h.get_by_label("‹ Joint N2").click();
    h.run();
    h.run();
    assert_eq!(h.state().joints.subject, Subject::Joint);
    assert_eq!(h.state().joints.joint.as_deref(), Some("N2"));
    assert_eq!(h.state().selected, vec![Handle::Joint("N2".into())]);
    // … and Escape (the Detail view's details in front).
    h.state_mut().selected = vec![Handle::Member("D1".into())];
    h.run();
    h.run();
    assert_eq!(h.state().joints.subject, Subject::Member);
    h.key_press(eframe::egui::Key::Escape);
    h.run();
    h.run();
    assert_eq!(h.state().joints.subject, Subject::Joint);
    assert_eq!(h.state().joints.joint.as_deref(), Some("N2"));
    // A bolt picked (the Edit tool's act): its properties below.
    let details = h.state_mut().joint_details().unwrap();
    let details = details.as_ref().as_ref().unwrap().clone();
    h.state_mut()
        .apply_joint_acts(vec![Act::PickBolt("D1".into(), "N2".into(), 1)], &details);
    h.run();
    h.run();
    assert_eq!(h.state_mut().focused(), Some(Panel::JointDetails));
    h.get_by_label_contains("Bolt 2 of 3 — D1 at N2");
    h.get_by_label("Bolt of the library");
    // The bolt library offered: one of its bolts for this end's bolts.
    h.get_by_value("typed here").click();
    h.run();
    h.get_by_label("Example bolt 17 mm (invented)").click();
    h.run();
    h.run();
    let c = file(&h).connection("D1", "N2").cloned().unwrap();
    let b = c.bolts.unwrap();
    assert_eq!(b.bolt.as_deref(), Some("X17-example"));
    assert_eq!(b.count, 3);
    h.get_by_label("Library bolt");
    // Escape lets go of the bolt picked.
    h.key_press(eframe::egui::Key::Escape);
    h.run();
    assert!(h.state().joints.picked.is_none());
    // Two plates at the joint: each one undo step.
    for _ in 0..2 {
        h.get_by_label("+ Plate").scroll_to_me();
        h.run();
        h.get_by_label("+ Plate").click();
        h.run();
        h.run();
    }
    let t = file(&h);
    assert_eq!(t.plates.len(), 2);
    let (first, second) = (t.plates[0].id.clone(), t.plates[1].id.clone());
    // The plate just added is picked: what it is welded to, its edges.
    assert_eq!(h.state().joints.picked, Some(Picked::Plate(second.clone())));
    h.get_by_label_contains(&format!("Plate {second} at joint N2"));
    // Welded to the first (its Welded to menu, a box per other plate; the
    // menu stays open on a tick): one welded plate.
    h.get_all_by_label("not welded").next().unwrap().click();
    h.run();
    h.get_all_by_role(eframe::egui::accesskit::Role::CheckBox)
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l == first || l == second)
        })
        .expect("a box per other plate")
        .click();
    h.run();
    h.run();
    let t = file(&h);
    assert_eq!(t.welded_assemblies("N2").len(), 1, "{:?}", t.plates);
    assert!(h.get_all_by_label_contains("one welded plate").count() >= 1);
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(file(&h).welded_assemblies("N2").len(), 2);
    std::fs::remove_dir_all(dir).ok();
}

/// A member's roll (owner 2026-11-04: "i need to have a correction angle
/// for the members to rotate them on their axis as one face need to come
/// in contact with the plates"): its buttons in Member details write the
/// member's `roll` (one undo step each), Lay flat turns it onto what it
/// bolts onto; Joint details names a member lying off what it bolts onto
/// and turns it flat in one click.
#[test]
fn the_roll_buttons_turn_a_member_and_undo() {
    use crate::member_panel::Subject;
    let path = copy_of("type-02.ohls", "member-roll");
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
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Member details");
    h.state_mut().selected = vec![Handle::Member("D1".into())];
    h.run();
    h.run();
    assert_eq!(h.state().joints.subject, Subject::Member);
    let roll = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>, m: &str| {
        let t = h.state().file.as_ref().unwrap().structure();
        ohl_structure_edit::member_roll(t.members.iter().find(|x| x.id == m).unwrap()).to_degrees()
    };
    let text = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>| {
        h.state().file.as_ref().unwrap().text_ref().to_owned()
    };
    assert_eq!(roll(&h, "D1"), 0.0);
    // A quarter turn on, then a half turn: the file's roll.
    h.get_all_by_label("+90°").next().unwrap().click();
    h.run();
    h.run();
    assert!((roll(&h, "D1") - 90.0).abs() < 1e-9);
    assert!(text(&h).contains("roll = \"90 deg\""), "{}", text(&h));
    h.get_all_by_label("180°").next().unwrap().click();
    h.run();
    h.run();
    assert!((roll(&h, "D1") + 90.0).abs() < 1e-9, "{}", roll(&h, "D1"));
    // Undone one step at a time.
    h.get_by_label("Undo").click();
    h.run();
    assert!((roll(&h, "D1") - 90.0).abs() < 1e-9);
    h.get_by_label("Undo").click();
    h.run();
    h.run();
    assert_eq!(roll(&h, "D1"), 0.0);
    assert!(!text(&h).contains("roll ="));
    // Lay flat: its leg bolted on the face of what it bolts onto.
    let off = |h: &mut egui_kittest::Harness<'static, crate::app::StructureApp>, m: &str| {
        let d = h.state_mut().joint_details().unwrap();
        let d = d.as_ref().as_ref().unwrap();
        d.joint("N2")
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == m)
            .and_then(|a| a.flat)
            .map(|f| f.off)
    };
    h.get_all_by_label("+90°").next().unwrap().click();
    h.run();
    h.run();
    let before = off(&mut h, "D1").expect("D1 bolts onto the leg");
    h.get_all_by_label("Lay flat").next().unwrap().click();
    h.run();
    h.run();
    let after = off(&mut h, "D1").unwrap();
    assert!(after < 0.5, "{before}° off before, {after}° after");
    // Joint details: D1 turned a quarter off is named, Turn it flat lays
    // it flat again (one undo step).
    h.get_all_by_label("+90°").next().unwrap().click();
    h.run();
    h.run();
    assert!(off(&mut h, "D1").unwrap() > 80.0);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run();
    h.run();
    h.get_by_label_contains("N2: D1 (");
    let turned = roll(&h, "D1");
    let fix = h.get_all_by_label("Turn it flat").count();
    assert!(fix >= 1);
    // The fix of D1: the first row named D1.
    let d = h.state_mut().joint_details().unwrap();
    let first_off = d
        .as_ref()
        .as_ref()
        .unwrap()
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.flat.is_some_and(|f| f.off > ohl_structure_link::FLAT_OFF))
        .map(|a| a.member.clone())
        .unwrap();
    h.get_all_by_label("Turn it flat").next().unwrap().click();
    h.run();
    h.run();
    assert!(off(&mut h, &first_off).unwrap() < 0.5);
    if first_off == "D1" {
        assert!((roll(&h, "D1") - turned).abs() > 45.0);
    }
    h.get_by_label("Undo").click();
    h.run();
    assert!(off(&mut h, &first_off).unwrap() > ohl_structure_link::FLAT_OFF);
    std::fs::remove_dir_all(dir).ok();
}

/// The plates' and welds' checks in Joint details (owner 2026-11-04: "add
/// weld size and length and the plate checks"): a gusset plate G1 at N2
/// with D1 and H1 bolted onto it and a plate G2 welded to it (in its
/// plane: they do not meet, the weld is said not checked); its weld row
/// takes a size (one undo step, `[[plate.weld]]`); a run (Results › Run)
/// checks the plates per case, and Joint details shows each part's usage
/// with what governs (its clause), each check in a box.
#[test]
fn joint_details_show_the_plate_checks() {
    use std::time::{Duration, Instant};
    let path = copy_of("type-02.ohls", "plate-checks");
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("plates.ohls");
    std::fs::write(
        &tower,
        format!(
            "{}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
             \n[[plate]]\nid = \"G2\"\njoint = \"N2\"\nthickness = \"8 mm\"\nwelded_to = [\"G1\"]\n\
             \n[[connection]]\nmember = \"H1\"\njoint = \"N2\"\nonto = \"G1\"\n\
             bolts = {{ count = 2, diameter = \"16 mm\" }}\ngauges = [\"30 mm\"]\n\
             gauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n\
             \n[[load]]\nnode = \"N5\"\nforce = [\"10 kN\", \"0 kN\", \"-5 kN\"]\n",
            ANGLES.replace("onto = \"L1\"", "onto = \"G1\"")
        ),
    )
    .unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run();
    h.run();
    // The weld row: G1 – G2, no size yet.
    h.get_by_label("G1 – G2");
    h.get_by_label("Size a mm");
    let details = h.state_mut().joint_details().unwrap();
    let details = details.as_ref().as_ref().unwrap().clone();
    h.state_mut().apply_joint_acts(
        vec![crate::joint_tools::Act::WeldData(
            "G1".into(),
            "G2".into(),
            ohl_structure_edit::connection_edits::WeldEdit {
                size: Some(Some(0.005)),
                ..Default::default()
            },
        )],
        &details,
    );
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let w = t.weld_between("G1", "G2").expect("the weld written");
    assert!((w.size.unwrap().si() - 0.005).abs() < 1e-12);
    // Results › Run: the plates checked with the members.
    menu(&mut h, "Results", "FE results");
    h.get_by_label("▶ Run").click();
    let started = Instant::now();
    loop {
        h.step();
        if !h.state().results.running() {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(60), "no result");
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run();
    let r = h.state().results.shown().expect("solved").clone();
    assert!(
        r.plates.iter().any(|p| p.joint == "N2"
            && p.part == ohl_structure_link::PlatePart::Member("D1".into())
            && p.usage.is_some()),
        "{:?}",
        r.plates
    );
    // Joint details: each part's usage and what governs, with its clause.
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run();
    h.run();
    h.get_by_label("Plate checks (the last run)");
    h.get_by_label("D1's bolts");
    h.get_by_label("weld to G2");
    assert!(h.get_all_by_label_contains("ASCE 10-97").count() >= 1);
    h.get_by_label_contains("Each check in case").click();
    h.run();
    assert!(h.get_all_by_label_contains("bearing on the plate").count() >= 1);
    assert!(
        h.get_all_by_label_contains("do not meet (parallel, or apart)")
            .count()
            >= 1
    );
    std::fs::remove_dir_all(dir).ok();
}

/// A gusset plate corrected by hand (owner 2026-11-04: "the user should be
/// able to access the gusset plate details if needed to correct some of
/// the joint in case the automatic sizing fails"): G1 at N2 picked, its
/// automatic outline copied into the file; a corner's u typed in the
/// corner table moves it in the plate's plane (one undo step, undone);
/// the + corner button adds one; the plate moved square to its plane;
/// Back to automatic takes the outline out again.
#[test]
fn a_plate_outline_is_corrected_by_hand() {
    use crate::joint_panel::Drawn;
    use crate::joint_tools::Act;
    use ohl_structure_link::PlateFrame;
    let path = copy_of("type-02.ohls", "plate-outline");
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
            "{}\n[[plate]]\nid = \"G1\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\
             [[connection]]\nmember = \"H1\"\njoint = \"N2\"\nonto = \"G1\"\n\
             bolts = {{ count = 2, diameter = \"16 mm\" }}\ngauges = [\"30 mm\"]\n\
             gauge_lines = 1\npitch = \"50 mm\"\nend_distance = \"30 mm\"\n",
            ANGLES.replace("onto = \"L1\"", "onto = \"G1\"")
        ),
    )
    .unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.state_mut().joints.drawn = Drawn::Face;
    h.run();
    h.run();
    let file = |h: &egui_kittest::Harness<'static, crate::app::StructureApp>| {
        h.state().file.as_ref().unwrap().structure().clone()
    };
    let plate = |h: &mut egui_kittest::Harness<'static, crate::app::StructureApp>| {
        let d = h.state_mut().joint_details().unwrap();
        let d = d.as_ref().as_ref().unwrap();
        let j = d.joint("N2").unwrap().clone();
        let p = j.plates[0].clone();
        (j, p)
    };
    // G1 picked (the Edit tool's act): its outline section.
    let details = h.state_mut().joint_details().unwrap();
    let details = details.as_ref().as_ref().unwrap().clone();
    h.state_mut()
        .apply_joint_acts(vec![Act::PickPlate("G1".into())], &details);
    h.run();
    h.run();
    h.get_by_label("Plate details / outline");
    h.get_by_label_contains("Edge distances it keeps");
    assert!(file(&h).plates[0].outline.is_empty());
    // Start from the automatic outline: written into the file.
    let (_, auto) = plate(&mut h);
    assert!(!auto.outline_given && auto.outline.len() >= 3);
    h.get_by_label("Start from the automatic outline")
        .scroll_to_me();
    h.run();
    h.get_by_label("Start from the automatic outline").click();
    h.run();
    h.run();
    assert_eq!(file(&h).plates[0].outline.len(), auto.outline.len());
    let (j, given) = plate(&mut h);
    assert!(given.outline_given);
    let frame = PlateFrame::of(&j, &given).unwrap();
    let corners = frame.corners(&given.outline);
    // Corner 1's u typed: 25 mm less.
    let steps = h.state().file.as_ref().unwrap().undo_steps();
    let shown = format!("{:.1}", corners[0][0] * 1000.0);
    let new_u = corners[0][0] * 1000.0 - 25.0;
    h.query_all_by_value(&shown).next().unwrap().focus();
    h.run();
    h.key_press_modifiers(eframe::egui::Modifiers::COMMAND, eframe::egui::Key::A);
    h.run();
    h.query_all_by_value(&shown)
        .next()
        .unwrap()
        .type_text(&format!("{new_u:.1}"));
    h.run();
    h.key_press(eframe::egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().undo_steps(), steps + 1);
    let (j2, edited) = plate(&mut h);
    let f2 = PlateFrame::of(&j2, &edited).unwrap();
    let c2 = f2.corners(&edited.outline);
    assert!((c2[0][0] * 1000.0 - new_u).abs() < 0.2, "{c2:?}");
    assert!((c2[0][1] - corners[0][1]).abs() < 2e-4);
    // In the same plane.
    let n = frame.n;
    for q in &edited.outline {
        let d = [
            q[0] - given.outline[0][0],
            q[1] - given.outline[0][1],
            q[2] - given.outline[0][2],
        ];
        assert!((d[0] * n[0] + d[1] * n[1] + d[2] * n[2]).abs() < 2e-4);
    }
    // Undo: the corner back.
    h.state_mut().undo();
    h.run();
    h.run();
    let (_, back) = plate(&mut h);
    for (a, b) in back.outline.iter().zip(&given.outline) {
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-9);
        }
    }
    // + corner: one more.
    h.get_by_label("+ corner").click();
    h.run();
    h.run();
    assert_eq!(file(&h).plates[0].outline.len(), given.outline.len() + 1);
    // The plate moved 12 mm square to its plane (the act Move the plate
    // makes): the work point 12 mm nearer.
    let (j3, p3) = plate(&mut h);
    let f3 = PlateFrame::of(&j3, &p3).unwrap();
    let moved = ohl_structure_link::offset_outline(&p3.outline, f3.n, 0.012);
    h.state_mut().apply_joint_acts(
        vec![Act::Outline {
            id: "G1".into(),
            outline: Some(moved),
            what: "Plate G1 moved".into(),
        }],
        &details,
    );
    h.run();
    let (j4, p4) = plate(&mut h);
    let f4 = PlateFrame::of(&j4, &p4).unwrap();
    assert!((f3.work_point - f4.work_point - 0.012).abs() < 2e-4);
    // Back to automatic: the outline out of the file (one undo step).
    h.get_by_label("Back to automatic").scroll_to_me();
    h.run();
    h.get_by_label("Back to automatic").click();
    h.run();
    h.run();
    assert!(file(&h).plates[0].outline.is_empty());
    let (_, again) = plate(&mut h);
    assert!(!again.outline_given && again.outline.len() >= 3);
    h.state_mut().undo();
    h.run();
    assert_eq!(file(&h).plates[0].outline.len(), given.outline.len() + 1);
    std::fs::remove_dir_all(dir).ok();
}
